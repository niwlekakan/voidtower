#![cfg(test)]
//! Acceptance tests for the P0-06 scope-bypass fix.
//!
//! Covers the historical scope-bypass regression: `bearer_auth::middleware` used to upgrade
//! any valid API token into a full
//! session carrying the token owner's role, and nothing downstream ever
//! consults the token's declared scopes except two endpoints. These tests
//! drive the real router end-to-end (`tower::ServiceExt::oneshot`) so they
//! exercise the actual middleware stack, not just handler bodies.
//!
//! Every endpoint hit here is deliberately chosen to be side-effect-free
//! (pure DB reads, or requests that fail closed on a nonexistent resource
//! before touching Docker/systemd/git/mkfs) so that running these tests
//! *before* the fix lands — which is required by TDD, and which means the
//! request really does reach the handler — cannot do anything destructive
//! to the host the tests run on.

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{header, Request, StatusCode},
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tower::ServiceExt;

use crate::api::mcp::test_support;

async fn setup_db() -> SqlitePool {
    // Full schema (baseline + post-baseline ALTER TABLEs, e.g. users.totp_*,
    // users.expires_at) is only assembled by `init_pool`, not the baseline-only
    // `run_migrations` — `auth::validate_session` selects those columns, so
    // tests that mint real sessions need the full path, not an in-memory-only
    // shortcut. A unique temp file per test avoids cross-test interference.
    let path = std::env::temp_dir().join(format!("vt-p0-06-test-{}.sqlite", uuid::Uuid::new_v4()));
    crate::db::init_pool(&path).await.expect("init test db")
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

async fn insert_user(db: &SqlitePool, role: &str) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, force_password_change, created_at, updated_at)
         VALUES (?, ?, 'x', ?, 0, ?, ?)",
    )
    .bind(&id)
    .bind(format!("user-{id}"))
    .bind(role)
    .bind(now)
    .bind(now)
    .execute(db)
    .await
    .unwrap();
    id
}

async fn insert_session(db: &SqlitePool, user_id: &str) -> String {
    let id = crate::auth::generate_session_token();
    let now = unix_now();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at, created_at) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(user_id)
        .bind(now + 3600)
        .bind(now)
        .execute(db)
        .await
        .unwrap();
    id
}

/// Inserts a scoped API token directly (bypassing the HTTP create-token
/// endpoint, which is what's under test elsewhere) and returns the raw token.
async fn insert_token(db: &SqlitePool, user_id: &str, scopes: &[&str]) -> String {
    insert_token_with_expiry(db, user_id, scopes, None).await.1
}

async fn insert_token_with_expiry(
    db: &SqlitePool,
    user_id: &str,
    scopes: &[&str],
    expires_at: Option<i64>,
) -> (String, String) {
    let raw = format!("vt_test_{}", uuid::Uuid::new_v4().simple());
    let mut h = Sha256::new();
    h.update(raw.as_bytes());
    let hash = hex::encode(h.finalize());
    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let scopes_json = serde_json::to_string(scopes).unwrap();
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, expires_at, created_at)
         VALUES (?, ?, 'test-token', ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&hash)
    .bind(&scopes_json)
    .bind(expires_at)
    .bind(now)
    .execute(db)
    .await
    .unwrap();
    (id, raw)
}

/// Stores a *validly* encrypted value under the same all-zero key
/// `test_support::build` wires up, so a request that reaches the handler
/// gets a clean 200 with the decrypted value — a request blocked by scope
/// enforcement never gets that far, so this keeps the two cases visually
/// distinct (403 vs. 200) instead of both looking like errors.
async fn insert_secret(db: &SqlitePool) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let value_enc = crate::api::secrets::encrypt(&[0u8; 32], "super-secret-value").unwrap();
    sqlx::query(
        "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES (?, 'test-secret', ?, ?, ?)",
    )
    .bind(&id)
    .bind(&value_enc)
    .bind(now)
    .bind(now)
    .execute(db)
    .await
    .unwrap();
    id
}

fn with_connect_info(mut req: Request<Body>) -> Request<Body> {
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

fn bearer_req(
    method: &str,
    uri: &str,
    token: &str,
    body: Option<serde_json::Value>,
) -> Request<Body> {
    let b = body.map(|v| v.to_string()).unwrap_or_default();
    with_connect_info(
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b))
            .unwrap(),
    )
}

fn bearer_raw_req(method: &str, uri: &str, token: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    with_connect_info(builder.body(Body::from(body.to_owned())).unwrap())
}

fn cookie_req(
    method: &str,
    uri: &str,
    session_id: &str,
    body: Option<serde_json::Value>,
) -> Request<Body> {
    let b = body.map(|v| v.to_string()).unwrap_or_default();
    with_connect_info(
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("vt_session={session_id}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b))
            .unwrap(),
    )
}

fn cookie_raw_req(
    method: &str,
    uri: &str,
    session_id: &str,
    content_type: &str,
    body: &str,
) -> Request<Body> {
    with_connect_info(
        Request::builder()
            .method(method)
            .uri(uri)
            .header(header::COOKIE, format!("vt_session={session_id}"))
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(body.to_owned()))
            .unwrap(),
    )
}

async fn assert_insufficient_scope(res: axum::response::Response, context: &str) {
    assert_eq!(
        res.status(),
        StatusCode::FORBIDDEN,
        "{context}: expected bearer scope enforcement to return 403"
    );
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["error"]["code"], "insufficient_scope",
        "{context}: expected the structured insufficient_scope denial"
    );
}

/// Reproduces the exact scope-bypass bug: a token scoped to
/// only `metrics:read`, owned by an admin user, must not be able to reach an
/// admin-gated action just because the middleware upgraded it to a full
/// session. `GET /api/secrets/:id/reveal` is the literal example the map
/// cites, and is a pure DB read + local decrypt — safe to actually reach if
/// this test fails red.
#[tokio::test]
async fn token_scoped_to_metrics_read_cannot_call_admin_endpoint() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let secret_id = insert_secret(&db).await;
    let token = insert_token(&db, &admin, &["metrics:read"]).await;

    let app = crate::api::router(test_support::build(db));
    let res = app
        .oneshot(bearer_req(
            "GET",
            &format!("/api/secrets/{secret_id}/reveal"),
            &token,
            None,
        ))
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

/// A `deploy`-tier token (apps:read + apps:deploy + apps:restart) may reach
/// deploy-surface endpoints but must not be able to read secrets metadata.
/// The deploy call targets a nonexistent app id so it fails closed (404/503)
/// before touching Docker regardless of whether this test runs pre- or
/// post-fix — the assertion is "not blocked by scope", not "deploy succeeds".
#[tokio::test]
async fn token_scoped_to_deploy_can_deploy_but_not_read_secrets() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(&db, &admin, &["apps:read", "apps:deploy", "apps:restart"]).await;

    let app = crate::api::router(test_support::build(db));

    let deploy_res = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/apps/deploy",
            &token,
            Some(serde_json::json!({ "app_id": "nonexistent-test-app-id-xyz" })),
        ))
        .await
        .unwrap();
    assert_ne!(
        deploy_res.status(),
        StatusCode::FORBIDDEN,
        "deploy-scoped token should not be scope-blocked from POST /api/apps/deploy"
    );

    let secrets_res = app
        .oneshot(bearer_req("GET", "/api/secrets", &token, None))
        .await
        .unwrap();
    assert_eq!(secrets_res.status(), StatusCode::FORBIDDEN);
}

/// An `exec`-tier token (containers:*, services:*, automation:*) may run
/// container actions but must not reach owner-only system/disaster-recovery
/// actions. The container action targets a nonexistent container id — with
/// no Docker socket in the test sandbox this fails closed at the Docker
/// unavailability check regardless of scope. `POST /api/disaster/export-config`
/// stands in for the "system update"-class admin action named in the test:
/// like `POST /api/system/update`, it's owner-gated and outside every
/// existing scope, but unlike `/api/system/update` it only reads this
/// process's own in-memory test DB — it never shells out, so it's safe to
/// actually execute if this test runs red pre-fix.
#[tokio::test]
async fn token_scoped_to_exec_can_run_container_actions_but_not_system_update() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(
        &db,
        &admin,
        &[
            "containers:read",
            "containers:restart",
            "containers:logs",
            "services:read",
            "services:restart",
        ],
    )
    .await;

    let app = crate::api::router(test_support::build(db));

    let container_res = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/containers/nonexistent-test-container/action",
            &token,
            Some(serde_json::json!({ "action": "restart" })),
        ))
        .await
        .unwrap();
    assert_ne!(
        container_res.status(),
        StatusCode::FORBIDDEN,
        "exec-scoped token should not be scope-blocked from POST /api/containers/:id/action"
    );

    let disaster_res = app
        .oneshot(bearer_req(
            "POST",
            "/api/disaster/export-config",
            &token,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(disaster_res.status(), StatusCode::FORBIDDEN);
}

/// A token minted under the `admin-never` capability tier (empty scope set)
/// must not reach any admin/owner-gated route — not a sample, the full set
/// of admin-gated routes this test suite can safely execute for real (i.e.
/// pure DB reads, or requests with no external side effects). Endpoints that
/// shell out or touch Docker/git/mkfs (system.rs, storage.rs, wireguard.rs,
/// lxc.rs, mods.rs) are deliberately excluded from live execution here for
/// test-environment safety, not because they're exempt from the fix — they
/// are covered structurally by the same default-deny table.
#[tokio::test]
async fn admin_never_scope_cannot_reach_any_admin_gated_route() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(&db, &admin, &[]).await;

    let admin_gated_routes: &[(&str, &str)] = &[
        ("GET", "/api/integrations/tokens"),
        ("GET", "/api/policy/rules"),
        ("GET", "/api/ai/providers"),
        ("GET", "/api/proxmox/hosts"),
        ("GET", "/api/users"),
        ("GET", "/api/nodes"),
        ("POST", "/api/disaster/export-config"),
    ];

    let app = crate::api::router(test_support::build(db));
    for (method, uri) in admin_gated_routes {
        let res = app
            .clone()
            .oneshot(bearer_req(method, uri, &token, None))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "admin-never token should never reach {method} {uri}, got {}",
            res.status()
        );
    }
}

/// The whole point of routing scope enforcement through a middleware that's
/// a no-op unless `bearer_auth::middleware` marked the request as
/// token-originated: human session-cookie logins must be provably unaffected.
/// An admin session must still succeed against an admin-gated route exactly
/// as before, and a non-admin session must still be blocked by the handler's
/// own role check (not by the new middleware silently taking over that job).
#[tokio::test]
async fn human_session_cookie_login_is_unaffected_by_scope_changes() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let viewer = insert_user(&db, "viewer").await;
    let admin_session = insert_session(&db, &admin).await;
    let viewer_session = insert_session(&db, &viewer).await;

    let app = crate::api::router(test_support::build(db));

    let admin_res = app
        .clone()
        .oneshot(cookie_req(
            "GET",
            "/api/integrations/tokens",
            &admin_session,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(admin_res.status(), StatusCode::OK);

    let viewer_res = app
        .oneshot(cookie_req(
            "GET",
            "/api/integrations/tokens",
            &viewer_session,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(viewer_res.status(), StatusCode::FORBIDDEN);
}

/// Part 2 of the task contract: capability-tier minting on top of part 1's
/// enforcement fix. Minting a `read`-tier token must produce a working,
/// narrowly-scoped token, and the legacy explicit-`scopes` minting path
/// (what every existing integration already uses) must keep working
/// unchanged — "without breaking existing integrations".
#[tokio::test]
async fn voidtower_token_migration_splits_into_capability_tokens_without_breaking_existing_integrations(
) {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let owner_session = insert_session(&db, &owner).await;

    let app = crate::api::router(test_support::build(db.clone()));

    // Legacy path: explicit scopes array, unchanged.
    let legacy_res = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/integrations/tokens",
            &owner_session,
            Some(serde_json::json!({ "name": "legacy-integration", "scopes": ["metrics:read"] })),
        ))
        .await
        .unwrap();
    assert_eq!(legacy_res.status(), StatusCode::OK);
    let legacy_body = axum::body::to_bytes(legacy_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let legacy_json: serde_json::Value = serde_json::from_slice(&legacy_body).unwrap();
    assert_eq!(legacy_json["scopes"], serde_json::json!(["metrics:read"]));

    // New path: capability tier, derives its scopes server-side.
    let tier_res = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/integrations/tokens",
            &owner_session,
            Some(serde_json::json!({ "name": "read-tier-integration", "tier": "read" })),
        ))
        .await
        .unwrap();
    assert_eq!(tier_res.status(), StatusCode::OK);
    let tier_body = axum::body::to_bytes(tier_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let tier_json: serde_json::Value = serde_json::from_slice(&tier_body).unwrap();
    let minted_scopes = tier_json["scopes"].as_array().unwrap();
    assert!(!minted_scopes.is_empty());
    // No mutating scope (":restart", ":deploy", ":run", ":ack", ":manage") may
    // leak into the read tier — check the complement rather than guessing at
    // every read-class suffix (metrics:read, secrets:list, containers:logs
    // are all read-only despite not sharing one suffix).
    const MUTATING_SUFFIXES: &[&str] = &[":restart", ":deploy", ":run", ":ack", ":manage"];
    assert!(
        minted_scopes.iter().all(|s| !MUTATING_SUFFIXES
            .iter()
            .any(|suf| s.as_str().unwrap().ends_with(suf))),
        "read tier must not mint any mutating scope, got {minted_scopes:?}"
    );

    // admin-never tier: mints a token whose scopes structurally match nothing
    // in the route table, so it can't reach any admin-gated route end to end.
    let never_res = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/integrations/tokens",
            &owner_session,
            Some(serde_json::json!({ "name": "admin-never-integration", "tier": "admin-never" })),
        ))
        .await
        .unwrap();
    assert_eq!(never_res.status(), StatusCode::OK);
    let never_body = axum::body::to_bytes(never_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let never_json: serde_json::Value = serde_json::from_slice(&never_body).unwrap();
    let never_token = never_json["token"].as_str().unwrap().to_string();

    let blocked_res = app
        .oneshot(bearer_req("GET", "/api/policy/rules", &never_token, None))
        .await
        .unwrap();
    assert_eq!(blocked_res.status(), StatusCode::FORBIDDEN);
}

/// `GET /api/webhooks` is explicitly bearer-denied in the unified registry. A token must be
/// rejected even when it holds an unrelated real scope, while a human session-cookie login still
/// works because bearer enforcement is a no-op for non-token-originated requests.
#[tokio::test]
async fn explicitly_denied_bearer_route_preserves_human_session_access() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let admin_session = insert_session(&db, &admin).await;
    let token = insert_token(&db, &admin, &["metrics:read"]).await;

    let app = crate::api::router(test_support::build(db));

    let token_res = app
        .clone()
        .oneshot(bearer_req("GET", "/api/webhooks", &token, None))
        .await
        .unwrap();
    assert_eq!(
        token_res.status(),
        StatusCode::FORBIDDEN,
        "a Bearer token must be denied by explicit route metadata even when it holds an \
         unrelated real scope"
    );

    let human_res = app
        .oneshot(cookie_req("GET", "/api/webhooks", &admin_session, None))
        .await
        .unwrap();
    assert_eq!(
        human_res.status(),
        StatusCode::OK,
        "an explicitly bearer-denied route must still work normally for human session logins"
    );
}

/// The embed router intentionally omits scope enforcement so iframe responses can keep their
/// distinct security-header policy. Any valid API token is therefore upgraded to a session and
/// reaches these handlers regardless of its scope set. Registry metadata must describe that
/// shipped exception explicitly rather than claiming the routes are bearer-denied.
#[tokio::test]
async fn embed_router_preserves_unscoped_bearer_session_bridge() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(&db, &admin, &[]).await;
    let app = crate::api::router(test_support::build(db));

    for path in [
        "/api/apps/embed/missing/index.html",
        "/plugin-assets/missing/file.js",
    ] {
        let response = app
            .clone()
            .oneshot(bearer_req("GET", path, &token, None))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "unscoped bearer token must reach embed handler for GET {path}"
        );
    }
}

/// Diagnostic clients receive narrowly-scoped access to ordinary host metadata and detailed
/// diagnostics that the session contract exposes to administrators.
/// These requests drive the complete Bearer -> temporary session -> scope -> handler chain.
#[tokio::test]
async fn diagnostics_read_token_reaches_all_three_authorized_host_reads() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(&db, &admin, &["diagnostics:read"]).await;
    let app = crate::api::router(test_support::build(db));

    for path in [
        "/api/capabilities",
        "/api/diagnostics",
        "/api/system/version",
    ] {
        let res = app
            .clone()
            .oneshot(bearer_req("GET", path, &token, None))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "diagnostics:read token must reach GET {path}"
        );
    }
}

#[tokio::test]
async fn unrelated_bearer_scope_cannot_reach_s0_02_host_reads() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(&db, &admin, &["metrics:read"]).await;
    let app = crate::api::router(test_support::build(db));

    for path in [
        "/api/capabilities",
        "/api/diagnostics",
        "/api/system/version",
    ] {
        let res = app
            .clone()
            .oneshot(bearer_req("GET", path, &token, None))
            .await
            .unwrap();
        assert_insufficient_scope(res, &format!("GET {path}")).await;
    }
}

/// Model-job state is intentionally session-only. Even a token owned by an administrator and
/// carrying the otherwise relevant diagnostics scope must fail at the bearer choke point.
#[tokio::test]
async fn model_job_status_routes_are_session_only_for_bearer_clients() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let token = insert_token(&db, &admin, &["diagnostics:read"]).await;
    let app = crate::api::router(test_support::build(db));

    for path in [
        "/api/models/download/missing",
        "/api/models/ollama/create/missing",
        "/api/models/ollama/pull/missing",
    ] {
        let res = app
            .clone()
            .oneshot(bearer_req("GET", path, &token, None))
            .await
            .unwrap();
        assert_insufficient_scope(res, &format!("GET {path}")).await;
    }
}

#[tokio::test]
async fn revoking_a_cached_token_denies_the_next_real_router_request() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let admin_session = insert_session(&db, &admin).await;
    let (token_id, token) =
        insert_token_with_expiry(&db, &admin, &["diagnostics:read"], None).await;
    let app = crate::api::router(test_support::build(db));

    let first = app
        .clone()
        .oneshot(bearer_req("GET", "/api/system/version", &token, None))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let revoke = app
        .clone()
        .oneshot(cookie_req(
            "DELETE",
            &format!("/api/integrations/tokens/{token_id}"),
            &admin_session,
            None,
        ))
        .await
        .unwrap();
    assert_eq!(revoke.status(), StatusCode::OK);

    let after_revoke = app
        .oneshot(bearer_req("GET", "/api/system/version", &token, None))
        .await
        .unwrap();
    assert_eq!(after_revoke.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn an_expired_cached_token_denies_the_next_real_router_request() {
    let db = setup_db().await;
    let admin = insert_user(&db, "admin").await;
    let (_, token) =
        insert_token_with_expiry(&db, &admin, &["diagnostics:read"], Some(unix_now() + 1)).await;
    let app = crate::api::router(test_support::build(db));

    let first = app
        .clone()
        .oneshot(bearer_req("GET", "/api/system/version", &token, None))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let after_expiry = app
        .oneshot(bearer_req("GET", "/api/system/version", &token, None))
        .await
        .unwrap();
    assert_eq!(after_expiry.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn built_in_mcp_bearer_reaches_handler_before_tool_scope_is_checked() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let token = insert_token(&db, &owner, &["metrics:read"]).await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let app = crate::api::router(test_support::build(db));
    let response = app
        .oneshot(bearer_req("GET", "/api/mcp", &token, None))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "valid MCP bearer auth must reach the handler; individual tools enforce their scopes"
    );
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(String::from_utf8(body.to_vec())
        .unwrap()
        .contains("/api/mcp/message"));
}

#[tokio::test]
async fn built_in_mcp_message_reaches_json_rpc_dispatch_and_denies_wrong_tool_scope() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let token = insert_token(&db, &owner, &["containers:read"]).await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let app = crate::api::router(test_support::build(db));
    let response = app
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "container.start",
                    "arguments": {
                        "resource_id": "missing-resource",
                        "request_id": "mcp-router-scope-denial"
                    }
                }
            })),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["jsonrpc"], "2.0");
    assert_eq!(json["result"]["isError"], true);
    assert!(json["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("scope"));
}

#[tokio::test]
async fn built_in_mcp_rejects_invalid_json_rpc_version_and_unknown_request_fields() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let token = insert_token(&db, &owner, &["metrics:read"]).await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let app = crate::api::router(test_support::build(db));
    let unauthenticated_malformed = app
        .clone()
        .oneshot(bearer_raw_req(
            "POST",
            "/api/mcp/message",
            None,
            "{ malformed",
        ))
        .await
        .unwrap();
    assert_eq!(unauthenticated_malformed.status(), StatusCode::UNAUTHORIZED);

    let malformed = app
        .clone()
        .oneshot(bearer_raw_req(
            "POST",
            "/api/mcp/message",
            Some(&token),
            "{ malformed",
        ))
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    let malformed_body = axum::body::to_bytes(malformed.into_body(), usize::MAX)
        .await
        .unwrap();
    let malformed_json: serde_json::Value = serde_json::from_slice(&malformed_body).unwrap();
    assert_eq!(malformed_json["error"]["code"], -32600);

    let invalid_version = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &token,
            Some(serde_json::json!({
                "jsonrpc": "1.0",
                "id": 1,
                "method": "initialize"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(invalid_version.status(), StatusCode::OK);
    let body = axum::body::to_bytes(invalid_version.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], -32600);

    let unknown_field = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "unexpected": true
            })),
        ))
        .await
        .unwrap();
    assert_eq!(unknown_field.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let unknown_params = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "list_nodes",
                    "arguments": {},
                    "unexpected": true
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(unknown_params.status(), StatusCode::OK);
    let unknown_params_body = axum::body::to_bytes(unknown_params.into_body(), usize::MAX)
        .await
        .unwrap();
    let unknown_params_json: serde_json::Value =
        serde_json::from_slice(&unknown_params_body).unwrap();
    assert_eq!(unknown_params_json["error"]["code"], -32602);

    let invalid_id = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": { "not": "allowed" },
                "method": "initialize"
            })),
        ))
        .await
        .unwrap();
    assert_eq!(invalid_id.status(), StatusCode::UNPROCESSABLE_ENTITY);

    for request in [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": "not-an-object"
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": null
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "list_nodes", "arguments": null }
        }),
    ] {
        let invalid_params = app
            .clone()
            .oneshot(bearer_req(
                "POST",
                "/api/mcp/message",
                &token,
                Some(request),
            ))
            .await
            .unwrap();
        assert_eq!(invalid_params.status(), StatusCode::OK);
        let invalid_params_body = axum::body::to_bytes(invalid_params.into_body(), usize::MAX)
            .await
            .unwrap();
        let invalid_params_json: serde_json::Value =
            serde_json::from_slice(&invalid_params_body).unwrap();
        assert_eq!(invalid_params_json["error"]["code"], -32602);
    }

    let oversized_body = "x".repeat(65 * 1024);
    let unauthenticated_oversized = app
        .clone()
        .oneshot(bearer_raw_req(
            "POST",
            "/api/mcp/message",
            None,
            &oversized_body,
        ))
        .await
        .unwrap();
    assert_eq!(unauthenticated_oversized.status(), StatusCode::UNAUTHORIZED);

    let oversized = app
        .oneshot(bearer_raw_req(
            "POST",
            "/api/mcp/message",
            Some(&token),
            &oversized_body,
        ))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn studio_mcp_router_exposes_tools_and_rejects_unknown_request_fields() {
    let db = setup_db().await;
    let session = test_support::user_with_role_session(&db, "owner").await;
    let app = crate::api::router(test_support::build(db));

    let tools = app
        .clone()
        .oneshot(cookie_req("GET", "/api/studio/mcp/tools", &session, None))
        .await
        .unwrap();
    assert_eq!(tools.status(), StatusCode::OK);
    let body = axum::body::to_bytes(tools.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "container.start"));

    let invalid = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "list_nodes",
                "arguments": {},
                "unexpected": true
            })),
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = axum::body::to_bytes(invalid.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"]["code"], "unprocessable_entity");

    let malformed = app
        .clone()
        .oneshot(cookie_raw_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            "application/json",
            "{",
        ))
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

    let unsupported = app
        .clone()
        .oneshot(cookie_raw_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            "text/plain",
            "{}",
        ))
        .await
        .unwrap();
    assert_eq!(unsupported.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let oversized_body = "x".repeat(65 * 1024);
    let unauthenticated_oversized = app
        .clone()
        .oneshot(cookie_raw_req(
            "POST",
            "/api/studio/mcp/invoke",
            "missing-session",
            "application/json",
            &oversized_body,
        ))
        .await
        .unwrap();
    assert_eq!(unauthenticated_oversized.status(), StatusCode::UNAUTHORIZED);

    let oversized = app
        .oneshot(cookie_raw_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            "application/json",
            &oversized_body,
        ))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
