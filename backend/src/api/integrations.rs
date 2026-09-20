use crate::{
    audit, auth,
    error::{AppError, Result},
    operations::{events::sse_frame_fits, invocation::CredentialContext},
    services::ServiceAction,
    voidwatch, AppState,
};
use axum::{
    body::to_bytes,
    extract::{Extension, Path, Query, State},
    http::HeaderMap,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

async fn send_bounded_legacy_event(
    tx: &tokio::sync::mpsc::Sender<Event>,
    event_name: &str,
    data: String,
) -> bool {
    if !sse_frame_fits(event_name, data.len(), None) {
        return false;
    }
    tx.send(Event::default().event(event_name).data(data))
        .await
        .is_ok()
}

// ---------------------------------------------------------------------------
// Scope definitions
// ---------------------------------------------------------------------------

pub const ALL_SCOPES: &[(&str, &str)] = &[
    ("metrics:read", "Read CPU, RAM, disk and network metrics"),
    ("services:read", "List systemd services and their state"),
    (
        "services:restart",
        "Start, stop and restart systemd services",
    ),
    ("containers:read", "List Docker containers and images"),
    (
        "containers:restart",
        "Start, stop and restart Docker containers",
    ),
    ("containers:logs", "Read container log output"),
    ("apps:read", "List deployed App Vault applications"),
    (
        "apps:deploy",
        "Deploy applications from the App Vault catalog",
    ),
    ("apps:restart", "Restart deployed App Vault applications"),
    ("backups:read", "List backup jobs and snapshots"),
    ("backups:run", "Trigger a backup job to run now"),
    ("alerts:read", "List active alerts and status checks"),
    ("alerts:ack", "Acknowledge or resolve alerts"),
    ("automation:read", "List automation jobs and run history"),
    ("automation:run", "Trigger an automation job"),
    ("timeline:read", "Read the audit timeline"),
    ("network:read", "List network interfaces and LAN neighbours"),
    ("files:read", "Browse and read files (read-only)"),
    ("storage:read", "List storage devices and mount points"),
    ("proxy:read", "List nginx reverse proxy rules"),
    ("proxy:manage", "Add, toggle and reload nginx proxy rules"),
    ("diagnostics:read", "Run and read system diagnostics checks"),
    (
        "secrets:list",
        "List secret names and descriptions (values never returned)",
    ),
    ("vms:read", "List KVM and Proxmox virtual machines"),
    (
        "vms:control",
        "Start, stop, reboot and shut down Proxmox guests",
    ),
    ("tags:read", "List resource tags"),
];

/// Coarser, user-facing minting convenience layered on top of fine-grained scope enforcement in
/// `auth::scope_enforce` — each tier is just a fixed subset of `ALL_SCOPES`.
/// `admin-never` is a hard invariant enforced structurally: its scope set is
/// empty, so it can never satisfy a scoped route in `action_registry::ROUTES`,
/// not because minting trusts a self-reported label.
pub const CAPABILITY_TIERS: &[(&str, &[&str])] = &[
    (
        "read",
        &[
            "metrics:read",
            "services:read",
            "containers:read",
            "containers:logs",
            "apps:read",
            "backups:read",
            "alerts:read",
            "automation:read",
            "timeline:read",
            "network:read",
            "files:read",
            "storage:read",
            "proxy:read",
            "diagnostics:read",
            "secrets:list",
            "vms:read",
            "tags:read",
        ],
    ),
    ("deploy", &["apps:read", "apps:deploy", "apps:restart"]),
    (
        "exec",
        &[
            "containers:read",
            "containers:restart",
            "containers:logs",
            "services:read",
            "services:restart",
            "automation:read",
            "automation:run",
        ],
    ),
    ("admin-never", &[]),
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

const WEBHOOK_SOURCE: &str = "odysseus";
const WEBHOOK_TIMESTAMP_HEADER: &str = "X-VoidTower-Timestamp";
const WEBHOOK_NONCE_HEADER: &str = "X-VoidTower-Nonce";
const WEBHOOK_SIGNATURE_HEADER: &str = "X-VoidTower-Signature";
const WEBHOOK_TIMESTAMP_SKEW_SECONDS: u64 = 300;
const WEBHOOK_REPLAY_RETENTION_SECONDS: i64 = 900;

type HmacSha256 = Hmac<Sha256>;

fn webhook_authentication_error() -> AppError {
    AppError::WebhookAuthentication
}

fn parse_signed_webhook_headers(headers: &HeaderMap) -> Result<(i64, String, String)> {
    let timestamp = headers
        .get(WEBHOOK_TIMESTAMP_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(webhook_authentication_error)?;
    if timestamp.abs_diff(unix_now()) > WEBHOOK_TIMESTAMP_SKEW_SECONDS {
        return Err(webhook_authentication_error());
    }

    let nonce = headers
        .get(WEBHOOK_NONCE_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte))
        })
        .map(str::to_owned)
        .ok_or_else(webhook_authentication_error)?;

    let signature = headers
        .get(WEBHOOK_SIGNATURE_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("sha256=") && value.len() == 71)
        .map(str::to_owned)
        .ok_or_else(webhook_authentication_error)?;
    let decoded =
        hex::decode(&signature["sha256=".len()..]).map_err(|_| webhook_authentication_error())?;
    if decoded.len() != 32 {
        return Err(webhook_authentication_error());
    }

    Ok((timestamp, nonce, signature))
}

fn verify_signed_webhook(
    secret: &str,
    timestamp: i64,
    nonce: &str,
    signature: &str,
    body: &[u8],
) -> Result<()> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| webhook_authentication_error())?;
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(nonce.as_bytes());
    mac.update(b".");
    mac.update(body);
    let provided =
        hex::decode(&signature["sha256=".len()..]).map_err(|_| webhook_authentication_error())?;
    mac.verify_slice(&provided)
        .map_err(|_| webhook_authentication_error())
}

async fn claim_webhook_replay(
    db: &sqlx::SqlitePool,
    timestamp: i64,
    nonce: &str,
    signature: &str,
) -> Result<()> {
    let now = unix_now();
    let mut transaction = db.begin().await?;
    sqlx::query("DELETE FROM webhook_replay_receipts WHERE source_id = ? AND created_at < ?")
        .bind(WEBHOOK_SOURCE)
        .bind(now - WEBHOOK_REPLAY_RETENTION_SECONDS)
        .execute(&mut *transaction)
        .await?;
    let inserted = sqlx::query(
        "INSERT INTO webhook_replay_receipts (source_id, nonce, timestamp, signature, created_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(source_id, nonce) DO NOTHING",
    )
    .bind(WEBHOOK_SOURCE)
    .bind(nonce)
    .bind(timestamp)
    .bind(signature)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    if inserted.rows_affected() != 1 {
        return Err(AppError::WebhookReplay);
    }
    transaction.commit().await?;
    Ok(())
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

async fn get_setting_checked(state: &AppState, key: &str) -> Result<Option<String>> {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)
}

async fn emergency_disabled(state: &AppState) -> Result<bool> {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = 'odysseus.emergency_disabled'",
    )
    .fetch_optional(&state.db)
    .await
    .map(|value| value.as_deref() == Some("true"))
    .map_err(|error| AppError::Internal(error.into()))
}

async fn set_setting(state: &AppState, key: &str, value: &str) -> Result<()> {
    let now = unix_now();
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(now)
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(AppError::Database)
}

async fn webhook_secret_metadata(state: &AppState) -> (Option<String>, &'static str) {
    let secret_id = get_setting(
        state,
        crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING,
    )
    .await;
    if secret_id.is_empty() {
        return (None, "");
    }
    let disabled = sqlx::query_scalar::<_, bool>("SELECT disabled FROM secrets WHERE id = ?")
        .bind(&secret_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let hint = match disabled {
        Some(true) => "disabled",
        Some(false) => "configured",
        None => "unavailable",
    };
    (Some(secret_id), hint)
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
    pub secret_ids: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct CreateTokenReq {
    pub name: String,
    /// Explicit scope list — the original minting path, still supported
    /// unchanged for existing integrations. Ignored when `tier` is set.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Capability-tier minting convenience (see `CAPABILITY_TIERS`): one of
    /// "read", "deploy", "exec", "admin-never". When set, this replaces
    /// `scopes` with the tier's fixed scope subset server-side — the caller
    /// cannot widen a tier by also passing `scopes`.
    #[serde(default)]
    pub tier: Option<String>,
    pub expires_days: Option<i64>,
    /// When set, this token can only access the listed secret IDs. None = unrestricted.
    pub secret_ids: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct CreateTokenResp {
    pub id: String,
    pub token: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub secret_ids: Option<Vec<String>>,
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
        secret_ids: Option<String>,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT id, name, scopes, last_used_at, expires_at, created_at, secret_ids
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
            secret_ids: r
                .secret_ids
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok()),
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
    // Capability tier replaces the explicit `scopes` list server-side — a
    // caller can't request "exec" and also smuggle in extra scopes via the
    // `scopes` field, since that field is simply not consulted here.
    let scopes: Vec<String> = if let Some(tier) = &req.tier {
        let (_, tier_scopes) = CAPABILITY_TIERS
            .iter()
            .find(|(name, _)| name == tier)
            .ok_or_else(|| AppError::BadRequest(format!("Unknown capability tier: {tier}")))?;
        tier_scopes.iter().map(|s| s.to_string()).collect()
    } else {
        if req.scopes.is_empty() {
            return Err(AppError::BadRequest(
                "At least one scope is required".into(),
            ));
        }
        req.scopes.clone()
    };

    let valid: std::collections::HashSet<&str> = ALL_SCOPES.iter().map(|(s, _)| *s).collect();
    for scope in &scopes {
        if !valid.contains(scope.as_str()) {
            return Err(AppError::BadRequest(format!("Unknown scope: {scope}")));
        }
    }

    let raw_token = generate_api_token();
    let token_hash = sha256_hex(&raw_token);
    let id = Uuid::new_v4().to_string();
    let now = unix_now();
    let expires_at = req.expires_days.map(|d| now + d * 86400);
    let scopes_json = serde_json::to_string(&scopes).unwrap_or_default();
    let secret_ids_json = req
        .secret_ids
        .as_ref()
        .map(|ids| serde_json::to_string(ids).unwrap_or_default());

    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, expires_at, created_at, secret_ids)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.name)
    .bind(&token_hash)
    .bind(&scopes_json)
    .bind(expires_at)
    .bind(now)
    .bind(&secret_ids_json)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let secret_ids_detail = secret_ids_json
        .as_deref()
        .map(|s| format!(", secret_ids={s}"))
        .unwrap_or_default();
    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "integrations.token.created",
        Some("api_token"),
        Some(&id),
        "success",
        None,
        Some(&format!(
            "name={}, scopes={}{}",
            req.name, scopes_json, secret_ids_detail
        )),
    )
    .await;

    Ok(Json(CreateTokenResp {
        id,
        token: raw_token,
        name: req.name,
        scopes,
        created_at: now,
        secret_ids: req.secret_ids,
    }))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(token_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    // Hold the same lifecycle guard used by Bearer resolution across deletion and cache removal.
    // This prevents a concurrent cache hit or miss from surviving the revoke response.
    let _token_session_guard = state.token_session_lock.lock().await;

    let deleted = sqlx::query("DELETE FROM api_tokens WHERE id = ?")
        .bind(&token_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .rows_affected();

    if deleted == 0 {
        return Err(AppError::NotFound);
    }

    state
        .token_sessions
        .write()
        .await
        .retain(|_, cached| cached.token.token_id != token_id);

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
    /// Legacy plaintext credential input is recognized only to reject it after
    /// authentication; it is never persisted or returned.
    pub webhook_secret: Option<String>,
    pub regenerate_webhook_secret: Option<bool>,
    pub revoke_webhook_secret: Option<bool>,
    pub emergency_disable: Option<bool>,
}

pub async fn get_config(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Result<Json<OdysseusConfig>> {
    require_admin(&state, &jar).await?;

    let enabled = get_setting(&state, "odysseus.enabled").await == "true";
    let mcp_enabled = get_setting(&state, "odysseus.mcp_enabled").await == "true";
    let raw_url = get_setting(&state, "odysseus.allowed_url").await;
    // Rewrite localhost/127.0.0.1 to the server's LAN IP so that browsers
    // accessing VoidTower remotely can reach Odysseus via the iframe.
    let server_ip = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|h| h.rsplit_once(':').map(|(ip, _)| ip).unwrap_or(h))
        .unwrap_or("localhost");
    let allowed_url = if server_ip != "localhost" && server_ip != "127.0.0.1" {
        raw_url
            .replace("//localhost:", &format!("//{}:", server_ip))
            .replace("//127.0.0.1:", &format!("//{}:", server_ip))
    } else {
        raw_url
    };
    let emergency_disabled = get_setting(&state, "odysseus.emergency_disabled").await == "true";
    let (_, webhook_secret_hint) = webhook_secret_metadata(&state).await;

    Ok(Json(OdysseusConfig {
        enabled,
        mcp_enabled,
        allowed_url,
        webhook_secret_hint: webhook_secret_hint.to_string(),
        emergency_disabled,
    }))
}

pub async fn save_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SaveConfigReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if req.webhook_secret.is_some() {
        return Err(AppError::BadRequest(
            "plaintext webhook credentials are not accepted; regenerate the secret instead".into(),
        ));
    }
    if req.regenerate_webhook_secret == Some(true) && req.revoke_webhook_secret == Some(true) {
        return Err(AppError::BadRequest(
            "regenerate_webhook_secret and revoke_webhook_secret cannot be combined".into(),
        ));
    }

    if let Some(e) = req.enabled {
        set_setting(&state, "odysseus.enabled", if e { "true" } else { "false" }).await?;
    }
    if let Some(e) = req.mcp_enabled {
        set_setting(
            &state,
            "odysseus.mcp_enabled",
            if e { "true" } else { "false" },
        )
        .await?;
    }
    if let Some(url) = &req.allowed_url {
        set_setting(&state, "odysseus.allowed_url", url).await?;
    }
    let mut new_webhook_secret: Option<String> = None;
    if req.regenerate_webhook_secret == Some(true) {
        let secret = generate_webhook_secret();
        let encrypted = crate::api::secrets::encrypt(&state.secrets_key, &secret)
            .map_err(AppError::Internal)?;
        let now = unix_now();
        let mut tx = state.db.begin().await.map_err(AppError::Database)?;
        let current_id: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
                .bind(crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::Database)?;
        if let Some(secret_id) = current_id {
            let updated = sqlx::query(
                "UPDATE secrets SET value_enc = ?, version = version + 1, disabled = 0, updated_at = ? WHERE id = ?",
            )
            .bind(&encrypted)
            .bind(now)
            .bind(&secret_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
            if updated.rows_affected() == 0 {
                let new_id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at)
                     VALUES (?, 'odysseus-webhook', 'Odysseus inbound webhook credential', ?, ?, ?)",
                )
                .bind(&new_id)
                .bind(&encrypted)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(AppError::Database)?;
                sqlx::query("UPDATE settings SET value = ?, updated_at = ? WHERE key = ?")
                    .bind(new_id)
                    .bind(now)
                    .bind(crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::Database)?;
            }
        } else {
            let secret_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at)
                 VALUES (?, 'odysseus-webhook', 'Odysseus inbound webhook credential', ?, ?, ?)",
            )
            .bind(&secret_id)
            .bind(&encrypted)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
            sqlx::query(
                "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            )
            .bind(crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING)
            .bind(secret_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
        }
        sqlx::query("DELETE FROM settings WHERE key = 'odysseus.webhook_secret'")
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
        tx.commit().await.map_err(AppError::Database)?;
        new_webhook_secret = Some(secret);
    }
    if req.revoke_webhook_secret == Some(true) {
        let secret_id = get_setting_checked(
            &state,
            crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING,
        )
        .await?
        .unwrap_or_default();
        if !secret_id.is_empty() {
            sqlx::query("UPDATE secrets SET disabled = 1, updated_at = ? WHERE id = ?")
                .bind(unix_now())
                .bind(secret_id)
                .execute(&state.db)
                .await
                .map_err(AppError::Database)?;
        }
    }
    if let Some(disable) = req.emergency_disable {
        set_setting(
            &state,
            "odysseus.emergency_disabled",
            if disable { "true" } else { "false" },
        )
        .await?;
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
// Item #7C: Odysseus context redaction
// ---------------------------------------------------------------------------

/// Strip any secret-related fields before serialising a context payload
/// sent to Odysseus (manifest, SSE events, webhook responses).
/// This ensures secret names/IDs never leak into Odysseus-visible responses.
#[allow(dead_code)]
pub fn redact_secrets(mut value: serde_json::Value) -> serde_json::Value {
    redact_value(&mut value);
    value
}

fn redact_value(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            // Remove any key that looks like it refers to secrets
            let secret_keys: Vec<String> = map
                .keys()
                .filter(|k| {
                    let k = k.to_lowercase();
                    k.contains("secret") || k == "secret_ids"
                })
                .cloned()
                .collect();
            for k in secret_keys {
                map.remove(&k);
            }
            for v in map.values_mut() {
                redact_value(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                redact_value(v);
            }
        }
        _ => {}
    }
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
            "auth": "Authorization: Bearer <api_token>",
            "required_scope": "alerts:read",
            "cursor": "after=<sequence> or Last-Event-ID",
            "events": ["stream.ready", "durable_event", "stream.gap"],
            "legacy_url": "/api/integrations/events/legacy"
        },
        "webhook": {
            "url": "/api/integrations/webhooks",
            "auth": "X-VoidTower-Timestamp, X-VoidTower-Nonce, X-VoidTower-Signature: sha256=<hmac>",
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

pub async fn legacy_event_stream(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(_q): Query<StreamQuery>,
    headers: HeaderMap,
    token_context: Option<Extension<super::bearer_auth::AuthenticatedApiToken>>,
) -> Result<
    Sse<impl futures_util::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    // A middleware-authenticated Bearer token carries a temporary cookie, so its extension must
    // be checked before genuine browser-session precedence.
    let (authed, token_backed) = if token_context.is_some() {
        let raw = headers
            .get("Authorization")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "))
            .unwrap_or("");
        (
            auth::validate_api_token(&state.db, raw, "alerts:read")
                .await
                .is_ok(),
            true,
        )
    } else if let Some(sid) = jar.get("vt_session").map(|c| c.value().to_string()) {
        match auth::validate_session(&state.db, &sid).await {
            Ok(Some(user)) => {
                super::role_guard::require_operator(&user)?;
                (true, false)
            }
            _ => (false, false),
        }
    } else if let Some(hdr) = headers.get("Authorization") {
        let raw = hdr
            .to_str()
            .unwrap_or("")
            .strip_prefix("Bearer ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("")
            .to_string();
        (
            auth::validate_api_token(&state.db, &raw, "alerts:read")
                .await
                .is_ok(),
            true,
        )
    } else {
        (false, false)
    };

    if !authed {
        return Err(AppError::Unauthorized);
    }

    if _q.token.is_some() {
        return Err(AppError::BadRequest(
            "query-string API tokens are not supported; use Authorization: Bearer".into(),
        ));
    }

    // Check emergency disable
    if token_backed && emergency_disabled(&state).await? {
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
                            if !send_bounded_legacy_event(&tx, "metrics", data.to_string()).await {
                                break;
                            }
                            if snap.cpu_usage > 90.0 {
                                let alert = serde_json::json!({
                                    "type": "threshold", "metric": "cpu",
                                    "value": snap.cpu_usage, "threshold": 90,
                                    "message": format!("CPU at {:.0}%", snap.cpu_usage),
                                });
                                if !send_bounded_legacy_event(&tx, "alert", alert.to_string()).await { break; }
                            }
                            let ram_pct = (snap.ram_used * 100).checked_div(snap.ram_total).unwrap_or(0);
                            if ram_pct > 90 {
                                let alert = serde_json::json!({
                                    "type": "threshold", "metric": "ram",
                                    "value": ram_pct, "threshold": 90,
                                    "message": format!("RAM at {}%", ram_pct),
                                });
                                if !send_bounded_legacy_event(&tx, "alert", alert.to_string()).await { break; }
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
                            // Item #7C: never leak secret-related audit events to Odysseus
                            if resource_type.as_deref() == Some("secret") {
                                continue;
                            }
                            let ev = serde_json::json!({
                                "type": "audit", "id": id, "action": action,
                                "resource_type": resource_type, "outcome": outcome, "timestamp": ts,
                            });
                            if !send_bounded_legacy_event(&tx, "audit", ev.to_string()).await { break; }
                        }
                    }
                    last_audit_ts = new_ts;
                    let _ = send_bounded_legacy_event(&tx, "ping", unix_now().to_string()).await;
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WebhookReq {
    pub automation_id: Option<String>,
    /// Structured action: "container.restart" | "container.start" | "container.stop"
    ///                   | "service.restart" | "service.start" | "service.stop"
    pub action: Option<String>,
    /// Resource ID — container ID or service name depending on action
    pub resource_id: Option<String>,
    pub dry_run: Option<bool>,
}

fn validate_webhook_request(req: &WebhookReq) -> Result<()> {
    let has_automation = req.automation_id.is_some();
    let has_action = req.action.is_some();
    if has_automation == has_action {
        return Err(AppError::BadRequest(
            "exactly one of automation_id or action is required".into(),
        ));
    }
    if let Some(automation_id) = &req.automation_id {
        if automation_id.trim().is_empty() || automation_id.chars().count() > 200 {
            return Err(AppError::BadRequest("invalid automation_id".into()));
        }
        if req.resource_id.is_some() {
            return Err(AppError::BadRequest(
                "resource_id is only valid with action".into(),
            ));
        }
    }
    if let Some(action) = &req.action {
        if action.chars().count() > 100 || action.trim().is_empty() {
            return Err(AppError::BadRequest("invalid action".into()));
        }
        let resource_id = req
            .resource_id
            .as_deref()
            .filter(|value| !value.trim().is_empty() && value.chars().count() <= 200)
            .ok_or_else(|| AppError::BadRequest("resource_id required".into()))?;
        let _ = resource_id;
    }
    Ok(())
}

/// Legacy automation and service webhook mutations cannot park a verdict for later approval
/// or satisfy a required snapshot, so every non-`Allow` verdict blocks. Container webhook
/// actions use the canonical durable-operation path instead and do not call this helper.
fn verdict_block_reason(verdict: &voidwatch::Verdict) -> Option<&str> {
    match verdict {
        voidwatch::Verdict::Allow => None,
        voidwatch::Verdict::Deny(reason)
        | voidwatch::Verdict::RequireApproval(reason)
        | voidwatch::Verdict::AllowRequireSnapshot(reason) => Some(reason),
    }
}

#[cfg(test)]
async fn run_automation_job(
    db: &sqlx::SqlitePool,
    automation_id: &str,
    _job: (String, String, i64),
    _dry_run: bool,
) -> Result<()> {
    let verdict = voidwatch::evaluate(
        db,
        voidwatch::Actor {
            kind: voidwatch::ActorKind::Automation,
        },
        voidwatch::ActionKind::Mutating,
        "automation.run",
        voidwatch::Resource {
            resource_type: "automation_job",
            resource_id: automation_id,
        },
    )
    .await;
    if let Some(reason) = verdict_block_reason(&verdict) {
        return Err(AppError::PolicyDenied(reason.to_string()));
    }
    Ok(())
}

pub async fn webhook(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> super::operation_adoption::CompatibilityResult<Response> {
    let headers = request.headers().clone();
    if get_setting(&state, "odysseus.enabled").await != "true" {
        return Err(
            AppError::FeatureUnavailable("Odysseus integration is not enabled".into()).into(),
        );
    }
    if get_setting(&state, "odysseus.emergency_disabled").await == "true" {
        return Err(AppError::FeatureUnavailable(
            "Odysseus integration is emergency-disabled".into(),
        )
        .into());
    }

    let (timestamp, nonce, signature) = parse_signed_webhook_headers(&headers)?;
    let secret_id = get_setting(
        &state,
        crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING,
    )
    .await;
    if secret_id.is_empty() {
        return Err(AppError::FeatureUnavailable(
            "Webhook secret not configured — generate one in Settings → Integrations".into(),
        )
        .into());
    }
    let expected_secret = crate::api::secrets::resolve(
        &state.db,
        &state.secrets_key,
        &secret_id,
        crate::api::secrets::ODYSSEUS_WEBHOOK_PURPOSE,
    )
    .await
    .map_err(|_| AppError::FeatureUnavailable("Webhook secret unavailable".into()))?;

    let content_type = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| value.eq_ignore_ascii_case("application/json"));
    if content_type.is_none() {
        return Err(AppError::RequestBody {
            status: axum::http::StatusCode::UNSUPPORTED_MEDIA_TYPE,
        }
        .into());
    }
    let body = to_bytes(request.into_body(), 64 * 1024)
        .await
        .map_err(|_| {
            super::operation_adoption::CompatibilityError::from(AppError::RequestBody {
                status: axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            })
        })?;
    verify_signed_webhook(&expected_secret, timestamp, &nonce, &signature, &body)?;
    claim_webhook_replay(&state.db, timestamp, &nonce, &signature).await?;
    let req: WebhookReq = serde_json::from_slice(&body).map_err(|_| {
        super::operation_adoption::CompatibilityError::from(AppError::RequestBody {
            status: axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        })
    })?;
    validate_webhook_request(&req)?;
    let dry_run = req.dry_run.unwrap_or(false);

    if let Some(ref automation_id) = req.automation_id {
        let credential = CredentialContext::Webhook {
            source_id: "odysseus".into(),
        };
        let resource =
            super::automation::resolve_run_resource(&state, &credential, automation_id).await?;
        let input = serde_json::json!({});
        let idempotency_key = if headers.get("Idempotency-Key").is_some() {
            super::operation_adoption::idempotency_key(&headers)?
        } else {
            format!(
                "webhook-{}",
                sha256_hex(&serde_json::to_string(&req).unwrap_or_default())
            )
        };
        if dry_run {
            let prepared = super::operation_adoption::prepare(
                &state,
                &credential,
                &resource.id,
                "automation.run",
                input,
            )
            .await?;
            let view = prepared.view();
            return Ok(Json(serde_json::json!({
                "dry_run": true,
                "plan": view.operation,
                "policy": view.policy,
                "resource": view.resource,
            }))
            .into_response());
        }
        return super::operation_adoption::submit_with_key(
            &state,
            &credential,
            &resource.id,
            "automation.run",
            input,
            &idempotency_key,
        )
        .await;
    }

    // ── Structured resource actions ──────────────────────────────────────────
    if let Some(action_str) = req.action {
        let resource_id = req
            .resource_id
            .ok_or_else(|| AppError::BadRequest("resource_id required".into()))?;

        let (resource_type, action_name, container_action, service_action) = match action_str
            .as_str()
        {
            "container.restart" => ("container", "container.restart", true, None),
            "container.start" => ("container", "container.start", true, None),
            "container.stop" => ("container", "container.stop", true, None),
            "service.restart" => ("service", "restart", false, Some(ServiceAction::Restart)),
            "service.start" => ("service", "start", false, Some(ServiceAction::Start)),
            "service.stop" => ("service", "stop", false, Some(ServiceAction::Stop)),
            other => return Err(AppError::BadRequest(format!("Unknown action: {}", other)).into()),
        };

        if container_action {
            let credential = CredentialContext::Webhook {
                source_id: "odysseus".into(),
            };
            let resource = super::containers::resolve_action_resource(
                &state,
                &credential,
                &resource_id,
                action_name,
            )
            .await?;
            let input = serde_json::json!({});
            if dry_run {
                let prepared = super::operation_adoption::prepare(
                    &state,
                    &credential,
                    &resource.id,
                    action_name,
                    input,
                )
                .await?;
                let view = prepared.view();
                return Ok(Json(serde_json::json!({
                    "dry_run": true,
                    "plan": view.operation,
                    "policy": view.policy,
                    "resource": view.resource,
                }))
                .into_response());
            }
            return super::operation_adoption::submit(
                &state,
                &credential,
                &resource.id,
                action_name,
                input,
                &headers,
            )
            .await;
        }

        // Policy check — actor_type "automation" for webhook-sourced actions
        let verdict = voidwatch::evaluate(
            &state.db,
            voidwatch::Actor {
                kind: voidwatch::ActorKind::Automation,
            },
            voidwatch::ActionKind::Mutating,
            action_name,
            voidwatch::Resource {
                resource_type,
                resource_id: &resource_id,
            },
        )
        .await;
        // The remaining structured branch is the explicitly deferred legacy service path.
        if let Some(reason) = verdict_block_reason(&verdict) {
            audit::log_sourced(
                &state.db,
                None,
                "agent",
                &format!("integrations.webhook.{}.{}", resource_type, action_name),
                Some(resource_type),
                Some(&resource_id),
                "blocked",
                None,
                Some(reason),
                Some("odysseus"),
            )
            .await;
            return Err(AppError::PolicyDenied("Webhook action denied by policy".into()).into());
        }

        // Service actions remain unsupported until a canonical operation adapter can
        // resolve the service resource and submit an immutable durable plan. Never
        // execute the legacy systemd helper from this compatibility ingress path.
        if service_action.is_some() {
            return Err(AppError::FeatureUnavailable(
                "service webhook actions require a canonical operation adapter".into(),
            )
            .into());
        }

        audit::log_sourced(
            &state.db,
            None,
            "agent",
            &format!("integrations.webhook.{}.{}", resource_type, action_name),
            Some(resource_type),
            Some(&resource_id),
            if dry_run { "dry_run" } else { "success" },
            None,
            Some(&format!("dry_run={dry_run}")),
            Some("odysseus"),
        )
        .await;

        return Ok(Json(serde_json::json!({
            "ok": true,
            "dry_run": dry_run,
            "action": action_str,
            "resource_id": resource_id,
        }))
        .into_response());
    }

    Ok(Json(serde_json::json!({ "ok": true })).into_response())
}

// ---------------------------------------------------------------------------
// Recent AI-triggered actions (from audit log, actor_type = 'agent')
// ---------------------------------------------------------------------------

pub async fn sync_theme(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let raw_url = get_setting(&state, "odysseus.allowed_url").await;
    if raw_url.is_empty() {
        return Err(AppError::BadRequest("Odysseus URL not configured".into()));
    }
    let base = raw_url.trim_end_matches('/');
    let endpoint = format!("{base}/api/prefs/theme");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;

    let resp = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Odysseus unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "Odysseus returned {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::BadRequest(format!("Invalid response: {e}")))?;

    let name = body
        .get("value")
        .and_then(|v| v.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("dark")
        .to_string();

    Ok(Json(serde_json::json!({ "name": name })))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::operation_adoption::CompatibilityError;
    use axum::http::{HeaderValue, Method, Request};
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
    use tower::ServiceExt;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn odysseus_config_creates_metadata_only_secret_and_revoke_disables_it() {
        let db = setup_db().await;
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "admin").await;
        let app = crate::api::router(crate::api::mcp::test_support::build(db.clone()));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/integrations/odysseus/config")
                    .header("content-type", "application/json")
                    .header("cookie", format!("vt_session={session}"))
                    .body(axum::body::Body::from(
                        r#"{"webhook_secret":"plaintext-must-not-be-accepted"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_client_error());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/integrations/odysseus/config")
                    .header("content-type", "application/json")
                    .header("cookie", format!("vt_session={session}"))
                    .body(axum::body::Body::from(
                        r#"{"regenerate_webhook_secret":true,"revoke_webhook_secret":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(response.status().is_client_error());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/integrations/odysseus/config")
                    .header("content-type", "application/json")
                    .header("cookie", format!("vt_session={session}"))
                    .body(axum::body::Body::from(
                        r#"{"enabled":true,"regenerate_webhook_secret":true}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let response_body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response_json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
        assert_eq!(response_json["ok"], true);
        assert!(response_json["webhook_secret"].as_str().is_some());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/integrations/odysseus/config")
                    .header("cookie", format!("vt_session={session}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let response_body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let response_json: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
        assert!(response_json.get("webhook_secret").is_none());

        let secret_id: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'odysseus.webhook_secret_id'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        let encrypted: String = sqlx::query_scalar("SELECT value_enc FROM secrets WHERE id = ?")
            .bind(&secret_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(!encrypted.is_empty());
        assert!(!encrypted.contains("odysseus"));
        assert!(
            crate::api::secrets::decrypt(&[0u8; 32], &encrypted)
                .unwrap()
                .len()
                >= 32
        );
        let legacy: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'odysseus.webhook_secret'")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert!(legacy.is_none());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/integrations/odysseus/config")
                    .header("content-type", "application/json")
                    .header("cookie", format!("vt_session={session}"))
                    .body(axum::body::Body::from(r#"{"revoke_webhook_secret":true}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let disabled: bool = sqlx::query_scalar("SELECT disabled FROM secrets WHERE id = ?")
            .bind(&secret_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(disabled);
    }

    #[tokio::test]
    async fn vm_control_scope_is_advertised_for_token_minting() {
        let Json(payload) = scopes_list().await;
        let scopes = payload["scopes"].as_array().unwrap();

        assert!(scopes.iter().any(|scope| {
            scope["name"] == "vms:control"
                && scope["description"] == "Start, stop, reboot and shut down Proxmox guests"
        }));
    }

    /// Reproduces the pre-P0-01 bypass: `automation_id`-triggered jobs ran regardless
    /// of any policy rule, because nothing in this path ever consulted `policy_rules`.
    /// Once `run_automation_job` routes through `voidwatch::evaluate`, a matching deny
    /// rule must block the job (dry_run=true so a false pass can't hide behind "it
    /// would have failed to spawn anyway" — a blocked job must return `Err` regardless).
    #[tokio::test]
    async fn integrations_automation_id_path_is_policy_gated() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('deny-automation-run', 'test deny', 'automation', 'automation.run', 'automation_job', NULL, 'deny', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = run_automation_job(
            &pool,
            "job-1",
            ("job-1".to_string(), "echo pwned".to_string(), 5),
            true,
        )
        .await;

        assert!(
            result.is_err(),
            "a matching deny policy rule must block the automation_id-triggered job"
        );
    }

    /// An automation job with no matching policy rule and no
    /// `voidwatch_default_allowlist` entry is now denied by default.
    #[tokio::test]
    async fn integrations_automation_id_path_denies_by_default_when_no_rule_or_allowlist_matches() {
        let pool = setup_db().await;

        let result = run_automation_job(
            &pool,
            "job-1",
            ("job-1".to_string(), "echo hi".to_string(), 5),
            true,
        )
        .await;

        assert!(result.is_err(), "default-deny (P0.2): an unallowlisted automation job must not run absent an allow rule or allowlist entry");
    }

    /// A `voidwatch_default_allowlist` entry grandfathers a pre-existing automation
    /// job's action back to `Allow` — the mechanism the P0.2 upgrade migration
    /// relies on (`db::seed_default_allowlist_if_empty`).
    #[tokio::test]
    async fn integrations_automation_id_path_runs_when_allowlisted() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a1', 'automation', 'automation.run', 'automation_job', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = run_automation_job(
            &pool,
            "job-1",
            ("job-1".to_string(), "echo hi".to_string(), 5),
            true,
        )
        .await;

        assert!(result.is_ok(), "an allowlisted action must still run");
    }

    #[tokio::test]
    async fn service_webhook_mutation_fails_closed_until_canonical_adapter_exists() {
        let pool = setup_db().await;
        let secret_id = uuid::Uuid::new_v4().to_string();
        let encrypted = crate::api::secrets::encrypt(&[0u8; 32], "fixture-secret").unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at)
             VALUES (?, 'test-webhook', 'test webhook credential', ?, 0, 0)",
        )
        .bind(&secret_id)
        .bind(encrypted)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES
             ('odysseus.enabled', 'true', 0),
             (?, ? , 0)",
        )
        .bind(crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING)
        .bind(secret_id)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('allow-service-start', 'test allow', 'automation', 'start', 'service', NULL, 'allow', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let body = serde_json::to_vec(&WebhookReq {
            automation_id: None,
            action: Some("service.start".into()),
            resource_id: Some("fixture.service".into()),
            dry_run: Some(false),
        })
        .unwrap();
        let timestamp = unix_now();
        let nonce = "service-webhook-test";
        let mut mac = HmacSha256::new_from_slice(b"fixture-secret").unwrap();
        mac.update(format!("{timestamp}.{nonce}.").as_bytes());
        mac.update(&body);
        let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        let request = axum::http::Request::builder()
            .method("POST")
            .uri("/api/integrations/webhooks")
            .header("X-VoidTower-Timestamp", timestamp.to_string())
            .header("X-VoidTower-Nonce", nonce)
            .header("X-VoidTower-Signature", signature)
            .header(
                axum::http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        let result = webhook(State(crate::api::mcp::test_support::build(pool)), request).await;

        assert!(
            matches!(
                result,
                Err(CompatibilityError::Legacy(AppError::FeatureUnavailable(ref message)))
                    if message.contains("canonical operation")
            ),
            "service webhook mutation must fail closed instead of executing systemctl: {result:?}"
        );
    }

    /// Deferred legacy webhook mutations still fail closed for every non-`Allow` verdict.
    #[test]
    fn verdict_block_reason_blocks_everything_but_allow() {
        assert_eq!(verdict_block_reason(&voidwatch::Verdict::Allow), None);
        assert_eq!(
            verdict_block_reason(&voidwatch::Verdict::Deny("denied".into())),
            Some("denied")
        );
        assert_eq!(
            verdict_block_reason(&voidwatch::Verdict::RequireApproval(
                "needs approval".into()
            )),
            Some("needs approval")
        );
        assert_eq!(
            verdict_block_reason(&voidwatch::Verdict::AllowRequireSnapshot(
                "needs snapshot".into()
            )),
            Some("needs snapshot"),
            "AllowRequireSnapshot must block here (no snapshot mechanism is wired to this \
             ingress path) — treating it like Allow is the regression this test guards \
             against"
        );
    }
}
