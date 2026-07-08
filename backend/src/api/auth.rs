use crate::{
    audit,
    auth::{self, PublicUser},
    error::{AppError, Result},
    oidc, AppState,
};
use axum::{
    extract::{ConnectInfo, Query, State},
    response::Redirect,
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

    if let Some(exp) = user.expires_at {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        if exp <= now {
            audit::log(
                &state.db, None, "human", "auth.login.expired",
                Some("user"), Some(&user.id), "failure",
                Some(&addr.ip().to_string()), None,
            ).await;
            return Err(AppError::BadRequest("This guest account has expired".to_string()));
        }
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

    let mut public_user: PublicUser = user.into();
    public_user.mfa_required = crate::api::settings::mfa_required_for_role(&state, &public_user.role).await;
    Ok((jar.add(cookie), Json(AuthResponse { user: public_user })))
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

    let mut public_user: PublicUser = user.into();
    public_user.mfa_required = crate::api::settings::mfa_required_for_role(&state, &public_user.role).await;
    Ok(Json(AuthResponse { user: public_user }))
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

    let mut public_user: PublicUser = user.into();
    public_user.mfa_required = crate::api::settings::mfa_required_for_role(&state, &public_user.role).await;
    Ok((jar.add(cookie), Json(AuthResponse { user: public_user })))
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

// ─── Authentik / OIDC SSO ───────────────────────────────────────────────────

fn oidc_now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

#[derive(Serialize)]
pub struct OidcStatusResponse {
    pub enabled: bool,
    pub button_label: String,
}

pub async fn oidc_status(State(state): State<AppState>) -> Result<Json<OidcStatusResponse>> {
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM oidc_config WHERE id = 'default'")
            .fetch_optional(&state.db)
            .await?;
    Ok(Json(OidcStatusResponse {
        enabled: enabled.unwrap_or(false),
        button_label: "Login with Authentik".to_string(),
    }))
}

pub async fn oidc_login(State(state): State<AppState>, jar: CookieJar) -> Result<(CookieJar, Redirect)> {
    let settings = oidc::load_settings(&state.db, &state.secrets_key)
        .await?
        .ok_or_else(|| AppError::BadRequest("OIDC is not configured".into()))?;
    let client = oidc::build_client(&settings).await?;
    let start = oidc::start_authorization(&client, &settings.scopes);

    let cookie = Cookie::build(("vt_oidc_flow", start.flow_state.encode()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(600))
        .path("/api/auth/oidc")
        .build();

    Ok((jar.add(cookie), Redirect::to(&start.authorize_url)))
}

#[derive(Deserialize)]
pub struct OidcCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

async fn pick_oidc_username(db: &sqlx::SqlitePool, identity: &oidc::OidcIdentity) -> Result<String> {
    let candidates = [
        identity.preferred_username.clone(),
        identity
            .email
            .as_ref()
            .and_then(|e| e.split('@').next().map(String::from)),
    ];
    for candidate in candidates.into_iter().flatten() {
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE username = ?")
            .bind(&candidate)
            .fetch_optional(db)
            .await?;
        if exists.is_none() {
            return Ok(candidate);
        }
    }
    Ok(format!("oidc-{}", uuid::Uuid::new_v4()))
}

pub async fn oidc_callback(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    jar: CookieJar,
    Query(q): Query<OidcCallbackQuery>,
) -> Result<(CookieJar, Redirect)> {
    if let Some(err) = q.error {
        return Err(AppError::BadRequest(format!(
            "Authentik returned an error: {err} {}",
            q.error_description.unwrap_or_default()
        )));
    }
    let code = q.code.ok_or_else(|| AppError::BadRequest("missing code".into()))?;
    let returned_state = q
        .state
        .ok_or_else(|| AppError::BadRequest("missing state".into()))?;

    let flow_cookie = jar.get("vt_oidc_flow").ok_or_else(|| {
        AppError::BadRequest("OIDC flow expired — please try logging in again".into())
    })?;
    let flow = oidc::FlowState::decode(flow_cookie.value())?;

    if flow.csrf_state != returned_state {
        return Err(AppError::BadRequest("OIDC state mismatch".into()));
    }

    let settings = oidc::load_settings(&state.db, &state.secrets_key)
        .await?
        .ok_or_else(|| AppError::BadRequest("OIDC is not configured".into()))?;
    let client = oidc::build_client(&settings).await?;

    let identity = oidc::exchange_and_verify(&client, code, flow.pkce_verifier, &flow.nonce).await?;

    let groups = match oidc::discover_userinfo_endpoint(&settings.issuer_url).await? {
        Some(userinfo_url) => {
            oidc::fetch_role_claim_values(&userinfo_url, &identity.access_token, &settings.role_claim)
                .await
        }
        None => Vec::new(),
    };
    let role = oidc::map_role(&groups, &settings.role_map, &settings.default_role);

    let user = match auth::find_user_by_oidc_subject(&state.db, &identity.subject).await? {
        Some(mut existing) => {
            if existing.role != role {
                auth::update_user_role(&state.db, &existing.id, &role).await?;
                existing.role = role;
            }
            existing
        }
        None => {
            if !settings.auto_provision {
                return Err(AppError::Forbidden);
            }
            let username = pick_oidc_username(&state.db, &identity).await?;
            auth::create_oidc_user(&state.db, &username, &identity.subject, &role).await?
        }
    };

    let session = auth::create_session(&state.db, &user.id, Some(&addr.ip().to_string()), None).await?;

    audit::log(
        &state.db, Some(&user.id), "human", "auth.login.oidc",
        Some("user"), Some(&user.id), "success",
        Some(&addr.ip().to_string()), None,
    ).await;

    let session_cookie = Cookie::build(("vt_session", session.id))
        .http_only(true)
        .same_site(SameSite::Strict)
        .max_age(Duration::days(7))
        .path("/")
        .build();

    let expired_flow_cookie = Cookie::build(("vt_oidc_flow", ""))
        .path("/api/auth/oidc")
        .max_age(Duration::seconds(0))
        .build();

    Ok((
        jar.add(session_cookie).remove(expired_flow_cookie),
        Redirect::to("/dashboard"),
    ))
}

#[derive(Serialize)]
pub struct OidcConfigResponse {
    pub enabled: bool,
    pub issuer_url: Option<String>,
    pub client_id: Option<String>,
    pub has_client_secret: bool,
    pub redirect_url: Option<String>,
    pub scopes: String,
    pub role_claim: String,
    pub role_map: HashMap<String, String>,
    pub default_role: String,
    pub auto_provision: bool,
}

#[allow(clippy::type_complexity)]
pub async fn get_oidc_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<OidcConfigResponse>> {
    require_admin(&state, &jar).await?;

    let row: Option<(
        bool,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        bool,
    )> = sqlx::query_as(
        "SELECT enabled, issuer_url, client_id, client_secret_id, redirect_url, scopes, role_claim, role_map, default_role, auto_provision FROM oidc_config WHERE id = 'default'"
    )
    .fetch_optional(&state.db)
    .await?;

    let Some((
        enabled,
        issuer_url,
        client_id,
        client_secret_id,
        redirect_url,
        scopes,
        role_claim,
        role_map,
        default_role,
        auto_provision,
    )) = row
    else {
        return Ok(Json(OidcConfigResponse {
            enabled: false,
            issuer_url: None,
            client_id: None,
            has_client_secret: false,
            redirect_url: None,
            scopes: "openid profile email groups".into(),
            role_claim: "groups".into(),
            role_map: HashMap::new(),
            default_role: "viewer".into(),
            auto_provision: true,
        }));
    };

    Ok(Json(OidcConfigResponse {
        enabled,
        issuer_url,
        client_id,
        has_client_secret: client_secret_id.is_some(),
        redirect_url,
        scopes,
        role_claim,
        role_map: serde_json::from_str(&role_map).unwrap_or_default(),
        default_role,
        auto_provision,
    }))
}

#[derive(Deserialize)]
pub struct SaveOidcConfigRequest {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    /// Only set when the admin is changing the secret; omitted to keep the existing one.
    pub client_secret: Option<String>,
    pub redirect_url: String,
    pub scopes: String,
    pub role_claim: String,
    pub role_map: HashMap<String, String>,
    pub default_role: String,
    pub auto_provision: bool,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn save_oidc_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SaveOidcConfigRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if req.dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Save Authentik SSO Configuration",
                "risk": "medium",
                "changes": [
                    { "label": "Enabled", "value": if req.enabled { "yes — shows \"Login with Authentik\" on the login page" } else { "no — SSO button hidden" } },
                    { "label": "Issuer", "value": req.issuer_url },
                    { "label": "Client ID", "value": req.client_id },
                    { "label": "Client secret", "value": if req.client_secret.is_some() { "updated" } else { "unchanged" } },
                    { "label": "Redirect URL", "value": req.redirect_url },
                    { "label": "Role mapping", "value": serde_json::to_string(&req.role_map).unwrap_or_default() },
                    { "label": "Default role", "value": req.default_role },
                    { "label": "Rollback", "value": "Disable SSO again from this page" },
                ],
                "preview": null,
            }
        })));
    }

    let existing_secret_id: Option<String> = sqlx::query_scalar(
        "SELECT client_secret_id FROM oidc_config WHERE id = 'default'",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten();

    let secret_id = if let Some(new_secret) = &req.client_secret {
        let enc = crate::api::secrets::encrypt(&state.secrets_key, new_secret).map_err(AppError::Internal)?;
        match &existing_secret_id {
            Some(id) => {
                sqlx::query("UPDATE secrets SET value_enc = ?, updated_at = ? WHERE id = ?")
                    .bind(&enc)
                    .bind(oidc_now_ts())
                    .bind(id)
                    .execute(&state.db)
                    .await?;
                id.clone()
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                let now = oidc_now_ts();
                sqlx::query(
                    "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) VALUES (?,?,?,?,?,?)",
                )
                .bind(&id)
                .bind("oidc_client_secret")
                .bind(Some("Authentik OIDC client secret"))
                .bind(&enc)
                .bind(now)
                .bind(now)
                .execute(&state.db)
                .await?;
                id
            }
        }
    } else {
        existing_secret_id
            .ok_or_else(|| AppError::BadRequest("client_secret is required on first save".into()))?
    };

    let now = oidc_now_ts();
    let role_map_json = serde_json::to_string(&req.role_map).unwrap_or_else(|_| "{}".to_string());

    sqlx::query(
        "INSERT INTO oidc_config (id, enabled, issuer_url, client_id, client_secret_id, redirect_url, scopes, role_claim, role_map, default_role, auto_provision, updated_at)
         VALUES ('default', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            enabled = excluded.enabled, issuer_url = excluded.issuer_url, client_id = excluded.client_id,
            client_secret_id = excluded.client_secret_id, redirect_url = excluded.redirect_url,
            scopes = excluded.scopes, role_claim = excluded.role_claim, role_map = excluded.role_map,
            default_role = excluded.default_role, auto_provision = excluded.auto_provision, updated_at = excluded.updated_at",
    )
    .bind(req.enabled)
    .bind(&req.issuer_url)
    .bind(&req.client_id)
    .bind(&secret_id)
    .bind(&req.redirect_url)
    .bind(&req.scopes)
    .bind(&req.role_claim)
    .bind(&role_map_json)
    .bind(&req.default_role)
    .bind(req.auto_provision)
    .bind(now)
    .execute(&state.db)
    .await?;

    audit::log(
        &state.db, Some(&user.id), "human", "oidc.config.save",
        Some("oidc_config"), None, "success", None,
        Some(&format!("enabled={}", req.enabled)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}
