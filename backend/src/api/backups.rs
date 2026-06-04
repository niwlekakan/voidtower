use crate::{
    auth,
    backups::{self, BackupConfig},
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use uuid::Uuid;

const SELECT_COLS: &str =
    "id, name, source_path, repo_path, schedule, retention_days, enabled,
     last_run_at, last_status, created_at,
     last_check_at, last_check_status, last_restore_test_at, last_restore_test_status";

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

fn restic_password() -> String {
    std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| "changeme".into())
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let configs = sqlx::query_as::<_, BackupConfig>(
        &format!("SELECT {SELECT_COLS} FROM backup_configs ORDER BY created_at DESC")
    ).fetch_all(&state.db).await.map_err(AppError::Database)?;

    let configs_with_confidence: Vec<serde_json::Value> = configs.iter().map(|c| {
        let mut v = serde_json::to_value(c).unwrap_or_default();
        v["confidence"] = serde_json::Value::String(backups::confidence(c).to_string());
        v
    }).collect();

    Ok(Json(serde_json::json!({
        "configs": configs_with_confidence,
        "restic_available": backups::is_restic_available(),
    })))
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub source_path: String,
    pub repo_path: String,
    pub schedule: Option<String>,
    pub retention_days: Option<i64>,
}

pub async fn create(State(state): State<AppState>, jar: CookieJar, Json(req): Json<CreateRequest>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    let id = Uuid::new_v4().to_string();
    let retention = req.retention_days.unwrap_or(30);
    sqlx::query(
        "INSERT INTO backup_configs (id, name, source_path, repo_path, schedule, retention_days, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, 1, ?)"
    ).bind(&id).bind(&req.name).bind(&req.source_path).bind(&req.repo_path)
     .bind(&req.schedule).bind(retention).bind(now())
    .execute(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

pub async fn run_now(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    if !backups::is_restic_available() {
        return Err(AppError::FeatureUnavailable("restic is not installed".into()));
    }
    let cfg = sqlx::query_as::<_, BackupConfig>(
        &format!("SELECT {SELECT_COLS} FROM backup_configs WHERE id = ?")
    ).bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let password = restic_password();
    backups::init_repo(&cfg.repo_path, &password).await.map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    let run = backups::run_backup(&cfg, &password).await.map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    sqlx::query("UPDATE backup_configs SET last_run_at = ?, last_status = ? WHERE id = ?")
        .bind(run.finished_at.unwrap_or_else(now)).bind(&run.status).bind(&id)
        .execute(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({
        "status": run.status,
        "snapshot_id": run.snapshot_id,
        "output": run.output.lines().take(30).collect::<Vec<_>>().join("\n"),
    })))
}

pub async fn check(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    if !backups::is_restic_available() {
        return Err(AppError::FeatureUnavailable("restic is not installed".into()));
    }
    let cfg = sqlx::query_as::<_, BackupConfig>(
        &format!("SELECT {SELECT_COLS} FROM backup_configs WHERE id = ?")
    ).bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let password = restic_password();
    let (status, message) = match backups::run_check(&cfg.repo_path, &password).await {
        Ok(s) => (s, None),
        Err(e) => ("failed".to_string(), Some(e.to_string())),
    };
    sqlx::query("UPDATE backup_configs SET last_check_at = ?, last_check_status = ? WHERE id = ?")
        .bind(now()).bind(&status).bind(&id)
        .execute(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "status": status, "message": message })))
}

pub async fn restore_test(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    if !backups::is_restic_available() {
        return Err(AppError::FeatureUnavailable("restic is not installed".into()));
    }
    let cfg = sqlx::query_as::<_, BackupConfig>(
        &format!("SELECT {SELECT_COLS} FROM backup_configs WHERE id = ?")
    ).bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let password = restic_password();
    let (status, message) = match backups::run_restore_test(&cfg.repo_path, &password).await {
        Ok(s) => (s, None),
        Err(e) => ("failed".to_string(), Some(e.to_string())),
    };
    sqlx::query("UPDATE backup_configs SET last_restore_test_at = ?, last_restore_test_status = ? WHERE id = ?")
        .bind(now()).bind(&status).bind(&id)
        .execute(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "status": status, "message": message })))
}

pub async fn delete(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" || user.role == "operator" { return Err(AppError::Forbidden); }
    sqlx::query("DELETE FROM backup_configs WHERE id = ?").bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
