use axum::{extract::{Path, Query, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{audit, auth, error::{AppError, Result}, AppState};

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AutomationJob {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub schedule: Option<String>,
    pub enabled: bool,
    pub timeout_secs: i64,
    pub last_run_at: Option<i64>,
    pub last_status: Option<String>,
    pub last_exit_code: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct AutomationRun {
    pub id: String,
    pub job_id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub exit_code: Option<i64>,
    pub output: String,
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let jobs = sqlx::query_as::<_, AutomationJob>(
        "SELECT id, name, description, command, schedule, enabled, timeout_secs,
                last_run_at, last_status, last_exit_code, created_at, updated_at
         FROM automation_jobs ORDER BY created_at DESC"
    ).fetch_all(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "jobs": jobs })))
}

#[derive(Deserialize)]
pub struct CreateJob {
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub schedule: Option<String>,
    pub timeout_secs: Option<i64>,
    pub enabled: Option<bool>,
}

pub async fn create(State(state): State<AppState>, jar: CookieJar, Json(body): Json<CreateJob>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    if body.name.trim().is_empty() { return Err(AppError::BadRequest("name required".into())); }
    if body.command.trim().is_empty() { return Err(AppError::BadRequest("command required".into())); }

    let id = Uuid::new_v4().to_string();
    let ts = now();
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, description, command, schedule, enabled, timeout_secs, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    ).bind(&id).bind(&body.name).bind(&body.description).bind(&body.command)
     .bind(&body.schedule).bind(body.enabled.unwrap_or(true))
     .bind(body.timeout_secs.unwrap_or(300)).bind(ts).bind(ts)
     .execute(&state.db).await.map_err(AppError::Database)?;

    audit::log(&state.db, Some(&user.id), "human", "create_automation_job", Some("automation_job"), Some(&id), "success", None, None).await;
    Ok(Json(serde_json::json!({ "id": id })))
}

#[derive(Deserialize)]
pub struct UpdateJob {
    pub name: Option<String>,
    pub description: Option<String>,
    pub command: Option<String>,
    pub schedule: Option<String>,
    pub timeout_secs: Option<i64>,
    pub enabled: Option<bool>,
}

pub async fn update(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<UpdateJob>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    let ts = now();
    if let Some(v) = &body.name    { sqlx::query("UPDATE automation_jobs SET name=?, updated_at=? WHERE id=?").bind(v).bind(ts).bind(&id).execute(&state.db).await.map_err(AppError::Database)?; }
    if let Some(v) = &body.description { sqlx::query("UPDATE automation_jobs SET description=?, updated_at=? WHERE id=?").bind(v).bind(ts).bind(&id).execute(&state.db).await.map_err(AppError::Database)?; }
    if let Some(v) = &body.command { sqlx::query("UPDATE automation_jobs SET command=?, updated_at=? WHERE id=?").bind(v).bind(ts).bind(&id).execute(&state.db).await.map_err(AppError::Database)?; }
    if body.schedule.is_some()     { sqlx::query("UPDATE automation_jobs SET schedule=?, updated_at=? WHERE id=?").bind(&body.schedule).bind(ts).bind(&id).execute(&state.db).await.map_err(AppError::Database)?; }
    if let Some(v) = body.timeout_secs { sqlx::query("UPDATE automation_jobs SET timeout_secs=?, updated_at=? WHERE id=?").bind(v).bind(ts).bind(&id).execute(&state.db).await.map_err(AppError::Database)?; }
    if let Some(v) = body.enabled  { sqlx::query("UPDATE automation_jobs SET enabled=?, updated_at=? WHERE id=?").bind(v).bind(ts).bind(&id).execute(&state.db).await.map_err(AppError::Database)?; }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    sqlx::query("DELETE FROM automation_jobs WHERE id=?").bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), "human", "delete_automation_job", Some("automation_job"), Some(&id), "success", None, None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn run_now(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }

    let job = sqlx::query_as::<_, AutomationJob>(
        "SELECT id, name, description, command, schedule, enabled, timeout_secs,
                last_run_at, last_status, last_exit_code, created_at, updated_at
         FROM automation_jobs WHERE id=?"
    ).bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?.ok_or(AppError::NotFound)?;

    let run_id = Uuid::new_v4().to_string();
    let started = now();
    sqlx::query("INSERT INTO automation_runs (id, job_id, started_at, status, output) VALUES (?,?,?,'running','')")
        .bind(&run_id).bind(&id).bind(started).execute(&state.db).await.map_err(AppError::Database)?;

    let (status, exit_code, output) = execute_job(&job).await;
    let finished = now();

    sqlx::query("UPDATE automation_runs SET finished_at=?, status=?, exit_code=?, output=? WHERE id=?")
        .bind(finished).bind(&status).bind(exit_code).bind(&output).bind(&run_id)
        .execute(&state.db).await.map_err(AppError::Database)?;

    sqlx::query("UPDATE automation_jobs SET last_run_at=?, last_status=?, last_exit_code=?, updated_at=? WHERE id=?")
        .bind(finished).bind(&status).bind(exit_code).bind(finished).bind(&id)
        .execute(&state.db).await.map_err(AppError::Database)?;

    audit::log(&state.db, Some(&user.id), "human", "run_automation_job", Some("automation_job"), Some(&id), &status, None, Some(&job.name)).await;

    Ok(Json(serde_json::json!({ "run_id": run_id, "status": status, "exit_code": exit_code, "output": output })))
}

#[derive(Deserialize)]
pub struct RunsQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 { 20 }

pub async fn runs(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Query(q): Query<RunsQuery>) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let runs = sqlx::query_as::<_, AutomationRun>(
        "SELECT id, job_id, started_at, finished_at, status, exit_code, output
         FROM automation_runs WHERE job_id=? ORDER BY started_at DESC LIMIT ?"
    ).bind(&id).bind(q.limit).fetch_all(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "runs": runs })))
}

async fn execute_job(job: &AutomationJob) -> (String, Option<i64>, String) {
    let timeout = std::time::Duration::from_secs(job.timeout_secs.max(1) as u64);
    let result = tokio::time::timeout(timeout, async {
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&job.command)
            .output()
            .await
    }).await;

    match result {
        Ok(Ok(out)) => {
            let code = out.status.code().map(|c| c as i64);
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let combined = if stderr.is_empty() { stdout } else { format!("{}{}", stdout, stderr) };
            let status = if out.status.success() { "success" } else { "failed" };
            (status.into(), code, combined)
        }
        Ok(Err(e)) => ("failed".into(), None, format!("Failed to spawn: {e}")),
        Err(_) => ("timeout".into(), None, format!("Timed out after {}s", job.timeout_secs)),
    }
}

/// Called from the scheduler loop in main.rs
pub async fn run_scheduled_jobs(pool: &SqlitePool) {
    let ts = now();
    let Ok(jobs) = sqlx::query_as::<_, AutomationJob>(
        "SELECT id, name, description, command, schedule, enabled, timeout_secs,
                last_run_at, last_status, last_exit_code, created_at, updated_at
         FROM automation_jobs WHERE enabled = 1 AND schedule IS NOT NULL"
    ).fetch_all(pool).await else { return };

    for job in jobs {
        let Some(ref sched) = job.schedule else { continue };
        if !is_due(sched, job.last_run_at, ts) { continue }

        let run_id = Uuid::new_v4().to_string();
        let started = now();
        let _ = sqlx::query("INSERT INTO automation_runs (id, job_id, started_at, status, output) VALUES (?,?,?,'running','')")
            .bind(&run_id).bind(&job.id).bind(started).execute(pool).await;

        let (status, exit_code, output) = execute_job(&job).await;
        let finished = now();

        let _ = sqlx::query("UPDATE automation_runs SET finished_at=?, status=?, exit_code=?, output=? WHERE id=?")
            .bind(finished).bind(&status).bind(exit_code).bind(&output).bind(&run_id).execute(pool).await;

        let _ = sqlx::query("UPDATE automation_jobs SET last_run_at=?, last_status=?, last_exit_code=?, updated_at=? WHERE id=?")
            .bind(finished).bind(&status).bind(exit_code).bind(finished).bind(&job.id).execute(pool).await;
    }
}

/// Simple cron-style check: supports "@hourly", "@daily", "@weekly", and "*/N min" patterns.
fn is_due(schedule: &str, last_run: Option<i64>, now_ts: i64) -> bool {
    let interval_secs: i64 = match schedule.trim() {
        "@minutely"              => 60,
        "@hourly"                => 3600,
        "@daily" | "@midnight"   => 86400,
        "@weekly"                => 86400 * 7,
        "@monthly"               => 86400 * 30,
        s if s.starts_with("*/") => {
            // "*/5 minutes" or "*/30" — parse number, treat as minutes
            s[2..].split_whitespace().next()
                .and_then(|n| n.parse::<i64>().ok())
                .map(|n| n * 60)
                .unwrap_or(3600)
        }
        _ => 3600, // default: hourly for unrecognised patterns
    };
    match last_run {
        None => true,
        Some(last) => now_ts - last >= interval_secs,
    }
}
