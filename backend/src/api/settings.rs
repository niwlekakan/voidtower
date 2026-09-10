use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

const AI_URL_KEY: &str = "ai_proxy_url";
const INSTANCE_NAME_KEY: &str = "instance_name";
const LOGIN_TAGLINE_KEY: &str = "login_tagline";
const CUSTOM_CSS_KEY: &str = "custom_css";
const LOGIN_BG_URL_KEY: &str = "login_bg_url";
const INSTANCE_LOGO_KEY: &str = "instance_logo";
const MAX_CUSTOM_CSS_LEN: usize = 8192;
const MAX_LOGO_LEN: usize = 256 * 1024; // 256 KB base64
const NOTIF_NTFY_URL_KEY: &str = "notif_ntfy_url";
const NOTIF_DISCORD_KEY: &str = "notif_discord_webhook";
const NOTIF_SLACK_KEY: &str = "notif_slack_webhook";
const AI_PORT_KEY: &str = "ai_proxy_port";
const MFA_REQUIRED_ROLES_KEY: &str = "mfa_required_roles";
const AI_PROXY_CONF: &str = "/var/lib/voidtower/nginx/conf.d/voidtower-ai-proxy.conf";
const DEFAULT_AI_PORT: u16 = 7001;

/// The AI proxy's HTTPS listener always sits one port above the HTTP one — no
/// separate setting to track, and the two are always opened/closed together.
fn ai_tls_port(port: u16) -> u16 {
    port + 1
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn db_get(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
}

async fn db_set(state: &AppState, key: &str, value: &str) -> Result<()> {
    let now = unix_now();
    sqlx::query("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn db_delete(state: &AppState, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

// ─── Handlers ────────────────────────────────────────────────────────────────

pub async fn get_ai_url(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let url = db_get(&state, AI_URL_KEY).await;
    let port: u16 = db_get(&state, AI_PORT_KEY)
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_AI_PORT);
    let proxy_active = std::path::Path::new(AI_PROXY_CONF).exists();

    Ok(Json(serde_json::json!({
        "url": url,
        "port": port,
        "tls_port": ai_tls_port(port),
        "proxy_active": proxy_active,
    })))
}

#[allow(dead_code)] // retained as the request contract for the future canonical adapter
#[derive(Deserialize)]
pub struct SetAiUrlReq {
    pub url: Option<String>,
    pub port: Option<u16>,
}

pub async fn set_ai_url(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(_req): Json<SetAiUrlReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "AI proxy settings require a canonical operation adapter".into(),
    ))
}

// ─── MFA policy ──────────────────────────────────────────────────────────────

/// Roles that get TOTP enrollment forced on first login when no explicit
/// policy has been saved yet — the two roles with real infrastructure control.
fn default_mfa_required_roles() -> Vec<String> {
    vec!["owner".to_string(), "admin".to_string()]
}

fn valid_role(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "operator" | "viewer" | "guest" | "demo" | "member")
}

/// Whether the given role currently requires mandatory MFA enrollment.
/// Reads the admin-configurable policy, falling back to the default list
/// when unset or unparseable — never lets a bad value silently disable it.
pub async fn mfa_required_for_role(state: &AppState, role: &str) -> bool {
    let roles: Vec<String> = match db_get(state, MFA_REQUIRED_ROLES_KEY).await {
        Some(v) => serde_json::from_str(&v).unwrap_or_else(|_| default_mfa_required_roles()),
        None => default_mfa_required_roles(),
    };
    roles.iter().any(|r| r == role)
}

pub async fn get_mfa_policy(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let roles = match db_get(&state, MFA_REQUIRED_ROLES_KEY).await {
        Some(v) => serde_json::from_str(&v).unwrap_or_else(|_| default_mfa_required_roles()),
        None => default_mfa_required_roles(),
    };
    Ok(Json(serde_json::json!({ "required_roles": roles })))
}

#[derive(Deserialize)]
pub struct SetMfaPolicyReq {
    pub required_roles: Vec<String>,
}

pub async fn set_mfa_policy(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetMfaPolicyReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    for role in &req.required_roles {
        if !valid_role(role) {
            return Err(AppError::BadRequest(format!("Invalid role: {role}")));
        }
    }

    let value = serde_json::to_string(&req.required_roles)
        .map_err(|e| AppError::Internal(e.into()))?;
    db_set(&state, MFA_REQUIRED_ROLES_KEY, &value).await?;

    audit::log(
        &state.db, Some(&user.id), "human", "settings.mfa_policy.set",
        Some("settings"), None, "success", None,
        Some(&format!("required_roles={}", req.required_roles.join(","))),
    ).await;
    Ok(Json(serde_json::json!({ "ok": true, "required_roles": req.required_roles })))
}

// ─── General settings ────────────────────────────────────────────────────────

pub async fn get_general(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let name       = db_get(&state, INSTANCE_NAME_KEY).await.unwrap_or_else(|| "VoidTower".into());
    let tagline    = db_get(&state, LOGIN_TAGLINE_KEY).await.unwrap_or_default();
    let custom_css = db_get(&state, CUSTOM_CSS_KEY).await.unwrap_or_default();
    let bg_url     = db_get(&state, LOGIN_BG_URL_KEY).await.unwrap_or_default();
    let logo       = db_get(&state, INSTANCE_LOGO_KEY).await.unwrap_or_default();
    Ok(Json(serde_json::json!({
        "instance_name": name,
        "login_tagline": tagline,
        "custom_css":    custom_css,
        "login_bg_url":  bg_url,
        "instance_logo": logo,
    })))
}

#[derive(Deserialize)]
pub struct SetGeneralReq {
    pub instance_name: Option<String>,
    pub login_tagline: Option<String>,
    pub custom_css:    Option<String>,
    pub login_bg_url:  Option<String>,
    pub instance_logo: Option<String>,
}

pub async fn set_general(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetGeneralReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let name = req.instance_name.as_deref().unwrap_or("VoidTower").trim().to_string();
    let name = if name.is_empty() { "VoidTower".into() } else { name };
    db_set(&state, INSTANCE_NAME_KEY, &name).await?;

    let tagline = req.login_tagline.as_deref().unwrap_or("").trim().to_string();
    if tagline.is_empty() { db_delete(&state, LOGIN_TAGLINE_KEY).await?; }
    else { db_set(&state, LOGIN_TAGLINE_KEY, &tagline).await?; }

    let css = req.custom_css.as_deref().unwrap_or("").to_string();
    if css.len() > MAX_CUSTOM_CSS_LEN {
        return Err(AppError::BadRequest(format!("custom_css exceeds {} bytes", MAX_CUSTOM_CSS_LEN)));
    }
    if css.is_empty() { db_delete(&state, CUSTOM_CSS_KEY).await?; }
    else { db_set(&state, CUSTOM_CSS_KEY, &css).await?; }

    let bg_url = req.login_bg_url.as_deref().unwrap_or("").trim().to_string();
    if bg_url.is_empty() { db_delete(&state, LOGIN_BG_URL_KEY).await?; }
    else { db_set(&state, LOGIN_BG_URL_KEY, &bg_url).await?; }

    let logo = req.instance_logo.as_deref().unwrap_or("").to_string();
    if logo.len() > MAX_LOGO_LEN {
        return Err(AppError::BadRequest("instance_logo exceeds 256KB".into()));
    }
    if logo.is_empty() { db_delete(&state, INSTANCE_LOGO_KEY).await?; }
    else { db_set(&state, INSTANCE_LOGO_KEY, &logo).await?; }

    audit::log(
        &state.db, Some(&user.id), "human", "settings.general.set",
        Some("settings"), None, "success", None,
        Some(&format!("instance_name={name}")),
    ).await;
    Ok(Json(serde_json::json!({ "ok": true, "instance_name": name })))
}

/// Public endpoint — no authentication required.
/// Returns only the fields needed by the login page.
pub async fn get_public(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let name    = db_get(&state, INSTANCE_NAME_KEY).await.unwrap_or_else(|| "VoidTower".into());
    let tagline = db_get(&state, LOGIN_TAGLINE_KEY).await.unwrap_or_default();
    let bg_url  = db_get(&state, LOGIN_BG_URL_KEY).await.unwrap_or_default();
    let logo    = db_get(&state, INSTANCE_LOGO_KEY).await.unwrap_or_default();
    Json(serde_json::json!({
        "instance_name": name,
        "login_tagline": tagline,
        "login_bg_url":  bg_url,
        "instance_logo": logo,
    }))
}

// ─── Notification webhooks ────────────────────────────────────────────────────

pub async fn get_notifications(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Ok(Json(serde_json::json!({
        "ntfy_url":        db_get(&state, NOTIF_NTFY_URL_KEY).await,
        "discord_webhook": db_get(&state, NOTIF_DISCORD_KEY).await,
        "slack_webhook":   db_get(&state, NOTIF_SLACK_KEY).await,
    })))
}

#[derive(Deserialize)]
pub struct SetNotificationsReq {
    pub ntfy_url:        Option<String>,
    pub discord_webhook: Option<String>,
    pub slack_webhook:   Option<String>,
}

pub async fn set_notifications(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetNotificationsReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let save = |key: &'static str, val: Option<String>| async move {
        // can't capture state easily in async closure — return the pair
        (key, val)
    };

    for (key, val) in [
        (NOTIF_NTFY_URL_KEY, req.ntfy_url),
        (NOTIF_DISCORD_KEY,  req.discord_webhook),
        (NOTIF_SLACK_KEY,    req.slack_webhook),
    ] {
        let _ = save(key, val.clone()).await;
        match val.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => { db_set(&state, key, v).await?; }
            None    => { db_delete(&state, key).await?; }
        }
    }

    audit::log(
        &state.db, Some(&user.id), "human", "settings.notifications.set",
        Some("settings"), None, "success", None, None,
    ).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct TestNotificationReq {
    pub channel: String, // "ntfy" | "discord" | "slack"
}

pub async fn test_notification(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<TestNotificationReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let title   = "VoidTower test notification";
    let message = "Webhook is working correctly.";

    let result = match req.channel.as_str() {
        "ntfy" => {
            let url = db_get(&state, NOTIF_NTFY_URL_KEY).await
                .ok_or_else(|| AppError::BadRequest("ntfy URL not configured".into()))?;
            let client = reqwest::Client::new();
            client.post(&url)
                .header("Title", title)
                .body(message)
                .send().await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        "discord" => {
            let url = db_get(&state, NOTIF_DISCORD_KEY).await
                .ok_or_else(|| AppError::BadRequest("Discord webhook not configured".into()))?;
            let client = reqwest::Client::new();
            client.post(&url)
                .json(&serde_json::json!({ "content": format!("**{title}**\n{message}") }))
                .send().await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        "slack" => {
            let url = db_get(&state, NOTIF_SLACK_KEY).await
                .ok_or_else(|| AppError::BadRequest("Slack webhook not configured".into()))?;
            let client = reqwest::Client::new();
            client.post(&url)
                .json(&serde_json::json!({ "text": format!("*{title}*\n{message}") }))
                .send().await
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
        _ => return Err(AppError::BadRequest("Unknown channel".into())),
    };

    match result {
        Ok(_)    => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(msg) => Ok(Json(serde_json::json!({ "ok": false, "error": msg }))),
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt;

    #[tokio::test]
    async fn ai_url_mutation_fails_closed_without_persisting_or_reconfiguring() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/settings/ai-url")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"url": "http://ai-provider.invalid", "port": 7001}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "AI proxy settings require a canonical operation adapter"
        );

        let stored_url: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'ai_proxy_url'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        let stored_port: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'ai_proxy_port'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert_eq!(stored_url, None);
        assert_eq!(stored_port, None);
    }

    #[tokio::test]
    async fn ai_url_mutation_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/settings/ai-url")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"url": "http://ai-provider.invalid"}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let payload: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(payload["error"]["code"], "unauthorized");
    }

    #[test]
    fn ai_url_compatibility_handler_has_no_direct_mutation_path() {
        let source = include_str!("settings.rs");
        let handler = source
            .split("pub async fn set_ai_url")
            .nth(1)
            .and_then(|rest| rest.split("// ─── MFA policy").next())
            .expect("AI URL handler must be followed by the MFA section");

        for marker in [
            "db_set(",
            "db_delete(",
            "write_ai_proxy_conf(",
            "patch_nginx_compose_port(",
            "reload_nginx_pub(",
            "open_firewall_port(",
            "close_firewall_port(",
        ] {
            assert!(!handler.contains(marker), "direct mutation marker: {marker}");
        }
    }
}
