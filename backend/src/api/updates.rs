use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::{Path, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::{Mutex, OnceLock}};

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

fn project_root() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent()?.parent()?.parent()?.parent().map(|p| p.to_path_buf())
}

fn run(root: &std::path::Path, cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd).args(args).current_dir(root)
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn is_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
}

fn is_dev_install() -> bool {
    std::env::current_exe().ok()
        .map(|p| p.to_string_lossy().contains("/target/"))
        .unwrap_or(false)
}

fn install_dir() -> std::path::PathBuf {
    std::env::current_exe().ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("/opt/voidtower"))
}

fn github_latest_commit(repo: &str, branch: &str) -> Option<String> {
    let url = format!("https://api.github.com/repos/{repo}/commits/{branch}");
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "10", "-A", "voidtower-update",
               "-H", "Accept: application/vnd.github.sha", &url])
        .output().ok()?;
    if !out.status.success() { return None; }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(sha[..7].to_string())
    } else {
        None
    }
}

// ─── VoidTower updates ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CommitInfo { pub hash: String, pub subject: String, pub author: String, pub date: String }

#[derive(Serialize)]
pub struct VtUpdateInfo {
    pub mode: String,           // "git" | "docker"
    // git mode
    pub current_commit: String,
    pub remote_commit: String,
    pub behind: usize,
    pub ahead: usize,
    pub commits: Vec<CommitInfo>,
    pub backup_tags: Vec<String>,
    pub fetch_error: Option<String>,
    // docker mode
    pub current_image: Option<String>,
    pub update_status: Option<String>, // "unknown"|"checking"|"up-to-date"|"update-available"|"error"
    pub update_detail: Option<String>,
}

#[derive(Clone)]
struct VtDockerStatus { status: String, detail: Option<String> }
static VT_DOCKER_CACHE: OnceLock<Mutex<VtDockerStatus>> = OnceLock::new();
fn vt_docker_cache() -> &'static Mutex<VtDockerStatus> {
    VT_DOCKER_CACHE.get_or_init(|| Mutex::new(VtDockerStatus { status: "unknown".into(), detail: None }))
}

// Returns (image_name, container_id_short, compose_project, compose_config_files)
fn vt_own_info() -> Option<(String, String, String, String)> {
    let hostname = std::env::var("HOSTNAME").ok()?;
    let out = std::process::Command::new("docker")
        .args(["inspect", &hostname, "--format",
            "{{.Config.Image}}|{{.Id}}|{{index .Config.Labels \"com.docker.compose.project\"}}|{{index .Config.Labels \"com.docker.compose.project.config_files\"}}"])
        .output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = s.trim().splitn(4, '|').collect();
    if parts.len() < 4 { return None; }
    Some((
        parts[0].to_string(),
        parts[1].chars().take(12).collect(),
        parts[2].to_string(),
        parts[3].to_string(),
    ))
}

// Returns (current_commit, remote_commit, behind, ahead, commits, fetch_error) for the
// upstream branch tracked by `root`. Shared by vt_info and apply_vt's dry-run plan so
// both always report the same numbers.
fn git_ahead_behind(root: &std::path::Path) -> (String, String, usize, usize, Vec<CommitInfo>, Option<String>) {
    let branch = { let b = run(root, "git", &["rev-parse", "--abbrev-ref", "HEAD"]); if b.is_empty() { "main".into() } else { b } };
    let remote_ref = format!("origin/{branch}");

    let fetch_err = std::process::Command::new("git").args(["fetch", "origin"]).current_dir(root)
        .output().err().map(|e| e.to_string());

    let current_commit = run(root, "git", &["rev-parse", "--short", "HEAD"]);
    let remote_commit  = run(root, "git", &["rev-parse", "--short", &remote_ref]);
    let behind: usize  = run(root, "git", &["rev-list", &format!("HEAD..{remote_ref}"), "--count"]).parse().unwrap_or(0);
    let ahead: usize   = run(root, "git", &["rev-list", &format!("{remote_ref}..HEAD"), "--count"]).parse().unwrap_or(0);

    let log = run(root, "git", &["log", &format!("HEAD..{remote_ref}"), "--format=%H|%s|%an|%ci", "--no-merges"]);
    let commits = log.lines().filter(|l| !l.is_empty()).map(|l| {
        let parts: Vec<&str> = l.splitn(4, '|').collect();
        CommitInfo {
            hash:    parts.first().unwrap_or(&"").chars().take(7).collect(),
            subject: parts.get(1).unwrap_or(&"").to_string(),
            author:  parts.get(2).unwrap_or(&"").to_string(),
            date:    parts.get(3).unwrap_or(&"").get(..10).unwrap_or("").to_string(),
        }
    }).collect();

    (current_commit, remote_commit, behind, ahead, commits, fetch_err)
}

fn git_update_risk(behind: usize) -> &'static str {
    if behind <= 3 { "low" } else if behind <= 10 { "medium" } else { "high" }
}

pub async fn vt_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<VtUpdateInfo>> {
    require_admin(&state, &jar).await?;

    if is_docker() {
        let current_image = vt_own_info().map(|(img, ..)| img).unwrap_or_else(|| "unknown".into());
        let cached = vt_docker_cache().lock().unwrap().clone();
        return Ok(Json(VtUpdateInfo {
            mode: "docker".into(),
            current_commit: String::new(), remote_commit: String::new(),
            behind: 0, ahead: 0, commits: vec![], backup_tags: vec![], fetch_error: None,
            current_image: Some(current_image),
            update_status: Some(cached.status),
            update_detail: cached.detail,
        }));
    }

    if !is_dev_install() {
        // prod bare-metal: binary at /opt/voidtower, no git repo
        let dir = install_dir();
        let commit_hash = std::fs::read_to_string(dir.join(".commit"))
            .unwrap_or_default().trim().to_string();
        let current_commit = if commit_hash.len() >= 7 {
            commit_hash[..7].to_string()
        } else if !commit_hash.is_empty() {
            commit_hash
        } else {
            std::fs::read_to_string(dir.join(".version"))
                .unwrap_or_else(|_| "unknown".into()).trim().to_string()
        };
        let remote_commit = github_latest_commit("niwlekakan/voidtower", "voidtower-aio")
            .unwrap_or_else(|| "unknown".into());
        let behind = if remote_commit != "unknown" && !current_commit.is_empty()
            && remote_commit != current_commit { 1 } else { 0 };
        return Ok(Json(VtUpdateInfo {
            mode: "git".into(),
            current_commit, remote_commit, behind, ahead: 0,
            commits: vec![], backup_tags: vec![], fetch_error: None,
            current_image: None, update_status: None, update_detail: None,
        }));
    }

    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let (current_commit, remote_commit, behind, ahead, commits, fetch_err) = git_ahead_behind(&root);

    let tags_raw = run(&root, "git", &["tag", "--list", "vt-backup-*", "--sort=-creatordate"]);
    let backup_tags = tags_raw.lines().filter(|l| !l.is_empty()).map(String::from).take(10).collect();

    Ok(Json(VtUpdateInfo {
        mode: "git".into(),
        current_commit, remote_commit, behind, ahead, commits, backup_tags, fetch_error: fetch_err,
        current_image: None, update_status: None, update_detail: None,
    }))
}

pub async fn check_vt(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if !is_docker() {
        return Err(AppError::FeatureUnavailable("only available in Docker mode".into()));
    }
    let (image, ..) = vt_own_info()
        .ok_or_else(|| AppError::FeatureUnavailable("Docker socket not available or container not found — mount /var/run/docker.sock".into()))?;

    { let mut c = vt_docker_cache().lock().unwrap(); c.status = "checking".into(); c.detail = None; }

    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking({
            let image = image.clone();
            move || std::process::Command::new("docker").args(["pull", &image])
                .output()
                .map(|o| (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string()))
        }).await;

        let (status, detail) = match result {
            Ok(Ok((true, out))) => {
                if out.contains("Downloaded newer image") {
                    ("update-available".into(), Some("New image downloaded and ready to apply".into()))
                } else {
                    ("up-to-date".into(), None)
                }
            }
            Ok(Ok((false, out))) => ("error".into(), Some(out.lines().last().unwrap_or("docker pull failed").to_string())),
            _ => ("error".into(), Some("docker pull failed".into())),
        };
        let mut c = vt_docker_cache().lock().unwrap();
        c.status = status;
        c.detail = detail;
    });

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize, Default)]
pub struct ApplyReq { #[serde(default)] pub dry_run: bool }

pub async fn apply_vt(
    State(state): State<AppState>, jar: CookieJar, Json(req): Json<ApplyReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    if is_docker() {
        let (image, hostname, compose_project, compose_file) = vt_own_info()
            .ok_or_else(|| AppError::FeatureUnavailable("Docker socket not available — mount /var/run/docker.sock".into()))?;

        if req.dry_run {
            return Ok(Json(serde_json::json!({
                "dry_run": true,
                "plan": {
                    "title": "Update VoidTower (Docker)",
                    "risk": "high",
                    "changes": [
                        { "label": "Current image", "value": image },
                        { "label": "Action",        "value": "Pull latest image and recreate container" },
                        { "label": "Downtime",      "value": "Brief — UI unavailable during restart" },
                    ],
                    "preview": null
                }
            })));
        }

        // compose_file is a HOST path baked into the container label — it won't
        // exist inside the container unless explicitly volume-mounted. Fall back
        // to pull + restart when the file isn't accessible from here.
        let cmd = if !compose_file.is_empty() && !compose_project.is_empty()
            && std::path::Path::new(&compose_file).exists()
        {
            format!(
                "docker compose -p {proj} -f {file} pull voidtower && docker compose -p {proj} -f {file} up -d voidtower",
                proj = compose_project, file = compose_file,
            )
        } else {
            format!("docker pull {image} && docker restart {hostname}")
        };

        std::process::Command::new("sh")
            .args(["-c", &cmd])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| AppError::Internal(e.into()))?;

        return Ok(Json(serde_json::json!({ "ok": true })));
    }

    if !is_dev_install() {
        if req.dry_run {
            return Ok(Json(serde_json::json!({
                "dry_run": true,
                "plan": {
                    "title": "Update VoidTower (binary)",
                    "risk": "medium",
                    "changes": [
                        { "label": "Action", "value": "Download latest release binary and restart" },
                    ],
                    "preview": null
                }
            })));
        }
        // prod bare-metal: download latest release binary, replace in place, let systemd restart
        let dir_s = install_dir().to_string_lossy().to_string();
        let pid = std::process::id();
        let script = format!(
            "#!/bin/sh\nset -e\n\
             ARCH=$(uname -m)\n\
             INSTALL_DIR={dir_s}\n\
             LATEST=$(curl -fsSL --max-time 30 -A voidtower-update \
             'https://api.github.com/repos/niwlekakan/voidtower/releases/latest' \
             | grep '\"tag_name\"' | sed 's/.*\"v\\([^\"]*\\)\".*/\\1/')\n\
             [ -z \"$LATEST\" ] && {{ echo 'Failed to fetch latest version' >&2; exit 1; }}\n\
             ARCHIVE=\"voidtower-$LATEST-$ARCH-unknown-linux-musl.tar.gz\"\n\
             curl -fsSL --max-time 120 \
             \"https://github.com/niwlekakan/voidtower/releases/download/v$LATEST/$ARCHIVE\" \
             -o /tmp/vt-update.tar.gz\n\
             cd /tmp && tar -xzf /tmp/vt-update.tar.gz voidtower\n\
             mv /tmp/voidtower \"$INSTALL_DIR/voidtower\"\n\
             chmod +x \"$INSTALL_DIR/voidtower\"\n\
             echo \"$LATEST\" > \"$INSTALL_DIR/.version\"\n\
             sleep 1\n\
             kill -TERM {pid}\n"
        );
        std::fs::write("/tmp/voidtower-update.sh", script).map_err(|e| AppError::Internal(e.into()))?;
        std::process::Command::new("sh").args(["/tmp/voidtower-update.sh"])
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .spawn().map_err(|e| AppError::Internal(e.into()))?;
        return Ok(Json(serde_json::json!({ "ok": true })));
    }

    // dev: git pull + rebuild + restart
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;

    if req.dry_run {
        let (_, _, behind, _, commits, _) = git_ahead_behind(&root);
        let preview = commits.iter().map(|c| c.subject.clone()).collect::<Vec<_>>().join("\n");
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Update VoidTower (git)",
                "risk": git_update_risk(behind),
                "changes": [
                    { "label": "Backup tag",     "value": "vt-backup-<timestamp> (created automatically)" },
                    { "label": "Commits behind", "value": behind.to_string() },
                    { "label": "Action",         "value": "git pull + cargo build + npm build + restart" },
                ],
                "preview": if preview.is_empty() { None } else { Some(preview) }
            }
        })));
    }

    let pid  = std::process::id();
    let ts   = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let tag  = format!("vt-backup-{ts}");
    let branch = { let b = run(&root, "git", &["rev-parse", "--abbrev-ref", "HEAD"]); if b.is_empty() { "main".into() } else { b } };

    let script = format!(
        "#!/bin/sh\nset -e\ncd {root}\n\
         git tag {tag}\n\
         git pull origin {branch}\n\
         cargo build --manifest-path backend/Cargo.toml --release\n\
         npm --prefix frontend run build\n\
         sleep 1\nkill -TERM {pid}\nsleep 1\n\
         exec {root}/backend/target/release/voidtower >> /tmp/voidtower.log 2>&1\n",
        root = root.display()
    );
    std::fs::write("/tmp/voidtower-update.sh", script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("sh").args(["/tmp/voidtower-update.sh"])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true, "backup_tag": tag })))
}

#[derive(Deserialize)]
pub struct RollbackReq { pub tag: String, #[serde(default)] pub dry_run: bool }

pub async fn rollback_vt(
    State(state): State<AppState>, jar: CookieJar, Json(req): Json<RollbackReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if is_docker() {
        return Err(AppError::FeatureUnavailable("rollback via git tags is not available in Docker mode".into()));
    }
    if !req.tag.starts_with("vt-backup-") || req.tag.contains('/') || req.tag.contains("..") {
        return Err(AppError::BadRequest("Invalid backup tag".into()));
    }

    if req.dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Rollback VoidTower",
                "risk": "high",
                "changes": [
                    { "label": "Target tag", "value": req.tag },
                    { "label": "Action",     "value": "git checkout + rebuild + restart" },
                ],
                "preview": null
            }
        })));
    }

    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let pid  = std::process::id();
    let script = format!(
        "#!/bin/sh\nset -e\ncd {root}\n\
         git checkout {tag}\n\
         cargo build --manifest-path backend/Cargo.toml --release\n\
         npm --prefix frontend run build\n\
         sleep 1\nkill -TERM {pid}\nsleep 1\n\
         exec {root}/backend/target/release/voidtower >> /tmp/voidtower.log 2>&1\n",
        root = root.display(), tag = req.tag
    );
    std::fs::write("/tmp/voidtower-rollback.sh", script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("sh").args(["/tmp/voidtower-rollback.sh"])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true, "rolling_back_to": req.tag })))
}

// ─── Docker image updates ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DockerImageRow {
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub status: String, // "unknown" | "checking" | "up-to-date" | "update-available" | "error"
    pub detail: Option<String>,
}

static DOCKER_CACHE: OnceLock<Mutex<HashMap<String, DockerImageRow>>> = OnceLock::new();
fn docker_cache() -> &'static Mutex<HashMap<String, DockerImageRow>> {
    DOCKER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn list_running_containers() -> Vec<(String, String, String)> {
    let out = match std::process::Command::new("docker")
        .args(["ps", "--format", "{{.ID}}|{{.Names}}|{{.Image}}"])
        .output() { Ok(o) => o, Err(_) => return vec![] };
    String::from_utf8_lossy(&out.stdout).lines()
        .filter_map(|l| {
            let p: Vec<&str> = l.splitn(3, '|').collect();
            if p.len() == 3 { Some((p[0].to_string(), p[1].to_string(), p[2].to_string())) } else { None }
        }).collect()
}

pub async fn docker_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<DockerImageRow>>> {
    require_admin(&state, &jar).await?;
    let containers = list_running_containers();
    let cache = docker_cache().lock().unwrap();
    let rows = containers.into_iter().map(|(id, name, image)| {
        cache.get(&id).cloned().unwrap_or(DockerImageRow {
            container_id: id, container_name: name, image, status: "unknown".into(), detail: None,
        })
    }).collect();
    Ok(Json(rows))
}

pub async fn docker_check(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let containers = list_running_containers();

    {
        let mut cache = docker_cache().lock().unwrap();
        for (id, name, image) in &containers {
            cache.insert(id.clone(), DockerImageRow {
                container_id: id.clone(), container_name: name.clone(),
                image: image.clone(), status: "checking".into(), detail: None,
            });
        }
    }

    tokio::spawn(async move {
        for (id, name, image) in containers {
            let result = tokio::task::spawn_blocking({
                let image = image.clone();
                move || std::process::Command::new("docker").args(["pull", &image])
                    .output()
                    .map(|o| (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string()))
            }).await;

            let (status, detail) = match result {
                Ok(Ok((true, out))) => {
                    if out.contains("Downloaded newer image") {
                        ("update-available".to_string(), Some("New image downloaded".to_string()))
                    } else {
                        ("up-to-date".to_string(), None)
                    }
                }
                Ok(Ok((false, out))) => ("error".to_string(), Some(out.trim().to_string())),
                _ => ("error".to_string(), Some("Failed to run docker pull".to_string())),
            };

            let mut cache = docker_cache().lock().unwrap();
            cache.insert(id.clone(), DockerImageRow {
                container_id: id, container_name: name, image, status, detail,
            });
        }
    });

    Ok(Json(serde_json::json!({ "ok": true, "message": "Check started" })))
}

pub async fn docker_apply(
    State(state): State<AppState>, jar: CookieJar, Path(container_id): Path<String>,
    Json(req): Json<ApplyReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    if !container_id.chars().all(|c| c.is_alphanumeric()) {
        return Err(AppError::BadRequest("Invalid container ID".into()));
    }

    let inspect = std::process::Command::new("docker")
        .args(["inspect", &container_id, "--format",
               "{{.Name}}|{{.Config.Image}}|{{index .Config.Labels \"com.docker.compose.project\"}}|{{index .Config.Labels \"com.docker.compose.project.config_files\"}}"])
        .output().map_err(|e| AppError::Internal(e.into()))?;
    let info = String::from_utf8_lossy(&inspect.stdout);
    let parts: Vec<&str> = info.trim().splitn(4, '|').collect();
    let name            = parts.first().unwrap_or(&"").trim_start_matches('/').to_string();
    let image           = parts.get(1).unwrap_or(&"").to_string();
    let compose_project = parts.get(2).unwrap_or(&"").to_string();
    let compose_file    = parts.get(3).unwrap_or(&"").to_string();

    // compose_file is a HOST path from the container label — only use it if
    // it's accessible from inside this container (i.e. volume-mounted).
    let use_compose = !compose_file.is_empty()
        && !compose_project.is_empty()
        && std::path::Path::new(&compose_file).exists();

    if req.dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Update container",
                "risk": "medium",
                "changes": [
                    { "label": "Container", "value": name },
                    { "label": "Image",     "value": image },
                    { "label": "Action",    "value": if use_compose { "docker compose pull + up -d" } else { "docker pull + restart" } },
                ],
                "preview": null
            }
        })));
    }

    let output = if use_compose {
        let pull = std::process::Command::new("docker")
            .args(["compose", "-p", &compose_project, "-f", &compose_file, "pull"])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        let up = std::process::Command::new("docker")
            .args(["compose", "-p", &compose_project, "-f", &compose_file, "up", "-d"])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        format!("{}\n{}", String::from_utf8_lossy(&pull.stdout), String::from_utf8_lossy(&up.stdout))
    } else {
        let pull = std::process::Command::new("docker").args(["pull", &image])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        let restart = std::process::Command::new("docker").args(["restart", &container_id])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        format!("{}\n{}", String::from_utf8_lossy(&pull.stdout), String::from_utf8_lossy(&restart.stdout))
    };

    {
        let mut cache = docker_cache().lock().unwrap();
        if let Some(row) = cache.get_mut(&container_id) {
            row.status = "up-to-date".into();
            row.detail = None;
        }
    }

    Ok(Json(serde_json::json!({ "ok": true, "output": output.trim(), "image": image })))
}

// ─── Odysseus bare-metal updates ─────────────────────────────────────────────

const ODYSSEUS_INSTALL_DIR: &str = "/opt/odysseus";
const ODYSSEUS_BRANCH: &str = "odysseus-voidlink";

#[derive(Serialize)]
pub struct OdyInfo {
    pub installed: bool,
    pub mode: String,
    pub current_commit: String,
    pub remote_commit: String,
    pub behind: usize,
    pub ahead: usize,
    pub fetch_error: Option<String>,
}

pub async fn odysseus_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<OdyInfo>> {
    require_admin(&state, &jar).await?;
    let ody = std::path::Path::new(ODYSSEUS_INSTALL_DIR);
    if !ody.join("app.py").exists() {
        return Ok(Json(OdyInfo {
            installed: false, mode: "none".into(),
            current_commit: String::new(), remote_commit: String::new(),
            behind: 0, ahead: 0, fetch_error: None,
        }));
    }
    let current_commit = run(ody, "git", &["rev-parse", "--short", "HEAD"]);
    let remote_ref = format!("origin/{ODYSSEUS_BRANCH}");
    let fetch_err = std::process::Command::new("git")
        .args(["fetch", "origin"]).current_dir(ody)
        .output().err().map(|e| e.to_string());
    let remote_commit = run(ody, "git", &["rev-parse", "--short", &remote_ref]);
    let behind: usize = run(ody, "git", &["rev-list", &format!("HEAD..{remote_ref}"), "--count"])
        .parse().unwrap_or(0);
    let ahead: usize = run(ody, "git", &["rev-list", &format!("{remote_ref}..HEAD"), "--count"])
        .parse().unwrap_or(0);
    Ok(Json(OdyInfo { installed: true, mode: "git".into(), current_commit, remote_commit, behind, ahead, fetch_error: fetch_err }))
}

pub async fn apply_odysseus(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let ody = std::path::Path::new(ODYSSEUS_INSTALL_DIR);
    if !ody.join("app.py").exists() {
        return Err(AppError::FeatureUnavailable("Odysseus is not installed at /opt/odysseus".into()));
    }
    let script = format!(
        "#!/bin/sh\nset -e\n\
         cd {dir}\n\
         git pull origin {branch}\n\
         {dir}/venv/bin/pip install --quiet -r {dir}/requirements.txt\n\
         sudo systemctl restart odysseus.service\n",
        dir = ODYSSEUS_INSTALL_DIR, branch = ODYSSEUS_BRANCH
    );
    std::fs::write("/tmp/odysseus-update.sh", script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("sh").args(["/tmp/odysseus-update.sh"])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ─── OS package updates ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct OsUpdateInfo {
    pub package_manager: String,
    pub available: bool,
    pub count: usize,
    pub packages: Vec<String>,
    pub error: Option<String>,
}

fn detect_pm() -> Option<&'static str> {
    ["apt-get", "pacman", "dnf", "yum", "zypper", "apk", "xbps-install"].iter().find(|&pm| std::process::Command::new("which").arg(pm).output().map(|o| o.status.success()).unwrap_or(false)).map(|v| v as _)
}

// Shared by os_info and apply_os's dry-run plan so both always report the same
// package list/count for a given package manager.
fn list_upgradable_packages(pm: &str) -> (Vec<String>, Option<String>) {
    match pm {
        "apt-get" => {
            let _ = std::process::Command::new("apt-get").args(["-qq", "update"]).output();
            match std::process::Command::new("apt-get").args(["-s", "upgrade", "-V"]).output() {
                Ok(o) => {
                    let pkgs: Vec<String> = String::from_utf8_lossy(&o.stdout).lines()
                        .filter(|l| l.starts_with("   ")).map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty()).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        "pacman" => {
            match std::process::Command::new("checkupdates").output() {
                Ok(o) => {
                    let pkgs = String::from_utf8_lossy(&o.stdout).lines()
                        .filter(|l| !l.is_empty()).map(String::from).collect();
                    (pkgs, None)
                }
                Err(_) => match std::process::Command::new("pacman").args(["-Qu"]).output() {
                    Ok(o) => {
                        let pkgs = String::from_utf8_lossy(&o.stdout).lines()
                            .filter(|l| !l.is_empty()).map(String::from).collect();
                        (pkgs, None)
                    }
                    Err(e) => (vec![], Some(e.to_string())),
                },
            }
        }
        "dnf" | "yum" => {
            match std::process::Command::new(pm).args(["check-update", "-q"]).output() {
                Ok(o) => {
                    let pkgs = String::from_utf8_lossy(&o.stdout).lines()
                        .filter(|l| !l.is_empty() && !l.starts_with("Last metadata"))
                        .map(String::from).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        "zypper" => {
            match std::process::Command::new("zypper").args(["list-updates"]).output() {
                Ok(o) => {
                    let pkgs: Vec<String> = String::from_utf8_lossy(&o.stdout).lines()
                        .filter(|l| l.contains('|') && !l.contains("Name")).map(String::from).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        "apk" => {
            let _ = std::process::Command::new("apk").args(["update", "-q"]).output();
            match std::process::Command::new("apk").args(["list", "--upgradable"]).output() {
                Ok(o) => {
                    let pkgs: Vec<String> = String::from_utf8_lossy(&o.stdout).lines()
                        .filter(|l| !l.is_empty()).map(String::from).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        "xbps-install" => {
            let _ = std::process::Command::new("xbps-install").args(["-S"]).output();
            match std::process::Command::new("xbps-install").args(["-nu"]).output() {
                Ok(o) => {
                    let pkgs: Vec<String> = String::from_utf8_lossy(&o.stdout).lines()
                        .filter(|l| !l.is_empty()).map(String::from).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        _ => (vec![], Some("Unsupported package manager".into())),
    }
}

fn os_update_risk(count: usize) -> &'static str {
    if count == 0 { "low" } else if count <= 10 { "medium" } else { "high" }
}

pub async fn os_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<OsUpdateInfo>> {
    require_admin(&state, &jar).await?;

    let pm = match detect_pm() {
        Some(p) => p,
        None => return Ok(Json(OsUpdateInfo {
            package_manager: "unknown".into(), available: false, count: 0,
            packages: vec![], error: Some("No supported package manager found".into()),
        })),
    };

    let (packages, error) = list_upgradable_packages(pm);
    let count = packages.len();
    Ok(Json(OsUpdateInfo {
        package_manager: pm.replace("apt-get", "apt").to_string(),
        available: count > 0, count, packages, error,
    }))
}

#[derive(Deserialize)]
pub struct OsApplyReq { pub dry_run: bool }

pub async fn apply_os(
    State(state): State<AppState>, jar: CookieJar, Json(req): Json<OsApplyReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let pm = detect_pm().ok_or_else(|| AppError::FeatureUnavailable("No package manager found".into()))?;

    if req.dry_run {
        let (packages, error) = list_upgradable_packages(pm);
        let count = packages.len();
        let preview = packages.join("\n");
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": format!("Apply OS updates ({})", pm.replace("apt-get", "apt")),
                "risk": os_update_risk(count),
                "changes": [
                    { "label": "Package manager",    "value": pm.replace("apt-get", "apt") },
                    { "label": "Packages to update", "value": count.to_string() },
                ],
                "preview": if preview.is_empty() { None } else { Some(preview) }
            },
            "error": error
        })));
    }

    let output = match pm {
        "apt-get" => {
            std::process::Command::new("sudo").args(["apt-get", "-y", "upgrade"]).arg("-o").arg("Dpkg::Progress-Fancy=0")
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        "pacman" => {
            std::process::Command::new("sudo").args(["pacman", "-Syu", "--noconfirm"])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        "dnf" | "yum" => {
            std::process::Command::new("sudo").args([pm, "upgrade", "-y"])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        "zypper" => {
            std::process::Command::new("sudo").args(["zypper", "--non-interactive", "update"])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        "apk" => {
            std::process::Command::new("sudo").args(["apk", "upgrade"])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        "xbps-install" => {
            std::process::Command::new("sudo").args(["xbps-install", "-Syu"])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        _ => "Unsupported package manager".into(),
    };

    Ok(Json(serde_json::json!({ "ok": true, "dry_run": req.dry_run, "output": output.trim() })))
}
