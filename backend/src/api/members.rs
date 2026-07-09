//! Self-hosting hub: per-member app access + self-deployed apps.
//!
//! Admin-facing CRUD for the `member` role's app allow-list, custom-deploy
//! opt-in, and storage (quota directory + admin-assigned drives), plus the
//! resolution helpers `apps.rs` calls into at deploy time. See the design doc
//! this was built from for the full picture — in short: everything here is
//! additive and only ever consulted when the acting user's role is `"member"`;
//! every other role's behavior is completely untouched.

use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Matches the `member_storage` table's own column defaults — used when a
/// member has never had a quota row written for them yet.
const DEFAULT_QUOTA_BYTES: i64 = 5 * 1024 * 1024 * 1024; // 5 GiB
const DEFAULT_MAX_APPS: i64 = 5;

/// Host port range reserved for member custom-tier deploys. Kept well away
/// from catalog apps' own (arbitrary, YAML-declared) ports and from anything
/// an admin might have manually exposed, so a member can never claim or
/// collide with a port they don't own.
pub(crate) const MEMBER_CUSTOM_PORT_RANGE: std::ops::RangeInclusive<u16> = 20000..=29999;

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let user = require_user(state, jar).await?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

// ─── Shared summary types ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StorageSummary {
    pub quota_bytes: i64,
    pub max_apps: i64,
    pub used_bytes: i64,
    pub last_check_at: Option<i64>,
    pub app_count: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DriveSummary {
    pub id: String,
    pub label: String,
    pub host_path: String,
    pub total_bytes: Option<i64>,
    pub free_bytes: Option<i64>,
    pub last_check_at: Option<i64>,
}

#[derive(Serialize)]
pub struct MemberAccessSummary {
    pub app_ids: Vec<String>,
    pub can_deploy_custom: bool,
    pub storage: StorageSummary,
    pub drives: Vec<DriveSummary>,
}

async fn build_access_summary(state: &AppState, user_id: &str) -> Result<MemberAccessSummary> {
    let app_ids: Vec<String> = sqlx::query_scalar(
        "SELECT app_id FROM member_app_access WHERE user_id = ? ORDER BY app_id",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let can_deploy_custom: bool = sqlx::query_scalar(
        "SELECT can_deploy_custom FROM member_settings WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .unwrap_or(false);

    let storage_row: Option<(i64, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT quota_bytes, max_apps, used_bytes, last_check_at FROM member_storage WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let (quota_bytes, max_apps, used_bytes, last_check_at) =
        storage_row.unwrap_or((DEFAULT_QUOTA_BYTES, DEFAULT_MAX_APPS, 0, None));

    let app_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deployed_apps WHERE owner_user_id = ?",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let drives: Vec<DriveSummary> = sqlx::query_as(
        "SELECT id, label, host_path, total_bytes, free_bytes, last_check_at \
         FROM member_drives WHERE user_id = ? ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(MemberAccessSummary {
        app_ids,
        can_deploy_custom,
        storage: StorageSummary { quota_bytes, max_apps, used_bytes, last_check_at, app_count },
        drives,
    })
}

// ─── Admin handlers ───────────────────────────────────────────────────────────

pub async fn list_members(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let users: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, username FROM users WHERE role = 'member' ORDER BY username",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut out = Vec::with_capacity(users.len());
    for (id, username) in users {
        let summary = build_access_summary(&state, &id).await?;
        out.push(serde_json::json!({
            "id": id,
            "username": username,
            "app_ids": summary.app_ids,
            "can_deploy_custom": summary.can_deploy_custom,
            "storage": summary.storage,
            "drives": summary.drives,
        }));
    }
    Ok(Json(serde_json::json!({ "members": out })))
}

pub async fn get_access(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> Result<Json<MemberAccessSummary>> {
    require_admin(&state, &jar).await?;
    Ok(Json(build_access_summary(&state, &user_id).await?))
}

/// Self-service equivalent of `get_access` — any authenticated user can read
/// their own access summary (only meaningful when their role is `member`,
/// but harmless for anyone else since the tables are simply empty for them).
pub async fn get_my_access(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<MemberAccessSummary>> {
    let user = require_user(&state, &jar).await?;
    Ok(Json(build_access_summary(&state, &user.id).await?))
}

/// A member's own `agent_capable` nodes, best (most free storage) first, so
/// the deploy UI's node picker can default-select the top entry — this is
/// what makes target-node selection "automatic by default, overridable"
/// without the backend silently guessing a placement it can't yet act on.
#[derive(Serialize)]
pub struct MemberNodeOption {
    pub id: String,
    pub display_name: String,
    pub storage_free_bytes: Option<i64>,
    pub last_seen: Option<i64>,
}

pub async fn list_my_nodes(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        display_name: String,
        last_seen: Option<i64>,
        last_telemetry: Option<String>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT id, display_name, last_seen, last_telemetry FROM nodes \
         WHERE owner_user_id = ? AND agent_capable = 1 ORDER BY display_name",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut options: Vec<MemberNodeOption> = rows
        .into_iter()
        .map(|r| {
            let storage_free_bytes = r
                .last_telemetry
                .as_deref()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
                .and_then(|v| v.get("storage_free_bytes").and_then(|x| x.as_i64()));
            MemberNodeOption { id: r.id, display_name: r.display_name, storage_free_bytes, last_seen: r.last_seen }
        })
        .collect();
    options.sort_by_key(|o| std::cmp::Reverse(o.storage_free_bytes.unwrap_or(0)));

    Ok(Json(serde_json::json!({ "nodes": options })))
}

#[derive(Deserialize)]
pub struct GrantAccessReq {
    pub app_id: String,
}

pub async fn grant_access(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<GrantAccessReq>,
) -> Result<Json<serde_json::Value>> {
    let admin = require_admin(&state, &jar).await?;
    let now = unix_now();

    sqlx::query("INSERT OR IGNORE INTO member_app_access (user_id, app_id, granted_at) VALUES (?, ?, ?)")
        .bind(&user_id)
        .bind(&req.app_id)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&admin.id), "human", "members.access.grant",
        Some("member"), Some(&user_id), "success", None,
        Some(&format!("app_id={}", req.app_id)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn revoke_access(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((user_id, app_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let admin = require_admin(&state, &jar).await?;

    sqlx::query("DELETE FROM member_app_access WHERE user_id = ? AND app_id = ?")
        .bind(&user_id)
        .bind(&app_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&admin.id), "human", "members.access.revoke",
        Some("member"), Some(&user_id), "success", None,
        Some(&format!("app_id={app_id}")),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SetCustomDeployReq {
    pub enabled: bool,
}

pub async fn set_custom_deploy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<SetCustomDeployReq>,
) -> Result<Json<serde_json::Value>> {
    let admin = require_admin(&state, &jar).await?;
    let now = unix_now();

    sqlx::query(
        "INSERT INTO member_settings (user_id, can_deploy_custom, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(user_id) DO UPDATE SET can_deploy_custom = excluded.can_deploy_custom, updated_at = excluded.updated_at",
    )
    .bind(&user_id)
    .bind(req.enabled)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&admin.id), "human", "members.custom_deploy.set",
        Some("member"), Some(&user_id), "success", None,
        Some(&format!("enabled={}", req.enabled)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "enabled": req.enabled })))
}

#[derive(Deserialize)]
pub struct SetQuotaReq {
    pub quota_bytes: i64,
    pub max_apps: i64,
}

pub async fn set_quota(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<SetQuotaReq>,
) -> Result<Json<serde_json::Value>> {
    let admin = require_admin(&state, &jar).await?;
    if req.quota_bytes < 0 || req.max_apps < 0 {
        return Err(AppError::BadRequest("quota_bytes and max_apps must be non-negative".into()));
    }

    sqlx::query(
        "INSERT INTO member_storage (user_id, quota_bytes, max_apps, used_bytes, last_check_at) VALUES (?, ?, ?, 0, NULL) \
         ON CONFLICT(user_id) DO UPDATE SET quota_bytes = excluded.quota_bytes, max_apps = excluded.max_apps",
    )
    .bind(&user_id)
    .bind(req.quota_bytes)
    .bind(req.max_apps)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&admin.id), "human", "members.quota.set",
        Some("member"), Some(&user_id), "success", None,
        Some(&format!("quota_bytes={},max_apps={}", req.quota_bytes, req.max_apps)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Absolute paths a drive must never resolve to — refuses to let an admin
/// accidentally hand a whole system directory to a member as "their storage".
fn validate_host_path(path: &str) -> Result<()> {
    if !path.starts_with('/') {
        return Err(AppError::BadRequest("host_path must be an absolute path".into()));
    }
    if matches!(path, "/" | "/etc" | "/proc" | "/sys" | "/dev" | "/root" | "/var" | "/usr" | "/bin" | "/boot" | "/home") {
        return Err(AppError::BadRequest("Refusing to register a system directory as a drive".into()));
    }
    if !std::path::Path::new(path).is_dir() {
        return Err(AppError::BadRequest(
            "host_path does not exist or is not a directory — it must already be mounted at the OS level".into(),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct AddDriveReq {
    pub label: String,
    pub host_path: String,
}

pub async fn add_drive(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
    Json(req): Json<AddDriveReq>,
) -> Result<Json<serde_json::Value>> {
    let admin = require_admin(&state, &jar).await?;
    let label = req.label.trim().to_string();
    let host_path = req.host_path.trim().to_string();
    if label.is_empty() {
        return Err(AppError::BadRequest("label is required".into()));
    }
    validate_host_path(&host_path)?;

    let id = Uuid::new_v4().to_string();
    let now = unix_now();
    let (total, free) = statvfs_bytes(&host_path)
        .map(|(t, f)| (Some(t as i64), Some(f as i64)))
        .unwrap_or((None, None));

    sqlx::query(
        "INSERT INTO member_drives (id, user_id, label, host_path, total_bytes, free_bytes, last_check_at, created_at) \
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&user_id)
    .bind(&label)
    .bind(&host_path)
    .bind(total)
    .bind(free)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&admin.id), "human", "members.drive.add",
        Some("member"), Some(&user_id), "success", None,
        Some(&format!("label={label},host_path={host_path}")),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

pub async fn remove_drive(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(drive_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let admin = require_admin(&state, &jar).await?;

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT user_id, label FROM member_drives WHERE id = ?",
    )
    .bind(&drive_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let (user_id, label) = row.ok_or_else(|| AppError::BadRequest("Drive not found".into()))?;

    sqlx::query("DELETE FROM member_drives WHERE id = ?")
        .bind(&drive_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&admin.id), "human", "members.drive.remove",
        Some("member"), Some(&user_id), "success", None,
        Some(&format!("label={label}")),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ─── Deploy-path resolution helpers (used by api::apps) ──────────────────────

pub(crate) async fn check_member_app_access(state: &AppState, user_id: &str, app_id: &str) -> Result<()> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM member_app_access WHERE user_id = ? AND app_id = ?",
    )
    .bind(user_id)
    .bind(app_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if exists == 0 {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

pub(crate) async fn member_can_deploy_custom(state: &AppState, user_id: &str) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT can_deploy_custom FROM member_settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

/// Soft quota check (v1 trade-off, see the design doc) — usage is only as
/// fresh as the last poll (`poll_member_storage`), not enforced at the
/// filesystem level.
pub(crate) async fn check_member_quota(state: &AppState, user_id: &str) -> Result<()> {
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT quota_bytes, max_apps, used_bytes FROM member_storage WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let (quota_bytes, max_apps, used_bytes) = row.unwrap_or((DEFAULT_QUOTA_BYTES, DEFAULT_MAX_APPS, 0));

    if used_bytes >= quota_bytes {
        return Err(AppError::BadRequest(format!(
            "Storage quota exceeded ({used_bytes} of {quota_bytes} bytes used) — remove something or ask an admin to raise your quota"
        )));
    }

    let app_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM deployed_apps WHERE owner_user_id = ?",
    )
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if app_count >= max_apps {
        return Err(AppError::BadRequest(format!("App limit reached ({max_apps} apps max)")));
    }

    Ok(())
}

pub(crate) struct StorageResolution {
    pub path: String,
    #[allow(dead_code)]
    pub drive_id: Option<String>,
}

/// Picks where a member's deployed app's volumes live: a manual drive
/// override if given, else the least-full of their assigned drives (a live
/// free-space check, not the possibly-stale polled value — deploy-time
/// selection should always be current), else their quota directory.
pub(crate) async fn resolve_member_storage_root(
    state: &AppState,
    user_id: &str,
    override_drive_id: Option<&str>,
) -> Result<StorageResolution> {
    if let Some(drive_id) = override_drive_id.filter(|s| !s.is_empty()) {
        let host_path: Option<String> = sqlx::query_scalar(
            "SELECT host_path FROM member_drives WHERE id = ? AND user_id = ?",
        )
        .bind(drive_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        let host_path = host_path
            .ok_or_else(|| AppError::BadRequest("Drive not found or not assigned to you".into()))?;
        return Ok(StorageResolution { path: host_path, drive_id: Some(drive_id.to_string()) });
    }

    let drives: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, host_path FROM member_drives WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if !drives.is_empty() {
        let mut best: Option<(String, String, u64)> = None;
        for (id, host_path) in drives {
            let free = statvfs_bytes(&host_path).map(|(_, free)| free).unwrap_or(0);
            if best.as_ref().map(|(_, _, f)| free > *f).unwrap_or(true) {
                best = Some((id, host_path, free));
            }
        }
        if let Some((id, host_path, _)) = best {
            return Ok(StorageResolution { path: host_path, drive_id: Some(id) });
        }
    }

    let quota_dir = state.config.data_dir.join("members").join(user_id);
    std::fs::create_dir_all(&quota_dir).map_err(|e| AppError::Internal(e.into()))?;
    Ok(StorageResolution { path: quota_dir.to_string_lossy().to_string(), drive_id: None })
}

/// Validates (and, if given, applies) a manual target-node override for a
/// member deploy. There is no remote-execution channel to enrolled nodes yet
/// (`node_enroll.rs` only ever receives heartbeats) — so this is bookkeeping
/// for a future dispatch mechanism, not real placement. The compose deploy
/// itself always runs on the primary host today regardless of this value.
/// The auto (no-override) case intentionally returns `None` — "primary host" —
/// rather than silently guessing a node it can't actually deploy onto; the
/// frontend's node picker (`list_my_nodes`) surfaces the best candidate so a
/// member can make that choice explicit instead.
pub(crate) async fn resolve_member_target_node(
    state: &AppState,
    user_id: &str,
    override_node_id: Option<&str>,
) -> Result<Option<String>> {
    let Some(node_id) = override_node_id.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let ok: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM nodes WHERE id = ? AND owner_user_id = ? AND agent_capable = 1",
    )
    .bind(node_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if ok == 0 {
        return Err(AppError::BadRequest("Node not found, not yours, or not agent-capable".into()));
    }
    Ok(Some(node_id.to_string()))
}

pub(crate) async fn allocate_member_port(state: &AppState) -> Result<u16> {
    let next: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(primary_port), 19999) + 1 FROM deployed_apps \
         WHERE primary_port IS NOT NULL AND primary_port >= 20000 AND primary_port < 30000",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok((next as u16).clamp(*MEMBER_CUSTOM_PORT_RANGE.start(), *MEMBER_CUSTOM_PORT_RANGE.end()))
}

// ─── Filesystem polling (mirrors backup_configs.last_check_at) ───────────────

/// `df`-based free/total space for a mount — used both for the admin drive
/// dashboard and the live "least-full drive" pick at deploy time.
fn statvfs_bytes(path: &str) -> Option<(u64, u64)> {
    let out = std::process::Command::new("df")
        .args(["-B1", "--output=size,avail", path])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let mut parts = line.split_whitespace();
    let total: u64 = parts.next()?.parse().ok()?;
    let avail: u64 = parts.next()?.parse().ok()?;
    Some((total, avail))
}

/// `du`-style recursive size, with a manual walk fallback for hosts without
/// `du` (e.g. a minimal container image).
fn dir_size_bytes(path: &std::path::Path) -> u64 {
    if let Ok(out) = std::process::Command::new("du").args(["-sb", &path.to_string_lossy()]).output() {
        if out.status.success() {
            if let Some(field) = String::from_utf8_lossy(&out.stdout).split_whitespace().next() {
                if let Ok(n) = field.parse::<u64>() {
                    return n;
                }
            }
        }
    }
    let mut total = 0u64;
    let Ok(entries) = std::fs::read_dir(path) else { return 0 };
    for entry in entries.flatten() {
        let p = entry.path();
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                total = total.saturating_add(dir_size_bytes(&p));
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    total
}

/// One pass of the periodic storage-quota/drive-usage poll (see main.rs for
/// the interval loop that calls this, same shape as the status-check and
/// restore-test schedulers).
pub async fn poll_member_storage(state: AppState) {
    let now = unix_now();

    let user_ids: Vec<String> = sqlx::query_scalar("SELECT user_id FROM member_storage")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for user_id in user_ids {
        let dir = state.config.data_dir.join("members").join(&user_id);
        let used = tokio::task::spawn_blocking(move || {
            if dir.is_dir() { dir_size_bytes(&dir) } else { 0 }
        })
        .await
        .unwrap_or(0);
        let _ = sqlx::query("UPDATE member_storage SET used_bytes = ?, last_check_at = ? WHERE user_id = ?")
            .bind(used as i64)
            .bind(now)
            .bind(&user_id)
            .execute(&state.db)
            .await;
    }

    let drives: Vec<(String, String)> = sqlx::query_as("SELECT id, host_path FROM member_drives")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    for (id, host_path) in drives {
        if let Some((total, free)) = statvfs_bytes(&host_path) {
            let _ = sqlx::query(
                "UPDATE member_drives SET total_bytes = ?, free_bytes = ?, last_check_at = ? WHERE id = ?",
            )
            .bind(total as i64)
            .bind(free as i64)
            .bind(now)
            .bind(&id)
            .execute(&state.db)
            .await;
        }
    }
}
