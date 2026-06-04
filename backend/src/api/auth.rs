use crate::{
    audit,
    auth::{self, PublicUser},
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{ConnectInfo, State},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use time::Duration;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct BootstrapRequest {
    pub token: String,
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub user: PublicUser,
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>)> {
    let user = auth::find_user_by_username(&state.db, &req.username)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::Unauthorized)?;

    if !auth::verify_password(&req.password, &user.password_hash) {
        audit::log(
            &state.db, None, "human", "auth.login.failed",
            Some("user"), Some(&user.id), "failure",
            Some(&addr.ip().to_string()), Some(&format!("username={}", req.username)),
        ).await;
        return Err(AppError::Unauthorized);
    }

    let session = auth::create_session(
        &state.db,
        &user.id,
        Some(&addr.ip().to_string()),
        None,
    )
    .await
    .map_err(|e| AppError::Internal(e))?;

    audit::log(
        &state.db, Some(&user.id), "human", "auth.login",
        Some("user"), Some(&user.id), "success",
        Some(&addr.ip().to_string()), None,
    ).await;

    let cookie = Cookie::build(("vt_session", session.id))
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(Duration::days(7))
        .path("/")
        .build();

    Ok((jar.add(cookie), Json(AuthResponse { user: user.into() })))
}

pub async fn logout(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<(CookieJar, Json<serde_json::Value>)> {
    if let Some(cookie) = jar.get("vt_session") {
        let _ = auth::delete_session(&state.db, cookie.value()).await;
    }
    let removed = Cookie::build(("vt_session", ""))
        .path("/")
        .max_age(Duration::seconds(0))
        .build();
    Ok((jar.remove(removed), Json(serde_json::json!({ "ok": true }))))
}

pub async fn me(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<AuthResponse>> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;

    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::Unauthorized)?;

    Ok(Json(AuthResponse { user: user.into() }))
}

pub async fn bootstrap(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<BootstrapRequest>,
) -> Result<(CookieJar, Json<AuthResponse>)> {
    // Only allowed when no users exist
    if auth::has_any_user(&state.db)
        .await
        .map_err(|e| AppError::Internal(e))?
    {
        return Err(AppError::Forbidden);
    }

    let expected = auth::read_bootstrap_token(&state.config.bootstrap_token_path())
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or_else(|| AppError::BadRequest("No bootstrap token found".to_string()))?;

    if req.token != expected {
        return Err(AppError::Unauthorized);
    }

    if req.username.len() < 3 || req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Username must be ≥3 chars, password ≥8 chars".to_string(),
        ));
    }

    let user = auth::create_user(&state.db, &req.username, &req.password, "owner", false)
        .await
        .map_err(|e| AppError::Internal(e))?;

    let session = auth::create_session(
        &state.db,
        &user.id,
        Some(&addr.ip().to_string()),
        None,
    )
    .await
    .map_err(|e| AppError::Internal(e))?;

    audit::log(
        &state.db, Some(&user.id), "human", "auth.bootstrap",
        Some("user"), Some(&user.id), "success",
        Some(&addr.ip().to_string()), None,
    ).await;

    let cookie = Cookie::build(("vt_session", session.id))
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(Duration::days(7))
        .path("/")
        .build();

    Ok((jar.add(cookie), Json(AuthResponse { user: user.into() })))
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
