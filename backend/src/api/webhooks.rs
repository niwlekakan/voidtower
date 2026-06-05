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
    if body.url.trim().is_empty() {
        return Err(AppError::BadRequest("url required".into()));
    }
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return Err(AppError::BadRequest(
            "url must start with http:// or https://".into(),
        ));
    }
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
    .bind(body.url.trim())
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
    let ts = now();

    if let Some(v) = &body.name {
        sqlx::query("UPDATE webhook_configs SET name = ? WHERE id = ?")
            .bind(v)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }
    if let Some(v) = &body.url {
        sqlx::query("UPDATE webhook_configs SET url = ? WHERE id = ?")
            .bind(v)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }
    if let Some(v) = &body.webhook_type {
        sqlx::query("UPDATE webhook_configs SET type = ? WHERE id = ?")
            .bind(v)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }
    if let Some(v) = &body.events {
        let s = serde_json::to_string(v)
            .map_err(|e| AppError::BadRequest(format!("invalid events: {e}")))?;
        sqlx::query("UPDATE webhook_configs SET events = ? WHERE id = ?")
            .bind(&s)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }
    if let Some(v) = body.enabled {
        sqlx::query("UPDATE webhook_configs SET enabled = ? WHERE id = ?")
            .bind(v)
            .bind(id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }

    // touch a virtual updated_at by just logging
    let _ = ts;
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

    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT url, type FROM webhook_configs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

    let (url, wh_type) = row;
    let result = dispatch_one(&url, &wh_type, "webhook.test", "This is a test notification from VoidTower.").await;

    match result {
        Ok(_) => Ok(Json(serde_json::json!({ "ok": true }))),
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "error": e }))),
    }
}

// ─── Dispatch logic ───────────────────────────────────────────────────────────

async fn dispatch_one(
    url: &str,
    wh_type: &str,
    event: &str,
    message: &str,
) -> std::result::Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let title = "VoidTower Alert";

    let resp = match wh_type {
        "ntfy" => {
            client
                .post(url)
                .header("Title", title)
                .header("Priority", "high")
                .body(format!("[{event}] {message}"))
                .send()
                .await
                .map_err(|e| e.to_string())?
        }
        "discord" => {
            let color: u32 = if event.contains("resolved") {
                0x57F287 // green
            } else {
                0xE74C3C // red
            };
            client
                .post(url)
                .json(&serde_json::json!({
                    "embeds": [{
                        "title": format!("{title}: {event}"),
                        "description": message,
                        "color": color,
                    }]
                }))
                .send()
                .await
                .map_err(|e| e.to_string())?
        }
        "slack" => {
            client
                .post(url)
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
                .map_err(|e| e.to_string())?
        }
        _ => {
            // generic
            let ts = now();
            client
                .post(url)
                .json(&serde_json::json!({
                    "event":     event,
                    "message":   message,
                    "timestamp": ts,
                }))
                .send()
                .await
                .map_err(|e| e.to_string())?
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
            if let Err(e) = dispatch_one(&url_c, &wh_type_c, &event_c, &message_c).await {
                tracing::warn!("webhook dispatch failed (url={url_c}): {e}");
            }
        });
    }
}
