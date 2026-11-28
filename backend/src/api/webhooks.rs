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
use sqlx::SqlitePool;
use std::future::Future;
use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::{Arc, OnceLock},
    time::Duration,
};

const MAX_WEBHOOK_URL_BYTES: usize = 2_048;
const DNS_RESOLUTION_TIMEOUT: Duration = Duration::from_secs(5);
const DNS_RESOLUTION_BLOCKING_LIMIT: usize = 8;

static DNS_RESOLUTION_LIMIT: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();

fn dns_resolution_limit() -> Arc<tokio::sync::Semaphore> {
    Arc::clone(
        DNS_RESOLUTION_LIMIT
            .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(DNS_RESOLUTION_BLOCKING_LIMIT))),
    )
}

fn invalid_destination() -> String {
    "webhook destination rejected by outbound policy".into()
}

fn unsafe_ip(ip: IpAddr) -> bool {
    let extra_webhook_prohibited = match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            octets[0] == 0
                || (224..=239).contains(&octets[0])
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        }
        IpAddr::V6(_) => false,
    };
    extra_webhook_prohibited || crate::ai::egress::is_prohibited(ip)
}

fn safe_ip(ip: IpAddr) -> bool {
    !unsafe_ip(ip)
}

fn canonical_host(host: &str) -> Option<String> {
    if host.ends_with("..") {
        return None;
    }
    let normalized = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if normalized.is_empty() || normalized.split('.').any(str::is_empty) {
        None
    } else {
        Some(normalized)
    }
}

fn validate_webhook_url(raw: &str) -> std::result::Result<reqwest::Url, String> {
    let trimmed = raw.trim();
    if raw.is_empty()
        || raw.len() > MAX_WEBHOOK_URL_BYTES
        || raw != trimmed
        || raw
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(invalid_destination());
    }

    let mut url = reqwest::Url::parse(raw).map_err(|_| invalid_destination())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid_destination());
    }

    let authority = raw
        .split_once("://")
        .and_then(|(_, rest)| rest.split(['/', '?', '#']).next())
        .unwrap_or_default();
    if authority.contains('@') {
        return Err(invalid_destination());
    }

    let host = url.host_str().ok_or_else(invalid_destination)?;
    let normalized_host = canonical_host(host).ok_or_else(invalid_destination)?;
    if normalized_host.is_empty()
        || normalized_host == "localhost"
        || normalized_host == "metadata"
        || normalized_host == "metadata.google.internal"
        || normalized_host.ends_with(".localhost")
        || normalized_host.ends_with(".local")
        || normalized_host.ends_with(".internal")
    {
        return Err(invalid_destination());
    }
    let host_ip = host.trim_start_matches('[').trim_end_matches(']');
    if host_ip.parse::<IpAddr>().map(unsafe_ip).unwrap_or(false) {
        return Err(invalid_destination());
    }
    if normalized_host != host {
        url.set_host(Some(&normalized_host))
            .map_err(|_| invalid_destination())?;
    }
    Ok(url)
}

fn select_safe_destination_with<F>(
    addresses: &[SocketAddr],
    is_safe: F,
) -> std::result::Result<Option<SocketAddr>, String>
where
    F: Fn(IpAddr) -> bool,
{
    if addresses.is_empty() || addresses.iter().any(|address| !is_safe(address.ip())) {
        return Err(invalid_destination());
    }
    Ok(addresses.first().copied())
}

async fn resolve_hostname(host: String, port: u16) -> std::result::Result<Vec<SocketAddr>, String> {
    let limit = dns_resolution_limit();
    resolve_hostname_with_timeout(
        host,
        port,
        DNS_RESOLUTION_TIMEOUT,
        move |host, port| async move {
            resolve_hostname_with_blocking_limit(host, port, limit, |host, port| {
                (host.as_str(), port)
                    .to_socket_addrs()
                    .map(|resolved| resolved.collect())
                    .map_err(|_| invalid_destination())
            })
            .await
        },
    )
    .await
}

async fn resolve_hostname_with_blocking_limit<F>(
    host: String,
    port: u16,
    limit: Arc<tokio::sync::Semaphore>,
    resolve: F,
) -> std::result::Result<Vec<SocketAddr>, String>
where
    F: FnOnce(String, u16) -> std::result::Result<Vec<SocketAddr>, String> + Send + 'static,
{
    let permit = limit
        .try_acquire_owned()
        .map_err(|_| invalid_destination())?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        resolve(host, port)
    })
    .await
    .map_err(|_| invalid_destination())?
}

async fn resolve_hostname_with_timeout<F, Fut>(
    host: String,
    port: u16,
    timeout: Duration,
    resolve: F,
) -> std::result::Result<Vec<SocketAddr>, String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = std::result::Result<Vec<SocketAddr>, String>>,
{
    match tokio::time::timeout(timeout, resolve(host, port)).await {
        Ok(Ok(addresses)) => Ok(addresses),
        Ok(Err(_)) | Err(_) => Err(invalid_destination()),
    }
}

async fn safe_destination<F, Fut>(
    url: &reqwest::Url,
    resolve: F,
    is_safe: impl Fn(IpAddr) -> bool,
) -> std::result::Result<Option<SocketAddr>, String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = std::result::Result<Vec<SocketAddr>, String>>,
{
    let host = canonical_host(url.host_str().ok_or_else(invalid_destination)?)
        .ok_or_else(invalid_destination)?;
    let port = url
        .port_or_known_default()
        .ok_or_else(invalid_destination)?;
    let host_ip = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host_ip.parse::<IpAddr>() {
        return if !is_safe(ip) {
            Err(invalid_destination())
        } else {
            Ok(Some(SocketAddr::new(ip, port)))
        };
    }

    let resolved = resolve(host, port).await?;
    select_safe_destination_with(&resolved, is_safe)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

// ─── Structs ─────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct WebhookConfig {
    pub id: i64,
    pub name: String,
    pub url: String,
    #[serde(rename = "type")]
    pub webhook_type: String,
    pub events: String, // JSON array stored as text
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateWebhook {
    pub name: String,
    pub url: String,
    #[serde(rename = "type", default = "default_type")]
    pub webhook_type: String,
    #[serde(default = "default_events")]
    pub events: serde_json::Value,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_type() -> String {
    "generic".into()
}
fn default_events() -> serde_json::Value {
    serde_json::json!(["alert.created"])
}
fn default_enabled() -> bool {
    true
}

#[derive(Deserialize)]
pub struct UpdateWebhook {
    pub name: Option<String>,
    pub url: Option<String>,
    #[serde(rename = "type")]
    pub webhook_type: Option<String>,
    pub events: Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// GET /api/webhooks
pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let rows = sqlx::query_as::<_, WebhookConfig>(
        "SELECT id, name, url, type AS webhook_type, events, enabled, created_at
         FROM webhook_configs ORDER BY id ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "webhooks": rows })))
}

/// POST /api/webhooks
pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<CreateWebhook>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("name required".into()));
    }
    let url = validate_webhook_url(&body.url).map_err(AppError::BadRequest)?;
    let valid_types = ["ntfy", "discord", "slack", "generic"];
    if !valid_types.contains(&body.webhook_type.as_str()) {
        return Err(AppError::BadRequest(
            "type must be ntfy, discord, slack, or generic".into(),
        ));
    }
    let events_str = serde_json::to_string(&body.events)
        .map_err(|e| AppError::BadRequest(format!("invalid events: {e}")))?;
    let ts = now();
    let row = sqlx::query_as::<_, (i64,)>(
        "INSERT INTO webhook_configs (name, url, type, events, enabled, created_at)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(body.name.trim())
    .bind(url.as_str())
    .bind(&body.webhook_type)
    .bind(&events_str)
    .bind(body.enabled)
    .bind(ts)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::Database)?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "webhook.create",
        Some("webhook"),
        Some(&row.0.to_string()),
        "success",
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "id": row.0 })))
}

/// PATCH /api/webhooks/:id
pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<i64>,
    Json(body): Json<UpdateWebhook>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let current = sqlx::query_as::<_, (String, String, String, String, bool)>(
        "SELECT name, url, type, events, enabled FROM webhook_configs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

    let name = body.name.as_deref().unwrap_or(&current.0).trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadRequest("name required".into()));
    }
    let url = validate_webhook_url(body.url.as_deref().unwrap_or(&current.1))
        .map_err(AppError::BadRequest)?;
    let webhook_type = body
        .webhook_type
        .as_deref()
        .unwrap_or(&current.2)
        .to_owned();
    let valid_types = ["ntfy", "discord", "slack", "generic"];
    if !valid_types.contains(&webhook_type.as_str()) {
        return Err(AppError::BadRequest(
            "type must be ntfy, discord, slack, or generic".into(),
        ));
    }
    let events = match body.events {
        Some(value) => serde_json::to_string(&value)
            .map_err(|_| AppError::BadRequest("invalid events".into()))?,
        None => current.3,
    };
    let enabled = body.enabled.unwrap_or(current.4);

    sqlx::query(
        "UPDATE webhook_configs SET name = ?, url = ?, type = ?, events = ?, enabled = ?
         WHERE id = ?",
    )
    .bind(name)
    .bind(url.as_str())
    .bind(webhook_type)
    .bind(events)
    .bind(enabled)
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;
    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "webhook.update",
        Some("webhook"),
        Some(&id.to_string()),
        "success",
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// DELETE /api/webhooks/:id
pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    sqlx::query("DELETE FROM webhook_configs WHERE id = ?")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "webhook.delete",
        Some("webhook"),
        Some(&id.to_string()),
        "success",
        None,
        None,
    )
    .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/webhooks/:id/test
pub async fn test_webhook(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let row =
        sqlx::query_as::<_, (String, String)>("SELECT url, type FROM webhook_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?
            .ok_or(AppError::NotFound)?;

    let (url, wh_type) = row;
    let result = dispatch_one(
        &url,
        &wh_type,
        "webhook.test",
        "This is a test notification from VoidTower.",
        resolve_hostname,
        safe_ip,
    )
    .await;

    match result {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "error": e }))),
    }
}

// ─── Dispatch logic ───────────────────────────────────────────────────────────

async fn dispatch_one<F, Fut, P>(
    url: &str,
    wh_type: &str,
    event: &str,
    message: &str,
    resolve: F,
    is_safe: P,
) -> std::result::Result<(), String>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = std::result::Result<Vec<SocketAddr>, String>>,
    P: Fn(IpAddr) -> bool + Copy,
{
    let parsed = validate_webhook_url(url)?;
    let resolved = safe_destination(&parsed, resolve, is_safe).await?;
    dispatch_one_with_destination(&parsed, wh_type, event, message, resolved, is_safe).await
}

async fn dispatch_one_with_destination<P>(
    url: &reqwest::Url,
    wh_type: &str,
    event: &str,
    message: &str,
    resolved: Option<SocketAddr>,
    is_safe: P,
) -> std::result::Result<(), String>
where
    P: Fn(IpAddr) -> bool,
{
    let validated_url = validate_webhook_url(url.as_str())?;
    let address = resolved.ok_or_else(invalid_destination)?;
    let expected_port = validated_url
        .port_or_known_default()
        .ok_or_else(invalid_destination)?;
    if address.port() != expected_port || !is_safe(address.ip()) {
        return Err(invalid_destination());
    }
    let host = validated_url.host_str().ok_or_else(invalid_destination)?;
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            attempt.error("redirects disabled")
        }));
    builder = builder.resolve(host, address);
    let client = builder.build().map_err(|_| "delivery failed".to_string())?;

    let title = "VoidTower Alert";

    let resp = match wh_type {
        "ntfy" => client
            .post(validated_url.clone())
            .header("Title", title)
            .header("Priority", "high")
            .body(format!("[{event}] {message}"))
            .send()
            .await
            .map_err(|_| "delivery failed".to_string())?,
        "discord" => {
            let color: u32 = if event.contains("resolved") {
                0x57F287 // green
            } else {
                0xE74C3C // red
            };
            client
                .post(validated_url.clone())
                .json(&serde_json::json!({
                    "embeds": [{
                        "title": format!("{title}: {event}"),
                        "description": message,
                        "color": color,
                    }]
                }))
                .send()
                .await
                .map_err(|_| "delivery failed".to_string())?
        }
        "slack" => client
            .post(validated_url.clone())
            .json(&serde_json::json!({
                "text": format!("*{title}*\n{message}"),
                "blocks": [
                    {
                        "type": "section",
                        "text": {
                            "type": "mrkdwn",
                            "text": format!("*{title}* — `{event}`\n{message}"),
                        }
                    }
                ]
            }))
            .send()
            .await
            .map_err(|_| "delivery failed".to_string())?,
        _ => {
            // generic
            let ts = now();
            client
                .post(validated_url.clone())
                .json(&serde_json::json!({
                    "event":     event,
                    "message":   message,
                    "timestamp": ts,
                }))
                .send()
                .await
                .map_err(|_| "delivery failed".to_string())?
        }
    };

    if resp.status().is_success() {
        Ok(())
    } else {
        Err(format!("HTTP {}", resp.status()))
    }
}

/// Called from the monitoring loop and alert handlers to fire matching webhooks.
pub async fn fire_webhooks(pool: &SqlitePool, event: &str, message: &str) {
    // Query all enabled webhooks and filter by event subscription
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT url, type, events FROM webhook_configs WHERE enabled = 1",
    )
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("fire_webhooks: failed to query configs: {e}");
            return;
        }
    };

    let subscribed: Vec<(String, String)> = rows
        .into_iter()
        .filter(|(_, _, events_json)| {
            serde_json::from_str::<Vec<String>>(events_json)
                .unwrap_or_default()
                .contains(&event.to_string())
        })
        .map(|(url, wh_type, _)| (url, wh_type))
        .collect();

    for (url, wh_type) in subscribed {
        let url_c = url.clone();
        let wh_type_c = wh_type.clone();
        let event_c = event.to_string();
        let message_c = message.to_string();
        tokio::spawn(async move {
            if let Err(e) = dispatch_one(
                &url_c,
                &wh_type_c,
                &event_c,
                &message_c,
                resolve_hostname,
                safe_ip,
            )
            .await
            {
                tracing::warn!(error = %e, "webhook dispatch failed");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        dispatch_one, dispatch_one_with_destination, resolve_hostname, safe_ip,
        select_safe_destination_with, validate_webhook_url,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use serde_json::json;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::{Notify, Semaphore};
    use tower::ServiceExt;

    #[test]
    fn webhook_destinations_reject_private_and_non_http_urls() {
        for url in [
            "http://127.0.0.1/notify",
            "http://localhost/notify",
            "http://[::1]/notify",
            "file:///etc/passwd",
            "https://user:password@example.com/notify",
        ] {
            assert!(
                validate_webhook_url(url).is_err(),
                "accepted unsafe URL: {url}"
            );
        }
    }

    #[test]
    fn webhook_destinations_reject_special_use_and_normalization_bypasses() {
        for url in [
            "http://0.1.2.3/notify",
            "http://224.0.0.1/notify",
            "http://239.255.255.250/notify",
            "http://192.0.2.1/notify",
            "http://[::ffff:7f00:1]/notify",
            "http://[::ffff:10.0.0.1]/notify",
            "http://[::ffff:192.0.2.1]/notify",
            "http://[fec0::1]/notify",
            "http://[2001:db8::1]/notify",
            "http://[2001:2::1]/notify",
            "http://[2001:10::1]/notify",
            "http://[3fff::1]/notify",
            "http://foo.LOCALHOST/notify",
            "http://foo.localhost./notify",
            "http://FOO.INTERNAL./notify",
            "http://foo.local./notify",
            "http://LOCALHOST./notify",
            "http://METADATA./notify",
            "http://metadata.google.internal./notify",
            "https://hooks.example.test../notify",
            "https://hooks.example.test.../notify",
            "http://@example.com/notify",
        ] {
            assert!(
                validate_webhook_url(url).is_err(),
                "accepted unsafe URL: {url}"
            );
        }
    }

    #[test]
    fn mixed_dns_answers_fail_closed_before_pinning() {
        let safe = "93.184.216.34:443".parse().unwrap();
        let unsafe_address = "127.0.0.1:443".parse().unwrap();
        assert!(select_safe_destination_with(&[unsafe_address, safe], safe_ip).is_err());
        assert!(select_safe_destination_with(&[safe, unsafe_address], safe_ip).is_err());
        assert_eq!(
            select_safe_destination_with(&[safe], safe_ip).unwrap(),
            Some(safe)
        );
        assert!(select_safe_destination_with(&[], safe_ip).is_err());
    }

    #[tokio::test]
    async fn hostname_resolution_fails_closed_for_lookup_error_and_timeout_table() {
        enum LookupCase {
            Error,
            Timeout,
        }

        for (label, case) in [
            ("lookup-error", LookupCase::Error),
            ("timeout", LookupCase::Timeout),
        ] {
            let result = match case {
                LookupCase::Error => {
                    super::resolve_hostname_with_timeout(
                        "hooks.example.test".to_string(),
                        443,
                        std::time::Duration::from_millis(10),
                        |_, _| async { Err("resolver failed".to_string()) },
                    )
                    .await
                }
                LookupCase::Timeout => {
                    super::resolve_hostname_with_timeout(
                        "hooks.example.test".to_string(),
                        443,
                        std::time::Duration::from_millis(1),
                        |_, _| async {
                            std::future::pending::<()>().await;
                            Ok(Vec::new())
                        },
                    )
                    .await
                }
            };
            assert_eq!(
                result.unwrap_err(),
                "webhook destination rejected by outbound policy",
                "{label} must fail closed with the bounded destination error"
            );
        }
    }

    #[tokio::test]
    async fn hostname_resolution_bounds_blocking_resources_table() {
        enum LookupCase {
            ResolverError,
            SaturatedBlockingSlot,
        }

        for (label, case) in [
            ("resolver-error", LookupCase::ResolverError),
            ("saturated-blocking-slot", LookupCase::SaturatedBlockingSlot),
        ] {
            match case {
                LookupCase::ResolverError => {
                    let limit = Arc::new(Semaphore::new(1));
                    let result = super::resolve_hostname_with_timeout(
                        "hooks.example.test".to_string(),
                        443,
                        std::time::Duration::from_secs(1),
                        move |host, port| {
                            let limit = Arc::clone(&limit);
                            async move {
                                super::resolve_hostname_with_blocking_limit(
                                    host,
                                    port,
                                    limit,
                                    |_, _| {
                                        Err::<Vec<std::net::SocketAddr>, _>(
                                            "resolver failed".into(),
                                        )
                                    },
                                )
                                .await
                            }
                        },
                    )
                    .await;
                    assert_eq!(
                        result.unwrap_err(),
                        "webhook destination rejected by outbound policy",
                        "{label} must fail closed"
                    );
                }
                LookupCase::SaturatedBlockingSlot => {
                    let limit = Arc::new(Semaphore::new(1));
                    let started = Arc::new(Notify::new());
                    let second_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let release = Arc::new(std::sync::Barrier::new(2));
                    let first_limit = Arc::clone(&limit);
                    let first_started = Arc::clone(&started);
                    let first_release = Arc::clone(&release);
                    let first = tokio::spawn(async move {
                        super::resolve_hostname_with_timeout(
                            "hooks.example.test".to_string(),
                            443,
                            std::time::Duration::from_secs(1),
                            move |host, port| {
                                let limit = Arc::clone(&first_limit);
                                async move {
                                    super::resolve_hostname_with_blocking_limit(
                                        host,
                                        port,
                                        limit,
                                        move |_, _| {
                                            first_started.notify_one();
                                            first_release.wait();
                                            Err::<Vec<std::net::SocketAddr>, _>(
                                                "resolver stalled".into(),
                                            )
                                        },
                                    )
                                    .await
                                }
                            },
                        )
                        .await
                    });
                    started.notified().await;

                    let second_started_for_resolver = Arc::clone(&second_started);
                    let second = super::resolve_hostname_with_blocking_limit(
                        "hooks.example.test".to_string(),
                        443,
                        limit,
                        move |_, _| {
                            second_started_for_resolver
                                .store(true, std::sync::atomic::Ordering::SeqCst);
                            Ok(Vec::new())
                        },
                    );
                    let second_result =
                        tokio::time::timeout(std::time::Duration::from_secs(1), second).await;
                    release.wait();

                    let second = second_result
                        .expect("{label} must reject without waiting for a stalled worker")
                        .unwrap_err();
                    assert_eq!(second, "webhook destination rejected by outbound policy");
                    assert!(!second_started.load(std::sync::atomic::Ordering::SeqCst));
                    assert_eq!(
                        first.await.unwrap().unwrap_err(),
                        "webhook destination rejected by outbound policy",
                        "{label} first lookup must fail closed after its worker exits"
                    );
                }
            }
        }
    }

    #[test]
    fn webhook_destinations_accept_external_http_urls() {
        let parsed = validate_webhook_url("https://hooks.example.test/notify").unwrap();
        assert_eq!(parsed.scheme(), "https");
        assert_eq!(parsed.host_str(), Some("hooks.example.test"));

        let normalized = validate_webhook_url("https://Hooks.Example.TEST./notify").unwrap();
        assert_eq!(normalized.host_str(), Some("hooks.example.test"));
    }

    #[tokio::test]
    async fn raw_dispatch_uses_explicit_url_port_for_canonical_host_table() {
        for (label, raw_host) in [
            ("trailing-dot", "Hooks.Example.TEST."),
            ("canonical", "hooks.example.test"),
        ] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
            let address = listener.local_addr().unwrap();
            let raw_url = format!("http://{raw_host}:{}/notify", address.port());
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0_u8; 1_024];
                    let read = stream.read(&mut buffer).await.unwrap();
                    assert!(read > 0, "request ended before its headers");
                    request.extend_from_slice(&buffer[..read]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .await
                    .unwrap();
                String::from_utf8(request).unwrap()
            });

            // The fixture is loopback-only, so only this test injects its policy predicate;
            // production test-delivery and alert paths always pass `safe_ip`.
            dispatch_one(
                &raw_url,
                "generic",
                "webhook.test",
                "message",
                move |host, port| {
                    let expected_port = address.port();
                    async move {
                        assert_eq!(host, "hooks.example.test", "{label} host");
                        assert_eq!(port, expected_port, "{label} port");
                        Ok(vec![address])
                    }
                },
                |_| true,
            )
            .await
            .unwrap();

            let request = server.await.unwrap();
            assert!(request.starts_with("POST /notify HTTP/1.1\r\n"));
            assert!(request
                .to_ascii_lowercase()
                .contains(&format!("host: hooks.example.test:{}\r\n", address.port())));
        }
    }

    #[tokio::test]
    async fn destination_pin_is_present_safe_and_port_matched_table() {
        for (label, raw_url, resolved, safe) in [
            (
                "missing",
                "http://hooks.example.test:443/notify",
                None,
                true,
            ),
            (
                "unsafe",
                "http://hooks.example.test:443/notify",
                Some("127.0.0.1:443"),
                false,
            ),
            (
                "wrong-port",
                "http://hooks.example.test:443/notify",
                Some("93.184.216.34:80"),
                true,
            ),
        ] {
            let parsed = reqwest::Url::parse(raw_url).unwrap();
            let resolved = resolved.map(|address| address.parse().unwrap());
            let error = dispatch_one_with_destination(
                &parsed,
                "generic",
                "webhook.test",
                "message",
                resolved,
                move |_| safe,
            )
            .await
            .unwrap_err();
            assert_eq!(
                error, "webhook destination rejected by outbound policy",
                "{label} destination must fail closed before dispatch"
            );
        }
    }

    #[tokio::test]
    async fn final_dispatch_rejects_unvalidated_urls_before_destination_policy_table() {
        for raw_url in [
            "http://hooks..example.test:443/notify",
            "http://user@hooks.example.test:443/notify",
            "http://hooks.example.test:443/notify#fragment",
        ] {
            let parsed = reqwest::Url::parse(raw_url).unwrap();
            let task = tokio::spawn(async move {
                dispatch_one_with_destination(
                    &parsed,
                    "generic",
                    "webhook.test",
                    "message",
                    Some("93.184.216.34:443".parse().unwrap()),
                    |_| panic!("destination policy must not run for an invalid URL"),
                )
                .await
            });
            let result = task.await.expect("invalid URL must not panic");
            assert_eq!(
                result.unwrap_err(),
                "webhook destination rejected by outbound policy",
                "{raw_url} must fail closed before destination evaluation"
            );
        }
    }

    #[tokio::test]
    async fn create_rejects_private_destination_before_persisting() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool.clone()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/webhooks")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "internal",
                            "url": "http://127.0.0.1/notify",
                            "type": "generic",
                            "events": ["alert.created"]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "bad_request");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM webhook_configs")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn update_validates_before_mutating_any_webhook_fields() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        sqlx::query(
            "INSERT INTO webhook_configs (name, url, type, events, enabled, created_at)
             VALUES ('original', 'https://hooks.example.test/notify', 'generic', '[\"alert.created\"]', 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let app = crate::api::router(crate::api::mcp::test_support::build(pool.clone()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/webhooks/1")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "name": "must-not-persist",
                            "url": "http://169.254.169.254/latest"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let row = sqlx::query_as::<_, (String, String)>(
            "SELECT name, url FROM webhook_configs WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "original".to_string(),
                "https://hooks.example.test/notify".to_string()
            )
        );
    }

    #[tokio::test]
    async fn dispatch_rejects_private_destination_without_network_access() {
        let error = dispatch_one(
            "http://169.254.169.254/latest",
            "generic",
            "webhook.test",
            "message",
            resolve_hostname,
            safe_ip,
        )
        .await
        .unwrap_err();
        assert_eq!(error, "webhook destination rejected by outbound policy");
    }

    #[tokio::test]
    async fn test_delivery_rechecks_legacy_private_destination_and_redacts_error() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        sqlx::query(
            "INSERT INTO webhook_configs (name, url, type, events, enabled, created_at)
             VALUES ('legacy', 'http://127.0.0.1/internal', 'generic', '[\"alert.created\"]', 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/webhooks/1/test")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["ok"], false);
        assert_eq!(
            payload["error"],
            "webhook destination rejected by outbound policy"
        );
        assert!(!payload.to_string().contains("127.0.0.1"));
    }
}
