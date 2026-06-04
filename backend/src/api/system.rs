use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

// Binary lives at <root>/backend/target/debug/voidtower
fn project_root() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?
        .parent()?.parent()?.parent()?.parent()
        .map(|p| p.to_path_buf())
}

fn git(root: &std::path::Path, args: &[&str]) -> std::result::Result<String, String> {
    let out = std::process::Command::new("git")
        .args(args).current_dir(root)
        .output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
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
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let commit      = git(&root, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    let branch      = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "unknown".into());
    let commit_date = git(&root, &["log", "-1", "--format=%ci"]).unwrap_or_default();
    let status      = git(&root, &["status", "--porcelain"]).unwrap_or_default();
    Ok(Json(VersionInfo { commit, branch, commit_date, dirty: !status.is_empty() }))
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
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;

    // fetch quietly; tolerate network errors
    let fetch_err = std::process::Command::new("git")
        .args(["fetch", "origin"]).current_dir(&root)
        .output().err().map(|e| e.to_string());

    let behind = git(&root, &["rev-list", "HEAD..origin/main", "--count"])
        .ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let ahead = git(&root, &["rev-list", "origin/main..HEAD", "--count"])
        .ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    let remote_commit = git(&root, &["rev-parse", "--short", "origin/main"]).unwrap_or_default();

    Ok(Json(UpdateCheck { behind, ahead, can_update: behind > 0, remote_commit, error: fetch_err }))
}

// ─── Restart ──────────────────────────────────────────────────────────────────

pub async fn restart(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let pid  = std::process::id();
    let script = format!(
        "#!/bin/sh\nsleep 1\nkill -TERM {pid}\nsleep 1\nexec bash {root}/start-dev.sh >> /tmp/voidtower.log 2>&1\n",
        root = root.display()
    );
    let script_path = "/tmp/voidtower-restart.sh";
    std::fs::write(script_path, script).map_err(|e| AppError::Internal(e.into()))?;
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
    let root = project_root().ok_or_else(|| AppError::FeatureUnavailable("cannot locate project root".into()))?;
    let pid  = std::process::id();
    let script = format!(
        "#!/bin/sh\nset -e\ncd {root}\ngit pull origin main\ncargo build --manifest-path backend/Cargo.toml\nnpm --prefix frontend run build\nsleep 1\nkill -TERM {pid}\nsleep 1\nexec bash {root}/start-dev.sh >> /tmp/voidtower.log 2>&1\n",
        root = root.display()
    );
    let script_path = "/tmp/voidtower-update.sh";
    std::fs::write(script_path, script).map_err(|e| AppError::Internal(e.into()))?;
    std::process::Command::new("bash")
        .args([script_path])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn().map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true, "message": "Updating… VoidTower will restart when done." })))
}
