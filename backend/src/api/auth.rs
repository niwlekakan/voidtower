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
    pub totp_code: Option<String>,
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

fn check_rate_limit(state: &AppState, ip: std::net::IpAddr) -> Result<()> {
    use crate::LoginAttempts;
    let mut map = state.login_limiter.lock().unwrap();
    let now = std::time::Instant::now();
    let entry = map.entry(ip).or_insert(LoginAttempts {
        count: 0,
        window_start: now,
        locked_until: None,
    });
    if let Some(locked_until) = entry.locked_until {
        if now < locked_until {
            return Err(AppError::TooManyRequests);
        }
        // Lock expired — reset
        entry.count = 0;
        entry.locked_until = None;
        entry.window_start = now;
    }
    // Reset window after 10 minutes
    if now.duration_since(entry.window_start).as_secs() > 600 {
        entry.count = 0;
        entry.window_start = now;
    }
    Ok(())
}

fn record_failed_attempt(state: &AppState, ip: std::net::IpAddr) {
    use crate::LoginAttempts;
    let mut map = state.login_limiter.lock().unwrap();
    let now = std::time::Instant::now();
    let entry = map.entry(ip).or_insert(LoginAttempts {
        count: 0,
        window_start: now,
        locked_until: None,
    });
    entry.count += 1;
    if entry.count >= 5 {
        entry.locked_until = Some(now + std::time::Duration::from_secs(900));
    }
}

fn clear_rate_limit(state: &AppState, ip: std::net::IpAddr) {
    state.login_limiter.lock().unwrap().remove(&ip);
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<(CookieJar, Json<AuthResponse>)> {
    let ip = addr.ip();
    check_rate_limit(&state, ip)?;

    let user = auth::find_user_by_username(&state.db, &req.username)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| { record_failed_attempt(&state, ip); AppError::Unauthorized })?;

    if !auth::verify_password(&req.password, &user.password_hash) {
        record_failed_attempt(&state, ip);
        audit::log(
            &state.db, None, "human", "auth.login.failed",
            Some("user"), Some(&user.id), "failure",
            Some(&addr.ip().to_string()), Some(&format!("username={}", req.username)),
        ).await;
        return Err(AppError::Unauthorized);
    }

    // TOTP check
    if user.totp_enabled {
        match &req.totp_code {
            None => return Err(AppError::TotpRequired),
            Some(code) => {
                let secret = user.totp_secret.as_deref().unwrap_or("");
                if !crate::api::totp::verify_totp(secret, code) {
                    record_failed_attempt(&state, ip);
                    audit::log(
                        &state.db, None, "human", "auth.login.totp_failed",
                        Some("user"), Some(&user.id), "failure",
                        Some(&addr.ip().to_string()), None,
                    ).await;
                    return Err(AppError::Unauthorized);
                }
            }
        }
    }

    clear_rate_limit(&state, ip);

    let session = auth::create_session(
        &state.db,
        &user.id,
        Some(&addr.ip().to_string()),
        None,
    )
    .await
    .map_err(AppError::Internal)?;

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
        .map_err(AppError::Internal)?
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
        .map_err(AppError::Internal)?
    {
        return Err(AppError::Forbidden);
    }

    let expected = auth::read_bootstrap_token(&state.config.bootstrap_token_path())
        .await
        .map_err(AppError::Internal)?
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
        .map_err(AppError::Internal)?;

    let session = auth::create_session(
        &state.db,
        &user.id,
        Some(&addr.ip().to_string()),
        None,
    )
    .await
    .map_err(AppError::Internal)?;

    audit::log(
        &state.db, Some(&user.id), "human", "auth.bootstrap",
        Some("user"), Some(&user.id), "success",
        Some(&addr.ip().to_string()), None,
    ).await;

    // Auto-provision Voidwatch token so Odysseus wires itself up without a second installer run
    let pending_path = state.config.config_dir.join("voidwatch-pending-token");
    let db2 = state.db.clone();
    let uid2 = user.id.clone();
    tokio::spawn(async move { provision_voidwatch(db2, uid2, pending_path).await });

    let cookie = Cookie::build(("vt_session", session.id))
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(Duration::days(7))
        .path("/")
        .build();

    Ok((jar.add(cookie), Json(AuthResponse { user: user.into() })))
}

async fn provision_voidwatch(db: sqlx::SqlitePool, user_id: String, token_path: std::path::PathBuf) {
    use crate::api::integrations::{generate_api_token, sha256_hex, unix_now};

    if !std::path::Path::new("/opt/odysseus/app.py").exists() {
        return;
    }

    let raw = generate_api_token();
    let hash = sha256_hex(&raw);
    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let scopes = serde_json::json!([
        "metrics:read","services:read","services:restart","containers:read",
        "containers:restart","containers:logs","apps:read","apps:restart",
        "backups:read","backups:run","alerts:read","alerts:ack",
        "automation:read","automation:run","timeline:read","network:read",
        "storage:read","diagnostics:read","proxy:read","tags:read",
        "secrets:list","vms:read"
    ]).to_string();

    let ok = sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id).bind(&user_id).bind("voidwatch-integration")
    .bind(&hash).bind(&scopes).bind::<Option<i64>>(None).bind(now)
    .execute(&db).await.is_ok();

    if ok {
        let _ = tokio::fs::write(&token_path, &raw).await;
        tracing::info!("Voidwatch token provisioned at {:?}", token_path);
    }
}

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
