use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<(auth::User, String)> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::Unauthorized)?;
    Ok((user, session_id))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SessionInfo {
    pub id: String,
    pub user_id: String,
    pub expires_at: i64,
    pub created_at: i64,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionInfo>,
    pub current_session_id: String,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<SessionsResponse>> {
    let (user, current_id) = require_user(&state, &jar).await?;

    // Admins see all active sessions; others see only their own
    let sessions = if matches!(user.role.as_str(), "owner" | "admin") {
        sqlx::query_as::<_, SessionInfo>(
            "SELECT id, user_id, expires_at, created_at, ip_address, user_agent \
             FROM sessions WHERE expires_at > ? ORDER BY created_at DESC LIMIT 200",
        )
        .bind(unix_now())
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        sqlx::query_as::<_, SessionInfo>(
            "SELECT id, user_id, expires_at, created_at, ip_address, user_agent \
             FROM sessions WHERE user_id = ? AND expires_at > ? ORDER BY created_at DESC",
        )
        .bind(&user.id)
        .bind(unix_now())
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };

    Ok(Json(SessionsResponse {
        sessions,
        current_session_id: current_id,
    }))
}

pub async fn revoke_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let (user, current_id) = require_user(&state, &jar).await?;

    if session_id == current_id {
        return Err(AppError::BadRequest(
            "Cannot revoke your current session — use logout instead".to_string(),
        ));
    }

    // Verify the session belongs to this user (or caller is admin)
    let target: Option<(String,)> = sqlx::query_as("SELECT user_id FROM sessions WHERE id = ?")
        .bind(&session_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (owner_id,) = target
        .ok_or_else(|| AppError::BadRequest("Session not found".to_string()))?;

    let is_admin = matches!(user.role.as_str(), "owner" | "admin");
    if owner_id != user.id && !is_admin {
        return Err(AppError::Forbidden);
    }

    auth::delete_session(&state.db, &session_id)
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn revoke_all_other(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let (user, current_id) = require_user(&state, &jar).await?;

    let affected = sqlx::query(
        "DELETE FROM sessions WHERE user_id = ? AND id != ?",
    )
    .bind(&user.id)
    .bind(&current_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .rows_affected();

    Ok(Json(serde_json::json!({ "ok": true, "revoked": affected })))
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
