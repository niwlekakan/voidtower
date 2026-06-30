use crate::{
    ai::ProviderConfig,
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::{Path, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

// ── List ─────────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ProviderConfig>>> {
    require_admin(&state, &jar).await?;
    let rows = sqlx::query_as::<_, ProviderConfig>(
        "SELECT id, kind, name, enabled, base_url, api_key_ref, model, priority, \
         created_at, updated_at FROM ai_providers ORDER BY priority ASC, created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(rows))
}

// ── Create ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateProviderReq {
    pub kind: String,
    pub name: String,
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    /// Settings key that holds the API key value.
    pub api_key_ref: Option<String>,
    /// If provided, the actual key value is stored in `settings` under `api_key_ref`.
    pub api_key_value: Option<String>,
    pub model: Option<String>,
    pub priority: Option<i64>,
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateProviderReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    validate_kind(&req.kind)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let enabled = req.enabled.unwrap_or(true);
    let priority = req.priority.unwrap_or(50);

    // Persist the API key in settings if provided
    if let (Some(key_ref), Some(key_val)) = (&req.api_key_ref, &req.api_key_value) {
        sqlx::query("INSERT OR REPLACE INTO settings(key, value) VALUES(?, ?)")
            .bind(key_ref)
            .bind(key_val)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }

    sqlx::query(
        "INSERT INTO ai_providers(id, kind, name, enabled, base_url, api_key_ref, model, \
         priority, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&req.kind)
    .bind(&req.name)
    .bind(enabled)
    .bind(&req.base_url)
    .bind(&req.api_key_ref)
    .bind(&req.model)
    .bind(priority)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

// ── Update ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateProviderReq {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    pub api_key_ref: Option<String>,
    pub api_key_value: Option<String>,
    pub model: Option<String>,
    pub priority: Option<i64>,
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<UpdateProviderReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    // Verify exists
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_providers WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;
    if count == 0 { return Err(AppError::NotFound); }

    let now = unix_now();

    if let Some(name) = &req.name {
        sqlx::query("UPDATE ai_providers SET name=?, updated_at=? WHERE id=?")
            .bind(name).bind(now).bind(&id)
            .execute(&state.db).await.map_err(AppError::Database)?;
    }
    if let Some(enabled) = req.enabled {
        sqlx::query("UPDATE ai_providers SET enabled=?, updated_at=? WHERE id=?")
            .bind(enabled).bind(now).bind(&id)
            .execute(&state.db).await.map_err(AppError::Database)?;
    }
    if let Some(base_url) = &req.base_url {
        sqlx::query("UPDATE ai_providers SET base_url=?, updated_at=? WHERE id=?")
            .bind(base_url).bind(now).bind(&id)
            .execute(&state.db).await.map_err(AppError::Database)?;
    }
    if let Some(model) = &req.model {
        sqlx::query("UPDATE ai_providers SET model=?, updated_at=? WHERE id=?")
            .bind(model).bind(now).bind(&id)
            .execute(&state.db).await.map_err(AppError::Database)?;
    }
    if let Some(priority) = req.priority {
        sqlx::query("UPDATE ai_providers SET priority=?, updated_at=? WHERE id=?")
            .bind(priority).bind(now).bind(&id)
            .execute(&state.db).await.map_err(AppError::Database)?;
    }
    if let Some(key_ref) = &req.api_key_ref {
        sqlx::query("UPDATE ai_providers SET api_key_ref=?, updated_at=? WHERE id=?")
            .bind(key_ref).bind(now).bind(&id)
            .execute(&state.db).await.map_err(AppError::Database)?;
        if let Some(key_val) = &req.api_key_value {
            sqlx::query("INSERT OR REPLACE INTO settings(key, value) VALUES(?, ?)")
                .bind(key_ref).bind(key_val)
                .execute(&state.db).await.map_err(AppError::Database)?;
        }
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Delete ───────────────────────────────────────────────────────────────────

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let rows = sqlx::query("DELETE FROM ai_providers WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?
        .rows_affected();
    if rows == 0 { return Err(AppError::NotFound); }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Health check ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResult {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

pub async fn health(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<HealthResult>> {
    require_admin(&state, &jar).await?;
    let orchestrator = crate::ai::AiOrchestrator::new(state.db.clone());
    match orchestrator.health_check(&id).await {
        Ok(()) => Ok(Json(HealthResult { id, ok: true, error: None })),
        Err(e) => Ok(Json(HealthResult { id, ok: false, error: Some(e) })),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

fn validate_kind(kind: &str) -> Result<()> {
    match kind {
        "odysseus" | "openai" | "anthropic" | "local" => Ok(()),
        _ => Err(AppError::BadRequest(format!(
            "Unknown provider kind '{}'. Valid: odysseus, openai, anthropic, local", kind
        ))),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
