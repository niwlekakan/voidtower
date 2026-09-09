use axum::{
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    audit,
    auth,
    error::{AppError, Result},
    operations::invocation::CredentialContext,
    AppState,
};

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
    super::role_guard::require_operator(&user)?;
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
    super::role_guard::require_operator(&user)?;
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
    super::role_guard::require_operator(&user)?;
    sqlx::query("DELETE FROM automation_jobs WHERE id=?").bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), "human", "delete_automation_job", Some("automation_job"), Some(&id), "success", None, None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub(crate) async fn resolve_run_resource(
    state: &AppState,
    credential: &CredentialContext,
    automation_id: &str,
) -> super::operation_adoption::CompatibilityResult<crate::operations::contracts::ResourceRef> {
    const ACTION: &str = "automation.run";
    super::operation_adoption::authorize(credential, ACTION)?;
    let name: String = sqlx::query_scalar(
        "SELECT name FROM automation_jobs WHERE id = ? AND enabled = 1",
    )
    .bind(automation_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    let existing = crate::operations::resources::resolve_alias(
        &state.db,
        "automation.job",
        "local",
        automation_id,
    )
    .await
    .map_err(AppError::Internal)?;
    if existing.is_some() {
        return super::operation_adoption::resolve_available(
            state,
            credential,
            "automation_job",
            "automation.job",
            "local",
            automation_id,
            &[ACTION],
        )
        .await;
    }
    super::operation_adoption::observe_available(
        state,
        credential,
        super::operation_adoption::CompatibilityResource {
            kind: "automation_job",
            display_name: &name,
            node_id: None,
            provider: Some("local"),
            namespace: "automation.job",
            scope_key: "local",
            alias: automation_id,
        },
        &[ACTION],
    )
    .await
}

pub async fn run_now(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    headers: HeaderMap,
    token: Option<Extension<super::bearer_auth::AuthenticatedApiToken>>,
) -> super::operation_adoption::CompatibilityResult<Response> {
    let credential = super::actions::credential(
        &state,
        &jar,
        token.map(|Extension(token)| token),
    )
    .await?;
    let resource = resolve_run_resource(&state, &credential, &id).await?;
    super::operation_adoption::submit(
        &state,
        &credential,
        &resource.id,
        "automation.run",
        serde_json::json!({}),
        &headers,
    )
    .await
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

/// Called from the scheduler loop in main.rs. Scheduled automation is submitted through the
/// same canonical resource/action/job boundary as HTTP and webhook ingress; the worker owns command
/// execution and durable run-state updates.
pub async fn run_scheduled_jobs(state: &AppState) {
    let ts = now();
    let Ok(jobs) = sqlx::query_as::<_, AutomationJob>(
        "SELECT id, name, description, command, schedule, enabled, timeout_secs,
                last_run_at, last_status, last_exit_code, created_at, updated_at
         FROM automation_jobs WHERE enabled = 1 AND schedule IS NOT NULL",
    )
    .fetch_all(&state.db)
    .await
    else {
        return;
    };

    let credential = CredentialContext::Scheduler;
    for job in jobs {
        let Some(schedule) = job.schedule.as_deref() else {
            continue;
        };
        if !is_due(schedule, job.last_run_at, ts) {
            continue;
        }
        let resource = match resolve_run_resource(state, &credential, &job.id).await {
            Ok(resource) => resource,
            Err(error) => {
                tracing::warn!(job_id = %job.id, error = ?error, "scheduled automation observation failed");
                continue;
            }
        };
        let slot = schedule_slot(schedule, ts);
        let idempotency_key = format!("schedule-{}-{slot}", job.id);
        if let Err(error) = super::operation_adoption::submit_with_key(
            state,
            &credential,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            &idempotency_key,
        )
        .await
        {
            tracing::warn!(job_id = %job.id, error = ?error, "scheduled automation submission failed");
        }
    }
}

fn schedule_slot(schedule: &str, now_ts: i64) -> i64 {
    let interval = match schedule.trim() {
        "@minutely" => 60,
        "@hourly" => 3600,
        "@daily" | "@midnight" => 86400,
        "@weekly" => 86400 * 7,
        "@monthly" => 86400 * 30,
        value if value.starts_with("*/") => value
            .split_whitespace()
            .next()
            .and_then(|part| part.strip_prefix("*/"))
            .and_then(|minutes| minutes.parse::<i64>().ok())
            .filter(|minutes| (1..=1440).contains(minutes))
            .map(|minutes| minutes * 60)
            .unwrap_or(60),
        _ => 60,
    };
    now_ts / interval
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
