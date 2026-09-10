use crate::{
    api::integrations::{generate_api_token, sha256_hex},
    audit, auth,
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
    #[serde(default = "default_provision_wireguard")]
    pub provision_wireguard: bool,
}
fn default_device_type() -> String {
    "other".to_string()
}
fn default_provision_wireguard() -> bool {
    false
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
        &state.db,
        Some(&user.id),
        "human",
        "nodes.pairing_code.create",
        Some("node_pairing_code"),
        Some(&id),
        "success",
        None,
        None,
    )
    .await;

    Ok(Json(PairingCodeResponse {
        code: raw,
        expires_at,
    }))
}

pub async fn enroll(
    State(state): State<AppState>,
    Json(req): Json<EnrollRequest>,
) -> Result<Json<EnrollResponse>> {
    if req.display_name.trim().is_empty() {
        return Err(AppError::BadRequest("display_name is required".into()));
    }
    if !matches!(
        req.device_type.as_str(),
        "phone" | "tablet" | "pi" | "other"
    ) {
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

    if req.provision_wireguard {
        return Err(AppError::FeatureUnavailable(
            "WireGuard mutations require a canonical operation adapter".into(),
        ));
    }

    // Atomically claim the code — `used_at IS NULL` in the WHERE means a concurrent
    // second enrollment attempt with the same code affects 0 rows and gets rejected.
    let claimed =
        sqlx::query("UPDATE node_pairing_codes SET used_at = ? WHERE id = ? AND used_at IS NULL")
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
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("pairing code owner no longer exists"))
        })?;

    let wg_peer_id: Option<String> = None;
    let wg_public_key = String::new();
    let client_config = String::new();
    let warnings: Vec<String> = Vec::new();

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
    .bind(wg_peer_id.as_deref())
    .bind(&wg_public_key)
    .bind(&node_token_hash)
    .bind(req.agent_capable)
    .bind(true)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db,
        Some(&owner.id),
        "human",
        "nodes.enroll",
        Some("node"),
        Some(&node_id),
        "success",
        None,
        Some(&format!(
            "display_name={},device_type={}",
            req.display_name, req.device_type
        )),
    )
    .await;

    Ok(Json(EnrollResponse {
        node_id,
        heartbeat_token: node_token_raw,
        wg_client_config: client_config,
        warnings,
    }))
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
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

    if row
        .wg_peer_id
        .as_deref()
        .is_some_and(|peer_id| !peer_id.is_empty())
    {
        return Err(AppError::FeatureUnavailable(
            "WireGuard mutations require a canonical operation adapter".into(),
        ));
    }

    sqlx::query("DELETE FROM nodes WHERE id = ?")
        .bind(&node_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "nodes.delete",
        Some("node"),
        Some(&node_id),
        "success",
        None,
        Some(&format!("display_name={}", row.display_name)),
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true, "warnings": [] })))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<serde_json::Value>> {
    verify_node_token(state.clone(), node_id.clone(), headers.clone()).await?;
    let raw_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(AppError::Unauthorized)?;
    let token_hash = sha256_hex(raw_token);

    let matched: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = ? AND token_hash = ?")
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

pub(crate) async fn verify_node_token(
    state: crate::AppState,
    node_id: String,
    headers: axum::http::HeaderMap,
) -> crate::error::Result<()> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(crate::error::AppError::Unauthorized)?;
    let matched: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = ? AND token_hash = ? AND approved = 1 AND agent_capable = 1").bind(node_id).bind(crate::api::integrations::sha256_hex(raw)).fetch_one(&state.db).await.map_err(|e| crate::error::AppError::Internal(e.into()))?;
    if matched == 0 {
        Err(crate::error::AppError::Unauthorized)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mcp::test_support;
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request, StatusCode},
    };
    use serde_json::json;
    use tower::ServiceExt;

    async fn pairing_code(db: &sqlx::SqlitePool, raw: &str) {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
             VALUES ('enroll-owner', 'enroll-owner', 'x', 'owner', 0, 0)",
        )
        .execute(db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO node_pairing_codes \
             (id, token_hash, created_by, expires_at, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(sha256_hex(raw))
        .bind("enroll-owner")
        .bind(unix_now() + PAIRING_CODE_TTL_SECS)
        .bind(unix_now())
        .execute(db)
        .await
        .unwrap();
    }

    #[test]
    fn omitted_wireguard_request_defaults_to_no_provisioning() {
        let request: EnrollRequest = serde_json::from_value(json!({
            "pairing_code": "pairing-code",
            "display_name": "legacy-client"
        }))
        .unwrap();

        assert!(!request.provision_wireguard);
    }

    #[tokio::test]
    async fn explicit_false_enrolls_without_wireguard_state() {
        let db = test_support::setup_db().await;
        pairing_code(&db, "no-wireguard-code").await;
        let app = crate::api::router(test_support::build(db.clone()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/enroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "pairing_code": "no-wireguard-code",
                            "display_name": "lan-agent",
                            "device_type": "pi",
                            "agent_capable": true,
                            "provision_wireguard": false
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(body["wg_client_config"], "");
        assert_eq!(body["warnings"], json!([]));

        let (wg_peer_id, wg_public_key): (Option<String>, String) =
            sqlx::query_as("SELECT wg_peer_id, wg_public_key FROM nodes WHERE id = ?")
                .bind(body["node_id"].as_str().unwrap())
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(wg_peer_id, None);
        assert!(wg_public_key.is_empty());

        let peer_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM wireguard_peers")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(peer_count, 0);
    }

    #[tokio::test]
    async fn explicit_wireguard_provisioning_fails_before_claiming_pairing_code() {
        let db = test_support::setup_db().await;
        pairing_code(&db, "wireguard-code").await;
        let app = crate::api::router(test_support::build(db.clone()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/enroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "pairing_code": "wireguard-code",
                            "display_name": "wireguard-agent",
                            "device_type": "pi",
                            "agent_capable": true,
                            "provision_wireguard": true
                        })
                        .to_string(),
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
            "WireGuard mutations require a canonical operation adapter"
        );

        let used_at: Option<i64> =
            sqlx::query_scalar("SELECT used_at FROM node_pairing_codes WHERE token_hash = ?")
                .bind(sha256_hex("wireguard-code"))
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(used_at, None);

        let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(node_count, 0);
    }

    #[tokio::test]
    async fn node_delete_with_wireguard_peer_fails_without_deleting_node() {
        let db = test_support::setup_db().await;
        let session = test_support::user_with_session(&db).await;
        sqlx::query(
            "INSERT INTO nodes (id, display_name, device_type, owner_user_id, wg_peer_id, \
             wg_public_key, token_hash, agent_capable, approved, created_at) \
             VALUES ('node-with-peer', 'fixture-node', 'pi', 'u1', 'peer-1', \
             'fixture-public-key', 'fixture-token-hash', 1, 1, 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let app = crate::api::router(test_support::build(db.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/nodes/node-with-peer")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .body(Body::empty())
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
            "WireGuard mutations require a canonical operation adapter"
        );
        let node_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = 'node-with-peer'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(node_count, 1);
    }
}
