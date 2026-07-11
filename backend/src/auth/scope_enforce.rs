//! Token-scope enforcement middleware — the fix for the scope-bypass gap
//! documented in docs/codebase-map.md §4 and docs/adr/ADR-003-auth-scope-enforcement.md.
//!
//! `bearer_auth::middleware` upgrades any *valid* Bearer token into a full
//! session carrying the token owner's role, but nothing downstream ever
//! consulted the token's own declared scopes except two endpoints
//! (`GET /api/events/stream`, `GET /api/integrations/events`). This
//! middleware is the single mandatory choke point ADR-003 decided on
//! instead of teaching all ~20 per-file `require_user`/`require_admin`
//! copies about scopes individually.
//!
//! # Design: allowlist, not denylist
//!
//! `ROUTE_SCOPES` below maps (method, route pattern) → the scope a Bearer
//! token must carry to reach it. A route with **no entry** is denied to
//! token-originated requests by default (ADR-003's explicit "deliberate,
//! tested choice" between the two options it named). This means the table
//! only needs to name routes that scopes actually cover — every
//! admin/owner-gated route in the app that *isn't* named here is closed to
//! tokens structurally, by construction, without having to enumerate it.
//! `NO_SCOPE_REQUIRED` is the escape hatch for routes that were already
//! fully public (no session/token check at all) before this middleware
//! existed, so this change doesn't newly lock out a Bearer-token request
//! that happens to reach them (e.g. the `/v1/*` OpenAI-compatible proxy is
//! explicitly documented as "no auth" and must stay that way).
//!
//! # No-op for human sessions
//!
//! This middleware only acts when `bearer_auth::middleware` marked the
//! request as token-originated (`TokenScopes` extension present). A normal
//! session-cookie request never carries that marker, so it passes straight
//! through — see `human_session_cookie_login_is_unaffected_by_scope_changes`
//! in `api/scope_bypass_tests.rs`.

use crate::{api::bearer_auth::TokenScopes, AppState};
use axum::{
    extract::{MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

enum Requirement {
    Scope(&'static str),
    NoScopeRequired,
}
use Requirement::*;

/// (HTTP method, exact route pattern as registered in `api/mod.rs`'s
/// `router()`) → what a Bearer token needs to reach it. Verified against
/// the routes actually mounted in `main_router`/`embed_router`, not the
/// possibly-stale route table in docs/codebase-map.md §2 — see CLAUDE.md's
/// warning about the map going stale.
const ROUTE_SCOPES: &[(&str, &str, Requirement)] = &[
    // Fully public before this middleware existed — must stay reachable
    // with or without a Bearer token attached.
    ("GET", "/api/health", NoScopeRequired),
    ("GET", "/status", NoScopeRequired),
    ("GET", "/api/capabilities", NoScopeRequired),
    ("GET", "/api/settings/public", NoScopeRequired),
    ("GET", "/v1/models", NoScopeRequired),
    ("POST", "/v1/chat/completions", NoScopeRequired),
    // metrics:read
    ("GET", "/api/metrics/current", Scope("metrics:read")),
    ("GET", "/api/metrics/ws", Scope("metrics:read")),
    // services:read / services:restart
    ("GET", "/api/services", Scope("services:read")),
    ("GET", "/api/services/:name", Scope("services:read")),
    (
        "POST",
        "/api/services/:name/action",
        Scope("services:restart"),
    ),
    // containers:read / containers:restart / containers:logs
    ("GET", "/api/containers", Scope("containers:read")),
    ("GET", "/api/containers/images", Scope("containers:read")),
    (
        "POST",
        "/api/containers/:id/action",
        Scope("containers:restart"),
    ),
    ("GET", "/api/containers/:id/logs", Scope("containers:logs")),
    (
        "GET",
        "/api/containers/:id/logs/stream",
        Scope("containers:logs"),
    ),
    // apps:read / apps:deploy / apps:restart
    ("GET", "/api/apps/catalog", Scope("apps:read")),
    ("GET", "/api/apps/detect-env", Scope("apps:read")),
    ("GET", "/api/apps/deployed", Scope("apps:read")),
    ("GET", "/api/apps/:project_name/status", Scope("apps:read")),
    ("GET", "/api/apps/:project_name/logs", Scope("apps:read")),
    ("GET", "/api/apps/:project_name/compose", Scope("apps:read")),
    ("POST", "/api/apps/deploy", Scope("apps:deploy")),
    (
        "POST",
        "/api/apps/:project_name/restart",
        Scope("apps:restart"),
    ),
    // backups:read / backups:run
    ("GET", "/api/backups", Scope("backups:read")),
    ("POST", "/api/backups/:id/run", Scope("backups:run")),
    // alerts:read / alerts:ack
    ("GET", "/api/alerts", Scope("alerts:read")),
    ("GET", "/api/events/stream", Scope("alerts:read")),
    ("GET", "/api/integrations/events", Scope("alerts:read")),
    ("POST", "/api/alerts/:id/acknowledge", Scope("alerts:ack")),
    ("POST", "/api/alerts/:id/resolve", Scope("alerts:ack")),
    // automation:read / automation:run
    ("GET", "/api/automation", Scope("automation:read")),
    ("GET", "/api/automation/:id/runs", Scope("automation:read")),
    ("POST", "/api/automation/:id/run", Scope("automation:run")),
    // timeline:read
    ("GET", "/api/timeline", Scope("timeline:read")),
    // network:read
    ("GET", "/api/network/neighbors", Scope("network:read")),
    // files:read
    ("GET", "/api/files/roots", Scope("files:read")),
    ("GET", "/api/files/list", Scope("files:read")),
    ("GET", "/api/files/read", Scope("files:read")),
    ("GET", "/api/files/activity", Scope("files:read")),
    ("GET", "/api/files/raw", Scope("files:read")),
    // storage:read
    ("GET", "/api/storage/devices", Scope("storage:read")),
    ("GET", "/api/storage/mounts", Scope("storage:read")),
    ("GET", "/api/storage/fstab", Scope("storage:read")),
    ("GET", "/api/storage/smart/:dev", Scope("storage:read")),
    ("GET", "/api/storage/raid", Scope("storage:read")),
    ("GET", "/api/storage/paths", Scope("storage:read")),
    // proxy:read / proxy:manage
    ("GET", "/api/proxy", Scope("proxy:read")),
    ("GET", "/api/proxy/nginx-setup", Scope("proxy:read")),
    ("GET", "/api/proxy/nginx/logs", Scope("proxy:read")),
    ("GET", "/api/proxy/nginx/status", Scope("proxy:read")),
    ("GET", "/api/proxy/:id/health", Scope("proxy:read")),
    ("POST", "/api/proxy", Scope("proxy:manage")),
    ("POST", "/api/proxy/nginx/action", Scope("proxy:manage")),
    ("POST", "/api/proxy/ai-auto", Scope("proxy:manage")),
    ("POST", "/api/proxy/:id/toggle", Scope("proxy:manage")),
    ("PUT", "/api/proxy/:id", Scope("proxy:manage")),
    ("DELETE", "/api/proxy/:id", Scope("proxy:manage")),
    // diagnostics:read
    ("GET", "/api/diagnostics", Scope("diagnostics:read")),
    // secrets:list — deliberately excludes reveal/rotate/create/update/delete,
    // which is the exact bypass docs/codebase-map.md §4 calls out by name.
    ("GET", "/api/secrets", Scope("secrets:list")),
    // vms:read
    ("GET", "/api/vms/local", Scope("vms:read")),
    ("GET", "/api/vms/proxmox/vms", Scope("vms:read")),
    // tags:read
    ("GET", "/api/tags", Scope("tags:read")),
    ("GET", "/api/tags/for", Scope("tags:read")),
    ("GET", "/api/tags/map", Scope("tags:read")),
];

fn required_for(method: &str, matched_path: &str) -> Option<&'static Requirement> {
    ROUTE_SCOPES
        .iter()
        .find(|(m, p, _)| *m == method && *p == matched_path)
        .map(|(_, _, req)| req)
}

pub async fn middleware(State(_state): State<AppState>, req: Request, next: Next) -> Response {
    // No-op for anything that didn't arrive via a Bearer token — human
    // session-cookie logins and fully unauthenticated requests pass through
    // completely unaffected, by construction (there's nothing to check).
    let Some(TokenScopes(token_scopes)) = req.extensions().get::<TokenScopes>().cloned() else {
        return next.run(req).await;
    };

    let method = req.method().as_str().to_string();
    let matched_path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string());

    let Some(matched_path) = matched_path else {
        // No route matched at this layer (shouldn't happen for a mounted
        // route, but fail closed rather than silently allow).
        return deny();
    };

    match required_for(&method, &matched_path) {
        Some(NoScopeRequired) => next.run(req).await,
        Some(Scope(scope)) if token_scopes.iter().any(|s| s == scope) => next.run(req).await,
        Some(Scope(_)) => deny(),
        // No table entry at all: default-deny for token-originated requests.
        None => deny(),
    }
}

fn deny() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": {
                "code": "insufficient_scope",
                "message": "This API token's scopes do not permit this action."
            }
        })),
    )
        .into_response()
}
