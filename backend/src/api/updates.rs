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

// ─── VoidTower updates ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CommitInfo { pub hash: String, pub subject: String, pub author: String, pub date: String }

#[derive(Serialize)]
pub struct VtUpdateInfo {
    pub current_commit: String,
    pub remote_commit: String,
    pub behind: usize,
    pub ahead: usize,
    pub commits: Vec<CommitInfo>,
    pub backup_tags: Vec<String>,
    pub fetch_error: Option<String>,
}

pub async fn vt_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<VtUpdateInfo>> {
    require_admin(&state, &jar).await?;
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;

    let fetch_err = std::process::Command::new("git").args(["fetch", "origin"]).current_dir(&root)
        .output().err().map(|e| e.to_string());

    let current_commit = run(&root, "git", &["rev-parse", "--short", "HEAD"]);
    let remote_commit  = run(&root, "git", &["rev-parse", "--short", "origin/main"]);
    let behind: usize  = run(&root, "git", &["rev-list", "HEAD..origin/main", "--count"]).parse().unwrap_or(0);
    let ahead: usize   = run(&root, "git", &["rev-list", "origin/main..HEAD", "--count"]).parse().unwrap_or(0);

    // Commits between current and remote, formatted as hash|subject|author|date
    let log = run(&root, "git", &["log", "HEAD..origin/main", "--format=%H|%s|%an|%ci", "--no-merges"]);
    let commits = log.lines().filter(|l| !l.is_empty()).map(|l| {
        let parts: Vec<&str> = l.splitn(4, '|').collect();
        CommitInfo {
            hash:    parts.first().unwrap_or(&"").chars().take(7).collect(),
            subject: parts.get(1).unwrap_or(&"").to_string(),
            author:  parts.get(2).unwrap_or(&"").to_string(),
            date:    parts.get(3).unwrap_or(&"").get(..10).unwrap_or("").to_string(),
        }
    }).collect();

    // List backup tags
    let tags_raw = run(&root, "git", &["tag", "--list", "vt-backup-*", "--sort=-creatordate"]);
    let backup_tags = tags_raw.lines().filter(|l| !l.is_empty()).map(String::from).take(10).collect();

    Ok(Json(VtUpdateInfo { current_commit, remote_commit, behind, ahead, commits, backup_tags, fetch_error: fetch_err }))
}

pub async fn apply_vt(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let pid  = std::process::id();
    let ts   = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let tag  = format!("vt-backup-{ts}");

    let script = format!(
        "#!/bin/sh\nset -e\ncd {root}\n\
         git tag {tag}\n\
         git pull origin main\n\
         cargo build --manifest-path backend/Cargo.toml\n\
         npm --prefix frontend run build\n\
         sleep 1\nkill -TERM {pid}\nsleep 1\n\
         exec bash {root}/start-dev.sh >> /tmp/voidtower.log 2>&1\n",
        root = root.display()
    );
    std::fs::write("/tmp/voidtower-update.sh", script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("bash").args(["/tmp/voidtower-update.sh"])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true, "backup_tag": tag })))
}

#[derive(Deserialize)]
pub struct RollbackReq { pub tag: String }

pub async fn rollback_vt(
    State(state): State<AppState>, jar: CookieJar, Json(req): Json<RollbackReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if !req.tag.starts_with("vt-backup-") || req.tag.contains('/') || req.tag.contains("..") {
        return Err(AppError::BadRequest("Invalid backup tag".into()));
    }
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let pid  = std::process::id();
    let script = format!(
        "#!/bin/sh\nset -e\ncd {root}\n\
         git checkout {tag}\n\
         cargo build --manifest-path backend/Cargo.toml\n\
         npm --prefix frontend run build\n\
         sleep 1\nkill -TERM {pid}\nsleep 1\n\
         exec bash {root}/start-dev.sh >> /tmp/voidtower.log 2>&1\n",
        root = root.display(), tag = req.tag
    );
    std::fs::write("/tmp/voidtower-rollback.sh", script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("bash").args(["/tmp/voidtower-rollback.sh"])
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
    // returns (id, name, image) triples
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

    // Mark all as checking
    {
        let mut cache = docker_cache().lock().unwrap();
        for (id, name, image) in &containers {
            cache.insert(id.clone(), DockerImageRow {
                container_id: id.clone(), container_name: name.clone(),
                image: image.clone(), status: "checking".into(), detail: None,
            });
        }
    }

    // Background check — pull each image and read docker output
    tokio::spawn(async move {
        for (id, name, image) in containers {
            let result = tokio::task::spawn_blocking({
                let image = image.clone();
                move || {
                    std::process::Command::new("docker")
                        .args(["pull", &image])
                        .output()
                        .map(|o| (o.status.success(), String::from_utf8_lossy(&o.stdout).to_string()))
                }
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
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    // Validate container ID — alphanumeric only
    if !container_id.chars().all(|c| c.is_alphanumeric()) {
        return Err(AppError::BadRequest("Invalid container ID".into()));
    }

    // Find compose path from deployed_apps if available
    let compose_path = sqlx::query_as::<_, (String,)>(
        "SELECT compose_path FROM deployed_apps WHERE project_name = \
         (SELECT label FROM (SELECT d.project_name as label FROM deployed_apps d) WHERE label != '') LIMIT 1"
    ).fetch_optional(&state.db).await.ok().flatten().map(|(p,)| p);

    // Get container image
    let inspect = std::process::Command::new("docker")
        .args(["inspect", &container_id, "--format", "{{.Config.Image}}|{{index .Config.Labels \"com.docker.compose.project\"}}|{{index .Config.Labels \"com.docker.compose.project.config_files\"}}"])
        .output().map_err(|e| AppError::Internal(e.into()))?;
    let info = String::from_utf8_lossy(&inspect.stdout);
    let parts: Vec<&str> = info.trim().splitn(3, '|').collect();
    let image = parts.first().unwrap_or(&"").to_string();
    let compose_project = parts.get(1).unwrap_or(&"").to_string();
    let compose_file    = parts.get(2).unwrap_or(&"").to_string();

    let output = if !compose_file.is_empty() && !compose_project.is_empty() {
        // Compose-managed: pull + recreate via compose
        let pull = std::process::Command::new("docker")
            .args(["compose", "-p", &compose_project, "-f", &compose_file, "pull"])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        let up = std::process::Command::new("docker")
            .args(["compose", "-p", &compose_project, "-f", &compose_file, "up", "-d"])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        format!("{}\n{}", String::from_utf8_lossy(&pull.stdout), String::from_utf8_lossy(&up.stdout))
    } else {
        // Pull image then restart container
        let pull = std::process::Command::new("docker").args(["pull", &image])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        let restart = std::process::Command::new("docker").args(["restart", &container_id])
            .output().map_err(|e| AppError::Internal(e.into()))?;
        format!("{}\n{}", String::from_utf8_lossy(&pull.stdout), String::from_utf8_lossy(&restart.stdout))
    };

    // Update cache
    {
        let mut cache = docker_cache().lock().unwrap();
        if let Some(row) = cache.get_mut(&container_id) {
            row.status = "up-to-date".into();
            row.detail = None;
        }
    }

    Ok(Json(serde_json::json!({ "ok": true, "output": output.trim(), "image": image, "_compose_path": compose_path })))
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
    for pm in &["apt-get", "pacman", "dnf", "yum", "zypper"] {
        if std::process::Command::new("which").arg(pm).output().map(|o| o.status.success()).unwrap_or(false) {
            return Some(pm);
        }
    }
    None
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

    let (packages, error) = match pm {
        "apt-get" => {
            // Refresh index silently, then list upgradable
            let _ = std::process::Command::new("apt-get")
                .args(["-qq", "update"]).output();
            let out = std::process::Command::new("apt-get")
                .args(["-s", "upgrade", "-V"]).output();
            match out {
                Ok(o) => {
                    let text = String::from_utf8_lossy(&o.stdout);
                    let pkgs: Vec<String> = text.lines()
                        .filter(|l| l.starts_with("   "))
                        .map(|l| l.trim().to_string())
                        .filter(|l| !l.is_empty())
                        .collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        "pacman" => {
            let out = std::process::Command::new("checkupdates").output();
            match out {
                Ok(o) => {
                    let pkgs = String::from_utf8_lossy(&o.stdout)
                        .lines().filter(|l| !l.is_empty()).map(String::from).collect();
                    (pkgs, None)
                }
                Err(_) => {
                    // Fallback: pacman -Qu
                    let o = std::process::Command::new("pacman").args(["-Qu"]).output();
                    match o {
                        Ok(o2) => {
                            let pkgs = String::from_utf8_lossy(&o2.stdout)
                                .lines().filter(|l| !l.is_empty()).map(String::from).collect();
                            (pkgs, None)
                        }
                        Err(e) => (vec![], Some(e.to_string())),
                    }
                }
            }
        }
        "dnf" | "yum" => {
            let out = std::process::Command::new(pm).args(["check-update", "-q"]).output();
            match out {
                Ok(o) => {
                    let pkgs = String::from_utf8_lossy(&o.stdout)
                        .lines().filter(|l| !l.is_empty() && !l.starts_with("Last metadata")).map(String::from).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        "zypper" => {
            let out = std::process::Command::new("zypper").args(["list-updates"]).output();
            match out {
                Ok(o) => {
                    let pkgs: Vec<String> = String::from_utf8_lossy(&o.stdout)
                        .lines().filter(|l| l.contains('|') && !l.contains("Name")).map(String::from).collect();
                    (pkgs, None)
                }
                Err(e) => (vec![], Some(e.to_string())),
            }
        }
        _ => (vec![], Some("Unsupported package manager".into())),
    };

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

    let output = match pm {
        "apt-get" => {
            let args = if req.dry_run { vec!["apt-get", "-s", "upgrade"] } else { vec!["apt-get", "-y", "upgrade"] };
            std::process::Command::new("sudo").args(&args[1..]).arg("-o").arg("Dpkg::Progress-Fancy=0")
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        "pacman" => {
            if req.dry_run {
                std::process::Command::new("pacman").args(["-Qu"])
                    .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_else(|e| e.to_string())
            } else {
                std::process::Command::new("sudo").args(["pacman", "-Syu", "--noconfirm"])
                    .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                    .unwrap_or_else(|e| e.to_string())
            }
        }
        "dnf" | "yum" => {
            let flag = if req.dry_run { "--assumeno" } else { "-y" };
            std::process::Command::new("sudo").args([pm, "upgrade", flag])
                .output().map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_else(|e| e.to_string())
        }
        _ => "Unsupported package manager".into(),
    };

    Ok(Json(serde_json::json!({ "ok": true, "dry_run": req.dry_run, "output": output.trim() })))
}
