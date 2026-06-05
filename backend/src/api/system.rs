use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;

const GITHUB_REPO: &str = "niwlekakan/voidtower";
const GITHUB_BRANCH: &str = "voidtower-aio";

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

// Dev binary lives at <root>/backend/target/{profile}/voidtower.
// Prod binary lives at /opt/voidtower/voidtower (no "target/" segment).
fn is_dev_install() -> bool {
    std::env::current_exe().ok()
        .map(|p| p.to_string_lossy().contains("/target/"))
        .unwrap_or(false)
}

// Dev: traverse target/debug/ → backend/ → project root
fn project_root() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?
        .parent()?.parent()?.parent()?.parent()
        .map(|p| p.to_path_buf())
}

// Prod: directory that contains the binary (e.g. /opt/voidtower)
fn install_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.to_path_buf())
}

fn git(root: &std::path::Path, args: &[&str]) -> std::result::Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args).current_dir(root)
        .output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn installed_version(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join(".version"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn github_latest_commit() -> Option<String> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/commits/{GITHUB_BRANCH}");
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "10", "-A", "voidtower-update",
               "-H", "Accept: application/vnd.github.sha", &url])
        .output().ok()?;
    if !out.status.success() { return None; }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // The SHA-only Accept header returns a bare 40-char hex string
    if sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(sha[..7].to_string())
    } else {
        None
    }
}

// ─── Version ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct VersionInfo {
    commit: String,
    branch: String,
    commit_date: String,
    dirty: bool,
}

pub async fn version() -> Result<Json<VersionInfo>> {
    if is_dev_install() {
        let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
        let commit      = git(&root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|_| "unknown".into());
        let branch      = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "unknown".into());
        let commit_date = git(&root, &["log", "-1", "--format=%ci"]).unwrap_or_default();
        let status      = git(&root, &["status", "--porcelain"]).unwrap_or_default();
        Ok(Json(VersionInfo { commit, branch, commit_date, dirty: !status.is_empty() }))
    } else {
        let dir = install_dir().ok_or_else(|| AppError::FeatureUnavailable("cannot locate install dir".into()))?;
        let commit_hash = std::fs::read_to_string(dir.join(".commit"))
            .unwrap_or_default().trim().to_string();
        let commit = if commit_hash.len() >= 7 {
            commit_hash[..7].to_string()
        } else {
            let ver = installed_version(&dir);
            if ver.is_empty() { "unknown".to_string() } else { format!("v{ver}") }
        };
        Ok(Json(VersionInfo { commit, branch: GITHUB_BRANCH.to_string(), commit_date: String::new(), dirty: false }))
    }
}

// ─── Update check ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct UpdateCheck {
    behind: usize,
    ahead: usize,
    can_update: bool,
    remote_commit: String,
    error: Option<String>,
}

pub async fn update_check(State(state): State<AppState>, jar: CookieJar) -> Result<Json<UpdateCheck>> {
    require_admin(&state, &jar).await?;

    if is_dev_install() {
        let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
        let branch = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| GITHUB_BRANCH.into());
        let remote_ref = format!("origin/{branch}");
        let fetch_err = std::process::Command::new("git")
            .args(["fetch", "origin"]).current_dir(&root)
            .output().err().map(|e| e.to_string());
        let behind = git(&root, &["rev-list", &format!("HEAD..{remote_ref}"), "--count"])
            .ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let ahead = git(&root, &["rev-list", &format!("{remote_ref}..HEAD"), "--count"])
            .ok().and_then(|s| s.parse().ok()).unwrap_or(0);
        let remote_commit = git(&root, &["rev-parse", "--short", &remote_ref]).unwrap_or_default();
        Ok(Json(UpdateCheck { behind, ahead, can_update: behind > 0, remote_commit, error: fetch_err }))
    } else {
        let dir = install_dir().ok_or_else(|| AppError::FeatureUnavailable("cannot locate install dir".into()))?;
        let commit_hash = std::fs::read_to_string(dir.join(".commit"))
            .unwrap_or_default().trim().to_string();
        let current = if commit_hash.len() >= 7 { commit_hash[..7].to_string() } else { commit_hash };
        let latest = github_latest_commit();
        let can_update = latest.as_deref()
            .map(|l| !l.is_empty() && !current.is_empty() && l != current)
            .unwrap_or(false);
        let remote_commit = latest.clone().unwrap_or_else(|| "unknown".to_string());
        let error = if latest.is_none() { Some("Could not reach GitHub API".to_string()) } else { None };
        Ok(Json(UpdateCheck {
            behind: if can_update { 1 } else { 0 },
            ahead: 0,
            can_update,
            remote_commit,
            error,
        }))
    }
}

// ─── Restart ──────────────────────────────────────────────────────────────────

pub async fn restart(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let pid = std::process::id();

    let script = if is_dev_install() {
        let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
        format!(
            "#!/bin/sh\nsleep 1\nkill -TERM {pid}\nsleep 1\nexec bash {root}/start-dev.sh >> /tmp/voidtower.log 2>&1\n",
            root = root.display()
        )
    } else {
        // systemd Restart=on-failure restarts after SIGTERM — no need for systemctl
        format!("#!/bin/sh\nsleep 1\nkill -TERM {pid}\n")
    };

    let script_path = "/tmp/voidtower-restart.sh";
    std::fs::write(script_path, &script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("bash")
        .args([script_path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true, "message": "Restarting…" })))
}

// ─── Update ───────────────────────────────────────────────────────────────────

pub async fn update(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let pid = std::process::id();

    let script = if is_dev_install() {
        let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
        let branch = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| GITHUB_BRANCH.into());
        format!(
            "#!/bin/sh\nset -e\ncd {root}\ngit pull origin {branch}\ncargo build --manifest-path backend/Cargo.toml\nnpm --prefix frontend run build\nsleep 1\nkill -TERM {pid}\nsleep 1\nexec bash {root}/start-dev.sh >> /tmp/voidtower.log 2>&1\n",
            root = root.display()
        )
    } else {
        let dir = install_dir()
            .ok_or_else(|| AppError::FeatureUnavailable("cannot locate install dir".into()))?;
        let dir_s = dir.to_string_lossy().to_string();
        let repo  = GITHUB_REPO;
        // Download the latest release binary, replace in-place, then let systemd restart us.
        format!(
            "#!/bin/sh\n\
set -e\n\
ARCH=$(uname -m)\n\
INSTALL_DIR={dir_s}\n\
LATEST=$(curl -fsSL --max-time 30 -A voidtower-update \
'https://api.github.com/repos/{repo}/releases/latest' \
| grep '\"tag_name\"' | sed 's/.*\"v\\([^\"]*\\)\".*/\\1/')\n\
[ -z \"$LATEST\" ] && {{ echo 'Failed to fetch latest version' >&2; exit 1; }}\n\
ARCHIVE=\"voidtower-$LATEST-$ARCH-unknown-linux-musl.tar.gz\"\n\
curl -fsSL --max-time 120 \
\"https://github.com/{repo}/releases/download/v$LATEST/$ARCHIVE\" \
-o /tmp/vt-update.tar.gz\n\
cd /tmp && tar -xzf /tmp/vt-update.tar.gz voidtower\n\
mv /tmp/voidtower \"$INSTALL_DIR/voidtower\"\n\
chmod +x \"$INSTALL_DIR/voidtower\"\n\
echo \"$LATEST\" > \"$INSTALL_DIR/.version\"\n\
sleep 1\n\
kill -TERM {pid}\n"
        )
    };

    let script_path = "/tmp/voidtower-update.sh";
    std::fs::write(script_path, &script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("bash")
        .args([script_path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true, "message": "Updating… VoidTower will restart when done." })))
}
