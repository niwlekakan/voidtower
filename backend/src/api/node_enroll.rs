use crate::{
    agent::state::MAX_NODE_TOKEN_BYTES,
    api::integrations::{generate_api_token, sha256_hex},
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, Path, State},
    http::HeaderMap,
    response::IntoResponse,
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
const MAX_PAIRING_CODE_BYTES: usize = 512;
const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_DEVICE_TYPE_BYTES: usize = 32;
const MAX_BATTERY_PERCENT: f32 = 100.0;

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
    request: std::result::Result<Json<EnrollRequest>, JsonRejection>,
) -> Result<Json<EnrollResponse>> {
    let Json(req) = request.map_err(|rejection| match rejection {
        JsonRejection::BytesRejection(
            axum::extract::rejection::BytesRejection::FailedToBufferBody(
                axum::extract::rejection::FailedToBufferBody::LengthLimitError(_),
            ),
        ) => AppError::PayloadTooLarge,
        _ => AppError::BadRequest("invalid request body".into()),
    })?;
    validate_enroll_request(&req)?;

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
    let mut transaction = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let claimed = sqlx::query(
        "UPDATE node_pairing_codes SET used_at = CAST(strftime('%s', 'now') AS INTEGER) WHERE id = ? AND used_at IS NULL AND expires_at > CAST(strftime('%s', 'now') AS INTEGER)",
    )
    .bind(&pairing.id)
    .execute(&mut *transaction)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if claimed.rows_affected() == 0 {
        return Err(AppError::Unauthorized);
    }

    let owner_id: String = sqlx::query_scalar("SELECT id FROM users WHERE id = ?")
        .bind(&pairing.created_by)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
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
    .bind(&owner_id)
    .bind(wg_peer_id.as_deref())
    .bind(&wg_public_key)
    .bind(&node_token_hash)
    .bind(req.agent_capable)
    .bind(true)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    transaction
        .commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db,
        Some(&owner_id),
        "human",
        "nodes.enroll",
        Some("node"),
        Some(&node_id),
        "success",
        None,
        Some(
            &serde_json::json!({
                "display_name": req.display_name,
                "device_type": req.device_type,
            })
            .to_string(),
        ),
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
        Some(
            &serde_json::json!({
                "display_name": row.display_name,
            })
            .to_string(),
        ),
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true, "warnings": [] })))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    headers: HeaderMap,
    body: std::result::Result<
        axum::body::Bytes,
        axum::extract::rejection::BytesRejection,
    >,
) -> Result<Json<serde_json::Value>> {
    verify_node_token(state.clone(), node_id.clone(), headers.clone()).await?;
    let body = match body {
        Ok(body) => body,
        Err(axum::extract::rejection::BytesRejection::FailedToBufferBody(
            axum::extract::rejection::FailedToBufferBody::LengthLimitError(_),
        )) => return Err(AppError::PayloadTooLarge),
        Err(_) => return Err(AppError::BadRequest("invalid request body".into())),
    };
    let req: HeartbeatRequest = serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("invalid heartbeat".into()))?;
    validate_heartbeat_request(&req)?;

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

fn validate_enroll_request(req: &EnrollRequest) -> Result<()> {
    if req.pairing_code.is_empty() || req.pairing_code.len() > MAX_PAIRING_CODE_BYTES {
        return Err(AppError::BadRequest("invalid pairing_code".into()));
    }
    if req.display_name.trim().is_empty()
        || req.display_name.len() > MAX_DISPLAY_NAME_BYTES
        || req.display_name.chars().any(|c| c.is_control())
    {
        return Err(AppError::BadRequest("invalid display_name".into()));
    }
    if req.device_type.len() > MAX_DEVICE_TYPE_BYTES
        || !matches!(
            req.device_type.as_str(),
            "phone" | "tablet" | "pi" | "other"
        )
    {
        return Err(AppError::BadRequest("Invalid device_type".into()));
    }
    Ok(())
}

fn validate_heartbeat_request(req: &HeartbeatRequest) -> Result<()> {
    if req.battery.is_some_and(|battery| {
        !battery.is_finite() || !(0.0..=MAX_BATTERY_PERCENT).contains(&battery)
    }) {
        return Err(AppError::BadRequest("battery must be between 0 and 100".into()));
    }
    if req.storage_free_bytes.is_some_and(|bytes| bytes < 0) {
        return Err(AppError::BadRequest("storage_free_bytes must be non-negative".into()));
    }
    Ok(())
}

pub(crate) async fn verify_node_token(
    state: crate::AppState,
    node_id: String,
    headers: axum::http::HeaderMap,
) -> crate::error::Result<()> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            let (scheme, credentials) = v.split_once(' ')?;
            scheme.eq_ignore_ascii_case("Bearer").then_some(credentials)
        })
        .map(str::trim)
        .ok_or(crate::error::AppError::Unauthorized)?;
    if raw.is_empty() || raw.len() > MAX_NODE_TOKEN_BYTES {
        return Err(crate::error::AppError::Unauthorized);
    }
    let matched: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = ? AND token_hash = ? AND approved = 1 AND agent_capable = 1").bind(node_id).bind(crate::api::integrations::sha256_hex(raw)).fetch_one(&state.db).await.map_err(|e| crate::error::AppError::Internal(e.into()))?;
    if matched == 0 {
        Err(crate::error::AppError::Unauthorized)
    } else {
        Ok(())
    }
}

pub(crate) async fn authenticate_node_request(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let node_id = request
        .uri()
        .path()
        .strip_prefix("/api/nodes/")
        .and_then(|path| path.split('/').next())
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    let Some(node_id) = node_id else {
        return AppError::Unauthorized.into_response();
    };
    if let Err(error) = verify_node_token(state, node_id, request.headers().clone()).await {
        return error.into_response();
    }
    next.run(request).await
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

    #[test]
    fn enrollment_validation_rejects_oversized_and_controlled_operator_fields() {
        let request = EnrollRequest {
            pairing_code: "pairing-code".into(),
            display_name: format!("{}\n", "x".repeat(MAX_DISPLAY_NAME_BYTES)),
            device_type: "pi".into(),
            agent_capable: true,
            provision_wireguard: false,
        };
        assert!(matches!(
            validate_enroll_request(&request),
            Err(AppError::BadRequest(message)) if message == "invalid display_name"
        ));
    }

    #[tokio::test]
    async fn malformed_heartbeat_is_authenticated_before_json_parsing() {
        let db = test_support::setup_db().await;
        let app = crate::api::router(test_support::build(db));
        for body in ["{".to_string(), "x".repeat(64 * 1024 + 1)] {
            let response = app
                .clone()
                .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/missing/heartbeat")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
    }

    #[tokio::test]
    async fn enrollment_rejects_oversized_body_with_stable_error() {
        let db = test_support::setup_db().await;
        let app = crate::api::router(test_support::build(db));
        let body = format!(
            "{{\"pairing_code\":\"code\",\"display_name\":\"{}\"}}",
            "x".repeat(64 * 1024 + 1)
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/enroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let payload: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(payload["error"]["code"], "payload_too_large");
    }

    #[tokio::test]
    async fn heartbeat_rejects_impossible_telemetry_after_authentication() {
        let db = test_support::setup_db().await;
        pairing_code(&db, "unused-pairing-code").await;
        let token = "heartbeat-secret";
        sqlx::query(
            "INSERT INTO nodes (id, display_name, device_type, owner_user_id, token_hash, agent_capable, approved, created_at) VALUES (?, ?, ?, ?, ?, 1, 1, 0)",
        )
        .bind("heartbeat-node")
        .bind("test node")
        .bind("pi")
        .bind("enroll-owner")
        .bind(sha256_hex(token))
        .execute(&db)
        .await
        .unwrap();
        let app = crate::api::router(test_support::build(db.clone()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/heartbeat-node/heartbeat")
                    .header(header::AUTHORIZATION, format!("Bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"battery": 101.0}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let last_seen: Option<i64> = sqlx::query_scalar("SELECT last_seen FROM nodes WHERE id = ?")
            .bind("heartbeat-node")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(last_seen, None);
    }

    #[tokio::test]
    async fn lowercase_bearer_scheme_authenticates_approved_agent() {
        let db = test_support::setup_db().await;
        let token = "x".repeat(MAX_NODE_TOKEN_BYTES);
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES ('lowercase-bearer-owner', 'lowercase-bearer-owner', 'x', 'owner', 0, 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO nodes (id, display_name, device_type, owner_user_id, token_hash, agent_capable, approved, created_at) VALUES (?, ?, ?, ?, ?, 1, 1, 0)",
        )
        .bind("lowercase-bearer-node")
        .bind("fixture agent")
        .bind("pi")
        .bind("lowercase-bearer-owner")
        .bind(sha256_hex(&token))
        .execute(&db)
        .await
        .unwrap();
        let app = crate::api::router(test_support::build(db));
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/lowercase-bearer-node/heartbeat")
                    .header(header::AUTHORIZATION, format!("bearer {token}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let oversized = "x".repeat(MAX_NODE_TOKEN_BYTES + 1);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/lowercase-bearer-node/heartbeat")
                    .header(header::AUTHORIZATION, format!("Bearer {oversized}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn heartbeat_rejects_empty_or_oversized_node_tokens_before_database_match() {
        let db = test_support::setup_db().await;
        let app = crate::api::router(test_support::build(db));
        for token in [String::new(), "x".repeat(MAX_NODE_TOKEN_BYTES + 1)] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/nodes/missing/heartbeat")
                        .header(header::AUTHORIZATION, format!("Bearer {token}"))
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
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

        let details: String = sqlx::query_scalar(
            "SELECT details FROM audit_log WHERE action = 'nodes.enroll' ORDER BY timestamp DESC LIMIT 1",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        let details: serde_json::Value = serde_json::from_str(&details).unwrap();
        assert_eq!(details["display_name"], "lan-agent");
        assert_eq!(details["device_type"], "pi");
    }

    #[tokio::test]
    async fn enrollment_rejects_pairing_code_at_expiry_boundary() {
        let db = test_support::setup_db().await;
        pairing_code(&db, "expired-at-boundary").await;
        sqlx::query("UPDATE node_pairing_codes SET expires_at = ? WHERE token_hash = ?")
            .bind(unix_now())
            .bind(sha256_hex("expired-at-boundary"))
            .execute(&db)
            .await
            .unwrap();
        let app = crate::api::router(test_support::build(db.clone()));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/enroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "pairing_code": "expired-at-boundary",
                            "display_name": "expired-agent",
                            "device_type": "pi",
                            "agent_capable": true
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let used_at: Option<i64> = sqlx::query_scalar(
            "SELECT used_at FROM node_pairing_codes WHERE token_hash = ?",
        )
        .bind(sha256_hex("expired-at-boundary"))
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(used_at, None);
    }

    #[tokio::test]
    async fn enrollment_rolls_back_pairing_claim_when_node_persistence_fails() {
        let db = test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
             VALUES ('insert-failure-owner', 'insert-failure-owner', 'x', 'owner', 0, 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO node_pairing_codes \
             (id, token_hash, created_by, expires_at, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("missing-owner-code")
        .bind(sha256_hex("missing-owner-code"))
        .bind("insert-failure-owner")
        .bind(unix_now() + PAIRING_CODE_TTL_SECS)
        .bind(unix_now())
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_node_enrollment BEFORE INSERT ON nodes BEGIN SELECT RAISE(ABORT, 'fixture enrollment failure'); END",
        )
        .execute(&db)
        .await
        .unwrap();
        let app = crate::api::router(test_support::build(db.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/nodes/enroll")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "pairing_code": "missing-owner-code",
                            "display_name": "orphaned-agent",
                            "device_type": "pi",
                            "agent_capable": true
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let used_at: Option<i64> = sqlx::query_scalar(
            "SELECT used_at FROM node_pairing_codes WHERE token_hash = ?",
        )
        .bind(sha256_hex("missing-owner-code"))
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
    async fn node_delete_audit_details_are_structured() {
        let db = test_support::setup_db().await;
        let session = test_support::user_with_session(&db).await;
        sqlx::query(
            "INSERT INTO nodes (id, display_name, device_type, owner_user_id, token_hash, agent_capable, approved, created_at) VALUES (?, ?, 'pi', 'u1', 'fixture-token-hash', 1, 1, 0)",
        )
        .bind("structured-audit-node")
        .bind("name=comma,quote\"unicode")
        .execute(&db)
        .await
        .unwrap();
        let app = crate::api::router(test_support::build(db.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/nodes/structured-audit-node")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let details: String = sqlx::query_scalar(
            "SELECT details FROM audit_log WHERE action = 'nodes.delete' ORDER BY timestamp DESC LIMIT 1",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        let details: serde_json::Value = serde_json::from_str(&details).unwrap();
        assert_eq!(details["display_name"], "name=comma,quote\"unicode");
    }

    #[tokio::test]
    async fn concurrent_enrollment_claims_a_pairing_code_once() {
        let db = test_support::setup_db().await;
        pairing_code(&db, "concurrent-code").await;
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/api/nodes/enroll")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "pairing_code": "concurrent-code",
                        "display_name": "concurrent-agent",
                        "device_type": "pi",
                        "agent_capable": true,
                        "provision_wireguard": false
                    })
                    .to_string(),
                ))
                .unwrap()
        };
        let app = crate::api::router(test_support::build(db.clone()));
        let (first, second) = tokio::join!(app.clone().oneshot(request()), app.oneshot(request()));
        let statuses = [first.unwrap().status(), second.unwrap().status()];
        assert!(statuses.contains(&StatusCode::OK));
        assert!(statuses.contains(&StatusCode::UNAUTHORIZED));

        let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(node_count, 1);
        let claimed_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM node_pairing_codes WHERE token_hash = ? AND used_at IS NOT NULL",
        )
        .bind(sha256_hex("concurrent-code"))
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(claimed_count, 1);
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
