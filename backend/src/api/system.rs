use crate::{
    auth,
    error::{AppError, Result},
    operations::update_adoption,
    AppState,
};
use axum::{extract::State, http::HeaderMap, response::Response, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;

const GITHUB_BRANCH: &str = "main";

use super::operation_adoption::{self, CompatibilityResult};

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

// Dev binary lives at <root>/backend/target/{profile}/voidtower.
// Prod binary lives at /opt/voidtower/voidtower (no "target/" segment).
fn is_dev_install() -> bool {
    std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().contains("/target/"))
        .unwrap_or(false)
}

// Dev: traverse target/debug/ → backend/ → project root
fn project_root() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()?
        .parent()?
        .parent()?
        .parent()
        .map(|p| p.to_path_buf())
}

// Prod: directory that contains the binary (e.g. /opt/voidtower)
fn install_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|p| p.to_path_buf())
}

fn git(root: &std::path::Path, args: &[&str]) -> std::result::Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn installed_version(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join(".version"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

// ─── Version ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct VersionInfo {
    commit: String,
    branch: String,
    commit_date: String,
    dirty: bool,
}

pub async fn version(State(state): State<AppState>, jar: CookieJar) -> Result<Json<VersionInfo>> {
    require_user(&state, &jar).await?;
    if is_dev_install() {
        let root = project_root()
            .ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
        let commit =
            git(&root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|_| "unknown".into());
        let branch =
            git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "unknown".into());
        let commit_date = git(&root, &["log", "-1", "--format=%ci"]).unwrap_or_default();
        let status = git(&root, &["status", "--porcelain"]).unwrap_or_default();
        Ok(Json(VersionInfo {
            commit,
            branch,
            commit_date,
            dirty: !status.is_empty(),
        }))
    } else {
        let dir = install_dir()
            .ok_or_else(|| AppError::FeatureUnavailable("cannot locate install dir".into()))?;
        let commit_hash = std::fs::read_to_string(dir.join(".commit"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let commit = if commit_hash.len() >= 7 {
            commit_hash[..7].to_string()
        } else {
            let ver = installed_version(&dir);
            if ver.is_empty() {
                "unknown".to_string()
            } else {
                format!("v{ver}")
            }
        };
        Ok(Json(VersionInfo {
            commit,
            branch: GITHUB_BRANCH.to_string(),
            commit_date: String::new(),
            dirty: false,
        }))
    }
}

// ─── Update check ─────────────────────────────────────────────────────────────

pub async fn update_check(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_voidtower_action(&state, &jar, "update.voidtower.check", &headers).await
}

// ─── Restart ──────────────────────────────────────────────────────────────────

pub async fn restart(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let pid = std::process::id();

    let script = if is_dev_install() {
        let root = project_root()
            .ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
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
    std::process::Command::new("setsid")
        .args(["bash", script_path])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(
        serde_json::json!({ "ok": true, "message": "Restarting…" }),
    ))
}

// ─── Update ───────────────────────────────────────────────────────────────────

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_voidtower_action(&state, &jar, "update.voidtower.apply", &headers).await
}

async fn submit_voidtower_action(
    state: &AppState,
    jar: &CookieJar,
    action: &str,
    headers: &HeaderMap,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(state, jar, None).await?;
    let adopted = update_adoption::resolve_target(&state.db, &credential, action, None).await?;
    operation_adoption::submit(
        state,
        &credential,
        &adopted.resource.id,
        action,
        serde_json::json!({}),
        headers,
    )
    .await
}
