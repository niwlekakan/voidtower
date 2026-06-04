use crate::{
    audit,
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::StreamExt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Scope definitions
// ---------------------------------------------------------------------------

pub const ALL_SCOPES: &[(&str, &str)] = &[
    ("metrics:read",      "Read CPU, RAM, disk and network metrics"),
    ("services:read",     "List systemd services and their state"),
    ("services:restart",  "Start, stop and restart systemd services"),
    ("containers:read",   "List Docker containers and images"),
    ("containers:restart","Start, stop and restart Docker containers"),
    ("containers:logs",   "Read container log output"),
    ("apps:read",         "List deployed App Vault applications"),
    ("apps:deploy",       "Deploy applications from the App Vault catalog"),
    ("apps:restart",      "Restart deployed App Vault applications"),
    ("backups:read",      "List backup jobs and snapshots"),
    ("backups:run",       "Trigger a backup job to run now"),
    ("alerts:read",       "List active alerts and status checks"),
    ("alerts:ack",        "Acknowledge or resolve alerts"),
    ("automation:read",   "List automation jobs and run history"),
    ("automation:run",    "Trigger an automation job"),
    ("timeline:read",     "Read the audit timeline"),
    ("network:read",      "List network interfaces and LAN neighbours"),
    ("files:read",        "Browse and read files (read-only)"),
    ("storage:read",      "List storage devices and mount points"),
    ("proxy:read",        "List nginx reverse proxy rules"),
    ("proxy:manage",      "Add, toggle and reload nginx proxy rules"),
    ("diagnostics:read",  "Run and read system diagnostics checks"),
    ("secrets:list",      "List secret names and descriptions (values never returned)"),
    ("vms:read",          "List KVM and Proxmox virtual machines"),
    ("tags:read",         "List resource tags"),
];

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

pub fn generate_api_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    format!("vt_{}", hex::encode(bytes))
}

fn generate_webhook_secret() -> String {
    let bytes: [u8; 24] = rand::thread_rng().gen();
    hex::encode(bytes)
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

async fn get_setting(state: &AppState, key: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

async fn set_setting(state: &AppState, key: &str, value: &str) {
    let now = unix_now();
    let _ = sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(&state.db)
    .await;
}

// ---------------------------------------------------------------------------
// API token CRUD
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TokenRow {
    pub id: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateTokenReq {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_days: Option<i64>,
}

#[derive(Serialize)]
pub struct CreateTokenResp {
    pub id: String,
    pub token: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
}

pub async fn list_tokens(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        name: String,
        scopes: String,
        last_used_at: Option<i64>,
        expires_at: Option<i64>,
        created_at: i64,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, name, scopes, last_used_at, expires_at, created_at
         FROM api_tokens ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let tokens: Vec<TokenRow> = rows
        .into_iter()
        .map(|r| TokenRow {
            id: r.id,
            name: r.name,
            scopes: serde_json::from_str(&r.scopes).unwrap_or_default(),
            last_used_at: r.last_used_at,
            expires_at: r.expires_at,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(serde_json::json!({ "tokens": tokens })))
}

pub async fn create_token(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateTokenReq>,
) -> Result<Json<CreateTokenResp>> {
    let user = require_admin(&state, &jar).await?;

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Token name is required".into()));
    }
    if req.scopes.is_empty() {
        return Err(AppError::BadRequest("At least one scope is required".into()));
    }

    let valid: std::collections::HashSet<&str> = ALL_SCOPES.iter().map(|(s, _)| *s).collect();
    for scope in &req.scopes {
        if !valid.contains(scope.as_str()) {
            return Err(AppError::BadRequest(format!("Unknown scope: {scope}")));
        }
    }

    let raw_token = generate_api_token();
    let token_hash = sha256_hex(&raw_token);
    let id = Uuid::new_v4().to_string();
    let now = unix_now();
    let expires_at = req.expires_days.map(|d| now + d * 86400);
    let scopes_json = serde_json::to_string(&req.scopes).unwrap_or_default();

    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.name)
    .bind(&token_hash)
    .bind(&scopes_json)
    .bind(expires_at)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "integrations.token.created",
        Some("api_token"),
        Some(&id),
        "success",
        None,
        Some(&format!("name={}, scopes={}", req.name, scopes_json)),
    )
    .await;

    Ok(Json(CreateTokenResp {
        id,
        token: raw_token,
        name: req.name,
        scopes: req.scopes,
        created_at: now,
    }))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let deleted = sqlx::query("DELETE FROM api_tokens WHERE id = ?")
        .bind(&token_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .rows_affected();

    if deleted == 0 {
        return Err(AppError::NotFound);
    }

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "integrations.token.revoked",
        Some("api_token"),
        Some(&token_id),
        "success",
        None,
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Odysseus configuration
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct OdysseusConfig {
    pub enabled: bool,
    pub mcp_enabled: bool,
    pub allowed_url: String,
    pub webhook_secret_hint: String,
    pub emergency_disabled: bool,
}

#[derive(Deserialize)]
pub struct SaveConfigReq {
    pub enabled: Option<bool>,
    pub mcp_enabled: Option<bool>,
    pub allowed_url: Option<String>,
    pub regenerate_webhook_secret: Option<bool>,
    pub emergency_disable: Option<bool>,
}

pub async fn get_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<OdysseusConfig>> {
    require_admin(&state, &jar).await?;

    let enabled = get_setting(&state, "odysseus.enabled").await == "true";
    let mcp_enabled = get_setting(&state, "odysseus.mcp_enabled").await == "true";
    let allowed_url = get_setting(&state, "odysseus.allowed_url").await;
    let emergency_disabled = get_setting(&state, "odysseus.emergency_disabled").await == "true";
    let secret = get_setting(&state, "odysseus.webhook_secret").await;
    let webhook_secret_hint = if secret.len() >= 4 {
        format!("…{}", &secret[secret.len() - 4..])
    } else if !secret.is_empty() {
        "****".to_string()
    } else {
        String::new()
    };

    Ok(Json(OdysseusConfig {
        enabled,
        mcp_enabled,
        allowed_url,
        webhook_secret_hint,
        emergency_disabled,
    }))
}

pub async fn save_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SaveConfigReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if let Some(e) = req.enabled {
        set_setting(&state, "odysseus.enabled", if e { "true" } else { "false" }).await;
    }
    if let Some(e) = req.mcp_enabled {
        set_setting(&state, "odysseus.mcp_enabled", if e { "true" } else { "false" }).await;
    }
    if let Some(url) = &req.allowed_url {
        set_setting(&state, "odysseus.allowed_url", url).await;
    }
    let mut new_webhook_secret: Option<String> = None;
    if req.regenerate_webhook_secret == Some(true) {
        let secret = generate_webhook_secret();
        set_setting(&state, "odysseus.webhook_secret", &secret).await;
        new_webhook_secret = Some(secret);
    }
    if let Some(disable) = req.emergency_disable {
        set_setting(
            &state,
            "odysseus.emergency_disabled",
            if disable { "true" } else { "false" },
        )
        .await;
        audit::log(
            &state.db,
            Some(&user.id),
            "human",
            if disable {
                "integrations.emergency_disable"
            } else {
                "integrations.emergency_reenable"
            },
            Some("integration"),
            Some("odysseus"),
            "success",
            None,
            None,
        )
        .await;
    }

    Ok(Json(serde_json::json!({
        "ok": true,
        "webhook_secret": new_webhook_secret,
    })))
}

// ---------------------------------------------------------------------------
// Tool manifest
// ---------------------------------------------------------------------------

pub async fn manifest(State(state): State<AppState>) -> Json<serde_json::Value> {
    let enabled = get_setting(&state, "odysseus.enabled").await == "true";
    if !enabled {
        return Json(serde_json::json!({
            "voidtower_version": "1.0",
            "integration_enabled": false,
            "tools": []
        }));
    }

    Json(serde_json::json!({
        "voidtower_version": "1.0",
        "integration_enabled": true,
        "auth": {
            "type": "bearer",
            "header": "Authorization",
            "format": "Bearer <api_token>"
        },
        "event_stream": {
            "url": "/api/integrations/events",
            "auth": "?token=<api_token> or Authorization header",
            "required_scope": "alerts:read",
            "events": ["metrics", "alert", "audit", "ping"]
        },
        "webhook": {
            "url": "/api/integrations/webhooks",
            "auth": "Authorization: Bearer <webhook_secret>",
            "description": "POST to trigger VoidTower automations from Odysseus"
        },
        "tools": [
            { "name": "get_metrics", "description": "Current CPU, RAM, disk and network metrics", "required_scope": "metrics:read", "risk": "read-only", "destructive": false, "api": "GET /api/metrics/current", "input": {}, "output": {"cpu_usage": "f32", "ram_used": "u64", "ram_total": "u64"} },
            { "name": "list_services", "description": "List systemd services and their state", "required_scope": "services:read", "risk": "read-only", "destructive": false, "api": "GET /api/services", "input": {}, "output": {"services": "array"} },
            { "name": "restart_service", "description": "Start, stop or restart a systemd service", "required_scope": "services:restart", "risk": "medium-risk", "destructive": false, "requires_confirmation": false, "api": "POST /api/services/:name/action", "input": {"name": "string", "action": "start|stop|restart"}, "output": {"ok": "boolean"} },
            { "name": "list_containers", "description": "List Docker containers", "required_scope": "containers:read", "risk": "read-only", "destructive": false, "api": "GET /api/containers", "input": {}, "output": {"containers": "array"} },
            { "name": "restart_container", "description": "Start, stop or restart a Docker container", "required_scope": "containers:restart", "risk": "medium-risk", "destructive": false, "requires_confirmation": false, "api": "POST /api/containers/:id/action", "input": {"id": "string", "action": "start|stop|restart"}, "output": {"ok": "boolean"} },
            { "name": "get_container_logs", "description": "Get recent logs from a container", "required_scope": "containers:logs", "risk": "read-only", "destructive": false, "api": "GET /api/containers/:id/logs", "input": {"id": "string"}, "output": {"logs": "string"} },
            { "name": "list_alerts", "description": "List active alerts and status check results", "required_scope": "alerts:read", "risk": "read-only", "destructive": false, "api": "GET /api/alerts", "input": {"state": "active|acknowledged|resolved (optional)"}, "output": {"alerts": "array"} },
            { "name": "acknowledge_alert", "description": "Acknowledge an active alert", "required_scope": "alerts:ack", "risk": "low-risk", "destructive": false, "api": "POST /api/alerts/:id/acknowledge", "input": {"id": "string"}, "output": {"ok": "boolean"} },
            { "name": "list_apps", "description": "List deployed App Vault applications", "required_scope": "apps:read", "risk": "read-only", "destructive": false, "api": "GET /api/apps/deployed", "input": {}, "output": {"apps": "array"} },
            { "name": "deploy_app", "description": "Deploy an application from the App Vault catalog", "required_scope": "apps:deploy", "risk": "medium-risk", "destructive": false, "requires_confirmation": true, "api": "POST /api/apps/deploy", "input": {"app_id": "string", "project_name": "string (optional)"}, "output": {"ok": "boolean", "project_name": "string"} },
            { "name": "list_backups", "description": "List backup jobs and their last status", "required_scope": "backups:read", "risk": "read-only", "destructive": false, "api": "GET /api/backups", "input": {}, "output": {"backups": "array"} },
            { "name": "run_backup", "description": "Trigger a backup job to run immediately", "required_scope": "backups:run", "risk": "low-risk", "destructive": false, "api": "POST /api/backups/:id/run", "input": {"id": "string"}, "output": {"ok": "boolean"} },
            { "name": "list_automations", "description": "List scheduled automation jobs", "required_scope": "automation:read", "risk": "read-only", "destructive": false, "api": "GET /api/automation", "input": {}, "output": {"automations": "array"} },
            { "name": "run_automation", "description": "Trigger an automation job to run now", "required_scope": "automation:run", "risk": "medium-risk", "destructive": false, "api": "POST /api/automation/:id/run", "input": {"id": "string"}, "output": {"run_id": "string", "status": "string"} }
        ]
    }))
}

// ---------------------------------------------------------------------------
// Available scopes list (for the UI)
// ---------------------------------------------------------------------------

pub async fn scopes_list() -> Json<serde_json::Value> {
    let scopes: Vec<serde_json::Value> = ALL_SCOPES
        .iter()
        .map(|(name, desc)| serde_json::json!({ "name": name, "description": desc }))
        .collect();
    Json(serde_json::json!({ "scopes": scopes }))
}

// ---------------------------------------------------------------------------
// SSE event stream
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct StreamQuery {
    pub token: Option<String>,
}

pub async fn event_stream(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<
    Sse<impl futures_util::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    // Accept: session cookie, Authorization header, or ?token= query param (SSE can't set headers)
    let authed = if let Some(raw) = q.token {
        auth::validate_api_token(&state.db, &raw, "alerts:read")
            .await
            .is_ok()
    } else if let Some(hdr) = headers.get("Authorization") {
        let raw = hdr
            .to_str()
            .unwrap_or("")
            .trim_start_matches("Bearer ")
            .to_string();
        auth::validate_api_token(&state.db, &raw, "alerts:read")
            .await
            .is_ok()
    } else {
        let sid = jar.get("vt_session").map(|c| c.value().to_string());
        if let Some(sid) = sid {
            auth::validate_session(&state.db, &sid)
                .await
                .map(|u| u.is_some())
                .unwrap_or(false)
        } else {
            false
        }
    };

    if !authed {
        return Err(AppError::Unauthorized);
    }

    // Check emergency disable
    if get_setting(&state, "odysseus.emergency_disabled").await == "true" {
        return Err(AppError::FeatureUnavailable(
            "AI access is emergency-disabled".into(),
        ));
    }

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(64);
    let mut metrics_rx = state.metrics_tx.subscribe();
    let db = state.db.clone();

    tokio::spawn(async move {
        let mut last_audit_ts = unix_now();
        let mut tick = tokio::time::interval(Duration::from_secs(10));

        loop {
            tokio::select! {
                result = metrics_rx.recv() => {
                    match result {
                        Ok(snap) => {
                            let data = serde_json::json!({
                                "type": "metrics",
                                "cpu_usage": snap.cpu_usage,
                                "ram_used": snap.ram_used,
                                "ram_total": snap.ram_total,
                                "timestamp": unix_now(),
                            });
                            if tx.send(Event::default().event("metrics").data(data.to_string())).await.is_err() {
                                break;
                            }
                            if snap.cpu_usage > 90.0 {
                                let alert = serde_json::json!({
                                    "type": "threshold", "metric": "cpu",
                                    "value": snap.cpu_usage, "threshold": 90,
                                    "message": format!("CPU at {:.0}%", snap.cpu_usage),
                                });
                                if tx.send(Event::default().event("alert").data(alert.to_string())).await.is_err() { break; }
                            }
                            let ram_pct = if snap.ram_total > 0 { snap.ram_used * 100 / snap.ram_total } else { 0 };
                            if ram_pct > 90 {
                                let alert = serde_json::json!({
                                    "type": "threshold", "metric": "ram",
                                    "value": ram_pct, "threshold": 90,
                                    "message": format!("RAM at {}%", ram_pct),
                                });
                                if tx.send(Event::default().event("alert").data(alert.to_string())).await.is_err() { break; }
                            }
                        }
                        Err(_) => break,
                    }
                }
                _ = tick.tick() => {
                    let new_ts = unix_now();
                    if let Ok(rows) = sqlx::query_as::<_, (String, String, Option<String>, String, i64)>(
                        "SELECT id, action, resource_type, outcome, timestamp FROM audit_log WHERE timestamp > ? ORDER BY timestamp ASC LIMIT 20"
                    )
                    .bind(last_audit_ts)
                    .fetch_all(&db)
                    .await {
                        for (id, action, resource_type, outcome, ts) in rows {
                            let ev = serde_json::json!({
                                "type": "audit", "id": id, "action": action,
                                "resource_type": resource_type, "outcome": outcome, "timestamp": ts,
                            });
                            if tx.send(Event::default().event("audit").data(ev.to_string())).await.is_err() { break; }
                        }
                    }
                    last_audit_ts = new_ts;
                    let _ = tx.send(Event::default().event("ping").data(unix_now().to_string())).await;
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(Ok);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

// ---------------------------------------------------------------------------
// Webhook receiver (Odysseus → VoidTower)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct WebhookReq {
    pub automation_id: Option<String>,
    pub dry_run: Option<bool>,
}

pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WebhookReq>,
) -> Result<Json<serde_json::Value>> {
    if get_setting(&state, "odysseus.enabled").await != "true" {
        return Err(AppError::FeatureUnavailable(
            "Odysseus integration is not enabled".into(),
        ));
    }
    if get_setting(&state, "odysseus.emergency_disabled").await == "true" {
        return Err(AppError::FeatureUnavailable(
            "Odysseus integration is emergency-disabled".into(),
        ));
    }

    let expected_secret = get_setting(&state, "odysseus.webhook_secret").await;
    if expected_secret.is_empty() {
        return Err(AppError::FeatureUnavailable(
            "Webhook secret not configured — generate one in Settings → Integrations".into(),
        ));
    }

    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim_start_matches("Bearer ")
        .trim();

    // Constant-time comparison via double-hash
    if sha256_hex(provided) != sha256_hex(&expected_secret) {
        return Err(AppError::Unauthorized);
    }

    let dry_run = req.dry_run.unwrap_or(false);

    if let Some(automation_id) = req.automation_id {
        let job = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT id, command, timeout_secs FROM automation_jobs WHERE id = ? AND enabled = 1",
        )
        .bind(&automation_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::NotFound)?;

        audit::log(
            &state.db,
            None,
            "agent",
            "integrations.webhook.automation_trigger",
            Some("automation_job"),
            Some(&automation_id),
            "success",
            None,
            Some(&format!("dry_run={dry_run}")),
        )
        .await;

        if !dry_run {
            let (job_id, command, timeout_secs) = job;
            let db = state.db.clone();
            tokio::spawn(async move {
                let run_id = Uuid::new_v4().to_string();
                let started = unix_now();
                let _ = sqlx::query(
                    "INSERT INTO automation_runs (id, job_id, started_at, status, output) VALUES (?,?,?,'running','')",
                )
                .bind(&run_id)
                .bind(&job_id)
                .bind(started)
                .execute(&db)
                .await;

                let timeout = Duration::from_secs(timeout_secs.max(1) as u64);
                let (status, exit_code, output) = match tokio::time::timeout(timeout, async {
                    tokio::process::Command::new("bash")
                        .arg("-c")
                        .arg(&command)
                        .output()
                        .await
                })
                .await
                {
                    Ok(Ok(out)) => {
                        let code = out.status.code().unwrap_or(-1) as i64;
                        let status = if out.status.success() { "success" } else { "failure" };
                        let output = String::from_utf8_lossy(&out.stdout).to_string()
                            + &String::from_utf8_lossy(&out.stderr);
                        (status.to_string(), Some(code), output)
                    }
                    _ => ("failure".to_string(), Some(-1), "Timeout or exec error".to_string()),
                };

                let finished = unix_now();
                let _ = sqlx::query(
                    "UPDATE automation_runs SET finished_at=?, status=?, exit_code=?, output=? WHERE id=?",
                )
                .bind(finished)
                .bind(&status)
                .bind(exit_code)
                .bind(&output)
                .bind(&run_id)
                .execute(&db)
                .await;

                let _ = sqlx::query(
                    "UPDATE automation_jobs SET last_run_at=?, last_status=?, last_exit_code=?, updated_at=? WHERE id=?",
                )
                .bind(finished)
                .bind(&status)
                .bind(exit_code)
                .bind(finished)
                .bind(&job_id)
                .execute(&db)
                .await;
            });
        }

        return Ok(Json(serde_json::json!({
            "ok": true,
            "dry_run": dry_run,
            "automation_id": automation_id,
        })));
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Recent AI-triggered actions (from audit log, actor_type = 'agent')
// ---------------------------------------------------------------------------

pub async fn recent_actions(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    #[derive(sqlx::FromRow, Serialize)]
    struct Row {
        id: String,
        timestamp: i64,
        action: String,
        resource_type: Option<String>,
        resource_id: Option<String>,
        outcome: String,
        ip_address: Option<String>,
        details: Option<String>,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, timestamp, action, resource_type, resource_id, outcome, ip_address, details
         FROM audit_log WHERE actor_type = 'agent' ORDER BY timestamp DESC LIMIT 50",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(serde_json::json!({ "actions": rows })))
}
