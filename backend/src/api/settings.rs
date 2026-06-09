use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use super::proxy::reload_nginx_pub;

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
const AI_PROXY_CONF: &str = "/var/lib/voidtower/nginx/conf.d/voidtower-ai-proxy.conf";
const DEFAULT_AI_PORT: u16 = 7001;

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

fn write_ai_proxy_conf(port: u16, upstream: &str) -> std::io::Result<()> {
    let upstream = upstream.trim_end_matches('/');
    if let Some(dir) = std::path::Path::new(AI_PROXY_CONF).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let content = format!(
        "# VoidTower AI proxy — auto-managed, do not edit\n\
server {{\n\
    listen {port};\n\
    server_name _;\n\
\n\
    location / {{\n\
        proxy_pass {upstream}/;\n\
        proxy_set_header Host $proxy_host;\n\
        proxy_set_header X-Real-IP $remote_addr;\n\
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n\
        proxy_http_version 1.1;\n\
        proxy_set_header Upgrade $http_upgrade;\n\
        proxy_set_header Connection \"upgrade\";\n\
\n\
        # Disable buffering — critical for LLM token streaming (SSE / chunked)\n\
        proxy_buffering off;\n\
        proxy_request_buffering off;\n\
        proxy_cache off;\n\
\n\
        proxy_read_timeout 600;\n\
        proxy_send_timeout 600;\n\
        proxy_connect_timeout 10;\n\
\n\
        proxy_hide_header X-Frame-Options;\n\
        proxy_hide_header Content-Security-Policy;\n\
        add_header X-Frame-Options \"ALLOWALL\" always;\n\
        add_header Content-Security-Policy \"frame-ancestors *\" always;\n\
    }}\n\
}}\n"
    );
    std::fs::write(AI_PROXY_CONF, content)
}

fn remove_ai_proxy_conf() {
    let _ = std::fs::remove_file(AI_PROXY_CONF);
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
        "proxy_active": proxy_active,
    })))
}

#[derive(Deserialize)]
pub struct SetAiUrlReq {
    pub url: Option<String>,
    pub port: Option<u16>,
}

pub async fn set_ai_url(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetAiUrlReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let port = req.port.unwrap_or(DEFAULT_AI_PORT);
    if port < 1024 {
        return Err(AppError::BadRequest("Port must be >= 1024".into()));
    }

    match req.url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(url) => {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(AppError::BadRequest("URL must start with http:// or https://".into()));
            }
            db_set(&state, AI_URL_KEY, url).await?;
            db_set(&state, AI_PORT_KEY, &port.to_string()).await?;

            // Write nginx conf and reload into Docker nginx conf.d
            let url_owned = url.to_string();
            let nginx_result = tokio::task::spawn_blocking(move || {
                write_ai_proxy_conf(port, &url_owned)
                    .map_err(|e| format!("Failed to write nginx config: {e}"))?;
                reload_nginx_pub()
            })
            .await
            .unwrap();

            audit::log(
                &state.db, Some(&user.id), "human", "settings.ai-url.set",
                Some("settings"), None, "success", None,
                Some(&format!("url={url},port={port}")),
            ).await;

            match nginx_result {
                Ok(msg) => Ok(Json(serde_json::json!({
                    "ok": true,
                    "proxy_active": true,
                    "port": port,
                    "nginx": msg,
                }))),
                Err(e) => Ok(Json(serde_json::json!({
                    "ok": true,
                    "proxy_active": false,
                    "port": port,
                    "nginx_error": e,
                }))),
            }
        }
        None => {
            db_delete(&state, AI_URL_KEY).await?;
            tokio::task::spawn_blocking(|| { remove_ai_proxy_conf(); reload_nginx_pub() })
                .await
                .unwrap()
                .ok();

            audit::log(
                &state.db, Some(&user.id), "human", "settings.ai-url.clear",
                Some("settings"), None, "success", None, None,
            ).await;

            Ok(Json(serde_json::json!({ "ok": true, "proxy_active": false })))
        }
    }
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
