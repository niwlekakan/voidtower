use crate::{
    audit,
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub message: String,
    pub severity: String,
    pub category: String,
    pub node_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub state: String,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<i64>,
    pub resolved_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub state: Option<String>,
    pub severity: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 { 100 }

#[derive(Serialize)]
pub struct AlertsResponse {
    pub alerts: Vec<Alert>,
    pub total: i64,
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::Unauthorized)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<ListQuery>,
) -> Result<Json<AlertsResponse>> {
    require_user(&state, &jar).await?;

    let state_filter = q.state.as_deref().unwrap_or("active");
    let limit = q.limit.min(500).max(1);

    let alerts = if let Some(sev) = &q.severity {
        sqlx::query_as::<_, Alert>(
            "SELECT * FROM alerts WHERE state = ? AND severity = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(state_filter)
        .bind(sev)
        .bind(limit)
        .bind(q.offset)
        .fetch_all(&state.db)
        .await
    } else {
        sqlx::query_as::<_, Alert>(
            "SELECT * FROM alerts WHERE state = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
        )
        .bind(state_filter)
        .bind(limit)
        .bind(q.offset)
        .fetch_all(&state.db)
        .await
    }
    .map_err(AppError::Database)?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alerts WHERE state = ?")
        .bind(state_filter)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(AlertsResponse { alerts, total }))
}

pub async fn acknowledge(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();
    let now = unix_now();

    sqlx::query(
        "UPDATE alerts SET state = 'acknowledged', acknowledged_by = ?, acknowledged_at = ?, updated_at = ? WHERE id = ?"
    )
    .bind(&user.username)
    .bind(now)
    .bind(now)
    .bind(&id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    audit::log(
        &state.db,
        Some(&user.id),
        &user.username,
        "alert.acknowledge",
        Some("alert"),
        Some(&id),
        "success",
        Some(&ip),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn resolve(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();
    let now = unix_now();

    sqlx::query(
        "UPDATE alerts SET state = 'resolved', resolved_at = ?, updated_at = ? WHERE id = ?"
    )
    .bind(now)
    .bind(now)
    .bind(&id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    audit::log(
        &state.db,
        Some(&user.id),
        &user.username,
        "alert.resolve",
        Some("alert"),
        Some(&id),
        "success",
        Some(&ip),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_alert(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    if user.role == "viewer" || user.role == "operator" {
        return Err(AppError::Forbidden);
    }

    sqlx::query("DELETE FROM alerts WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// Internal: create an alert (called by monitoring loop, not exposed externally)
pub async fn create_alert(
    pool: &sqlx::SqlitePool,
    title: &str,
    message: &str,
    severity: &str,
    category: &str,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let _ = sqlx::query(
        "INSERT OR IGNORE INTO alerts (id, title, message, severity, category, resource_type, resource_id, state, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)"
    )
    .bind(&id)
    .bind(title)
    .bind(message)
    .bind(severity)
    .bind(category)
    .bind(resource_type)
    .bind(resource_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await;
}
