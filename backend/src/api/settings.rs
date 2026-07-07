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
const AI_TLS_CERT: &str = "/var/lib/voidtower/nginx/conf.d/voidtower-ai-proxy.crt";
const AI_TLS_KEY: &str = "/var/lib/voidtower/nginx/conf.d/voidtower-ai-proxy.key";
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

/// Generates a self-signed cert for the AI proxy's HTTPS listener, once — same
/// approach as `docker/entrypoint.sh` uses for VoidTower's own bundled nginx, but
/// written to the conf.d bind-mount so nginx-proxy's container can read it
/// without a dedicated Docker volume (same trick `htpasswd_path` uses in proxy.rs).
fn ensure_ai_proxy_tls_cert() -> std::io::Result<()> {
    if std::path::Path::new(AI_TLS_CERT).exists() && std::path::Path::new(AI_TLS_KEY).exists() {
        return Ok(());
    }
    if let Some(dir) = std::path::Path::new(AI_TLS_CERT).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let out = std::process::Command::new("openssl")
        .args([
            "req", "-x509", "-nodes", "-days", "3650",
            "-newkey", "rsa:2048",
            "-keyout", AI_TLS_KEY,
            "-out", AI_TLS_CERT,
            "-subj", "/CN=voidtower-ai-proxy",
        ])
        .output()?;
    if !out.status.success() {
        return Err(std::io::Error::other(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(())
}

/// Writes both an HTTP (`port`) and HTTPS (`ai_tls_port(port)`) listener for the
/// AI proxy. The HTTPS one exists so the AI tab's iframe can be loaded from a page
/// served over HTTPS without the browser blocking it as mixed content — see
/// `PersistentAIFrame` in `frontend/src/components/layout/AppLayout.tsx`.
fn write_ai_proxy_conf(port: u16, upstream: &str) -> std::io::Result<()> {
    let upstream = upstream.trim_end_matches('/');
    // nginx-proxy always runs as its own Docker container (bare-metal or Docker
    // VoidTower installs alike) — rewrite localhost/127.0.0.1 so it resolves inside
    // that container. See `rewrite_upstream_for_docker` for why.
    let upstream = super::proxy::rewrite_upstream_for_docker(upstream);
    if let Some(dir) = std::path::Path::new(AI_PROXY_CONF).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    ensure_ai_proxy_tls_cert()?;
    let tls_port = ai_tls_port(port);
    let location = format!(
        "    location / {{\n\
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
    }}\n"
    );
    let content = format!(
        "# VoidTower AI proxy — auto-managed, do not edit\n\
server {{\n\
    listen {port};\n\
    server_name _;\n\
\n\
{location}\
}}\n\
\n\
server {{\n\
    listen {tls_port} ssl;\n\
    server_name _;\n\
    ssl_certificate     {AI_TLS_CERT};\n\
    ssl_certificate_key {AI_TLS_KEY};\n\
    ssl_protocols       TLSv1.2 TLSv1.3;\n\
    ssl_ciphers         HIGH:!aNULL:!MD5;\n\
\n\
{location}\
}}\n"
    );
    std::fs::write(AI_PROXY_CONF, content)
}

fn remove_ai_proxy_conf() {
    let _ = std::fs::remove_file(AI_PROXY_CONF);
}

/// Patch `ports` in the nginx-proxy docker-compose.yml, then re-deploy to publish the change.
/// `add_ports`: ports to add (as "{n}:{n}"); `remove_ports`: old ports to drop.
/// Only calls `docker compose up -d` if the ports list actually changed.
fn patch_nginx_compose_port(
    compose_path: &str,
    add_ports: &[u16],
    remove_ports: &[u16],
) -> std::result::Result<(), String> {
    let content = std::fs::read_to_string(compose_path)
        .map_err(|e| format!("Cannot read nginx-proxy compose file: {e}"))?;
    let mut doc: serde_yaml::Value = serde_yaml::from_str(&content)
        .map_err(|e| format!("Cannot parse nginx-proxy compose YAML: {e}"))?;

    // Locate the first service's ports list (nginx-proxy service)
    let ports = doc
        .get_mut("services")
        .and_then(|s| {
            if let serde_yaml::Value::Mapping(m) = s {
                m.values_mut().next()
            } else {
                None
            }
        })
        .and_then(|svc| svc.get_mut("ports"))
        .and_then(|p| p.as_sequence_mut())
        .ok_or_else(|| "ports list not found in nginx-proxy compose".to_string())?;

    let mut changed = false;

    for &old in remove_ports {
        let old_str = format!("{old}:{old}");
        let before = ports.len();
        ports.retain(|p| p.as_str().map(|s| s != old_str).unwrap_or(true));
        changed |= ports.len() != before;
    }

    for &new in add_ports {
        let new_str = format!("{new}:{new}");
        if !ports.iter().any(|p| p.as_str() == Some(&new_str)) {
            ports.push(serde_yaml::Value::String(new_str));
            changed = true;
        }
    }

    if !changed {
        return Ok(());
    }

    let new_content = serde_yaml::to_string(&doc)
        .map_err(|e| format!("Cannot serialize nginx-proxy compose YAML: {e}"))?;
    std::fs::write(compose_path, new_content)
        .map_err(|e| format!("Cannot write nginx-proxy compose file: {e}"))?;

    // Re-deploy to apply port binding change (brief container restart)
    let project = std::path::Path::new(compose_path)
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("vt-nginx-proxy");
    let out = std::process::Command::new("docker")
        .args(["compose", "-f", compose_path, "-p", project, "up", "-d", "--no-deps"])
        .output()
        .map_err(|e| format!("docker compose up failed: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "docker compose up failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
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
    // Upper-bounded so `ai_tls_port` (port + 1) can never overflow u16.
    if !(1024..=65534).contains(&port) {
        return Err(AppError::BadRequest("Port must be between 1024 and 65534".into()));
    }
    let tls_port = ai_tls_port(port);

    // Fetch current port and nginx-proxy compose path before making changes
    let old_port: Option<u16> = db_get(&state, AI_PORT_KEY).await.and_then(|v| v.parse().ok());
    let old_tls_port = old_port.map(ai_tls_port);
    let nginx_compose_path: Option<String> = sqlx::query_scalar(
        "SELECT compose_path FROM deployed_apps WHERE app_id = 'nginx-proxy' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();

    match req.url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(url) => {
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(AppError::BadRequest("URL must start with http:// or https://".into()));
            }
            db_set(&state, AI_URL_KEY, url).await?;
            db_set(&state, AI_PORT_KEY, &port.to_string()).await?;

            let url_owned = url.to_string();
            // Only patch compose + restart container when the port actually changed
            let port_changed = old_port != Some(port);
            let nginx_result = tokio::task::spawn_blocking(move || {
                write_ai_proxy_conf(port, &url_owned)
                    .map_err(|e| format!("Failed to write nginx config: {e}"))?;

                let result = if port_changed {
                    if let Some(cp) = nginx_compose_path {
                        let remove: Vec<u16> = old_port.map(|p| vec![p, ai_tls_port(p)]).unwrap_or_default();
                        patch_nginx_compose_port(&cp, &[port, tls_port], &remove)?;
                        Ok("nginx-proxy redeployed with updated port binding".to_string())
                    } else {
                        reload_nginx_pub()
                    }
                } else {
                    reload_nginx_pub()
                };

                // Best-effort — ensure the current ports are open, and close the old
                // ones if they moved, so a stale port doesn't stay reachable forever.
                super::proxy::open_firewall_port(&port.to_string());
                super::proxy::open_firewall_port(&tls_port.to_string());
                if port_changed {
                    if let Some(op) = old_port {
                        super::proxy::close_firewall_port(&op.to_string());
                    }
                    if let Some(otp) = old_tls_port {
                        super::proxy::close_firewall_port(&otp.to_string());
                    }
                }

                result
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
                    "tls_port": tls_port,
                    "nginx": msg,
                }))),
                Err(e) => Ok(Json(serde_json::json!({
                    "ok": true,
                    "proxy_active": false,
                    "port": port,
                    "tls_port": tls_port,
                    "nginx_error": e,
                }))),
            }
        }
        None => {
            db_delete(&state, AI_URL_KEY).await?;
            tokio::task::spawn_blocking(move || {
                remove_ai_proxy_conf();
                if let Some(cp) = nginx_compose_path {
                    let remove: Vec<u16> = old_port.map(|p| vec![p, ai_tls_port(p)]).unwrap_or_default();
                    let _ = patch_nginx_compose_port(&cp, &[], &remove);
                }
                if let Some(op) = old_port {
                    super::proxy::close_firewall_port(&op.to_string());
                }
                if let Some(otp) = old_tls_port {
                    super::proxy::close_firewall_port(&otp.to_string());
                }
                reload_nginx_pub()
            })
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
