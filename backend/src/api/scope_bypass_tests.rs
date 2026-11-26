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

use anyhow::Result;
use async_trait::async_trait;
use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{header, Request, StatusCode},
};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
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

struct RouterPlanningAdapter {
    plan_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl crate::operations::adapters::OperationAdapter for RouterPlanningAdapter {
    fn key(&self) -> &'static str {
        "containers"
    }

    fn actions(&self) -> &[&'static str] {
        &["container.start", "container.stop", "container.restart"]
    }

    async fn plan(
        &self,
        request: crate::operations::adapters::PlanRequest,
    ) -> Result<crate::operations::contracts::OperationPlanV1> {
        self.plan_calls.fetch_add(1, Ordering::SeqCst);
        Ok(crate::operations::contracts::OperationPlanV1 {
            schema_version: 1,
            title: format!("{} {}", request.action, request.resource.display_name),
            risk: "mutate".into(),
            changes: vec![],
            preview: None,
            external_fingerprint: "router-test-fingerprint".into(),
            steps: vec![crate::operations::contracts::PlannedStepV1 {
                kind: "execute".into(),
                name: request.action,
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
            }],
        })
    }

    async fn external_fingerprint(
        &self,
        _request: &crate::operations::adapters::PlanRequest,
    ) -> Result<String> {
        Ok("router-test-fingerprint".into())
    }

    async fn execute_step(
        &self,
        _request: crate::operations::adapters::StepRequest,
    ) -> Result<crate::operations::adapters::StepOutcome> {
        unreachable!("router proof must stop at durable submission")
    }

    async fn reconcile(
        &self,
        _request: crate::operations::adapters::StepRequest,
    ) -> Result<crate::operations::adapters::ReconcileOutcome> {
        unreachable!("router proof must stop at durable submission")
    }
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
async fn built_in_mcp_message_accepts_lifecycle_scope_before_resource_lookup() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let token = insert_token(&db, &owner, &["containers:restart"]).await;
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
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": "missing-resource",
                        "request_id": "mcp-router-stop"
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
    assert_eq!(json["result"]["isError"], true);
    let text = json["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(text, "Error: resource not found");
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
    for name in ["container.stop", "container.restart"] {
        assert!(json["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == name));
    }

    let mutation = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.restart",
                "arguments": {
                    "resource_id": "missing-resource",
                    "request_id": "studio-router-restart"
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(mutation.status(), StatusCode::OK);
    let body = axum::body::to_bytes(mutation.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"], "resource not found");

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

/// M1-02 exit criterion: an action the built-in surface does not expose must fail closed on both
/// ingresses with the same bounded, non-enumerating refusal, and must never reach planning, job
/// persistence, approval, audit, or event emission. `container.remove`, `service.restart`,
/// `automation.run`, and `container.compose.apply` are real registered actions that are not
/// exposed to MCP/Studio; `not.a.tool` was never registered. All of them must refuse identically
/// so the refusal cannot be used as a registry oracle.
#[tokio::test]
async fn unknown_and_non_exposed_tool_names_fail_closed_on_both_built_in_mcp_ingresses() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (_, token) = insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    for name in [
        "container.remove",
        "service.restart",
        "automation.run",
        "container.compose.apply",
        "not.a.tool",
    ] {
        let bearer = app
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
                        "name": name,
                        "arguments": {
                            "resource_id": "unused-resource",
                            "request_id": "unknown-tool-mcp"
                        }
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(
            bearer.status(),
            StatusCode::OK,
            "{name} must be answered as a JSON-RPC tool error"
        );
        let body = axum::body::to_bytes(bearer.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["result"]["isError"], true, "{name} must fail closed");
        let text = json["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, format!("Error: Unknown tool: {name}"));
        assert!(text.chars().count() <= 4096);

        let studio = app
            .clone()
            .oneshot(cookie_req(
                "POST",
                "/api/studio/mcp/invoke",
                &session,
                Some(serde_json::json!({
                    "name": name,
                    "arguments": {
                        "resource_id": "unused-resource",
                        "request_id": "unknown-tool-studio"
                    }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(
            studio.status(),
            StatusCode::OK,
            "{name} must be answered as a Studio tool error"
        );
        let body = axum::body::to_bytes(studio.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false, "{name} must fail closed");
        assert_eq!(json["error"], format!("Unknown tool: {name}"));
    }

    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        0,
        "a refused tool name must never reach provider planning"
    );

    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db)
        .await
        .unwrap();
    let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM approvals")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(jobs, 0, "a refused tool name must not persist a job");
    assert_eq!(
        approvals, 0,
        "a refused tool name must not request an approval"
    );

    let job_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type LIKE 'job.%' OR event_type LIKE 'approval.%'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        job_events, 0,
        "a refused tool name must not emit job events"
    );

    let audited: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE action IN
         ('container.remove', 'service.restart', 'automation.run',
          'container.compose.apply', 'not.a.tool')",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert!(
        audited.is_empty(),
        "a refused tool name must not write an audit record: {audited:?}"
    );
}

#[tokio::test]
async fn lifecycle_routers_persist_canonical_job_audit_and_event_correlations() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    let resource = crate::operations::resources::observe(
        &db,
        crate::operations::resources::ObserveResource {
            kind: "container",
            display_name: "router-proof",
            node_id: None,
            provider: Some("test"),
            namespace: "router-proof",
            scope_key: "containers",
            alias: "router-proof",
        },
        None,
        "router-proof-resource",
    )
    .await
    .unwrap();
    for action in ["container.stop", "container.restart"] {
        crate::operations::resources::set_capability(
            &db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            "router-proof-capability",
        )
        .await
        .unwrap();
    }

    for action in ["container.stop", "container.restart"] {
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist
             (id, actor_type, action, resource_type, created_at)
             VALUES (?, 'api_token', ?, 'container', ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action)
        .bind(unix_now())
        .execute(&db)
        .await
        .unwrap();
    }

    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: Arc::new(AtomicUsize::new(0)),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);
    let resource_id = resource.id.clone();

    let mcp = app
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
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource_id,
                        "request_id": "router-proof-mcp"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(mcp.status(), StatusCode::OK);
    let mcp_body = axum::body::to_bytes(mcp.into_body(), usize::MAX)
        .await
        .unwrap();
    let mcp_json: serde_json::Value = serde_json::from_slice(&mcp_body).unwrap();
    assert!(mcp_json["result"]["content"][0]["text"].is_string());
    let mcp_job: serde_json::Value =
        serde_json::from_str(mcp_json["result"]["content"][0]["text"].as_str().unwrap()).unwrap();

    let studio = app
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.restart",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "router-proof-studio"
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(studio.status(), StatusCode::OK);
    let studio_body = axum::body::to_bytes(studio.into_body(), usize::MAX)
        .await
        .unwrap();
    let studio_json: serde_json::Value = serde_json::from_slice(&studio_body).unwrap();
    let studio_job: serde_json::Value =
        serde_json::from_str(studio_json["result"].as_str().unwrap()).unwrap();

    for (job, action, ingress, actor_type, actor_id) in [
        (
            &mcp_job,
            "container.stop",
            "mcp",
            "api_token",
            token_id.as_str(),
        ),
        (
            &studio_job,
            "container.restart",
            "studio",
            "human",
            owner.as_str(),
        ),
    ] {
        let job_id = job["id"].as_str().unwrap();
        assert_eq!(job["action"], action);
        assert_eq!(job["resource"]["id"], resource.id);
        let row: (String, String, String, String, String, String) = sqlx::query_as(
            "SELECT action, resource_id, actor_type, actor_id, ingress, state FROM jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(row.0, action);
        assert_eq!(row.1, resource.id);
        assert_eq!(row.2, actor_type);
        assert_eq!(row.3, actor_id);
        assert_eq!(row.4, ingress);
        assert!(matches!(row.5.as_str(), "queued" | "awaiting_approval"));

        let audit: (String, String, String, String, String) = sqlx::query_as(
            "SELECT actor_type, action, resource_id, outcome, request_id FROM audit_log WHERE request_id = ?",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(audit.0, actor_type);
        assert_eq!(audit.1, action);
        assert_eq!(audit.2, resource.id);
        assert!(!audit.3.is_empty());
        assert_eq!(audit.4, job_id);

        let event_types: Vec<String> = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE correlation_id = ? ORDER BY sequence",
        )
        .bind(job_id)
        .fetch_all(&db)
        .await
        .unwrap();
        assert!(event_types.iter().any(|event| event == "job.submitted.v1"));
        assert!(event_types
            .iter()
            .any(|event| event == "job.queued.v1" || event == "job.awaiting_approval.v1"));
    }
}

#[tokio::test]
async fn lifecycle_routers_cover_start_payload_idempotency_and_scope_isolation() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let other_owner = insert_user(&db, "owner").await;
    let (token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let (other_token_id, other_token) =
        insert_token_with_expiry(&db, &other_owner, &["containers:restart"], None).await;
    let read_token = insert_token(&db, &owner, &["containers:read"]).await;
    let session = insert_session(&db, &owner).await;
    let resource = crate::operations::resources::observe(
        &db,
        crate::operations::resources::ObserveResource {
            kind: "container",
            display_name: "router-proof-coverage",
            node_id: None,
            provider: Some("test"),
            namespace: "router-proof-coverage",
            scope_key: "containers",
            alias: "router-proof-coverage",
        },
        None,
        "router-proof-coverage-resource",
    )
    .await
    .unwrap();
    for action in ["container.start", "container.stop", "container.restart"] {
        crate::operations::resources::set_capability(
            &db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            "router-proof-coverage-capability",
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist
             (id, actor_type, action, resource_type, created_at)
             VALUES (?, 'api_token', ?, 'container', ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action)
        .bind(unix_now())
        .execute(&db)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);
    let resource_id = resource.id.clone();

    let start = app
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
                    "name": "container.start",
                    "arguments": {
                        "resource_id": resource_id,
                        "request_id": "lifecycle-start"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::OK);
    let start_body = axum::body::to_bytes(start.into_body(), usize::MAX)
        .await
        .unwrap();
    let start_json: serde_json::Value = serde_json::from_slice(&start_body).unwrap();
    let start_job: serde_json::Value =
        serde_json::from_str(start_json["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(start_job["action"], "container.start");

    let stop = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "lifecycle-stop"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(stop.status(), StatusCode::OK);
    let stop_body = axum::body::to_bytes(stop.into_body(), usize::MAX)
        .await
        .unwrap();
    let stop_json: serde_json::Value = serde_json::from_slice(&stop_body).unwrap();
    let stop_job: serde_json::Value =
        serde_json::from_str(stop_json["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(stop_job["action"], "container.stop");
    assert_eq!(stop_job["actor"]["id"], token_id);

    let restart = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.restart",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "lifecycle-restart"
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(restart.status(), StatusCode::OK);
    let restart_body = axum::body::to_bytes(restart.into_body(), usize::MAX)
        .await
        .unwrap();
    let restart_json: serde_json::Value = serde_json::from_slice(&restart_body).unwrap();
    assert_eq!(restart_json["ok"], true);
    let restart_job: serde_json::Value =
        serde_json::from_str(restart_json["result"].as_str().unwrap()).unwrap();
    assert_eq!(restart_job["action"], "container.restart");
    assert_eq!(plan_calls.load(Ordering::SeqCst), 3);

    let restart_replay = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.restart",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "lifecycle-restart"
                }
            })),
        ))
        .await
        .unwrap();
    let replay_body = axum::body::to_bytes(restart_replay.into_body(), usize::MAX)
        .await
        .unwrap();
    let replay_json: serde_json::Value = serde_json::from_slice(&replay_body).unwrap();
    assert_eq!(replay_json["ok"], true);
    let replay_job: serde_json::Value =
        serde_json::from_str(replay_json["result"].as_str().unwrap()).unwrap();
    assert_eq!(replay_job["id"], restart_job["id"]);
    assert_eq!(plan_calls.load(Ordering::SeqCst), 3);

    let changed_intent = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.start",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "lifecycle-restart"
                }
            })),
        ))
        .await
        .unwrap();
    let changed_body = axum::body::to_bytes(changed_intent.into_body(), usize::MAX)
        .await
        .unwrap();
    let changed_json: serde_json::Value = serde_json::from_slice(&changed_body).unwrap();
    assert_eq!(changed_json["ok"], false);
    assert!(changed_json["error"]
        .as_str()
        .unwrap()
        .contains("idempotency"));
    assert_eq!(plan_calls.load(Ordering::SeqCst), 3);

    let isolated_actor = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &other_token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "lifecycle-stop"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    let isolated_body = axum::body::to_bytes(isolated_actor.into_body(), usize::MAX)
        .await
        .unwrap();
    let isolated_json: serde_json::Value = serde_json::from_slice(&isolated_body).unwrap();
    let isolated_job: serde_json::Value = serde_json::from_str(
        isolated_json["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_ne!(isolated_job["id"], stop_job["id"]);
    assert_eq!(isolated_job["actor"]["id"], other_token_id);
    assert_eq!(plan_calls.load(Ordering::SeqCst), 4);

    let wrong_scope = app
        .clone()
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            &read_token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 4,
                "method": "tools/call",
                "params": {
                    "name": "container.restart",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "lifecycle-wrong-scope"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    let wrong_scope_body = axum::body::to_bytes(wrong_scope.into_body(), usize::MAX)
        .await
        .unwrap();
    let wrong_scope_json: serde_json::Value = serde_json::from_slice(&wrong_scope_body).unwrap();
    assert_eq!(wrong_scope_json["result"]["isError"], true);
    assert!(wrong_scope_json["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("scope"));
    assert_eq!(plan_calls.load(Ordering::SeqCst), 4);

    for (job_id, action) in [
        (start_job["id"].as_str().unwrap(), "container.start"),
        (stop_job["id"].as_str().unwrap(), "container.stop"),
        (restart_job["id"].as_str().unwrap(), "container.restart"),
        (isolated_job["id"].as_str().unwrap(), "container.stop"),
    ] {
        let event_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT event_type, payload_json FROM events WHERE correlation_id = ? ORDER BY sequence",
        )
        .bind(job_id)
        .fetch_all(&db)
        .await
        .unwrap();
        for event_type in ["job.submitted.v1", "job.queued.v1"] {
            let (_, payload_json) = event_rows
                .iter()
                .find(|(kind, _)| kind == event_type)
                .unwrap_or_else(|| panic!("missing {event_type} for {job_id}"));
            let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap();
            assert_eq!(payload["action"], action);
            assert_eq!(payload["resource_id"], resource.id);
            assert!(payload["state"].is_string());
        }
    }
    assert_eq!(token_id.len(), 36);
}

#[tokio::test]
async fn lifecycle_routers_persist_approval_rows_and_non_denied_policy_results() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (_, token) = insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    let resource = crate::operations::resources::observe(
        &db,
        crate::operations::resources::ObserveResource {
            kind: "container",
            display_name: "router-approval-proof",
            node_id: None,
            provider: Some("test"),
            namespace: "router-approval-proof",
            scope_key: "containers",
            alias: "router-approval-proof",
        },
        None,
        "router-approval-proof-resource",
    )
    .await
    .unwrap();
    for action in ["container.stop", "container.restart"] {
        crate::operations::resources::set_capability(
            &db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            "router-approval-proof-capability",
        )
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) VALUES ('global', 'assisted', ?)",
    )
    .bind(unix_now())
    .execute(&db)
    .await
    .unwrap();

    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: Arc::new(AtomicUsize::new(0)),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    let mcp = app
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
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "router-approval-mcp"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(mcp.status(), StatusCode::OK);
    let mcp_body = axum::body::to_bytes(mcp.into_body(), usize::MAX)
        .await
        .unwrap();
    let mcp_json: serde_json::Value = serde_json::from_slice(&mcp_body).unwrap();
    let mcp_job: serde_json::Value =
        serde_json::from_str(mcp_json["result"]["content"][0]["text"].as_str().unwrap()).unwrap();

    let studio = app
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.restart",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "router-approval-studio"
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(studio.status(), StatusCode::OK);
    let studio_body = axum::body::to_bytes(studio.into_body(), usize::MAX)
        .await
        .unwrap();
    let studio_json: serde_json::Value = serde_json::from_slice(&studio_body).unwrap();
    let studio_job: serde_json::Value =
        serde_json::from_str(studio_json["result"].as_str().unwrap()).unwrap();

    for (job, action) in [
        (&mcp_job, "container.stop"),
        (&studio_job, "container.restart"),
    ] {
        assert!(
            job["error"].is_null(),
            "awaiting approval must not carry a policy error"
        );
        let job_id = job["id"].as_str().unwrap();
        assert_eq!(job["action"], action);
        assert_eq!(job["state"], "awaiting_approval");
        let approval_id = job["approval_id"].as_str().unwrap();

        let persisted_job: (String, Option<String>, Option<String>, String, String, String, i64) =
            sqlx::query_as(
                "SELECT state, error_code, error_message, request_digest, plan_digest, external_fingerprint, resource_revision
                 FROM jobs WHERE id = ?",
            )
            .bind(job_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(persisted_job.0, "awaiting_approval");
        assert_eq!(persisted_job.1, None);
        assert!(persisted_job
            .2
            .as_deref()
            .unwrap_or_default()
            .contains("approval"));

        let approval: (String, String, String, String, String, i64, String) = sqlx::query_as(
            "SELECT job_id, status, requirement, reason, request_digest, resource_revision, external_fingerprint
             FROM approvals WHERE id = ?",
        )
        .bind(approval_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(approval.0, job_id);
        assert_eq!(approval.1, "pending");
        assert_eq!(approval.2, "risk_ladder");
        assert!(!approval.3.is_empty());
        assert_eq!(approval.4, persisted_job.3);
        assert_eq!(approval.5, persisted_job.6);
        assert_eq!(approval.6, persisted_job.5);

        let audit: (String, String, String, String) = sqlx::query_as(
            "SELECT action, resource_id, outcome, request_id FROM audit_log WHERE request_id = ?",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(audit.0, action);
        assert_eq!(audit.1, resource.id);
        assert_eq!(audit.2, "pending_approval");
        assert_eq!(audit.3, job_id);

        let event_types: Vec<String> = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE correlation_id = ? ORDER BY sequence",
        )
        .bind(job_id)
        .fetch_all(&db)
        .await
        .unwrap();
        assert!(event_types
            .iter()
            .any(|event| event == "approval.requested.v1"));
        assert!(event_types
            .iter()
            .any(|event| event == "job.awaiting_approval.v1"));
    }
}

/// M1-02 exit criterion (handoff 0036): the built-in machine ingresses must persist a *policy
/// denial* — not only an approval requirement — through the canonical job boundary. An Observer
/// mode denial has to produce a rejected job with the canonical `policy_denied` error, no approval
/// row, no execution, linked audit and durable events, and the exact actor/ingress identity of the
/// ingress that submitted it.
#[tokio::test]
async fn built_in_mcp_and_studio_policy_denials_persist_rejected_jobs() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (_token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    let resource = crate::operations::resources::observe(
        &db,
        crate::operations::resources::ObserveResource {
            kind: "container",
            display_name: "router-denial-proof",
            node_id: None,
            provider: Some("test"),
            namespace: "router-denial-proof",
            scope_key: "containers",
            alias: "router-denial-proof",
        },
        None,
        "router-denial-proof-resource",
    )
    .await
    .unwrap();
    for action in ["container.stop", "container.restart"] {
        crate::operations::resources::set_capability(
            &db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            "router-denial-proof-capability",
        )
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) \
         VALUES ('global', 'observer', ?)",
    )
    .bind(unix_now())
    .execute(&db)
    .await
    .unwrap();

    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    let mcp = app
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
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "router-denial-mcp"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(mcp.status(), StatusCode::OK);
    let mcp_body = axum::body::to_bytes(mcp.into_body(), usize::MAX)
        .await
        .unwrap();
    let mcp_json: serde_json::Value = serde_json::from_slice(&mcp_body).unwrap();
    assert!(
        !mcp_json["result"]["isError"].as_bool().unwrap_or(false),
        "a canonical rejected job is a successful submission, not a JSON-RPC tool error"
    );
    let mcp_job: serde_json::Value = serde_json::from_str(
        mcp_json["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP submission must return the canonical job"),
    )
    .unwrap();

    let studio = app
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.restart",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "router-denial-studio"
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(studio.status(), StatusCode::OK);
    let studio_body = axum::body::to_bytes(studio.into_body(), usize::MAX)
        .await
        .unwrap();
    let studio_json: serde_json::Value = serde_json::from_slice(&studio_body).unwrap();
    assert_eq!(studio_json["ok"], serde_json::json!(true));
    let studio_job: serde_json::Value =
        serde_json::from_str(studio_json["result"].as_str().unwrap()).unwrap();

    for (job, action, ingress, actor_type, actor_source, audit_user) in [
        (
            &mcp_job,
            "container.stop",
            "mcp",
            "api_token",
            "mcp",
            None::<&str>,
        ),
        (
            &studio_job,
            "container.restart",
            "studio",
            "human",
            "studio",
            Some(owner.as_str()),
        ),
    ] {
        assert_eq!(job["state"], "rejected");
        assert_eq!(job["action"], action);
        assert_eq!(job["ingress"], ingress);
        assert_eq!(job["actor"]["actor_type"], actor_type);
        assert_eq!(job["actor"]["source"], actor_source);
        assert!(
            job["approval_id"].is_null(),
            "a denied submission must not create an approval"
        );
        assert_eq!(job["error"]["code"], "policy_denied");
        assert!(job["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Observer mode"));

        let job_id = job["id"].as_str().unwrap();
        let persisted: (
            String,
            Option<String>,
            String,
            String,
            String,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT j.state, j.error_code, j.ingress, j.actor_type, j.actor_source, a.id \
                 FROM jobs j LEFT JOIN approvals a ON a.job_id = j.id WHERE j.id = ?",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(persisted.0, "rejected");
        assert_eq!(persisted.1.as_deref(), Some("policy_denied"));
        assert_eq!(persisted.2, ingress);
        assert_eq!(persisted.3, actor_type);
        assert_eq!(persisted.4, actor_source);
        assert_eq!(persisted.5, None);

        let audit: (String, String, String, Option<String>) = sqlx::query_as(
            "SELECT action, resource_id, outcome, user_id FROM audit_log WHERE request_id = ?",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(audit.0, action);
        assert_eq!(audit.1, resource.id);
        assert_eq!(audit.2, "blocked");
        assert_eq!(audit.3.as_deref(), audit_user);

        let event_types: Vec<String> = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE correlation_id = ? ORDER BY sequence",
        )
        .bind(job_id)
        .fetch_all(&db)
        .await
        .unwrap();
        assert!(event_types.iter().any(|event| event == "job.submitted.v1"));
        assert!(event_types.iter().any(|event| event == "job.rejected.v1"));
        assert!(!event_types
            .iter()
            .any(|event| event == "approval.requested.v1"));
    }

    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db)
        .await
        .unwrap();
    let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM approvals")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(jobs, 2);
    assert_eq!(approvals, 0);
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        2,
        "each denial is planned once and then rejected before any provider execution"
    );
}

/// M1-02 exit criterion: a registered action aimed at the wrong canonical resource kind must be
/// refused on both built-in ingresses before planning. The resource deliberately advertises the
/// capability, so the refusal can only come from the canonical kind check, and it must leave no
/// job, approval, audit, or event residue.
#[tokio::test]
async fn built_in_mcp_and_studio_reject_mismatched_resource_kinds_before_planning() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (_token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    let resource = crate::operations::resources::observe(
        &db,
        crate::operations::resources::ObserveResource {
            kind: "service",
            display_name: "router-kind-proof",
            node_id: None,
            provider: Some("test"),
            namespace: "router-kind-proof",
            scope_key: "services",
            alias: "router-kind-proof",
        },
        None,
        "router-kind-proof-resource",
    )
    .await
    .unwrap();
    crate::operations::resources::set_capability(
        &db,
        &resource.id,
        "container.stop",
        crate::operations::contracts::CapabilityAvailability::Available,
        None,
        None,
        "router-kind-proof-capability",
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    let mcp = app
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
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "router-kind-mcp"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(mcp.status(), StatusCode::OK);
    let mcp_body = axum::body::to_bytes(mcp.into_body(), usize::MAX)
        .await
        .unwrap();
    let mcp_json: serde_json::Value = serde_json::from_slice(&mcp_body).unwrap();
    assert_eq!(mcp_json["result"]["isError"], serde_json::json!(true));
    assert_eq!(
        mcp_json["result"]["content"][0]["text"],
        "Error: action and resource kinds do not match"
    );

    let studio = app
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "container.stop",
                "arguments": {
                    "resource_id": resource.id,
                    "request_id": "router-kind-studio"
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(studio.status(), StatusCode::OK);
    let studio_body = axum::body::to_bytes(studio.into_body(), usize::MAX)
        .await
        .unwrap();
    let studio_json: serde_json::Value = serde_json::from_slice(&studio_body).unwrap();
    assert_eq!(studio_json["ok"], serde_json::json!(false));
    assert_eq!(
        studio_json["error"],
        "action and resource kinds do not match"
    );

    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        0,
        "a kind mismatch must be refused before any plan is derived"
    );
    for table in ["jobs", "approvals", "audit_log"] {
        let count: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(count, 0, "a kind mismatch must not persist into {table}");
    }
    // Resource observation and capability registration legitimately emit their own events; the
    // refused invocation must contribute none of its own.
    let invocation_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE job_id IS NOT NULL \
         OR approval_id IS NOT NULL OR event_type LIKE 'job.%' OR event_type LIKE 'approval.%'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        invocation_events, 0,
        "a kind mismatch must not emit job or approval events"
    );
}

/// M1-02 exit criterion: an approval created by an MCP-submitted job must be decidable by a human
/// session and resume that *same* job — same id, request/plan digest, resource revision, and
/// idempotency key. A non-admin session is refused and leaves the approval pending.
#[tokio::test]
async fn human_session_approval_resumes_the_mcp_submitted_job() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let member = insert_user(&db, "member").await;
    let (_token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let owner_session = insert_session(&db, &owner).await;
    let member_session = insert_session(&db, &member).await;
    let resource = crate::operations::resources::observe(
        &db,
        crate::operations::resources::ObserveResource {
            kind: "container",
            display_name: "router-approval-resume",
            node_id: None,
            provider: Some("test"),
            namespace: "router-approval-resume",
            scope_key: "containers",
            alias: "router-approval-resume",
        },
        None,
        "router-approval-resume-resource",
    )
    .await
    .unwrap();
    crate::operations::resources::set_capability(
        &db,
        &resource.id,
        "container.stop",
        crate::operations::contracts::CapabilityAvailability::Available,
        None,
        None,
        "router-approval-resume-capability",
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) \
         VALUES ('global', 'assisted', ?)",
    )
    .bind(unix_now())
    .execute(&db)
    .await
    .unwrap();

    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: Arc::new(AtomicUsize::new(0)),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    let mcp = app
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
                    "name": "container.stop",
                    "arguments": {
                        "resource_id": resource.id,
                        "request_id": "router-resume-mcp"
                    }
                }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(mcp.status(), StatusCode::OK);
    let mcp_body = axum::body::to_bytes(mcp.into_body(), usize::MAX)
        .await
        .unwrap();
    let mcp_json: serde_json::Value = serde_json::from_slice(&mcp_body).unwrap();
    let job: serde_json::Value = serde_json::from_str(
        mcp_json["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP submission must return the canonical job"),
    )
    .unwrap();
    assert_eq!(job["state"], "awaiting_approval");
    assert_eq!(job["ingress"], "mcp");
    let job_id = job["id"].as_str().unwrap().to_string();
    let approval_id = job["approval_id"]
        .as_str()
        .expect("an approval-required submission must name its approval")
        .to_string();

    let before: (String, String, i64, String) = sqlx::query_as(
        "SELECT request_digest, plan_digest, resource_revision, idempotency_key \
         FROM jobs WHERE id = ?",
    )
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();

    let pending: (String, Option<String>) =
        sqlx::query_as("SELECT status, decided_by FROM approvals WHERE id = ?")
            .bind(&approval_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(pending.0, "pending");
    assert_eq!(pending.1, None);

    let residue_before_member_attempt = operation_residue(&db).await;
    let member_attempt = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            &format!("/api/approvals/{approval_id}/approve"),
            &member_session,
            Some(serde_json::json!({ "comment": "member attempt" })),
        ))
        .await
        .unwrap();
    assert_eq!(
        member_attempt.status(),
        StatusCode::FORBIDDEN,
        "a non-admin session must not decide a machine-submitted approval"
    );
    assert_eq!(
        operation_residue(&db).await,
        residue_before_member_attempt,
        "a refused approval decision must not write a job, approval, audit or event row"
    );
    let still_pending: (String, String) = sqlx::query_as(
        "SELECT a.status, j.state FROM approvals a JOIN jobs j ON j.id = a.job_id WHERE a.id = ?",
    )
    .bind(&approval_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(still_pending.0, "pending");
    assert_eq!(still_pending.1, "awaiting_approval");

    let approved = app
        .clone()
        .oneshot(cookie_req(
            "POST",
            &format!("/api/approvals/{approval_id}/approve"),
            &owner_session,
            Some(serde_json::json!({ "comment": "router resume" })),
        ))
        .await
        .unwrap();
    assert_eq!(approved.status(), StatusCode::OK);
    let approved_body = axum::body::to_bytes(approved.into_body(), usize::MAX)
        .await
        .unwrap();
    let approved_json: serde_json::Value = serde_json::from_slice(&approved_body).unwrap();
    assert_eq!(approved_json["schema_version"], 1);
    assert_eq!(approved_json["resource_id"], resource.id);
    assert_eq!(approved_json["action"], "container.stop");
    assert_eq!(approved_json["job"]["id"], job_id.as_str());
    assert_eq!(approved_json["job"]["state"], "queued");

    let after: (String, Option<String>, Option<i64>) =
        sqlx::query_as("SELECT state, error_code, queued_at FROM jobs WHERE id = ?")
            .bind(&job_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(after.0, "queued");
    assert_eq!(after.1, None);
    assert!(after.2.is_some(), "an approved job must become queued");

    let after_plan: (String, String, i64, String) = sqlx::query_as(
        "SELECT request_digest, plan_digest, resource_revision, idempotency_key \
         FROM jobs WHERE id = ?",
    )
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        (&after_plan.0, &after_plan.1, after_plan.2, &after_plan.3),
        (&before.0, &before.1, before.2, &before.3),
        "approval must resume the same immutable plan, not replan the intent"
    );

    let bound_approval: Option<String> =
        sqlx::query_scalar("SELECT id FROM approvals WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(bound_approval.as_deref(), Some(approval_id.as_str()));

    let decided: (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT status, decided_by, decision_comment FROM approvals WHERE id = ?")
            .bind(&approval_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(decided.0, "approved");
    assert_eq!(decided.1.as_deref(), Some(owner.as_str()));
    assert_eq!(decided.2.as_deref(), Some("router resume"));

    let audit: (String, String, String, String, Option<String>) = sqlx::query_as(
        "SELECT action, resource_type, resource_id, outcome, user_id \
         FROM audit_log WHERE request_id = ? AND action = 'approval.approve'",
    )
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(audit.0, "approval.approve");
    assert_eq!(audit.1, "approval");
    assert_eq!(audit.2, approval_id);
    assert_eq!(audit.3, "approved");
    assert_eq!(audit.4.as_deref(), Some(owner.as_str()));

    let event_types: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM events WHERE correlation_id = ? ORDER BY sequence",
    )
    .bind(&job_id)
    .fetch_all(&db)
    .await
    .unwrap();
    assert!(event_types
        .iter()
        .any(|event| event == "approval.approved.v1"));
    assert!(event_types.iter().any(|event| event == "job.queued.v1"));

    let replay = app
        .oneshot(cookie_req(
            "POST",
            &format!("/api/approvals/{approval_id}/approve"),
            &owner_session,
            Some(serde_json::json!({ "comment": "router resume" })),
        ))
        .await
        .unwrap();
    assert_eq!(
        replay.status(),
        StatusCode::CONFLICT,
        "a decided approval must not be replayable"
    );
}

/// Observes one container resource, marks every container lifecycle capability available, and
/// allowlists the `api_token` actor for those actions so machine submissions reach the canonical
/// policy as `queued` instead of being default-denied.
async fn observe_allowlisted_container(
    db: &SqlitePool,
    label: &str,
) -> crate::operations::contracts::ResourceRef {
    let resource = crate::operations::resources::observe(
        db,
        crate::operations::resources::ObserveResource {
            kind: "container",
            display_name: label,
            node_id: None,
            provider: Some("test"),
            namespace: label,
            scope_key: "containers",
            alias: label,
        },
        None,
        &format!("{label}-resource"),
    )
    .await
    .unwrap();
    for action in ["container.start", "container.stop", "container.restart"] {
        crate::operations::resources::set_capability(
            db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            &format!("{label}-capability"),
        )
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist
             (id, actor_type, action, resource_type, created_at)
             VALUES (?, 'api_token', ?, 'container', ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action)
        .bind(unix_now())
        .execute(db)
        .await
        .unwrap();
    }
    resource
}

/// One `tools/call` through the real bearer-authenticated `/api/mcp/message` route.
async fn mcp_tool_json(
    app: axum::Router,
    token: &str,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let response = app
        .oneshot(bearer_req(
            "POST",
            "/api/mcp/message",
            token,
            Some(serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// One Studio panel invocation through the real session-authenticated route.
async fn studio_tool_json(
    app: axum::Router,
    session: &str,
    name: &str,
    arguments: serde_json::Value,
) -> serde_json::Value {
    let response = app
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            session,
            Some(serde_json::json!({ "name": name, "arguments": arguments })),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// One canonical action submission through the real bearer-authenticated HTTP action route.
async fn http_action_submit(
    app: axum::Router,
    token: &str,
    resource_id: &str,
    action: &str,
    idempotency_key: &str,
) -> (StatusCode, serde_json::Value) {
    let response = app
        .oneshot(with_connect_info(
            Request::builder()
                .method("POST")
                .uri(format!("/api/resources/{resource_id}/actions/{action}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", idempotency_key)
                .body(Body::from(r#"{"input":{}}"#))
                .unwrap(),
        ))
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

/// `(jobs, approvals, audit_log, events)` row counts, used to prove a refusal writes no residue.
async fn operation_residue(db: &SqlitePool) -> (i64, i64, i64, i64) {
    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(db)
        .await
        .unwrap();
    let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM approvals")
        .fetch_one(db)
        .await
        .unwrap();
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(db)
        .await
        .unwrap();
    let events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(db)
        .await
        .unwrap();
    (jobs, approvals, audits, events)
}

/// M1-02 residual clause — "Disable mutation exposure on uncertain recovery".
///
/// While a canonical resource has a job in `needs_attention`, machine ingresses must not accept a
/// new operation intent for that resource: the refusal happens before planning, writes no job,
/// approval, audit or event residue, and does not name the blocking job. The same-key replay of
/// the blocked job must still return it (recovery by query, never blind replay), an unrelated
/// resource must stay usable (the refusal is resource-scoped, not a global lockout), and the
/// refusal must clear as soon as the uncertain job is reconciled.
#[tokio::test]
async fn machine_ingresses_refuse_new_work_while_a_resource_operation_is_unresolved() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (_token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    let blocked = observe_allowlisted_container(&db, "uncertain-proof-a").await;
    let unrelated = observe_allowlisted_container(&db, "uncertain-proof-b").await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    // A machine submission succeeds while the resource has no unresolved operation.
    let first = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": blocked.id, "request_id": "uncertain-first" }),
    )
    .await;
    assert!(
        !first["result"]["isError"].as_bool().unwrap_or(false),
        "the first submission must be accepted: {first}"
    );
    let first_job: serde_json::Value = serde_json::from_str(
        first["result"]["content"][0]["text"]
            .as_str()
            .expect("MCP submission must return the canonical job"),
    )
    .unwrap();
    assert_eq!(first_job["state"], "queued");
    assert_eq!(first_job["action"], "container.start");
    let blocked_job_id = first_job["id"].as_str().unwrap().to_string();
    assert_eq!(plan_calls.load(Ordering::SeqCst), 1);

    // Same-key replay is not refused: recovery-by-query must return the blocked job unchanged.
    let replay = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": blocked.id, "request_id": "uncertain-first" }),
    )
    .await;
    let replay_job: serde_json::Value = serde_json::from_str(
        replay["result"]["content"][0]["text"]
            .as_str()
            .expect("a same-key replay must return the canonical job"),
    )
    .unwrap();
    assert_eq!(replay_job["id"], blocked_job_id.as_str());
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "a same-key replay must not re-plan the intent"
    );

    // The provider outcome for that job becomes uncertain.
    sqlx::query(
        "UPDATE jobs SET state = 'needs_attention', error_code = 'provider_unavailable', \
         error_message = 'outcome unresolved' WHERE id = ?",
    )
    .bind(&blocked_job_id)
    .execute(&db)
    .await
    .unwrap();
    let before = operation_residue(&db).await;

    // Recovery-by-query must still work in the unresolved state itself: the caller that submitted
    // the uncertain job replays its idempotency key and gets the same job back, unchanged, without
    // planning. This pins the guard's placement after the idempotency lookup — a guard moved above
    // it would turn this replay into a refusal and fail here.
    let blocked_replay = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": blocked.id, "request_id": "uncertain-first" }),
    )
    .await;
    assert!(
        !blocked_replay["result"]["isError"]
            .as_bool()
            .unwrap_or(false),
        "a same-key replay must return the blocked job, not a refusal: {blocked_replay}"
    );
    let blocked_replay_job: serde_json::Value = serde_json::from_str(
        blocked_replay["result"]["content"][0]["text"]
            .as_str()
            .expect("a same-key replay in the unresolved state must return the canonical job"),
    )
    .unwrap();
    assert_eq!(blocked_replay_job["id"], blocked_job_id.as_str());
    assert_eq!(blocked_replay_job["state"], "needs_attention");
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "a replay in the unresolved state must not re-plan the intent"
    );

    // Built-in MCP ingress: a new intent for the blocked resource is refused before planning.
    let mcp_refusal = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": blocked.id, "request_id": "uncertain-new-mcp" }),
    )
    .await;
    assert_eq!(
        mcp_refusal["result"]["isError"],
        serde_json::json!(true),
        "a new intent on an unresolved resource must be refused: {mcp_refusal}"
    );
    let mcp_error = mcp_refusal["result"]["content"][0]["text"]
        .as_str()
        .expect("a refused tool call must carry bounded error text");
    assert!(
        mcp_error.contains("unresolved operation"),
        "the refusal must name the unresolved state: {mcp_error}"
    );
    assert!(
        !mcp_error.contains(&blocked_job_id),
        "the refusal must not disclose the blocking job id: {mcp_error}"
    );

    // Studio ingress: the same refusal, with the same bounded text.
    let studio_refusal = studio_tool_json(
        app.clone(),
        &session,
        "container.start",
        serde_json::json!({ "resource_id": blocked.id, "request_id": "uncertain-new-studio" }),
    )
    .await;
    assert_eq!(
        studio_refusal["ok"],
        serde_json::json!(false),
        "a new Studio intent on an unresolved resource must be refused: {studio_refusal}"
    );
    assert!(
        studio_refusal["error"]
            .as_str()
            .unwrap_or_default()
            .contains("unresolved operation"),
        "the Studio refusal must name the unresolved state: {studio_refusal}"
    );

    // Canonical HTTP bearer ingress: a bounded 409 conflict, not a new job.
    let canonical = app
        .clone()
        .oneshot(with_connect_info(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/resources/{}/actions/container.start",
                    blocked.id
                ))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", "uncertain-new-bearer")
                .body(Body::from(r#"{"input":{}}"#))
                .unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(canonical.status(), StatusCode::CONFLICT);
    let canonical_json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(canonical.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(canonical_json["error"]["code"], "unresolved_operation");

    assert_eq!(
        operation_residue(&db).await,
        before,
        "refused submissions must not persist a job, approval, audit or event"
    );
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "the refusal must happen before planning on every machine ingress"
    );

    // A human web session stays admitted on purpose: the operator already holds the resource scope
    // and is not locked out (handoff 0038, limitation 2). For `container.*` the uncertainty clears
    // through worker reconciliation, not an explicit operator decision; `resolve-uncertain` exists
    // only for one-step `service.restart`. This pins the deliberate machine-only scope of the
    // refusal so a future change cannot silently widen it to humans — or narrow it away from
    // machine callers.
    let human = app
        .clone()
        .oneshot(with_connect_info(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/resources/{}/actions/container.start",
                    blocked.id
                ))
                .header(header::COOKIE, format!("vt_session={session}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", "uncertain-human")
                .body(Body::from(r#"{"input":{}}"#))
                .unwrap(),
        ))
        .await
        .unwrap();
    assert_eq!(
        human.status(),
        StatusCode::ACCEPTED,
        "a human session must still be able to submit while a machine caller is refused"
    );
    let human_body = axum::body::to_bytes(human.into_body(), usize::MAX)
        .await
        .unwrap();
    let human_json: serde_json::Value = serde_json::from_slice(&human_body).unwrap();
    assert_eq!(human_json["job"]["state"], "queued");
    assert_eq!(
        human_json["job"]["ingress"], "http_session",
        "the admitted submission must be the human session ingress, not a machine adapter"
    );
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        2,
        "the admitted human submission plans once, unlike the refused machine intents"
    );

    // The refusal is scoped to the affected resource, not a global mutation lockout.
    let unrelated_ok = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": unrelated.id, "request_id": "uncertain-other" }),
    )
    .await;
    assert!(
        !unrelated_ok["result"]["isError"].as_bool().unwrap_or(false),
        "an unrelated resource must stay usable: {unrelated_ok}"
    );
    assert_eq!(plan_calls.load(Ordering::SeqCst), 3);

    // Reconciling the uncertain job clears the refusal for later equivalent intent.
    sqlx::query("UPDATE jobs SET state = 'failed' WHERE id = ?")
        .bind(&blocked_job_id)
        .execute(&db)
        .await
        .unwrap();

    for (label, refusal_retry) in [
        (
            "mcp",
            mcp_tool_json(
                app.clone(),
                &token,
                "container.start",
                serde_json::json!({ "resource_id": blocked.id, "request_id": "uncertain-new-mcp" }),
            )
            .await,
        ),
        (
            "studio",
            studio_tool_json(
                app.clone(),
                &session,
                "container.start",
                serde_json::json!({
                    "resource_id": blocked.id,
                    "request_id": "uncertain-new-studio"
                }),
            )
            .await,
        ),
    ] {
        let accepted = if label == "mcp" {
            !refusal_retry["result"]["isError"]
                .as_bool()
                .unwrap_or(false)
        } else {
            refusal_retry["ok"].as_bool().unwrap_or(false)
        };
        assert!(
            accepted,
            "the {label} refusal must clear once the job is reconciled: {refusal_retry}"
        );
    }
    assert_eq!(plan_calls.load(Ordering::SeqCst), 5);
}

/// M1-02 residual — one mutating intent must have one canonical identity no matter which supported
/// machine ingress carries it.
///
/// `container.start` on the same canonical resource with the same (empty) input, submitted through
/// the bearer JSON-RPC MCP ingress, the Studio session ingress, and the canonical HTTP bearer action
/// route, must resolve to three *distinct* durable jobs — idempotency is actor-scoped, so no ingress
/// may reuse another ingress's row — while sharing exactly one typed action, one canonical
/// `resources.id`, one plan payload, one request digest, one plan digest, one `concurrency_key`,
/// and one policy result. Each job keeps its own actor, ingress, audit row and event correlation,
/// and no ingress reaches the adapter's `execute_step`.
#[tokio::test]
async fn container_start_keeps_one_canonical_identity_across_machine_ingresses() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let (token_id, token) =
        insert_token_with_expiry(&db, &owner, &["containers:restart"], None).await;
    let session = insert_session(&db, &owner).await;
    let resource = observe_allowlisted_container(&db, "ingress-equivalence").await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(RouterPlanningAdapter {
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        0,
        "no ingress may plan before a submission arrives"
    );

    // Ingress 1 — built-in MCP over bearer JSON-RPC.
    let mcp = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": resource.id, "request_id": "equivalence-mcp" }),
    )
    .await;
    assert!(
        !mcp["result"]["isError"].as_bool().unwrap_or(false),
        "the MCP submission must be accepted: {mcp}"
    );
    let mcp_job: serde_json::Value = serde_json::from_str(
        mcp["result"]["content"][0]["text"]
            .as_str()
            .expect("the MCP ingress must return the canonical job"),
    )
    .unwrap();
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "the MCP ingress must plan the intent exactly once"
    );

    // Ingress 2 — Studio session panel.
    let studio = studio_tool_json(
        app.clone(),
        &session,
        "container.start",
        serde_json::json!({ "resource_id": resource.id, "request_id": "equivalence-studio" }),
    )
    .await;
    assert_eq!(
        studio["ok"],
        serde_json::json!(true),
        "the Studio submission must be accepted: {studio}"
    );
    let studio_job: serde_json::Value = serde_json::from_str(
        studio["result"]
            .as_str()
            .expect("the Studio ingress must return the canonical job"),
    )
    .unwrap();
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        2,
        "the Studio ingress must plan the intent exactly once"
    );

    // Ingress 3 — canonical HTTP bearer action route.
    let (http_status, http_json) = http_action_submit(
        app.clone(),
        &token,
        &resource.id,
        "container.start",
        "equivalence-http",
    )
    .await;
    assert_eq!(
        http_status,
        StatusCode::ACCEPTED,
        "the canonical HTTP submission must be accepted: {http_json}"
    );
    let http_job = http_json["job"].clone();
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        3,
        "the HTTP ingress must plan the intent exactly once"
    );

    let jobs = [
        ("mcp", &mcp_job),
        ("studio", &studio_job),
        ("http", &http_job),
    ];

    // Distinct durable rows: an ingress never adopts another ingress's job.
    let mut job_ids: Vec<&str> = jobs
        .iter()
        .map(|(_, job)| {
            job["id"]
                .as_str()
                .expect("every job summary must carry an id")
        })
        .collect();
    job_ids.sort_unstable();
    job_ids.dedup();
    assert_eq!(
        job_ids.len(),
        3,
        "each ingress must own a distinct durable job row"
    );

    for (ingress, job) in jobs {
        assert_eq!(
            job["action"], "container.start",
            "{ingress} must submit the same typed action"
        );
        assert_eq!(
            job["state"], "queued",
            "{ingress} must reach the same policy result"
        );
        assert_eq!(
            job["resource"]["id"], resource.id,
            "{ingress} must resolve the same canonical resource"
        );
        assert_eq!(
            job["approval_id"],
            serde_json::Value::Null,
            "{ingress} must not fabricate an approval for a policy-allowed intent"
        );
    }
    assert_eq!(
        mcp_job["plan"], studio_job["plan"],
        "the immutable plan must not depend on the ingress"
    );
    assert_eq!(
        mcp_job["plan"], http_job["plan"],
        "the immutable plan must not depend on the ingress"
    );

    // One canonical intent identity: action, resource, digest and concurrency key are
    // ingress-independent, while actor scope and ingress stay per-caller.
    let mut addressed = Vec::new();
    let mut digests = Vec::new();
    for (ingress, job) in jobs {
        let job_id = job["id"].as_str().unwrap();
        let address: (String, String, String) =
            sqlx::query_as("SELECT action, resource_id, concurrency_key FROM jobs WHERE id = ?")
                .bind(job_id)
                .fetch_one(&db)
                .await
                .unwrap_or_else(|error| panic!("{ingress} job row must exist: {error}"));
        let digest: (String, String, i64) = sqlx::query_as(
            "SELECT request_digest, plan_digest, resource_revision FROM jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap_or_else(|error| panic!("{ingress} job row must exist: {error}"));
        addressed.push(address);
        digests.push(digest);
    }
    assert_eq!(
        addressed[0], addressed[1],
        "the Studio ingress must not change the canonical intent identity"
    );
    assert_eq!(
        addressed[0], addressed[2],
        "the HTTP ingress must not change the canonical intent identity"
    );
    assert_eq!(
        digests[0], digests[1],
        "the immutable request/plan digest and resource revision must not depend on the ingress"
    );
    assert_eq!(
        digests[0], digests[2],
        "the immutable request/plan digest and resource revision must not depend on the ingress"
    );
    assert_eq!(addressed[0].0, "container.start");
    assert_eq!(addressed[0].1, resource.id);
    assert_eq!(
        addressed[0].2, resource.id,
        "every canonical submission must serialize on the canonical resource id"
    );

    let expected_scopes = [
        (
            format!("v1:mcp:api_token:{token_id}"),
            "api_token",
            Some("mcp"),
            "mcp",
        ),
        (
            format!("v1:studio:human:{owner}"),
            "human",
            Some("studio"),
            "studio",
        ),
        (
            format!("v1:http_bearer:api_token:{token_id}"),
            "api_token",
            Some("http_bearer"),
            "http_bearer",
        ),
    ];
    for ((ingress, job), (scope, actor_type, actor_source, ingress_name)) in
        jobs.iter().zip(expected_scopes)
    {
        let row: (String, String, Option<String>, String) = sqlx::query_as(
            "SELECT idempotency_scope, actor_type, actor_source, ingress FROM jobs WHERE id = ?",
        )
        .bind(job["id"].as_str().unwrap())
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                scope,
                actor_type.to_string(),
                actor_source.map(str::to_string),
                ingress_name.to_string()
            ),
            "{ingress} must keep its own actor scope and ingress identity"
        );
    }

    // Each job keeps its own audit row and durable event correlation. The audited actor stays the
    // submitting caller: canonical intent identity is shared, actor identity is not.
    for ((ingress, job), (actor_type, actor_source)) in jobs.iter().zip([
        ("api_token", "mcp"),
        ("human", "studio"),
        ("api_token", "http_bearer"),
    ]) {
        let job_id = job["id"].as_str().unwrap();
        let audits: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(
            "SELECT actor_type, source, action, resource_id, outcome \
             FROM audit_log WHERE request_id = ?",
        )
        .bind(job_id)
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(
            audits.len(),
            1,
            "{ingress} must append exactly one submission audit row"
        );
        assert_eq!(
            audits[0].0, actor_type,
            "{ingress} must audit the submitting actor type"
        );
        assert_eq!(
            audits[0].1.as_deref(),
            Some(actor_source),
            "{ingress} must audit the submitting actor source"
        );
        assert_eq!(audits[0].2, "container.start");
        assert_eq!(audits[0].3, resource.id);
        assert_eq!(audits[0].4, "queued");

        let events: Vec<(String, String)> =
            sqlx::query_as("SELECT event_type, payload_json FROM events WHERE correlation_id = ?")
                .bind(job_id)
                .fetch_all(&db)
                .await
                .unwrap();
        for event_type in ["job.submitted.v1", "job.queued.v1"] {
            let (_, payload_json) = events
                .iter()
                .find(|(kind, _)| kind == event_type)
                .unwrap_or_else(|| panic!("missing {event_type} for the {ingress} job"));
            let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap();
            assert_eq!(payload["action"], "container.start");
            assert_eq!(payload["resource_id"], resource.id);
            assert_eq!(
                payload["state"], "queued",
                "{event_type} must carry the queued state for the {ingress} job"
            );
        }
    }

    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        3,
        "each ingress must plan the intent exactly once"
    );

    // Same-key replays stay inside each ingress's own scope and never re-plan.
    let mcp_replay = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": resource.id, "request_id": "equivalence-mcp" }),
    )
    .await;
    let mcp_replay_job: serde_json::Value = serde_json::from_str(
        mcp_replay["result"]["content"][0]["text"]
            .as_str()
            .expect("a same-key replay must return the canonical job"),
    )
    .unwrap();
    assert_eq!(mcp_replay_job["id"], mcp_job["id"]);

    let studio_replay = studio_tool_json(
        app.clone(),
        &session,
        "container.start",
        serde_json::json!({ "resource_id": resource.id, "request_id": "equivalence-studio" }),
    )
    .await;
    let studio_replay_job: serde_json::Value = serde_json::from_str(
        studio_replay["result"]
            .as_str()
            .expect("a same-key replay must return the canonical job"),
    )
    .unwrap();
    assert_eq!(studio_replay_job["id"], studio_job["id"]);
    assert_ne!(
        studio_replay_job["id"], mcp_job["id"],
        "an idempotency replay must never cross actor scopes"
    );

    let (http_replay_status, http_replay_json) = http_action_submit(
        app.clone(),
        &token,
        &resource.id,
        "container.start",
        "equivalence-http",
    )
    .await;
    assert_eq!(http_replay_status, StatusCode::ACCEPTED);
    assert_eq!(http_replay_json["job"]["id"], http_job["id"]);

    // The same key *text* on a different ingress is a different idempotency scope, so it must
    // produce a new job rather than silently adopting the HTTP ingress's job.
    let cross_ingress = mcp_tool_json(
        app.clone(),
        &token,
        "container.start",
        serde_json::json!({ "resource_id": resource.id, "request_id": "equivalence-http" }),
    )
    .await;
    let cross_ingress_job: serde_json::Value = serde_json::from_str(
        cross_ingress["result"]["content"][0]["text"]
            .as_str()
            .expect("a new scope must return a canonical job"),
    )
    .unwrap();
    assert_ne!(
        cross_ingress_job["id"], http_job["id"],
        "identical key text on another ingress must not reuse that ingress's job"
    );
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        4,
        "only the genuinely new intent may plan again"
    );
}

#[tokio::test]
async fn model_proxy_accepts_session_and_scopes_bearer_access() {
    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let member = insert_user(&db, "member").await;
    let session = insert_session(&db, &owner).await;
    let model_token = insert_token(&db, &owner, &["models:read"]).await;
    let member_model_token = insert_token(&db, &member, &["models:read"]).await;
    let wrong_scope_token = insert_token(&db, &owner, &["metrics:read"]).await;
    let app = crate::api::router(test_support::build(db));

    let session_response = app
        .clone()
        .oneshot(cookie_req("GET", "/v1/models", &session, None))
        .await
        .unwrap();
    assert_eq!(session_response.status(), StatusCode::OK);

    let bearer_response = app
        .clone()
        .oneshot(bearer_req("GET", "/v1/models", &model_token, None))
        .await
        .unwrap();
    assert_eq!(bearer_response.status(), StatusCode::OK);

    let member_bearer_response = app
        .clone()
        .oneshot(bearer_req("GET", "/v1/models", &member_model_token, None))
        .await
        .unwrap();
    assert_eq!(member_bearer_response.status(), StatusCode::OK);

    let insufficient_scope = app
        .clone()
        .oneshot(bearer_req("GET", "/v1/models", &wrong_scope_token, None))
        .await
        .unwrap();
    assert_insufficient_scope(insufficient_scope, "GET /v1/models").await;

    let malformed_authenticated = app
        .clone()
        .oneshot(bearer_raw_req(
            "POST",
            "/v1/chat/completions",
            Some(&model_token),
            "{",
        ))
        .await
        .unwrap();
    assert_eq!(malformed_authenticated.status(), StatusCode::BAD_REQUEST);

    let oversized_authenticated = app
        .clone()
        .oneshot(bearer_raw_req(
            "POST",
            "/v1/chat/completions",
            Some(&model_token),
            &"x".repeat(1024 * 1024 + 1),
        ))
        .await
        .unwrap();
    assert_eq!(
        oversized_authenticated.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    let insufficient_post_scope = app
        .oneshot(bearer_raw_req(
            "POST",
            "/v1/chat/completions",
            Some(&wrong_scope_token),
            "{",
        ))
        .await
        .unwrap();
    assert_insufficient_scope(insufficient_post_scope, "POST /v1/chat/completions").await;
}

/// The built-in MCP `read_file` tool exists to help read *project source*. It must never
/// serve credential files (`.env*`), data directories, or paths whose resolved target
/// leaves the project root, and it must fail closed on both ingresses (bearer MCP
/// JSON-RPC and the session-authenticated Studio panel) before any content is returned.
#[tokio::test]
async fn built_in_mcp_and_studio_read_file_confine_secret_and_out_of_root_paths() {
    async fn mcp_read_file(app: axum::Router, token: &str, path: &str) -> (bool, String) {
        let response = app
            .oneshot(bearer_req(
                "POST",
                "/api/mcp/message",
                token,
                Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": { "name": "read_file", "arguments": { "path": path } }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (
            json["result"]["isError"].as_bool().unwrap_or(false),
            json["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    }

    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let token = insert_token(&db, &owner, &["files:read"]).await;
    let session = insert_session(&db, &owner).await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let base = std::env::temp_dir().join(format!(
        "voidtower-mcp-read-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let root = base.join("project");
    std::fs::create_dir_all(root.join("instance/frontend")).unwrap();
    std::fs::create_dir_all(root.join("backend/src")).unwrap();
    std::fs::create_dir_all(root.join("dev-data")).unwrap();
    std::fs::write(root.join(".env"), "FOO_SETTING=alpha-beta-gamma\n").unwrap();
    std::fs::write(root.join("backend/src/app.rs"), "pub fn ok() {}\n").unwrap();
    std::fs::write(root.join("dev-data/voidtower.db"), "sqlite-placeholder").unwrap();
    std::fs::write(base.join("outside.txt"), "outside-root-marker").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        base.join("outside.txt"),
        root.join("backend/src/escape.txt"),
    )
    .unwrap();
    // One ASCII byte followed by 2-byte characters: the old byte-offset truncation
    // sliced at 8000 bytes, which lands inside a character and panics.
    std::fs::write(
        root.join("backend/src/multibyte.txt"),
        format!("a{}", "é".repeat(9000)),
    )
    .unwrap();

    let mut state = test_support::build(db.clone());
    let mut config = (*state.config).clone();
    config.frontend_dir = root.join("instance/frontend");
    state.config = Arc::new(config);
    let app = crate::api::router(state);

    let (is_error, text) = mcp_read_file(app.clone(), &token, ".env").await;
    assert!(is_error, "reading .env must fail closed, got: {text}");
    assert!(
        text.contains("not available"),
        "denial must be explicit and bounded, got: {text}"
    );
    assert!(
        !text.contains("alpha-beta-gamma"),
        "no .env content may reach the caller, got: {text}"
    );

    let (is_error, text) = mcp_read_file(app.clone(), &token, "dev-data/voidtower.db").await;
    assert!(is_error, "reading the data directory must fail closed");
    assert!(!text.contains("sqlite-placeholder"), "got: {text}");

    #[cfg(unix)]
    {
        let (is_error, text) = mcp_read_file(app.clone(), &token, "backend/src/escape.txt").await;
        assert!(
            is_error,
            "a symlink resolving outside the project root must fail closed, got: {text}"
        );
        assert!(!text.contains("outside-root-marker"), "got: {text}");
    }

    let (is_error, text) = mcp_read_file(app.clone(), &token, "backend/src/app.rs").await;
    assert!(!is_error, "reading project source must still work: {text}");
    assert!(text.contains("pub fn ok()"), "got: {text}");

    let (is_error, text) = mcp_read_file(app.clone(), &token, "backend/src/multibyte.txt").await;
    assert!(
        !is_error,
        "multibyte truncation must not fail the tool: {text}"
    );
    assert!(text.ends_with("…(truncated)"), "got: {text}");
    assert!(
        text.chars().count() <= 8100,
        "truncated output must stay bounded, got {} chars",
        text.chars().count()
    );

    let studio = app
        .oneshot(cookie_req(
            "POST",
            "/api/studio/mcp/invoke",
            &session,
            Some(serde_json::json!({
                "name": "read_file",
                "arguments": { "path": ".env" }
            })),
        ))
        .await
        .unwrap();
    assert_eq!(studio.status(), StatusCode::OK);
    let body = axum::body::to_bytes(studio.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        json["ok"], false,
        "the Studio ingress shares the same confinement: {json}"
    );
    assert!(
        !json.to_string().contains("alpha-beta-gamma"),
        "no .env content may reach the Studio caller: {json}"
    );

    let _ = std::fs::remove_dir_all(&base);
}

/// `read_file` exists to serve *project source*. It must therefore serve only the two
/// trees `search_code` also serves (`backend/src`, `frontend/src`) and refuse every
/// other path under the project root, including the dev bootstrap credential at
/// `backend/dev-config/bootstrap-token`, which the deny-list candidate of 0034 still
/// returned verbatim. Refusals must stay bounded (no file content, no path echo) and
/// must not block on special files, multi-link files, or oversized files.
#[tokio::test]
async fn built_in_mcp_and_studio_read_file_serves_only_project_source_trees() {
    async fn mcp_read(app: axum::Router, token: &str, path: &str) -> (bool, String) {
        let response = app
            .oneshot(bearer_req(
                "POST",
                "/api/mcp/message",
                token,
                Some(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "tools/call",
                    "params": { "name": "read_file", "arguments": { "path": path } }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        (
            json["result"]["isError"].as_bool().unwrap_or(false),
            json["result"]["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
        )
    }

    async fn studio_read(app: axum::Router, session: &str, path: &str) -> (bool, String) {
        let response = app
            .oneshot(cookie_req(
                "POST",
                "/api/studio/mcp/invoke",
                session,
                Some(serde_json::json!({
                    "name": "read_file",
                    "arguments": { "path": path }
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        if json["ok"].as_bool() == Some(true) {
            (
                false,
                json["result"].as_str().unwrap_or_default().to_string(),
            )
        } else {
            (true, json["error"].as_str().unwrap_or_default().to_string())
        }
    }

    let db = setup_db().await;
    let owner = insert_user(&db, "owner").await;
    let token = insert_token(&db, &owner, &["files:read"]).await;
    let session = insert_session(&db, &owner).await;
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let base = std::env::temp_dir().join(format!(
        "voidtower-mcp-source-only-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let root = base.join("project");
    std::fs::create_dir_all(root.join("instance/frontend")).unwrap();
    std::fs::create_dir_all(root.join("backend/src/api")).unwrap();
    std::fs::create_dir_all(root.join("frontend/src")).unwrap();
    std::fs::create_dir_all(root.join("backend/dev-config")).unwrap();
    std::fs::create_dir_all(root.join("backend/dev-data")).unwrap();
    std::fs::write(
        root.join("backend/src/api/apps.rs"),
        "pub fn apps_ok() {}\n",
    )
    .unwrap();
    // A credential-marked *source* basename stays readable: the confinement is
    // path-based, not name-based, so `secrets.rs` must not be over-blocked.
    std::fs::write(
        root.join("backend/src/api/secrets.rs"),
        "pub fn secrets_ok() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("frontend/src/App.tsx"),
        "export default function App() { return null; }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("backend/dev-config/bootstrap-token"),
        "vt_bootstrap_secret_marker\n",
    )
    .unwrap();
    std::fs::write(
        root.join("backend/dev-config/secrets.key"),
        "secrets-key-marker",
    )
    .unwrap();
    std::fs::write(
        root.join(".env.example"),
        "PLACEHOLDER=env-example-marker\n",
    )
    .unwrap();
    std::fs::write(
        root.join("backend/dev-data/voidtower.db"),
        "sqlite-placeholder",
    )
    .unwrap();
    std::fs::write(
        root.join("backend/src/oversize.txt"),
        "x".repeat(1024 * 1024 + 1),
    )
    .unwrap();
    std::fs::write(root.join("backend/src/exact_8000.txt"), "q".repeat(8000)).unwrap();
    std::fs::write(root.join("backend/src/over_8000.txt"), "q".repeat(8001)).unwrap();
    let outside = base.join("outside.txt");
    std::fs::write(&outside, "outside-hardlink-marker").unwrap();
    #[cfg(unix)]
    let fifo_ready = std::process::Command::new("mkfifo")
        .arg(root.join("backend/src/pipeline"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    #[cfg(unix)]
    let hardlink_ready =
        std::fs::hard_link(&outside, root.join("backend/src/hardlink.txt")).is_ok();

    let mut state = test_support::build(db.clone());
    let mut config = (*state.config).clone();
    config.frontend_dir = root.join("instance/frontend");
    state.config = Arc::new(config);
    let app = crate::api::router(state);

    // Every non-source path under the project root must be refused on both ingresses
    // and must not echo its content back to the caller.
    let mut leaks: Vec<String> = Vec::new();
    for (path, marker) in [
        (
            "backend/dev-config/bootstrap-token",
            "vt_bootstrap_secret_marker",
        ),
        ("backend/dev-config/secrets.key", "secrets-key-marker"),
        (".env.example", "env-example-marker"),
        ("backend/dev-data/voidtower.db", "sqlite-placeholder"),
    ] {
        let (is_error, text) = mcp_read(app.clone(), &token, path).await;
        if !is_error {
            leaks.push(format!("bearer served {path}: {text}"));
        } else if text.contains(marker) {
            leaks.push(format!("bearer echoed content of {path}"));
        }
        let (is_error, text) = studio_read(app.clone(), &session, path).await;
        if !is_error {
            leaks.push(format!("studio served {path}: {text}"));
        } else if text.contains(marker) {
            leaks.push(format!("studio echoed content of {path}"));
        }
    }

    // Malformed or non-regular targets: absolute path, traversal, a directory, a
    // special file, a multi-link file, and an oversized file.
    let mut denials: Vec<(String, bool)> = Vec::new();
    for path in [
        "/etc/passwd",
        "backend/src/../../.env",
        "backend/src",
        "backend/src/oversize.txt",
    ] {
        let (is_error, _) = mcp_read(app.clone(), &token, path).await;
        denials.push((path.to_string(), is_error));
    }
    #[cfg(unix)]
    if fifo_ready {
        let (is_error, _) = mcp_read(app.clone(), &token, "backend/src/pipeline").await;
        denials.push(("backend/src/pipeline".to_string(), is_error));
    }
    #[cfg(unix)]
    if hardlink_ready {
        let (is_error, text) = mcp_read(app.clone(), &token, "backend/src/hardlink.txt").await;
        denials.push(("backend/src/hardlink.txt".to_string(), is_error));
        if !is_error {
            leaks.push(format!(
                "bearer served a multi-link file: {}",
                text.chars().take(64).collect::<String>()
            ));
        } else if text.contains("outside-hardlink-marker") {
            leaks.push("bearer echoed content of a multi-link file".to_string());
        }
    }
    let served: Vec<&String> = denials
        .iter()
        .filter(|(_, is_error)| !is_error)
        .map(|(path, _)| path)
        .collect();

    assert!(
        leaks.is_empty(),
        "read_file exposed non-source paths: {leaks:#?}"
    );
    assert!(
        served.is_empty(),
        "read_file served malformed/non-regular targets: {served:?}"
    );

    // Positive controls: both ingresses still read project source.
    let (is_error, text) = mcp_read(app.clone(), &token, "backend/src/api/apps.rs").await;
    assert!(!is_error, "bearer source read must work: {text}");
    assert!(text.contains("pub fn apps_ok()"), "got: {text}");

    let (is_error, text) = mcp_read(app.clone(), &token, "backend/src/api/secrets.rs").await;
    assert!(
        !is_error,
        "a credential-named source file must stay readable: {text}"
    );
    assert!(text.contains("pub fn secrets_ok()"), "got: {text}");

    let (is_error, text) = studio_read(app.clone(), &session, "frontend/src/App.tsx").await;
    assert!(!is_error, "studio source read must work: {text}");
    assert!(
        text.contains("export default function App()"),
        "got: {text}"
    );

    // The 8000-character boundary is inclusive: exactly 8000 chars is served whole,
    // 8001 chars is truncated to 8000 plus the marker.
    let (is_error, text) = mcp_read(app.clone(), &token, "backend/src/exact_8000.txt").await;
    assert!(!is_error, "boundary file must be readable: {text}");
    assert_eq!(
        text.chars().count(),
        8000,
        "a file at the character limit must not be truncated"
    );

    let (is_error, text) = mcp_read(app.clone(), &token, "backend/src/over_8000.txt").await;
    assert!(!is_error, "boundary file must be readable: {text}");
    assert!(text.ends_with("…(truncated)"), "got: {text}");
    assert_eq!(
        text.chars().count(),
        8000 + "\n…(truncated)".chars().count(),
        "truncated output must keep exactly 8000 content chars"
    );

    let _ = std::fs::remove_dir_all(&base);
}
