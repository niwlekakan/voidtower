use crate::{
    audit, auth,
    api::integrations::{generate_api_token, sha256_hex},
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn unix_now() -> i64 {
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
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

const PAIRING_CODE_TTL_SECS: i64 = 900; // 15 minutes

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct PairingCodeResponse {
    pub code: String,
    pub expires_at: i64,
}

#[derive(Deserialize)]
pub struct EnrollRequest {
    pub pairing_code: String,
    pub display_name: String,
    #[serde(default = "default_device_type")]
    pub device_type: String,
    #[serde(default)]
    pub agent_capable: bool,
}
fn default_device_type() -> String {
    "other".to_string()
}

#[derive(Serialize)]
pub struct EnrollResponse {
    pub node_id: String,
    /// Bearer token this node uses for `/api/nodes/:id/heartbeat` — nothing else.
    /// Deliberately NOT an api_tokens-table token: those grant a full admin-equivalent
    /// session via bearer_auth.rs's generic token→session resolution (no per-route scope
    /// enforcement exists there today), which would be wildly over-privileged for a
    /// phone/tablet that only ever needs to post a heartbeat.
    pub heartbeat_token: String,
    pub wg_client_config: String,
    pub warnings: Vec<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct NodeRow {
    pub id: String,
    pub display_name: String,
    pub device_type: String,
    pub owner_user_id: String,
    pub last_seen: Option<i64>,
    pub last_telemetry: Option<String>,
    pub agent_capable: bool,
    pub approved: bool,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    pub battery: Option<f32>,
    pub storage_free_bytes: Option<i64>,
    #[serde(default)]
    pub online: bool,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub async fn create_pairing_code(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<PairingCodeResponse>> {
    let user = require_admin(&state, &jar).await?;

    let raw = generate_api_token();
    let hash = sha256_hex(&raw);
    let id = Uuid::new_v4().to_string();
    let now = unix_now();
    let expires_at = now + PAIRING_CODE_TTL_SECS;

    sqlx::query(
        "INSERT INTO node_pairing_codes (id, token_hash, created_by, expires_at, created_at) VALUES (?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&hash)
    .bind(&user.id)
    .bind(expires_at)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&user.id), "human", "nodes.pairing_code.create",
        Some("node_pairing_code"), Some(&id), "success", None, None,
    ).await;

    Ok(Json(PairingCodeResponse { code: raw, expires_at }))
}

pub async fn enroll(
    State(state): State<AppState>,
    Json(req): Json<EnrollRequest>,
) -> Result<Json<EnrollResponse>> {
    if req.display_name.trim().is_empty() {
        return Err(AppError::BadRequest("display_name is required".into()));
    }
    if !matches!(req.device_type.as_str(), "phone" | "tablet" | "pi" | "other") {
        return Err(AppError::BadRequest("Invalid device_type".into()));
    }

    let now = unix_now();
    let hash = sha256_hex(&req.pairing_code);

    #[derive(sqlx::FromRow)]
    struct PairingRow {
        id: String,
        created_by: String,
        expires_at: i64,
        used_at: Option<i64>,
    }

    let pairing: PairingRow = sqlx::query_as(
        "SELECT id, created_by, expires_at, used_at FROM node_pairing_codes WHERE token_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::Unauthorized)?;

    if pairing.used_at.is_some() || pairing.expires_at < now {
        return Err(AppError::Unauthorized);
    }

    // Atomically claim the code — `used_at IS NULL` in the WHERE means a concurrent
    // second enrollment attempt with the same code affects 0 rows and gets rejected.
    let claimed = sqlx::query(
        "UPDATE node_pairing_codes SET used_at = ? WHERE id = ? AND used_at IS NULL",
    )
    .bind(now)
    .bind(&pairing.id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if claimed.rows_affected() == 0 {
        return Err(AppError::Unauthorized);
    }

    let owner = auth::find_user_by_id(&state.db, &pairing.created_by)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("pairing code owner no longer exists")))?;

    let wg_result = crate::api::wireguard::add_peer_core(
        &state,
        &owner,
        &req.display_name,
        "wg0",
        None,
    )
    .await?;

    let wg_peer_id = wg_result.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let wg_public_key = wg_result.get("public_key").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let client_config = wg_result.get("client_config").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let warnings: Vec<String> = wg_result
        .get("warnings")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    let node_id = Uuid::new_v4().to_string();
    let node_token_raw = generate_api_token();
    let node_token_hash = sha256_hex(&node_token_raw);

    sqlx::query(
        "INSERT INTO nodes (id, display_name, device_type, owner_user_id, wg_peer_id, wg_public_key, token_hash, agent_capable, approved, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&node_id)
    .bind(&req.display_name)
    .bind(&req.device_type)
    .bind(&owner.id)
    .bind(&wg_peer_id)
    .bind(&wg_public_key)
    .bind(&node_token_hash)
    .bind(req.agent_capable)
    .bind(true)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&owner.id), "human", "nodes.enroll",
        Some("node"), Some(&node_id), "success", None,
        Some(&format!("display_name={},device_type={}", req.display_name, req.device_type)),
    ).await;

    Ok(Json(EnrollResponse {
        node_id,
        heartbeat_token: node_token_raw,
        wg_client_config: client_config,
        warnings,
    }))
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let nodes: Vec<NodeRow> = sqlx::query_as(
        "SELECT id, display_name, device_type, owner_user_id, last_seen, last_telemetry, agent_capable, approved, created_at
         FROM nodes ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(serde_json::json!({ "nodes": nodes })))
}

pub async fn delete_node(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(node_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        wg_peer_id: Option<String>,
        display_name: String,
    }

    let row: Row = sqlx::query_as("SELECT wg_peer_id, display_name FROM nodes WHERE id = ?")
        .bind(&node_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::BadRequest("Node not found".into()))?;

    let mut warnings = Vec::new();
    if let Some(peer_id) = row.wg_peer_id.as_deref().filter(|s| !s.is_empty()) {
        match crate::api::wireguard::delete_peer_core(&state, &user, peer_id).await {
            Ok(v) => {
                if let Some(w) = v.get("warnings").and_then(|w| w.as_array()) {
                    warnings.extend(w.iter().filter_map(|x| x.as_str().map(str::to_string)));
                }
            }
            Err(e) => warnings.push(format!("wg peer removal: {e}")),
        }
    }

    sqlx::query("DELETE FROM nodes WHERE id = ?")
        .bind(&node_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&user.id), "human", "nodes.delete",
        Some("node"), Some(&node_id), "success", None,
        Some(&format!("display_name={}", row.display_name)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "warnings": warnings })))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<serde_json::Value>> {
    let raw_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(AppError::Unauthorized)?;
    let token_hash = sha256_hex(raw_token);

    let matched: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = ? AND token_hash = ?")
        .bind(&node_id)
        .bind(&token_hash)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if matched == 0 {
        return Err(AppError::Unauthorized);
    }

    let now = unix_now();
    let telemetry = serde_json::json!({
        "battery": req.battery,
        "storage_free_bytes": req.storage_free_bytes,
        "online": req.online,
    })
    .to_string();

    sqlx::query("UPDATE nodes SET last_seen = ?, last_telemetry = ? WHERE id = ?")
        .bind(now)
        .bind(&telemetry)
        .bind(&node_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
