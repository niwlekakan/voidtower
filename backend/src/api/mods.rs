use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

async fn db_get(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key).fetch_optional(&state.db).await.ok().flatten().map(|(v,)| v)
}

async fn db_set(state: &AppState, key: &str, value: &str) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key).bind(value).bind(now)
        .execute(&state.db).await.map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

fn run(root: &std::path::Path, cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd).args(args).current_dir(root)
        .output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn run_checked(root: &std::path::Path, cmd: &str, args: &[&str]) -> std::result::Result<String, String> {
    match std::process::Command::new(cmd).args(args).current_dir(root).output() {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

fn project_root() -> Option<std::path::PathBuf> {
    std::env::current_exe().ok()?.parent()?.parent()?.parent()?.parent().map(|p| p.to_path_buf())
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
    let applied_at = db_get(&state, MOD_APPLIED_AT_KEY).await.and_then(|v| v.parse::<i64>().ok());
    let is_git_install = project_root().map(|r| r.join(".git").exists()).unwrap_or(false);
    Ok(Json(ModStatus {
        config: url.zip(branch).map(|(u, b)| ModConfig { url: u, branch: b }),
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
    if !body.url.starts_with("https://") && !body.url.starts_with("http://") && !body.url.starts_with("git@") {
        return Err(AppError::BadRequest("URL must start with https://, http://, or git@".into()));
    }
    db_set(&state, MOD_URL_KEY, &body.url).await?;
    db_set(&state, MOD_BRANCH_KEY, &body.branch).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn fetch_mod(State(state): State<AppState>, jar: CookieJar) -> Result<Json<ModFetchResult>> {
    require_admin(&state, &jar).await?;
    let root = git_root()?;
    let url = db_get(&state, MOD_URL_KEY).await
        .ok_or_else(|| AppError::BadRequest("No mod source configured".into()))?;
    let branch = db_get(&state, MOD_BRANCH_KEY).await
        .ok_or_else(|| AppError::BadRequest("No mod branch configured".into()))?;

    // Add or update the vt-mod remote
    let existing_url = run(&root, "git", &["remote", "get-url", MOD_REMOTE]);
    if existing_url.is_empty() {
        run_checked(&root, "git", &["remote", "add", MOD_REMOTE, &url])
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to add remote: {e}")))?;
    } else if existing_url != url {
        run(&root, "git", &["remote", "set-url", MOD_REMOTE, &url]);
    }

    run_checked(&root, "git", &["fetch", MOD_REMOTE])
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to fetch mod remote: {e}")))?;

    let remote_ref = format!("{MOD_REMOTE}/{branch}");

    // Verify the branch exists on the remote
    if run(&root, "git", &["rev-parse", "--verify", &remote_ref]).is_empty() {
        return Err(AppError::BadRequest(format!("Branch '{branch}' not found on the mod remote")));
    }

    // Commits ahead
    let log_out = run(&root, "git", &["log", &format!("HEAD..{remote_ref}"), "--format=%H|%s|%an|%ai"]);
    let commits: Vec<ModCommit> = log_out.lines().filter(|l| !l.is_empty()).map(|line| {
        let mut parts = line.splitn(4, '|');
        ModCommit {
            hash: parts.next().unwrap_or("").chars().take(7).collect(),
            subject: parts.next().unwrap_or("").to_string(),
            author: parts.next().unwrap_or("").to_string(),
            date: parts.next().unwrap_or("").to_string(),
        }
    }).collect();

    // Changed files
    let files_out = run(&root, "git", &["diff", "--name-status", &format!("HEAD..{remote_ref}")]);
    let changed_files: Vec<ChangedFile> = files_out.lines().filter(|l| !l.is_empty()).map(|line| {
        let mut parts = line.splitn(2, '\t');
        let status_char = parts.next().unwrap_or("M");
        let path = parts.next().unwrap_or("").to_string();
        let status = match status_char.chars().next().unwrap_or('M') {
            'A' => "added",
            'D' => "deleted",
            'R' => "renamed",
            _ => "modified",
        }.to_string();
        ChangedFile { path, status }
    }).collect();

    let commits_ahead = commits.len();
    let full_diff = run(&root, "git", &["diff", &format!("HEAD..{remote_ref}")]);
    let diff_preview = full_diff.lines().take(300).collect::<Vec<_>>().join("\n");
    let mod_name = url.trim_end_matches('/').split('/').next_back().unwrap_or(&url)
        .trim_end_matches(".git").to_string();

    Ok(Json(ModFetchResult { mod_name, branch, commits, changed_files, diff_preview, commits_ahead }))
}

pub async fn get_diff(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let root = git_root()?;
    let branch = db_get(&state, MOD_BRANCH_KEY).await
        .ok_or_else(|| AppError::BadRequest("No mod configured".into()))?;
    let remote_ref = format!("{MOD_REMOTE}/{branch}");
    let diff = run(&root, "git", &["diff", &format!("HEAD..{remote_ref}")]);
    Ok(Json(serde_json::json!({ "diff": diff })))
}

pub async fn apply_mod(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let root = git_root()?;
    let branch = db_get(&state, MOD_BRANCH_KEY).await
        .ok_or_else(|| AppError::BadRequest("No mod configured".into()))?;
    let remote_ref = format!("{MOD_REMOTE}/{branch}");

    let current_head = run(&root, "git", &["rev-parse", "HEAD"]);
    if current_head.is_empty() {
        return Err(AppError::Internal(anyhow::anyhow!("Cannot determine current HEAD")));
    }

    let merge_msg = format!("Apply VoidTower mod from {remote_ref}");
    match run_checked(&root, "git", &["merge", "--no-ff", &remote_ref, "-m", &merge_msg]) {
        Ok(out) => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
            db_set(&state, MOD_ROLLBACK_KEY, &current_head).await?;
            db_set(&state, MOD_APPLIED_AT_KEY, &now.to_string()).await?;
            Ok(Json(serde_json::json!({ "ok": true, "output": out })))
        }
        Err(e) => {
            run(&root, "git", &["merge", "--abort"]);
            Err(AppError::Internal(anyhow::anyhow!("Merge failed: {e}")))
        }
    }
}

pub async fn rollback_mod(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let root = git_root()?;
    let rollback_ref = db_get(&state, MOD_ROLLBACK_KEY).await
        .ok_or_else(|| AppError::BadRequest("No rollback point saved".into()))?;

    run_checked(&root, "git", &["reset", "--hard", &rollback_ref])
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Rollback failed: {e}")))?;

    sqlx::query("DELETE FROM settings WHERE key IN (?, ?)")
        .bind(MOD_ROLLBACK_KEY).bind(MOD_APPLIED_AT_KEY)
        .execute(&state.db).await.map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
