use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

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

fn require_admin(user: &auth::User) -> Result<()> {
    if matches!(user.role.as_str(), "owner" | "admin") {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

#[derive(Serialize)]
pub struct UsersResponse {
    pub users: Vec<auth::PublicUser>,
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
    /// Required (and must be in the future) when role = "guest"; ignored otherwise.
    pub expires_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub password: String,
    pub username: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<UsersResponse>> {
    let caller = require_user(&state, &jar).await?;
    require_admin(&caller)?;

    let users = sqlx::query_as::<_, auth::User>(
        "SELECT id, username, password_hash, role, force_password_change, \
                totp_enabled, totp_secret, created_at, updated_at, expires_at \
         FROM users ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(UsersResponse {
        users: users.into_iter().map(Into::into).collect(),
    }))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<serde_json::Value>> {
    let caller = require_user(&state, &jar).await?;
    require_admin(&caller)?;

    if req.username.len() < 3 || req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Username ≥3 chars, password ≥8 chars".to_string(),
        ));
    }
    if !matches!(req.role.as_str(), "admin" | "operator" | "viewer" | "guest" | "demo") {
        return Err(AppError::BadRequest("Invalid role".to_string()));
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let expires_at = if req.role == "guest" {
        match req.expires_at {
            Some(exp) if exp > now => Some(exp),
            _ => return Err(AppError::BadRequest(
                "Guest accounts require an expires_at timestamp in the future".to_string(),
            )),
        }
    } else {
        None
    };

    let user = auth::create_user_ext(&state.db, &req.username, &req.password, &req.role, true, expires_at)
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                AppError::BadRequest("Username already taken".to_string())
            } else {
                AppError::Internal(e)
            }
        })?;

    audit::log(
        &state.db, Some(&caller.id), "human", "users.create",
        Some("user"), Some(&user.id), "success", None,
        Some(&format!("username={},role={}", req.username, req.role)),
    ).await;

    let public: auth::PublicUser = user.into();
    Ok(Json(serde_json::json!({ "user": public })))
}

pub async fn delete_user(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let caller = require_user(&state, &jar).await?;
    require_admin(&caller)?;

    if caller.id == user_id {
        return Err(AppError::BadRequest("Cannot delete your own account".to_string()));
    }

    let target = sqlx::query_as::<_, auth::User>(
        "SELECT id, username, password_hash, role, force_password_change, \
                totp_enabled, totp_secret, created_at, updated_at, expires_at \
         FROM users WHERE id = ?",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::BadRequest("User not found".to_string()))?;

    if target.role == "owner" && caller.role != "owner" {
        return Err(AppError::Forbidden);
    }

    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&caller.id), "human", "users.delete",
        Some("user"), Some(&user_id), "success", None,
        Some(&format!("deleted username={}", target.username)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn change_my_password(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be ≥8 characters".to_string(),
        ));
    }
    if let Some(ref name) = req.username {
        if name.len() < 3 {
            return Err(AppError::BadRequest(
                "Username must be ≥3 characters".to_string(),
            ));
        }
    }

    auth::change_password(&state.db, &user.id, &req.password, req.username.as_deref())
        .await
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                AppError::BadRequest("Username already taken".to_string())
            } else {
                AppError::Internal(e)
            }
        })?;

    audit::log(
        &state.db, Some(&user.id), "human", "users.change_password",
        Some("user"), Some(&user.id), "success", None, None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}
