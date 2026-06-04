use crate::{
    audit, auth,
    error::{AppError, Result},
    storage,
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use tokio::process::Command;

// ─── Auth helper (mirrors settings.rs) ───────────────────────────────────────

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

// ─── Subprocess helpers ───────────────────────────────────────────────────────

/// Run cmd+args; if it fails or the binary isn't found, retry with `sudo -n`.
async fn run_privileged(prog: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    let direct = Command::new(prog).args(args).output().await;
    match direct {
        Ok(o) if o.status.success() => return Ok(o),
        _ => {}
    }
    Command::new("sudo")
        .arg("-n")
        .arg(prog)
        .args(args)
        .output()
        .await
}

// ─── GET /api/storage/devices ─────────────────────────────────────────────────

pub async fn list_devices(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    // read-only — any authenticated user is fine; but keep behind admin for consistency
    require_admin(&_state, &jar).await?;
    let devices = storage::list_block_devices().await;
    Ok(Json(serde_json::json!({ "devices": devices })))
}

// ─── GET /api/storage/mounts ──────────────────────────────────────────────────

pub async fn list_mounts_handler(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let mounts = storage::list_mounts().await;
    Ok(Json(serde_json::json!({ "mounts": mounts })))
}

// ─── POST /api/storage/mount ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MountReq {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: Option<String>,
}

pub async fn mount_device(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MountReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    // Validate paths — no shell metacharacters
    for s in [&req.device, &req.mountpoint, &req.fstype] {
        if s.contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n')) {
            return Err(AppError::BadRequest("Invalid characters in request".into()));
        }
    }

    // mkdir -p mountpoint
    let _ = run_privileged("mkdir", &["-p", &req.mountpoint]).await;

    // mount -t fstype [-o options] device mountpoint
    let mut args: Vec<&str> = vec!["-t", &req.fstype];
    let opts_owned;
    if let Some(ref o) = req.options {
        if !o.is_empty() {
            opts_owned = o.clone();
            args.extend_from_slice(&["-o", &opts_owned]);
        }
    }
    args.extend_from_slice(&[&req.device, &req.mountpoint]);

    let out = run_privileged("mount", &args)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let ok = out.status.success();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.mount",
        Some("storage"),
        Some(&req.mountpoint),
        if ok { "success" } else { "failure" },
        None,
        Some(&format!(
            "device={} mountpoint={} fstype={}",
            req.device, req.mountpoint, req.fstype
        )),
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(AppError::BadRequest(if stderr.is_empty() {
            "mount failed".into()
        } else {
            stderr
        }))
    }
}

// ─── POST /api/storage/umount ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UmountReq {
    pub mountpoint: String,
}

pub async fn umount_device(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<UmountReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if req
        .mountpoint
        .contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n'))
    {
        return Err(AppError::BadRequest("Invalid characters in mountpoint".into()));
    }

    let out = run_privileged("umount", &[&req.mountpoint])
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let ok = out.status.success();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.umount",
        Some("storage"),
        Some(&req.mountpoint),
        if ok { "success" } else { "failure" },
        None,
        None,
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(AppError::BadRequest(if stderr.is_empty() {
            "umount failed".into()
        } else {
            stderr
        }))
    }
}

// ─── GET /api/storage/fstab ───────────────────────────────────────────────────

pub async fn get_fstab(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let entries = storage::read_fstab().await;
    Ok(Json(serde_json::json!({ "entries": entries })))
}

// ─── POST /api/storage/fstab ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AddFstabReq {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: String,
    pub dump: Option<i32>,
    pub pass: Option<i32>,
}

pub async fn add_fstab(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AddFstabReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    for s in [&req.device, &req.mountpoint, &req.fstype, &req.options] {
        if s.contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n')) {
            return Err(AppError::BadRequest("Invalid characters in fstab entry".into()));
        }
    }

    let dump = req.dump.unwrap_or(0);
    let pass = req.pass.unwrap_or(0);
    let line = format!(
        "{}\t{}\t{}\t{}\t{}\t{}\n",
        req.device, req.mountpoint, req.fstype, req.options, dump, pass
    );

    // Try writing directly, then via sudo tee -a
    let write_result = tokio::fs::OpenOptions::new()
        .append(true)
        .open("/etc/fstab")
        .await;

    let ok = match write_result {
        Ok(mut f) => {
            use tokio::io::AsyncWriteExt;
            f.write_all(line.as_bytes()).await.is_ok()
        }
        Err(_) => {
            // Try sudo tee -a
            let mut child = Command::new("sudo")
                .args(["-n", "tee", "-a", "/etc/fstab"])
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .map_err(|e| AppError::Internal(e.into()))?;

            if let Some(stdin) = child.stdin.as_mut() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(line.as_bytes()).await;
            }
            child
                .wait()
                .await
                .map(|s| s.success())
                .unwrap_or(false)
        }
    };

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.fstab.add",
        Some("storage"),
        Some(&req.mountpoint),
        if ok { "success" } else { "failure" },
        None,
        Some(&format!("device={} fstype={}", req.device, req.fstype)),
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(AppError::BadRequest(
            "Failed to write /etc/fstab — check permissions".into(),
        ))
    }
}

// ─── DELETE /api/storage/fstab/:idx ──────────────────────────────────────────

pub async fn remove_fstab(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(idx): Path<usize>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let content = tokio::fs::read_to_string("/etc/fstab")
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut lines: Vec<&str> = content.lines().collect();
    if idx >= lines.len() {
        return Err(AppError::BadRequest("Line index out of range".into()));
    }

    lines.remove(idx);
    let new_content = lines.join("\n") + "\n";

    // Try direct write then sudo
    let write_ok = tokio::fs::write("/etc/fstab", &new_content).await.is_ok();
    let ok = if write_ok {
        true
    } else {
        let mut child = Command::new("sudo")
            .args(["-n", "tee", "/etc/fstab"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .map_err(|e| AppError::Internal(e.into()))?;

        if let Some(stdin) = child.stdin.as_mut() {
            use tokio::io::AsyncWriteExt;
            let _ = stdin.write_all(new_content.as_bytes()).await;
        }
        child
            .wait()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    };

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.fstab.remove",
        Some("storage"),
        Some(&idx.to_string()),
        if ok { "success" } else { "failure" },
        None,
        None,
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(AppError::BadRequest(
            "Failed to write /etc/fstab — check permissions".into(),
        ))
    }
}

// ─── GET /api/storage/smart/:dev ─────────────────────────────────────────────

pub async fn get_smart(
    State(_state): State<AppState>,
    jar: CookieJar,
    Path(dev): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let info = storage::smart_info(&dev).await;
    Ok(Json(serde_json::json!(info)))
}

// ─── GET /api/storage/raid ────────────────────────────────────────────────────

pub async fn get_raid(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let available = storage::which_cmd("mdadm");
    if !available {
        return Ok(Json(
            serde_json::json!({ "available": false, "arrays": [] }),
        ));
    }
    let arrays = storage::list_raid().await;
    Ok(Json(serde_json::json!({ "available": true, "arrays": arrays })))
}

// ─── POST /api/storage/raid/create ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateRaidReq {
    pub name: String,
    pub level: String,
    pub devices: Vec<String>,
}

pub async fn create_raid(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRaidReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if !storage::which_cmd("mdadm") {
        return Err(AppError::FeatureUnavailable("mdadm not installed".into()));
    }

    // Sanitize name: alphanumeric + dash only
    let name: String = req
        .name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect();
    if name.is_empty() {
        return Err(AppError::BadRequest("Invalid RAID name".into()));
    }

    if req.devices.len() < 2 {
        return Err(AppError::BadRequest(
            "At least 2 devices required".into(),
        ));
    }

    // Validate level
    let level_num = match req.level.as_str() {
        "0" | "raid0" | "RAID0" => "0",
        "1" | "raid1" | "RAID1" => "1",
        "5" | "raid5" | "RAID5" => "5",
        "6" | "raid6" | "RAID6" => "6",
        "10" | "raid10" | "RAID10" => "10",
        _ => return Err(AppError::BadRequest("Invalid RAID level".into())),
    };

    // Validate device paths
    for dev in &req.devices {
        if dev.contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '\n' | ' ')) {
            return Err(AppError::BadRequest(
                "Invalid characters in device path".into(),
            ));
        }
    }

    let md_path = format!("/dev/{name}");
    let n_str = req.devices.len().to_string();

    let mut args: Vec<String> = vec![
        "--create".into(),
        md_path.clone(),
        format!("--level={level_num}"),
        format!("--raid-devices={n_str}"),
    ];
    // --run skips the interactive confirmation prompt
    args.push("--run".into());
    args.extend(req.devices.iter().cloned());

    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_privileged("mdadm", &args_ref)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let ok = out.status.success();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.raid.create",
        Some("storage"),
        Some(&md_path),
        if ok { "success" } else { "failure" },
        None,
        Some(&format!(
            "level={level_num} devices={}",
            req.devices.join(",")
        )),
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true, "path": md_path })))
    } else {
        Err(AppError::BadRequest(if stderr.is_empty() {
            "mdadm create failed".into()
        } else {
            stderr
        }))
    }
}

// ─── POST /api/storage/raid/stop ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StopRaidReq {
    pub path: String,
}

pub async fn stop_raid(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<StopRaidReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if !storage::which_cmd("mdadm") {
        return Err(AppError::FeatureUnavailable("mdadm not installed".into()));
    }

    if req
        .path
        .contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '\n'))
    {
        return Err(AppError::BadRequest("Invalid path".into()));
    }

    let out = run_privileged("mdadm", &["--stop", &req.path])
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let ok = out.status.success();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.raid.stop",
        Some("storage"),
        Some(&req.path),
        if ok { "success" } else { "failure" },
        None,
        None,
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(AppError::BadRequest(if stderr.is_empty() {
            "mdadm stop failed".into()
        } else {
            stderr
        }))
    }
}

// ─── POST /api/storage/format ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FormatReq {
    pub device: String,
    pub fstype: String,
    pub label: Option<String>,
}

pub async fn format_device(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<FormatReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    for s in [&req.device, &req.fstype] {
        if s.contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '>' | '<' | '\n' | ' ')) {
            return Err(AppError::BadRequest("Invalid characters in request".into()));
        }
    }

    // Check the device is NOT currently mounted
    let mounts = storage::list_mounts().await;
    if mounts.iter().any(|m| m.device == req.device) {
        return Err(AppError::BadRequest(format!(
            "{} is currently mounted — unmount it first",
            req.device
        )));
    }

    // Map fstype -> mkfs command + label flag
    let (mkfs_cmd, label_flag): (&str, &str) = match req.fstype.to_lowercase().as_str() {
        "ext4" => ("mkfs.ext4", "-L"),
        "ext3" => ("mkfs.ext3", "-L"),
        "xfs"  => ("mkfs.xfs",  "-L"),
        "btrfs" => ("mkfs.btrfs", "-L"),
        "fat32" | "vfat" => ("mkfs.vfat", "-n"),
        "ntfs" => ("mkfs.ntfs", "-L"),
        _ => return Err(AppError::BadRequest(format!("Unsupported filesystem: {}", req.fstype))),
    };

    let mut args: Vec<String> = Vec::new();
    if let Some(ref lbl) = req.label {
        if !lbl.is_empty() {
            args.push(label_flag.into());
            args.push(lbl.clone());
        }
    }
    // Force / no-interactive flags per fs
    match req.fstype.to_lowercase().as_str() {
        "xfs"  => args.push("-f".into()),
        "btrfs" => args.push("-f".into()),
        "ntfs" => args.push("--fast".into()),
        _ => {}
    }
    args.push(req.device.clone());

    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = run_privileged(mkfs_cmd, &args_ref)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let ok = out.status.success();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "storage.format",
        Some("storage"),
        Some(&req.device),
        if ok { "success" } else { "failure" },
        None,
        Some(&format!("fstype={}", req.fstype)),
    )
    .await;

    if ok {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        Err(AppError::BadRequest(if stderr.is_empty() {
            "format failed".into()
        } else {
            stderr
        }))
    }
}

// ─── Storage location paths ───────────────────────────────────────────────────
// Persisted in the settings table under storage.paths.* keys.

async fn db_get_path(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
}

async fn db_set_path(state: &AppState, key: &str, value: &str) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

pub async fn get_storage_paths(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let containers = db_get_path(&state, "storage.paths.containers").await;
    let appvault   = db_get_path(&state, "storage.paths.appvault").await;
    let vms        = db_get_path(&state, "storage.paths.vms").await;
    let backups    = db_get_path(&state, "storage.paths.backups").await;
    Ok(Json(serde_json::json!({
        "containers": containers,
        "appvault":   appvault,
        "vms":        vms,
        "backups":    backups,
    })))
}

#[derive(Deserialize)]
pub struct SetStoragePathsReq {
    pub containers: Option<String>,
    pub appvault:   Option<String>,
    pub vms:        Option<String>,
    pub backups:    Option<String>,
}

pub async fn set_storage_paths(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetStoragePathsReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let pairs: &[(&str, Option<&String>)] = &[
        ("storage.paths.containers", req.containers.as_ref()),
        ("storage.paths.appvault",   req.appvault.as_ref()),
        ("storage.paths.vms",        req.vms.as_ref()),
        ("storage.paths.backups",    req.backups.as_ref()),
    ];

    for (key, val) in pairs {
        if let Some(v) = val {
            if !v.starts_with('/') {
                return Err(AppError::BadRequest(format!("{key}: path must be absolute")));
            }
            if v.contains(|c: char| matches!(c, ';' | '&' | '|' | '`' | '$' | '\n')) {
                return Err(AppError::BadRequest(format!("{key}: invalid characters")));
            }
            db_set_path(&state, key, v).await?;
        }
    }

    audit::log(
        &state.db, Some(&user.id), "human", "storage.paths.set",
        Some("storage"), None, "success", None, None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}
