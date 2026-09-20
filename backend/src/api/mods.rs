use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

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

async fn db_get(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
}

async fn db_set(state: &AppState, key: &str, value: &str) -> Result<()> {
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

fn run(root: &std::path::Path, cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .current_dir(root)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[allow(dead_code)]
fn run_checked(
    root: &std::path::Path,
    cmd: &str,
    args: &[&str],
) -> std::result::Result<String, String> {
    match std::process::Command::new(cmd)
        .args(args)
        .current_dir(root)
        .output()
    {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn project_root() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()?
        .parent()?
        .parent()?
        .parent()
        .map(|p| p.to_path_buf())
}

fn git_root() -> Result<std::path::PathBuf> {
    let root = project_root()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Cannot determine project root")))?;
    if root.join(".git").exists() {
        Ok(root)
    } else {
        Err(AppError::FeatureUnavailable(
            "Mods require a git-based VoidTower install. Docker installs can switch images via Updates instead.".into(),
        ))
    }
}

const MOD_URL_KEY: &str = "mod_source_url";
const MOD_BRANCH_KEY: &str = "mod_source_branch";
const MOD_ROLLBACK_KEY: &str = "mod_rollback_ref";
const MOD_APPLIED_AT_KEY: &str = "mod_applied_at";
const MOD_REMOTE: &str = "vt-mod";

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct ModConfig {
    pub url: String,
    pub branch: String,
}

#[derive(Serialize)]
pub struct ModCommit {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

#[derive(Serialize)]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct ModFetchResult {
    pub mod_name: String,
    pub branch: String,
    pub commits: Vec<ModCommit>,
    pub changed_files: Vec<ChangedFile>,
    pub diff_preview: String,
    pub commits_ahead: usize,
}

#[derive(Serialize)]
pub struct ModStatus {
    pub config: Option<ModConfig>,
    pub applied: bool,
    pub applied_at: Option<i64>,
    pub rollback_ref: Option<String>,
    pub is_git_install: bool,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub async fn get_status(State(state): State<AppState>, jar: CookieJar) -> Result<Json<ModStatus>> {
    require_admin(&state, &jar).await?;
    let url = db_get(&state, MOD_URL_KEY).await;
    let branch = db_get(&state, MOD_BRANCH_KEY).await;
    let rollback_ref = db_get(&state, MOD_ROLLBACK_KEY).await;
    let applied_at = db_get(&state, MOD_APPLIED_AT_KEY)
        .await
        .and_then(|v| v.parse::<i64>().ok());
    let is_git_install = project_root()
        .map(|r| r.join(".git").exists())
        .unwrap_or(false);
    Ok(Json(ModStatus {
        config: url
            .zip(branch)
            .map(|(u, b)| ModConfig { url: u, branch: b }),
        applied: rollback_ref.is_some(),
        applied_at,
        rollback_ref,
        is_git_install,
    }))
}

pub async fn save_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<ModConfig>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if body.url.is_empty() || body.branch.is_empty() {
        return Err(AppError::BadRequest("URL and branch are required".into()));
    }
    if !body.url.starts_with("https://")
        && !body.url.starts_with("http://")
        && !body.url.starts_with("git@")
    {
        return Err(AppError::BadRequest(
            "URL must start with https://, http://, or git@".into(),
        ));
    }
    db_set(&state, MOD_URL_KEY, &body.url).await?;
    db_set(&state, MOD_BRANCH_KEY, &body.branch).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn fetch_mod(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<ModFetchResult>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "mod mutations require a canonical operation adapter".into(),
    ))
}

pub async fn get_diff(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let root = git_root()?;
    let branch = db_get(&state, MOD_BRANCH_KEY)
        .await
        .ok_or_else(|| AppError::BadRequest("No mod configured".into()))?;
    let remote_ref = format!("{MOD_REMOTE}/{branch}");
    let diff = run(&root, "git", &["diff", &format!("HEAD..{remote_ref}")]);
    Ok(Json(serde_json::json!({ "diff": diff })))
}

pub async fn apply_mod(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "mod mutations require a canonical operation adapter".into(),
    ))
}

pub async fn rollback_mod(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "mod mutations require a canonical operation adapter".into(),
    ))
}
