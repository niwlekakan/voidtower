#![cfg(test)]
//! Generated probe tests for `authz_matrix.rs`'s `SESSION_ROLE_MATRIX` (gap-analysis P1 table
//! row 2, task P1-01). Drives the real router end-to-end (`tower::ServiceExt::oneshot`),
//! following `scope_bypass_tests.rs`'s setup pattern — every request here either gets rejected
//! by the auth layer before reaching handler logic (the overwhelming majority: unauthenticated
//! and wrong-role probes) or targets a pure DB read with no side effects (the handful of
//! reachability/regression checks that must reach the handler body).

use axum::{
    body::Body,
    extract::connect_info::ConnectInfo,
    http::{header, Request, StatusCode},
};
use sqlx::SqlitePool;
use std::{collections::HashMap, net::SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

use crate::api::mcp::test_support;

use super::authz_matrix::{Role, SESSION_ROLE_MATRIX, WS_UPGRADE_ROUTES};

async fn setup_db() -> SqlitePool {
    let path = std::env::temp_dir().join(format!("vt-p1-01-test-{}.sqlite", uuid::Uuid::new_v4()));
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

async fn session_for_role(db: &SqlitePool, role: &str) -> String {
    let user_id = insert_user(db, role).await;
    insert_session(db, &user_id).await
}

fn with_connect_info(mut req: Request<Body>) -> Request<Body> {
    req.extensions_mut()
        .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
    req
}

/// `fat_probe_body()` below builds a single JSON object covering the field names/types this
/// codebase's `Json<T>` request bodies actually use (surveyed via `grep`/regex over every
/// `struct` matching a `Json<X>`/`Query<X>` handler parameter type across `api/*.rs`), so a
/// generic probe request can satisfy any one route's required fields without this file
/// needing to hand-author ~150 distinct bodies. `serde` ignores unrecognized object keys by
/// default (no handler in this codebase sets `#[serde(deny_unknown_fields)]`), so extra keys
/// are harmless. Handlers whose role check runs *inside* the function body (every handler in
/// this codebase, per `docs/codebase-map.md` §6) only get a chance to reject with 401/403 if
/// axum's body-consuming `Json<T>` extractor -- which always runs before the handler body,
/// regardless of parameter order -- succeeds first; an unparseable/incomplete body instead
/// produces a 400 that never reaches the role check, which would make these generated probes
/// meaningless. `probe_body_override()` further below covers the handful of routes this fat
/// object still can't satisfy generically (field names that collide across structs with
/// genuinely incompatible types, and fixed-vocabulary enum fields).
const NUMBER_FIELDS: &[&str] = &[
    "port",
    "num",
    "priority",
    "max_apps",
    "quota_bytes",
    "size",
    "min",
    "max",
    "used_bytes",
    "priority_order",
    "rate_limit_rpm",
    "timeout_secs",
    "interval_secs",
    "keep_alive_secs",
    "batch_size",
    "threads",
    "ctx_size",
    "n_gpu_layers",
    "seed",
    "width",
    "height",
    "disk_gb",
    "cores",
    "memory",
    "expires_days",
    "sort_order",
    "level",
    "count",
    "total",
    "total_bytes",
    "downloaded_bytes",
    "pulled_bytes",
    "storage_free_bytes",
    "current_layer",
    "ahead",
    "behind",
    "commits_ahead",
    "steps",
    "cfg_scale",
    "speed",
    "primary_port",
    "retention_days",
    "limit",
    "offset",
    "tail",
    "vmid",
    "exported_at",
    "parallel",
];
const BOOL_FIELDS: &[&str] = &[
    "enabled",
    "allow_embed",
    "auto_provision",
    "available",
    "cont_batching",
    "docker_available",
    "dry_run",
    "flash_attn",
    "has_client_secret",
    "is_git_install",
    "libvirt_available",
    "mcp_enabled",
    "online",
    "ok",
    "password_set",
    "regenerate_webhook_secret",
    "ssl",
    "sso_protect",
    "systemd_available",
    "truncated",
    "verify_ssl",
    "websocket_extended",
    "emergency_disable",
    "emergency_disabled",
    "can_deploy_custom",
    "ipv6",
    "keep_data",
    "force",
    "custom_deploy",
    "raw",
    "deployed",
];
const ARRAY_FIELDS: &[&str] = &[
    "agents",
    "alert_rules",
    "alerts",
    "apps",
    "backups",
    "containers",
    "drives",
    "entries",
    "events",
    "images",
    "items",
    "models",
    "nodes",
    "packages",
    "processes",
    "proxy_rules",
    "rules",
    "services",
    "sessions",
    "tabs",
    "tags",
    "users",
    "vms",
    "warnings",
    "volumes",
    "changed_files",
    "commits",
    "ids",
    "app_ids",
    "secret_ids",
    "required_roles",
    "scopes",
    "nav_groups",
    "backup_tags",
    "ports",
    "devices",
];
const OBJECT_FIELDS: &[&str] = &[
    "config",
    "params",
    "role_map",
    "env",
    "env_overrides",
    "arguments",
];

/// Every remaining field name (defaults to a generic string value).
const STRING_FIELDS: &[&str] = &[
    "action",
    "activity",
    "actor_type",
    "agent_capable",
    "agent_id",
    "api_key_ref",
    "api_key_value",
    "app_id",
    "app_name",
    "applied_at",
    "appvault",
    "arch",
    "author",
    "automation_id",
    "automation_jobs",
    "backend",
    "basic_auth_password",
    "basic_auth_user",
    "battery",
    "branch",
    "bus",
    "button_label",
    "cache_static",
    "cache_type_k",
    "cache_type_v",
    "category",
    "channel",
    "client_id",
    "client_secret",
    "code",
    "color",
    "command",
    "comment",
    "compose_path",
    "compose_yaml",
    "content",
    "context",
    "created_at",
    "current_commit",
    "current_image",
    "current_session_id",
    "custom_css",
    "custom_headers",
    "default_role",
    "description",
    "device",
    "device_type",
    "diff_preview",
    "direction",
    "discord_webhook",
    "disk",
    "disk_path",
    "display_name",
    "domain",
    "dump",
    "effect",
    "email",
    "entry",
    "error",
    "error_description",
    "fetch_error",
    "filename",
    "fingerprint",
    "from",
    "fstype",
    "gpu",
    "options",
    "query",
    "type",
    "heartbeat_token",
    "host",
    "host_path",
    "hostname",
    "icon",
    "id",
    "image",
    "installed_at",
    "instance_logo",
    "instance_name",
    "interface",
    "issuer_url",
    "jsonrpc",
    "key_path",
    "kind",
    "label",
    "last_checked_at",
    "last_latency_ms",
    "last_status",
    "last_used",
    "login_bg_url",
    "login_tagline",
    "method",
    "mod_name",
    "mode",
    "model",
    "mountpoint",
    "name",
    "nav_group",
    "negative_prompt",
    "node",
    "node_id",
    "ntfy_url",
    "ostemplate",
    "package_manager",
    "pairing_code",
    "parent",
    "pass",
    "password",
    "path",
    "project_name",
    "prompt",
    "proto",
    "provider_id",
    "quota_bytes",
    "reason",
    "redirect_url",
    "remote_commit",
    "repo_path",
    "resource",
    "resource_id",
    "resource_tag",
    "resource_type",
    "restore_test_schedule",
    "result",
    "rollback_ref",
    "role",
    "rootfs",
    "roots",
    "schedule",
    "search",
    "secret",
    "server_endpoint",
    "service",
    "session_id",
    "severity",
    "slack_webhook",
    "source",
    "source_path",
    "sso_protect",
    "state",
    "status",
    "status_updated_at",
    "storage",
    "storage_drive_id",
    "tag",
    "tag_id",
    "target",
    "target_node_id",
    "task_id",
    "text",
    "title",
    "to",
    "token",
    "token_id",
    "token_secret",
    "totp_code",
    "update_detail",
    "update_status",
    "updated_at",
    "upstream",
    "uri",
    "url",
    "user",
    "username",
    "value",
    "value_enc",
    "verdict",
    "version",
    "voice",
    "voidtower_version",
    "volid",
    "warnings",
    "webhook_secret_hint",
    "webhook_type",
    "wg_client_config",
    "arguments_json",
    "http_method",
    "body",
    "basic_auth",
    "notify_url",
    "webhook_url",
    "api_key",
    "drive_path",
    "user_id",
    "drive_id",
];

fn fat_probe_body() -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for f in STRING_FIELDS {
        map.insert(
            f.to_string(),
            serde_json::Value::String("probe".to_string()),
        );
    }
    for f in NUMBER_FIELDS {
        map.insert(f.to_string(), serde_json::json!(1));
    }
    for f in BOOL_FIELDS {
        map.insert(f.to_string(), serde_json::json!(true));
    }
    for f in ARRAY_FIELDS {
        map.insert(f.to_string(), serde_json::json!([]));
    }
    for f in OBJECT_FIELDS {
        map.insert(f.to_string(), serde_json::json!({}));
    }
    // A handful of string-typed-looking fields are actually fixed-vocabulary enums
    // (`containers::ContainerAction`, `services::ServiceAction`, both `#[serde(rename_all)]`
    // with a `Start` variant) — "probe" doesn't deserialize into either, "start" does.
    map.insert("action".to_string(), serde_json::json!("start"));
    serde_json::Value::Object(map)
}

/// `:vmid` (`lxc.rs`, `proxmox.rs`) and `:idx` (`storage.rs`'s fstab routes) bind to `Path<u32>`/
/// `Path<u64>`/`Path<usize>`, not `Path<String>` — the literal placeholder text that works fine
/// for every *string*-typed path parameter in this table fails number parsing for these,
/// before the handler's role check ever runs. `webhooks.rs`'s `:id` is the one other
/// `Path<i64>` case; substituted only for that file's routes since `:id` is a `Path<String>`
/// everywhere else.
fn substitute_numeric_path_params(path: &str) -> String {
    let mut p = path.replace(":vmid", "100").replace(":idx", "0");
    if p.starts_with("/api/webhooks/") {
        p = p.replace(":id", "1");
    }
    p
}

/// Same field survey as above, applied as a URL query string — covers every `Query<T>`
/// struct's field names across `backend/src/api/*.rs` — appended to every generated probe
/// request (GET/DELETE routes that don't declare a `Query<T>` extractor simply ignore it;
/// `serde_urlencoded` doesn't reject unknown keys by default in this codebase).
fn probe_query_string() -> String {
    // `timeline.rs`'s `TimelineQuery` types `from`/`to` as `Option<i64>` (a unix-timestamp
    // range filter) rather than the free-text `from`/`to` (IP/host) meaning used elsewhere as
    // JSON body fields — numeric here so that query still parses.
    let string_fields = [
        "category",
        "code",
        "disk",
        "error",
        "error_description",
        "path",
        "search",
        "session_id",
        "severity",
        "state",
        "token",
        "volid",
    ];
    let mut parts: Vec<String> = string_fields.iter().map(|f| format!("{f}=probe")).collect();
    parts.push("limit=1".to_string());
    parts.push("offset=0".to_string());
    parts.push("tail=1".to_string());
    parts.push("from=1".to_string());
    parts.push("to=2".to_string());
    parts.join("&")
}

fn with_probe_query(uri: &str) -> String {
    format!(
        "{}?{}",
        substitute_numeric_path_params(uri),
        probe_query_string()
    )
}

/// A few field names collide across structs with genuinely incompatible types (e.g.
/// `port: Option<String>` in `firewall::AddRuleRequest` vs. a numeric `port` elsewhere,
/// `env: Vec<String>` in `apps::CustomDeployRequest` vs. `env: HashMap<String, String>`
/// elsewhere) — one shared generic value can't satisfy both. These routes get a minimal,
/// hand-built valid body instead of `fat_probe_body()`.
fn probe_body_override(method: &str, path: &str) -> Option<serde_json::Value> {
    match (method, path) {
        ("POST", "/api/apps/deploy-custom") => Some(serde_json::json!({
            "name": "probe", "image": "probe:latest"
        })),
        ("POST", "/api/disaster/import-config") => Some(serde_json::json!({
            "voidtower_version": "0.0.0",
            "exported_at": 1,
            "instance_name": "probe",
            "proxy_rules": [],
            "automation_jobs": [],
            "alert_rules": [],
            "tags": []
        })),
        ("POST", "/api/firewall/rules") => Some(serde_json::json!({ "action": "allow" })),
        ("PUT", "/api/oidc/config") => Some(serde_json::json!({
            "enabled": true, "issuer_url": "http://example.invalid", "client_id": "probe",
            "redirect_url": "http://example.invalid", "scopes": "openid",
            "role_claim": "role", "role_map": {}, "default_role": "viewer",
            "auto_provision": false
        })),
        ("POST", "/api/proxy") | ("PUT", "/api/proxy/:id") => Some(serde_json::json!({
            "domain": "probe.invalid", "upstream": "http://example.invalid"
        })),
        ("POST", "/api/storage/fstab") => Some(serde_json::json!({
            "device": "probe", "mountpoint": "/nonexistent", "fstype": "ext4", "options": "defaults"
        })),
        ("POST", "/api/storage/paths") => Some(serde_json::json!({})),
        ("POST", "/api/storage/raid/create") => Some(serde_json::json!({
            "name": "probe", "level": "1", "devices": []
        })),
        _ => None,
    }
}

fn probe_body(method: &str, path: &str) -> serde_json::Value {
    probe_body_override(method, path).unwrap_or_else(fat_probe_body)
}

/// `stt_transcribe` takes `axum::extract::Multipart`, not `Json<T>` — its extractor needs a
/// real `multipart/form-data` boundary, which this file's JSON-body probes can't construct.
/// Excluded from live probes for the same structural reason as `WS_UPGRADE_ROUTES` (still
/// fully classified in `SESSION_ROLE_MATRIX` for exhaustiveness).
const NON_JSON_BODY_ROUTES: &[(&str, &str)] = &[("POST", "/api/studio/stt/transcribe")];

/// A request with no cookie, no `Authorization` header — zero credentials of any kind.
fn unauth_req(method: &str, uri: &str) -> Request<Body> {
    with_connect_info(
        Request::builder()
            .method(method)
            .uri(with_probe_query(uri))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(probe_body(method, uri).to_string()))
            .unwrap(),
    )
}

fn cookie_req(
    method: &str,
    uri: &str,
    session_id: &str,
    body: Option<serde_json::Value>,
) -> Request<Body> {
    let b = body.unwrap_or_else(|| probe_body(method, uri)).to_string();
    with_connect_info(
        Request::builder()
            .method(method)
            .uri(with_probe_query(uri))
            .header(header::COOKIE, format!("vt_session={session_id}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b))
            .unwrap(),
    )
}

/// Sends a real WebSocket handshake through a loopback listener. `oneshot` requests do not
/// contain Hyper's connection-upgrade state, so Axum rejects them before the handler body.
async fn websocket_status(app: axum::Router, uri: &str, session_id: &str) -> StatusCode {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "GET {uri} HTTP/1.1\r\nHost: {addr}\r\nCookie: vt_session={session_id}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: REDACTED-WEBSOCKET-TEST-KEY\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut response = [0_u8; 1024];
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        stream.read(&mut response),
    )
    .await
    .expect("WebSocket probe timed out")
    .unwrap();
    server.abort();

    let status = std::str::from_utf8(&response[..read])
        .unwrap()
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .and_then(|code| StatusCode::from_u16(code).ok())
        .expect("WebSocket probe returned no HTTP status");
    status
}

fn is_probe_exempt(method: &str, path: &str) -> bool {
    WS_UPGRADE_ROUTES.contains(&(method, path)) || NON_JSON_BODY_ROUTES.contains(&(method, path))
}

/// Acceptance test: "generated, not sampled" -- every route the matrix marks as requiring
/// *some* session credential gets a real request with zero credentials and must return 401.
/// WebSocket-upgrade routes are excluded (see `WS_UPGRADE_ROUTES`'s doc comment) -- they still
/// have a table entry, just not a live probe here.
#[tokio::test]
async fn unauthenticated_request_is_rejected_for_every_non_public_route() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db));

    let mut failures = Vec::new();
    for (method, path, role) in SESSION_ROLE_MATRIX {
        if !role.requires_unauthenticated_401() || is_probe_exempt(method, path) {
            continue;
        }
        let res = app.clone().oneshot(unauth_req(method, path)).await.unwrap();
        if res.status() != StatusCode::UNAUTHORIZED {
            failures.push(format!(
                "{method} {path}: expected 401, got {}",
                res.status()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "unauthenticated requests not rejected with 401 ({} routes):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Acceptance test: for each role tier above the lowest (`viewer` for Operator-tier routes,
/// `operator` for Admin-tier routes, `admin` for Owner-tier routes), a real session minted at
/// the next-lower standard-ladder role must be rejected with 403. Guest/demo/member coverage
/// lives in the dedicated positive-guard regressions below.
#[tokio::test]
async fn wrong_role_session_is_rejected_for_every_role_gated_route() {
    let db = setup_db().await;
    let viewer_session = session_for_role(&db, "viewer").await;
    let operator_session = session_for_role(&db, "operator").await;
    let admin_session = session_for_role(&db, "admin").await;
    let session_by_probe_role: HashMap<&str, &str> = [
        ("viewer", viewer_session.as_str()),
        ("operator", operator_session.as_str()),
        ("admin", admin_session.as_str()),
    ]
    .into_iter()
    .collect();

    let app = crate::api::router(test_support::build(db));

    let mut failures = Vec::new();
    for (method, path, role) in SESSION_ROLE_MATRIX {
        if is_probe_exempt(method, path) {
            continue;
        }
        let Some(tier) = role.wrong_role_tier() else {
            continue;
        };
        let Some(probe_role) = tier.wrong_role_probe() else {
            continue;
        };
        let session_id = session_by_probe_role[probe_role];

        let res = app
            .clone()
            .oneshot(cookie_req(method, path, session_id, None))
            .await
            .unwrap();
        if res.status() != StatusCode::FORBIDDEN {
            failures.push(format!(
                "{method} {path}: a {probe_role} session (below the route's {tier:?} \
                 requirement) expected 403, got {}",
                res.status()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "wrong-role sessions not rejected with 403 ({} routes):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Acceptance test: the inverse regression guard -- routes the matrix marks `Public` must not
/// be accidentally caught by the assertions above. Restricted to a representative,
/// side-effect-free subset (the task spec's own examples plus two more truly-open GETs) so
/// this doesn't depend on network/AI-provider state (`POST /v1/chat/completions`, the other
/// `Public` route, would need a configured upstream to return 200 and isn't needed to prove
/// the point -- it's excluded from the *live* assertions here, though it still has a matrix
/// entry and is covered by the exhaustiveness test).
#[tokio::test]
async fn public_routes_remain_reachable_without_auth() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db));

    const SAFE_PUBLIC_GETS: &[&str] = &[
        "/api/health",
        "/status",
        "/api/settings/public",
        "/v1/models",
        "/api/integrations/scopes",
        "/api/integrations/odysseus/manifest",
    ];

    for path in SAFE_PUBLIC_GETS {
        let table_role = SESSION_ROLE_MATRIX
            .iter()
            .find(|(m, p, _)| *m == "GET" && p == path)
            .map(|(_, _, r)| *r)
            .unwrap_or_else(|| panic!("{path} missing from SESSION_ROLE_MATRIX"));
        assert_eq!(
            table_role,
            Role::Public,
            "{path} is in this test's safe-public list but not classified Role::Public"
        );

        let res = app.clone().oneshot(unauth_req("GET", path)).await.unwrap();
        assert_ne!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "{path}: a Public route must not be blocked by the session-auth layer, got 401"
        );
        assert_ne!(
            res.status(),
            StatusCode::FORBIDDEN,
            "{path}: a Public route must not be blocked by the session-auth layer, got 403"
        );
    }
}

#[tokio::test]
async fn host_detail_and_model_status_routes_require_authentication() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db));

    for path in [
        "/api/capabilities",
        "/api/diagnostics",
        "/api/models/download/missing",
        "/api/models/ollama/create/missing",
        "/api/models/ollama/pull/missing",
        "/api/system/version",
    ] {
        let res = app.clone().oneshot(unauth_req("GET", path)).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "credential-free GET {path} must be rejected, got {}",
            res.status()
        );
    }
}

#[tokio::test]
async fn diagnostics_and_model_status_require_admin_session_roles() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["viewer", "guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;
        for path in [
            "/api/diagnostics",
            "/api/models/download/missing",
            "/api/models/ollama/create/missing",
            "/api/models/ollama/pull/missing",
        ] {
            let res = app
                .clone()
                .oneshot(cookie_req("GET", path, &session_id, None))
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                StatusCode::FORBIDDEN,
                "a {role} session must be rejected by GET {path}'s admin guard, got {}",
                res.status()
            );
        }
    }
}

#[tokio::test]
async fn capabilities_and_version_admit_all_valid_session_roles() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in [
        "owner", "admin", "operator", "viewer", "guest", "demo", "member",
    ] {
        let session_id = session_for_role(&db, role).await;
        for path in ["/api/capabilities", "/api/system/version"] {
            let res = app
                .clone()
                .oneshot(cookie_req("GET", path, &session_id, None))
                .await
                .unwrap();
            assert_ne!(
                res.status(),
                StatusCode::UNAUTHORIZED,
                "a valid {role} session must reach GET {path}"
            );
            assert_ne!(
                res.status(),
                StatusCode::FORBIDDEN,
                "a valid {role} session must reach GET {path}"
            );
        }
    }
}

#[tokio::test]
async fn diagnostics_and_model_status_admit_admin_and_owner() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["admin", "owner"] {
        let session_id = session_for_role(&db, role).await;
        for (path, expected) in [
            ("/api/diagnostics", StatusCode::OK),
            ("/api/models/download/missing", StatusCode::NOT_FOUND),
            ("/api/models/ollama/create/missing", StatusCode::NOT_FOUND),
            ("/api/models/ollama/pull/missing", StatusCode::NOT_FOUND),
        ] {
            let res = app
                .clone()
                .oneshot(cookie_req("GET", path, &session_id, None))
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                expected,
                "a valid {role} session must clear GET {path}'s admin guard"
            );
        }
    }
}

/// Acceptance test: guest/demo/member can use any-session reads but cannot cross an admin
/// positive-role boundary.
#[tokio::test]
async fn member_and_guest_and_demo_roles_are_represented_in_the_matrix() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    // A Role::Session(RoleTier::Session) route every valid role, including the newest three,
    // must be able to reach.
    const ALLOW_WITNESS: (&str, &str) = ("GET", "/api/apps/deployed");
    // A Role::Session(RoleTier::Admin) route using the correct allowlist pattern -- must
    // reject guest/demo/member exactly as it rejects viewer/operator.
    const DENY_WITNESS: (&str, &str) = ("GET", "/api/users");

    for role in ["guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;

        let allow_res = app
            .clone()
            .oneshot(cookie_req(
                ALLOW_WITNESS.0,
                ALLOW_WITNESS.1,
                &session_id,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(
            allow_res.status(),
            StatusCode::OK,
            "{role} session should reach {} {} (any-session route), got {}",
            ALLOW_WITNESS.0,
            ALLOW_WITNESS.1,
            allow_res.status()
        );

        let deny_res = app
            .clone()
            .oneshot(cookie_req(
                DENY_WITNESS.0,
                DENY_WITNESS.1,
                &session_id,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(
            deny_res.status(),
            StatusCode::FORBIDDEN,
            "{role} session must be rejected by {} {} (admin-only, correct allowlist), got {}",
            DENY_WITNESS.0,
            DENY_WITNESS.1,
            deny_res.status()
        );
    }
}

// ---------------------------------------------------------------------------
// Positive-role regression tests for routes that historically admitted roles
// added later. These drive the real router and
// require guest/demo/member sessions to fail at the handler's role boundary.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn firewall_admin_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    // A deliberately-unknown `action` value returns 400 BadRequest *inside* the handler,
    // before any `ufw` subprocess is spawned (`firewall.rs`'s `firewall_action`) -- proves
    // the request got past the role check without touching the host firewall.
    //
    // `demo` is deliberately excluded here (unlike the GET-only regression tests below,
    // which do cover it): `demo_guard::middleware` separately blocks every non-GET `/api/*`
    // request from a `role == "demo"` session regardless of the handler's own role check, so
    // a 403 for `demo` on this POST route would be `demo_guard` working correctly, not
    // evidence that `firewall.rs`'s own role boundary ran.
    for role in ["guest", "member"] {
        let session_id = session_for_role(&db, role).await;
        let res = app
            .clone()
            .oneshot(cookie_req(
                "POST",
                "/api/firewall/action",
                &session_id,
                Some(serde_json::json!({ "action": "not-a-real-action" })),
            ))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "a {role} session must be rejected by POST /api/firewall/action's admin guard, got {}",
            res.status()
        );
    }
}

#[tokio::test]
async fn terminal_ws_operator_guard_rejects_guest_demo_member() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;
        let status = websocket_status(app.clone(), "/api/terminal/ws", &session_id).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a {role} session must be rejected by GET /api/terminal/ws's operator guard, got {}",
            status
        );
    }
}

#[tokio::test]
async fn terminal_session_management_operator_guard_rejects_guest_demo_member() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;
        for path in ["/api/terminal/ssh/sessions", "/api/terminal/local/sessions"] {
            let res = app
                .clone()
                .oneshot(cookie_req("GET", path, &session_id, None))
                .await
                .unwrap();
            assert_eq!(
                res.status(),
                StatusCode::FORBIDDEN,
                "a {role} session must be rejected by GET {path}'s operator guard, got {}",
                res.status()
            );
        }
    }
}

#[tokio::test]
async fn audit_operator_guard_rejects_guest_demo_member() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;
        let res = app
            .clone()
            .oneshot(cookie_req("GET", "/api/audit", &session_id, None))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "a {role} session must be rejected by GET /api/audit's operator guard, got {}",
            res.status()
        );
    }
}

#[tokio::test]
async fn timeline_operator_guard_rejects_guest_demo_member() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;
        let res = app
            .clone()
            .oneshot(cookie_req("GET", "/api/timeline", &session_id, None))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "a {role} session must be rejected by GET /api/timeline's operator guard, got {}",
            res.status()
        );
    }
}

// ---------------------------------------------------------------------------
// Positive-role regression tests for the 2026-07-13 scope extension. Every
// route below is a mutation (POST/DELETE/PATCH) except `containers::exec_ws`,
// which uses the same real WebSocket probe as `terminal::ws_handler` above.
//
// `demo` is deliberately excluded from every mutation-route test here, same
// rationale as `firewall_admin_guard_rejects_guest_and_member`:
// `demo_guard::middleware` blocks every non-GET `/api/*` request from a
// `role == "demo"` session before the handler's own check ever runs,
// so a 403 for `demo` on a POST/DELETE/PATCH route would be `demo_guard`
// working correctly, not evidence about the handler guard. `exec_ws` is a GET route
// (demo_guard only gates non-GET), so
// its test covers all three roles, matching `terminal_ws`'s test above.
//
// Two of these are the highest-severity items in this whole scope extension
// and get their own explicit tests rather than being folded into a loop:
// `automation::run_now` lets an admitted guest/demo/member session execute
// an arbitrary shell command server-side via `automation_jobs.command`, and
// `containers::exec_ws` opens an interactive `docker exec -it ... sh` shell
// in the container -- both the same risk class as the originally-escalated
// `terminal.rs` shell-access bug.
// ---------------------------------------------------------------------------

/// Shared assertion for mutation routes: guest/member must stop at the handler's role guard,
/// before route-specific validation or side effects.
async fn assert_mutation_guard_rejects_guest_and_member(
    db: &SqlitePool,
    method: &str,
    path: &str,
    route_label: &str,
) {
    let app = crate::api::router(test_support::build(db.clone()));
    for role in ["guest", "member"] {
        let session_id = session_for_role(db, role).await;
        let res = app
            .clone()
            .oneshot(cookie_req(method, path, &session_id, None))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "a {role} session must be rejected by {method} {path} ({route_label}), got {}",
            res.status()
        );
    }
}

#[tokio::test]
async fn alerts_delete_admin_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "DELETE",
        "/api/alerts/:id",
        "alerts::delete_alert",
    )
    .await;
}

#[tokio::test]
async fn apps_update_compose_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/apps/:project_name/compose",
        "apps::update_compose",
    )
    .await;
}

#[tokio::test]
async fn apps_patch_app_env_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/apps/:project_name/env",
        "apps::patch_app_env",
    )
    .await;
}

/// Also `RiskClass::Irreversible` (P0-04's denylist) -- this session-role bug is a second,
/// independent way to reach the same irreversible action, not a duplicate finding.
#[tokio::test]
async fn apps_delete_app_volumes_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/apps/:project_name/delete-volumes",
        "apps::delete_app_volumes",
    )
    .await;
}

#[tokio::test]
async fn app_vault_admin_guard_admits_owner_for_expose_and_purge() {
    let db = setup_db().await;
    let owner_session = session_for_role(&db, "owner").await;
    let app = crate::api::router(test_support::build(db));

    for (path, body) in [
        (
            "/api/apps/missing-project/expose",
            Some(serde_json::json!({ "domain": "missing.invalid" })),
        ),
        ("/api/apps/missing-project/purge", None),
    ] {
        let res = app
            .clone()
            .oneshot(cookie_req("POST", path, &owner_session, body))
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "an owner must clear {path}'s admin guard and reach the missing-app lookup, got {}",
            res.status()
        );
    }
}

#[tokio::test]
async fn automation_create_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/automation",
        "automation::create",
    )
    .await;
}

#[tokio::test]
async fn automation_update_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "PATCH",
        "/api/automation/:id",
        "automation::update",
    )
    .await;
}

#[tokio::test]
async fn automation_delete_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "DELETE",
        "/api/automation/:id",
        "automation::delete",
    )
    .await;
}

/// HIGH SEVERITY: `run_now` executes `automation_jobs.command` as a server-side shell command.
/// A guest/demo/member session wrongly admitted here gets arbitrary shell execution, the same
/// risk class as `terminal.rs`'s originally-escalated shell-access bug.
#[tokio::test]
async fn automation_run_now_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/automation/:id/run",
        "automation::run_now -- arbitrary shell command execution",
    )
    .await;
}

#[tokio::test]
async fn backups_create_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(&db, "POST", "/api/backups", "backups::create")
        .await;
}

#[tokio::test]
async fn backups_run_now_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/backups/:id/run",
        "backups::run_now",
    )
    .await;
}

#[tokio::test]
async fn backups_check_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/backups/:id/check",
        "backups::check",
    )
    .await;
}

#[tokio::test]
async fn backups_restore_test_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/backups/:id/restore-test",
        "backups::restore_test",
    )
    .await;
}

#[tokio::test]
async fn backups_delete_plan_admin_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/backups/:id/delete-plan",
        "backups::delete_plan",
    )
    .await;
}

#[tokio::test]
async fn backups_delete_admin_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "DELETE",
        "/api/backups/:id",
        "backups::delete",
    )
    .await;
}

#[tokio::test]
async fn containers_action_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/containers/:id/action",
        "containers::action",
    )
    .await;
}

/// HIGH SEVERITY: `exec_ws` opens an interactive `docker exec -it <container> sh` shell over
/// the upgraded WebSocket, the same risk class as `terminal.rs`'s originally-escalated
/// shell-access bug. GET route, so (unlike the mutation tests above) `demo_guard` doesn't
/// gate it -- all three roles are probed, matching `terminal_ws_operator_guard_rejects_guest_demo_member`.
#[tokio::test]
async fn containers_exec_ws_operator_guard_rejects_guest_demo_member() {
    let db = setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["guest", "demo", "member"] {
        let session_id = session_for_role(&db, role).await;
        let status = websocket_status(app.clone(), "/api/containers/%21/exec", &session_id).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a {role} session must be rejected by GET /api/containers/:id/exec's operator guard, got {}",
            status
        );
    }
}

#[tokio::test]
async fn services_action_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/services/:name/action",
        "services::action",
    )
    .await;
}

#[tokio::test]
async fn status_checks_create_operator_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "POST",
        "/api/status-checks",
        "status::create",
    )
    .await;
}

#[tokio::test]
async fn status_checks_delete_admin_guard_rejects_guest_and_member() {
    let db = setup_db().await;
    assert_mutation_guard_rejects_guest_and_member(
        &db,
        "DELETE",
        "/api/status-checks/:id",
        "status::delete",
    )
    .await;
}
