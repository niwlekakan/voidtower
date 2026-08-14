use crate::{
    auth,
    backups::{self, BackupConfigInput},
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

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let configs = backups::list_configs(&state.db)
        .await
        .map_err(AppError::Internal)?;

    let configs_with_confidence: Vec<serde_json::Value> = configs
        .iter()
        .map(|c| {
            let mut v = serde_json::to_value(c).unwrap_or_default();
            v["confidence"] = serde_json::Value::String(backups::confidence(c).to_string());
            v
        })
        .collect();

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
    pub restore_test_schedule: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    let id = Uuid::new_v4().to_string();
    let input = BackupConfigInput {
        name: req.name,
        source_path: req.source_path,
        repo_path: req.repo_path,
        schedule: req.schedule,
        retention_days: req.retention_days.unwrap_or(30),
        restore_test_schedule: req.restore_test_schedule,
    };
    input
        .validate()
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    backups::create_config(&state.db, &id, &input, &id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

pub async fn run_now(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    if !backups::is_restic_available() {
        return Err(AppError::FeatureUnavailable(
            "restic is not installed".into(),
        ));
    }
    let cfg = backups::get_config(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let password = backups::restic_password();
    backups::prepare_config_repository(&cfg, &password)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    let run = backups::run_config_backup(&state.db, &cfg, &password, None)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "status": run.status,
        "snapshot_id": run.snapshot_id,
        "output": run.output.lines().take(30).collect::<Vec<_>>().join("\n"),
    })))
}

pub async fn check(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    if !backups::is_restic_available() {
        return Err(AppError::FeatureUnavailable(
            "restic is not installed".into(),
        ));
    }
    let cfg = backups::get_config(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let probe = backups::check_config(&state.db, &cfg, &backups::restic_password())
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(
        serde_json::json!({ "status": probe.status, "message": probe.message }),
    ))
}

pub async fn restore_test(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    if !backups::is_restic_available() {
        return Err(AppError::FeatureUnavailable(
            "restic is not installed".into(),
        ));
    }
    let cfg = backups::get_config(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let probe = backups::restore_test_config(&state.db, &cfg, &backups::restic_password())
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(
        serde_json::json!({ "status": probe.status, "message": probe.message }),
    ))
}

pub async fn delete_plan(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_admin(&user)?;
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT name, source_path FROM backup_configs WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    Ok(Json(serde_json::json!({
        "dry_run": true,
        "plan": {
            "title": "Delete Backup Config",
            "risk": "high",
            "changes": [
                { "label": "Config",      "value": row.0 },
                { "label": "Source path", "value": row.1 },
                { "label": "Effect",      "value": "Removes schedule and config — existing backup data on disk is NOT deleted" },
                { "label": "Reversible",  "value": "No — config cannot be recovered" }
            ],
            "preview": null
        }
    })))
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_admin(&user)?;
    backups::delete_config(&state.db, &id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
