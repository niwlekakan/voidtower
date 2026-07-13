#![cfg(test)]
//! Session-role authz matrix (gap-analysis P1, task P1-01): a hand-maintained
//! `(method, path, Role)` table declaring the minimum **session-cookie** role every route
//! in `api::router()` requires, kept honest by `every_registered_route_declares_a_required_role`
//! below (same parser-based exhaustiveness technique `voidwatch::risk_class`'s
//! `every_registered_route_has_a_risk_class` already uses — this file writes its own copy of
//! the extractor rather than importing `risk_class`'s, per this task's spec, which explicitly
//! scopes `voidwatch/risk_class.rs` out as a forbidden zone with no ADR grant here).
//!
//! **This table governs session-cookie requests only.** `auth::scope_enforce::ROUTE_SCOPES`
//! (a separate, forbidden-zone table, P0-06) is the deny-by-default table for **Bearer-token**
//! requests — the two tables classify the same route surface along two independent auth
//! dimensions and are deliberately not unified, same rationale as `risk_class.rs`'s own two
//! tables.
//!
//! ## `Role` categories
//!
//! - [`Role::Public`] — no credential of any kind required (`/api/health`, `/status`,
//!   `/api/settings/public`, `/v1/*`, login/bootstrap/OIDC-entry routes, and a handful of
//!   read-only informational routes that are open by *design*: `/api/integrations/scopes`,
//!   `/api/integrations/odysseus/manifest`). **Also used, per this task's operator-resolution
//!   note below, for six routes that turn out to have no auth guard at all** — a real gap,
//!   not a public-by-design route; see the `NO-AUTH FINDING` comments inline in the table.
//! - [`Role::Session`]`(tier)` — gated by a session cookie whose role is checked with a
//!   correct **allowlist** (`matches!(user.role.as_str(), "owner" | "admin")` and friends).
//! - [`Role::SessionDenylist`]`(tier)` — gated by a session cookie, but the actual guard code
//!   is a **denylist** (`if user.role == "viewer" { Forbidden }`) instead of an allowlist. See
//!   "The denylist gap" below — this is a real, currently-shipped authorization bug, not a
//!   table-authoring choice.
//! - [`Role::NonSessionCredential`] — gated by a credential that isn't the `vt_session`
//!   cookie/role system at all (Bearer API token via `check_mcp_auth`, HMAC webhook secret,
//!   single-use pairing code, per-node device token). Out of scope for a *session-role*
//!   matrix by construction — still given a table entry for exhaustiveness, but excluded from
//!   this file's session-cookie-driven probes (`authz_matrix_tests.rs`).
//!
//! ## The denylist gap (operator decisions, 2026-07-12 and 2026-07-13,
//! `.devteam/active/P1-01-authz-route-matrix.md`)
//!
//! A prior worker session escalated from this task's read-only survey phase after finding
//! that five route groups — `firewall.rs`'s three admin routes, `terminal.rs`'s WS-upgrade
//! route and its `require_operator`-gated session-management routes, `audit.rs`, and
//! `timeline.rs` — gate access with a **denylist** (`role == "viewer"`, or
//! `role == "viewer" || role == "operator"`) instead of the **allowlist** pattern every other
//! handler in this codebase uses. The role ladder grew after these guards were written
//! (`guest`/`demo`/`member` added later, `docs/codebase-map.md` §3) and the denylists never
//! learned about the new low-trust roles, so those three roles fall through as authorized —
//! including `terminal.rs`'s interactive local/SSH shell routes, i.e. host shell access from
//! a guest/demo/member session-cookie login.
//!
//! The operator's 2026-07-12 resolution: **ship this matrix now with the gap explicitly
//! documented, not hidden or silently routed around.** `Role::SessionDenylist(min_intended_tier)`
//! records *both* facts at once — the tier the guard's own literal checks correctly enforce
//! against the `owner > admin > operator > viewer` ladder (so this file's standard wrong-role
//! probes, which only ever mint `owner`/`admin`/`operator`/`viewer` sessions, still pass — a
//! denylist that literally checks `role == "viewer"` does correctly reject a `viewer` probe)
//! *and* the fact that `guest`/`demo`/`member` sessions are wrongly admitted today.
//!
//! A later worker session, building this table from source per this task's spec rather than
//! trusting the first escalation's list, found the identical bug shape in **19 more locations**
//! across 7 more files — `apps.rs` (`update_compose`, `patch_app_env`, `delete_app_volumes`),
//! `automation.rs` (`create`/`update`/`delete`/`run_now`), `backups.rs` (six handlers),
//! `status.rs` (`create`/`delete`), `alerts.rs` (`delete_alert`), `containers.rs`
//! (`action`/`exec_ws`), and `services.rs` (`action`) — escalated again, and the operator's
//! **2026-07-13 resolution supersedes and extends** the 07-12 one: the exact same treatment
//! (tag `Role::SessionDenylist` here, and a dedicated current-bad-behavior regression test per
//! route, not just per file) now covers all ~24 locations in this one task/PR.
//! `authz_matrix_tests.rs` adds one dedicated, clearly-labeled regression test per affected
//! route — the original 5 groups plus the 19 found later — asserting the *current* (bad)
//! behavior, so a future fix that tightens any of these guards fails that test loudly and has
//! to update it as a reviewed, intentional change. `automation.rs`'s `run_now` (arbitrary
//! shell command execution via `automation_jobs.command`) and `containers.rs`'s `exec_ws`
//! (interactive `docker exec -it ... sh`) are the two highest-severity items among the 19,
//! same risk class as `terminal.rs`'s originally-escalated shell-access bug.
//!
//! `apps.rs`'s `expose_app`/`purge_app` are a different, opposite-direction bug also found
//! during this survey: `if user.role != "admin" { Forbidden }` is an *exact-match* allowlist
//! of literally just `"admin"`, excluding `owner` — the one role that should always pass.
//! Not a privilege-escalation risk (it's more restrictive than intended, not less), so it's
//! tagged plain `Role::Session(RoleTier::Admin)` here (accurate for every probe this file
//! generates, since none of them assert that a *higher*-privileged role also succeeds) and
//! just noted in the PR description.

/// A minimum session-role tier, matching the `owner > admin > operator > viewer` allowlist
/// checks used throughout `backend/src/api/*.rs` (`docs/codebase-map.md` §3's role ladder).
/// `guest`/`demo`/`member` are not distinct tiers here — every allowlist in this codebase
/// that isn't the flagged denylist bug treats them identically to (i.e. below) `viewer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoleTier {
    /// Any authenticated session, any role — `owner`/`admin`/`operator`/`viewer` and the
    /// newer `guest`/`demo`/`member` tiers alike.
    Session,
    Operator,
    Admin,
    Owner,
}

impl RoleTier {
    /// The role string to mint a session with when probing that a route requiring this tier
    /// correctly rejects the next-lower rung of the standard ladder. `RoleTier::Session` has
    /// no lower rung to probe with (every valid role satisfies it), hence `None`.
    pub(crate) fn wrong_role_probe(self) -> Option<&'static str> {
        match self {
            RoleTier::Session => None,
            RoleTier::Operator => Some("viewer"),
            RoleTier::Admin => Some("operator"),
            RoleTier::Owner => Some("admin"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    /// No credential required at all (see module doc comment — also used for the six
    /// no-guard-at-all findings, per the operator resolution).
    Public,
    /// Session-cookie gated with a correct allowlist check.
    Session(RoleTier),
    /// Session-cookie gated, but the actual guard is a denylist — see module doc comment's
    /// "The denylist gap". `RoleTier` here is the tier the denylist's literal checks
    /// correctly enforce against the standard ladder, not a claim that the route is fully
    /// correctly gated.
    SessionDenylist(RoleTier),
    /// Gated by a non-session credential (Bearer API token, webhook HMAC secret, pairing
    /// code, per-node device token) — not a session-role check in either direction.
    NonSessionCredential,
}

impl Role {
    /// Every non-`Public` route must reject a fully credential-free request with 401 —
    /// `NonSessionCredential` routes are excluded because their actual failure mode when
    /// missing credentials is governed by a different mechanism (feature-flag checks that
    /// run before the credential check, e.g. `mcp.rs`'s `check_mcp_auth` returning 403 when
    /// `odysseus.mcp_enabled` is off, independent of whether a token was supplied).
    pub(crate) fn requires_unauthenticated_401(self) -> bool {
        matches!(self, Role::Session(_) | Role::SessionDenylist(_))
    }

    /// `Some(tier)` for both session-gated variants (the wrong-role probe is identical for
    /// `Session` and `SessionDenylist` — see module doc comment), `None` otherwise.
    pub(crate) fn wrong_role_tier(self) -> Option<RoleTier> {
        match self {
            Role::Session(t) | Role::SessionDenylist(t) => Some(t),
            Role::Public | Role::NonSessionCredential => None,
        }
    }
}

/// Routes whose handler takes an `axum::extract::ws::WebSocketUpgrade` parameter. A plain
/// `oneshot` request without real WebSocket handshake headers (`Connection: upgrade`,
/// `Upgrade: websocket`, `Sec-WebSocket-Key`, `Sec-WebSocket-Version: 13`) fails that
/// extractor before the handler body — and therefore its role check — ever runs, so a live
/// probe against these routes can't validate session-role behavior without simulating a full
/// WS handshake. Excluded from `authz_matrix_tests.rs`'s generated probes for that reason;
/// still fully classified in `SESSION_ROLE_MATRIX` above for exhaustiveness.
pub(crate) const WS_UPGRADE_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/agents/ws"),
    ("GET", "/api/metrics/ws"),
    ("GET", "/api/containers/:id/logs/stream"),
    ("GET", "/api/containers/:id/exec"),
    ("GET", "/api/terminal/ws"),
    ("GET", "/api/terminal/ssh/ws"),
];

/// Exhaustive `(method, path, Role)` table for every route in `api::router()`. Kept sorted
/// by path then method for reviewability, mirroring `risk_class::ROUTE_RISK_CLASSES`.
pub(crate) const SESSION_ROLE_MATRIX: &[(&str, &str, Role)] = &[
    ("GET", "/api/agents", Role::Session(RoleTier::Session)),
    ("POST", "/api/agents", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/agents/:id", Role::Session(RoleTier::Admin)),
    ("PUT", "/api/agents/:id", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/agents/:id/status",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/agents/:id/status",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/agents/export", Role::Session(RoleTier::Admin)),
    ("POST", "/api/agents/import", Role::Session(RoleTier::Admin)),
    ("GET", "/api/agents/ws", Role::Session(RoleTier::Session)),
    ("POST", "/api/ai/ask", Role::Session(RoleTier::Session)),
    ("GET", "/api/ai/context", Role::Session(RoleTier::Session)),
    ("GET", "/api/ai/llama", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/ai/llama/unload",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/ai/providers", Role::Session(RoleTier::Admin)),
    ("POST", "/api/ai/providers", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/ai/providers/:id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "PUT",
        "/api/ai/providers/:id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/ai/providers/:id/health",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/alerts", Role::Session(RoleTier::Session)),
    (
        "DELETE",
        "/api/alerts/:id",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/alerts/:id/acknowledge",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/alerts/:id/resolve",
        Role::Session(RoleTier::Session),
    ),
    (
        "DELETE",
        "/api/apps/:project_name",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/apps/:project_name/compose",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/compose",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/apps/:project_name/convert",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/delete-volumes",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/apps/:project_name/env",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/apps/:project_name/expose",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/apps/:project_name/logs",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/pull",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/purge",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/apps/:project_name/redeploy",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/restart",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/start",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/apps/:project_name/status",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/:project_name/stop",
        Role::Session(RoleTier::Session),
    ),
    ("POST", "/api/apps/adopt", Role::Session(RoleTier::Session)),
    ("GET", "/api/apps/catalog", Role::Session(RoleTier::Session)),
    ("POST", "/api/apps/deploy", Role::Session(RoleTier::Session)),
    (
        "POST",
        "/api/apps/deploy-custom",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/deploy/cancel/:project_name",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/apps/deployed",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/apps/detect-env",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/apps/detect-external",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/apps/embed/:project_name/*path",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/apps/open-ui",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/audit",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    ("POST", "/api/auth/bootstrap", Role::Public),
    ("POST", "/api/auth/login", Role::Public),
    ("POST", "/api/auth/logout", Role::Public),
    ("GET", "/api/auth/me", Role::Session(RoleTier::Session)),
    ("GET", "/api/auth/oidc/callback", Role::Public),
    ("GET", "/api/auth/oidc/login", Role::Public),
    ("GET", "/api/auth/oidc/status", Role::Public),
    (
        "POST",
        "/api/auth/totp/disable",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/auth/totp/enable",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/auth/totp/setup",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/automation", Role::Session(RoleTier::Session)),
    (
        "POST",
        "/api/automation",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "DELETE",
        "/api/automation/:id",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "PATCH",
        "/api/automation/:id",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/automation/:id/run",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/automation/:id/runs",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/backups", Role::Session(RoleTier::Session)),
    (
        "POST",
        "/api/backups",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "DELETE",
        "/api/backups/:id",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/backups/:id/check",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/backups/:id/delete-plan",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/backups/:id/restore-test",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/backups/:id/run",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    ("GET", "/api/capabilities", Role::Public),
    ("GET", "/api/containers", Role::Session(RoleTier::Session)),
    (
        "POST",
        "/api/containers/:id/action",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/containers/:id/compose",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/containers/:id/compose/apply",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/containers/:id/compose/propose",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/containers/:id/exec",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/containers/:id/logs",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/containers/:id/logs/stream",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/containers/images",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/diagnostics", Role::Public),
    (
        "POST",
        "/api/disaster/emergency-disable",
        Role::Session(RoleTier::Owner),
    ),
    (
        "POST",
        "/api/disaster/emergency-reset-admin",
        Role::Session(RoleTier::Owner),
    ),
    (
        "POST",
        "/api/disaster/export-config",
        Role::Session(RoleTier::Owner),
    ),
    (
        "POST",
        "/api/disaster/import-config",
        Role::Session(RoleTier::Owner),
    ),
    (
        "GET",
        "/api/events/stream",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/files/activity",
        Role::Session(RoleTier::Operator),
    ),
    (
        "DELETE",
        "/api/files/delete",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/files/list", Role::Session(RoleTier::Operator)),
    ("POST", "/api/files/mkdir", Role::Session(RoleTier::Admin)),
    ("GET", "/api/files/raw", Role::Session(RoleTier::Operator)),
    ("GET", "/api/files/read", Role::Session(RoleTier::Operator)),
    ("POST", "/api/files/rename", Role::Session(RoleTier::Admin)),
    ("GET", "/api/files/roots", Role::Session(RoleTier::Operator)),
    ("POST", "/api/files/write", Role::Session(RoleTier::Admin)),
    ("GET", "/api/firewall", Role::Session(RoleTier::Session)),
    (
        "POST",
        "/api/firewall/action",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/firewall/rules",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/firewall/rules/delete",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    ("GET", "/api/health", Role::Public),
    (
        "GET",
        "/api/integrations/actions",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/integrations/events",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/integrations/odysseus/config",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/integrations/odysseus/config",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/integrations/odysseus/manifest", Role::Public),
    (
        "GET",
        "/api/integrations/odysseus/theme",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/integrations/scopes", Role::Public),
    (
        "GET",
        "/api/integrations/tokens",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/integrations/tokens",
        Role::Session(RoleTier::Admin),
    ),
    (
        "DELETE",
        "/api/integrations/tokens/:id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/integrations/webhooks",
        Role::NonSessionCredential,
    ),
    ("GET", "/api/lxc", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/lxc/:vmid/action",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/lxc/:vmid/config",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/mcp", Role::NonSessionCredential),
    ("POST", "/api/mcp/message", Role::NonSessionCredential),
    ("GET", "/api/members", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/members/:user_id/access",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/members/:user_id/access",
        Role::Session(RoleTier::Admin),
    ),
    (
        "DELETE",
        "/api/members/:user_id/access/:app_id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/members/:user_id/custom-deploy",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/members/:user_id/drives",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/members/:user_id/storage",
        Role::Session(RoleTier::Admin),
    ),
    (
        "DELETE",
        "/api/members/drives/:drive_id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/members/me/access",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/members/me/nodes",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/metrics/current",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/metrics/ws", Role::Session(RoleTier::Session)),
    ("GET", "/api/models", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/models/:filename",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/models/active", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/models/download",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/models/download/:id", Role::Public),
    (
        "GET",
        "/api/models/llama-config",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/models/llama-config",
        Role::Session(RoleTier::Admin),
    ),
    ("POST", "/api/models/load", Role::Session(RoleTier::Admin)),
    ("GET", "/api/models/ollama", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/models/ollama-config",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/models/ollama-config",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/models/ollama/create",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/models/ollama/create/:id", Role::Public),
    (
        "POST",
        "/api/models/ollama/pull",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/models/ollama/pull/:id", Role::Public),
    ("GET", "/api/mods", Role::Session(RoleTier::Admin)),
    ("POST", "/api/mods/apply", Role::Session(RoleTier::Admin)),
    ("POST", "/api/mods/config", Role::Session(RoleTier::Admin)),
    ("GET", "/api/mods/diff", Role::Session(RoleTier::Admin)),
    ("POST", "/api/mods/fetch", Role::Session(RoleTier::Admin)),
    ("POST", "/api/mods/rollback", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/nav-config",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/nav-config", Role::Session(RoleTier::Session)),
    ("POST", "/api/nav-config", Role::Session(RoleTier::Session)),
    (
        "DELETE",
        "/api/nav-config/default",
        Role::Session(RoleTier::Owner),
    ),
    (
        "GET",
        "/api/nav-config/default",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/nav-config/default",
        Role::Session(RoleTier::Owner),
    ),
    (
        "GET",
        "/api/network/neighbors",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/nodes", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/nodes/:id", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/nodes/:id/heartbeat",
        Role::NonSessionCredential,
    ),
    ("POST", "/api/nodes/enroll", Role::NonSessionCredential),
    (
        "POST",
        "/api/nodes/pairing-code",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/oidc/config", Role::Session(RoleTier::Admin)),
    ("PUT", "/api/oidc/config", Role::Session(RoleTier::Admin)),
    ("GET", "/api/plugins", Role::Session(RoleTier::Session)),
    ("POST", "/api/plugins", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/plugins/:id", Role::Session(RoleTier::Admin)),
    ("PATCH", "/api/plugins/:id", Role::Session(RoleTier::Admin)),
    ("POST", "/api/policy/check", Role::Session(RoleTier::Admin)),
    ("GET", "/api/policy/rules", Role::Session(RoleTier::Admin)),
    ("POST", "/api/policy/rules", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/policy/rules/:id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "PATCH",
        "/api/policy/rules/:id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/backup-jobs",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/lxc/deploy",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/nodes",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/nodes/:node/disks",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/nodes/:node/disks/init",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/nodes/:node/disks/smart",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/nodes/:node/disks/wipe",
        Role::Session(RoleTier::Admin),
    ),
    (
        "DELETE",
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/storage",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/tasks",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/vms",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/disk-passthrough",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/reboot",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/reset",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/resume",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/rollback/:snapname",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/shutdown",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/snapshot",
        Role::Session(RoleTier::Admin),
    ),
    (
        "DELETE",
        "/api/proxmox/:host_id/vms/:vmid/snapshot/:snapname",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxmox/:host_id/vms/:vmid/snapshots",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/start",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/stop",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/suspend",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/vncproxy",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/proxmox/hosts", Role::Session(RoleTier::Admin)),
    ("POST", "/api/proxmox/hosts", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/proxmox/hosts/:host_id",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/proxy", Role::Session(RoleTier::Admin)),
    ("POST", "/api/proxy", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/proxy/:id", Role::Session(RoleTier::Admin)),
    ("PUT", "/api/proxy/:id", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/proxy/:id/health",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxy/:id/toggle",
        Role::Session(RoleTier::Admin),
    ),
    ("POST", "/api/proxy/ai-auto", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/proxy/nginx-setup",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/proxy/nginx/action",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxy/nginx/logs",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/proxy/nginx/status",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/secrets", Role::Session(RoleTier::Session)),
    ("POST", "/api/secrets", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/secrets/:id", Role::Session(RoleTier::Admin)),
    ("PATCH", "/api/secrets/:id", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/secrets/:id/reveal",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/secrets/:id/rotate",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/security/sessions",
        Role::Session(RoleTier::Session),
    ),
    (
        "DELETE",
        "/api/security/sessions/:id",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/security/sessions/revoke-others",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/services", Role::Session(RoleTier::Session)),
    (
        "GET",
        "/api/services/:name",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/services/:name/action",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/services/:name/logs",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/settings/ai-url",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/settings/ai-url",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/settings/general",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/settings/general",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/settings/mfa-policy",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/settings/mfa-policy",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/settings/notifications",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/settings/notifications",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/settings/notifications/test",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/settings/public", Role::Public),
    (
        "GET",
        "/api/status-checks",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/status-checks",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "DELETE",
        "/api/status-checks/:id",
        Role::SessionDenylist(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/storage/devices",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/storage/format",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/storage/fstab", Role::Session(RoleTier::Admin)),
    ("POST", "/api/storage/fstab", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/storage/fstab/:idx",
        Role::Session(RoleTier::Admin),
    ),
    ("POST", "/api/storage/mount", Role::Session(RoleTier::Admin)),
    ("GET", "/api/storage/mounts", Role::Session(RoleTier::Admin)),
    ("GET", "/api/storage/paths", Role::Session(RoleTier::Admin)),
    ("POST", "/api/storage/paths", Role::Session(RoleTier::Admin)),
    ("GET", "/api/storage/raid", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/storage/raid/create",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/storage/raid/stop",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/storage/smart/:dev",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/storage/umount",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/studio/audio/:filename",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/studio/gallery",
        Role::Session(RoleTier::Session),
    ),
    (
        "DELETE",
        "/api/studio/gallery/:kind/:filename",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/studio/image/generate",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/studio/images/:filename",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/studio/mcp/invoke",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/studio/mcp/tools",
        Role::Session(RoleTier::Session),
    ),
    (
        "GET",
        "/api/studio/status",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/studio/stt/transcribe",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/studio/tts/generate",
        Role::Session(RoleTier::Session),
    ),
    (
        "POST",
        "/api/system/restart",
        Role::Session(RoleTier::Admin),
    ),
    ("POST", "/api/system/update", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/system/update-check",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/system/version", Role::Public),
    ("GET", "/api/tabs", Role::Session(RoleTier::Session)),
    ("POST", "/api/tabs", Role::Session(RoleTier::Session)),
    ("DELETE", "/api/tabs/:id", Role::Session(RoleTier::Session)),
    ("PUT", "/api/tabs/:id", Role::Session(RoleTier::Session)),
    ("GET", "/api/tabs/export", Role::Session(RoleTier::Session)),
    ("POST", "/api/tabs/import", Role::Session(RoleTier::Session)),
    ("PUT", "/api/tabs/order", Role::Session(RoleTier::Session)),
    ("GET", "/api/tags", Role::Session(RoleTier::Admin)),
    ("POST", "/api/tags", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/tags/:id", Role::Session(RoleTier::Admin)),
    ("PATCH", "/api/tags/:id", Role::Session(RoleTier::Admin)),
    ("POST", "/api/tags/assign", Role::Session(RoleTier::Admin)),
    ("GET", "/api/tags/for", Role::Session(RoleTier::Admin)),
    ("GET", "/api/tags/map", Role::Session(RoleTier::Admin)),
    ("POST", "/api/tags/unassign", Role::Session(RoleTier::Admin)),
    (
        "GET",
        "/api/terminal/local/sessions",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/terminal/local/sessions",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "DELETE",
        "/api/terminal/local/sessions/:id",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "PUT",
        "/api/terminal/local/sessions/:id",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/terminal/ssh/sessions",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "POST",
        "/api/terminal/ssh/sessions",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "DELETE",
        "/api/terminal/ssh/sessions/:id",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "PUT",
        "/api/terminal/ssh/sessions/:id",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/terminal/ssh/ws",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/terminal/ws",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    (
        "GET",
        "/api/timeline",
        Role::SessionDenylist(RoleTier::Operator),
    ),
    ("GET", "/api/updates/docker", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/updates/docker/:id/apply",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/updates/docker/check",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/updates/odysseus",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/updates/odysseus/apply",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/updates/os", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/updates/os/apply",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/updates/voidtower",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/updates/voidtower/apply",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/updates/voidtower/check",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/updates/voidtower/rollback",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/users", Role::Session(RoleTier::Admin)),
    ("POST", "/api/users", Role::Session(RoleTier::Admin)),
    ("DELETE", "/api/users/:id", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/users/me/password",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/api/vms/local", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/vms/local/action",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/vms/proxmox/action",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/vms/proxmox/config",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/vms/proxmox/config",
        Role::Session(RoleTier::Admin),
    ),
    (
        "POST",
        "/api/vms/proxmox/test",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/api/vms/proxmox/vms",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/webhooks", Role::Session(RoleTier::Admin)),
    ("POST", "/api/webhooks", Role::Session(RoleTier::Admin)),
    (
        "DELETE",
        "/api/webhooks/:id",
        Role::Session(RoleTier::Admin),
    ),
    ("PATCH", "/api/webhooks/:id", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/webhooks/:id/test",
        Role::Session(RoleTier::Admin),
    ),
    ("GET", "/api/wireguard", Role::Session(RoleTier::Admin)),
    (
        "POST",
        "/api/wireguard/peers",
        Role::Session(RoleTier::Admin),
    ),
    (
        "DELETE",
        "/api/wireguard/peers/:id",
        Role::Session(RoleTier::Admin),
    ),
    (
        "GET",
        "/plugin-assets/:id/*path",
        Role::Session(RoleTier::Session),
    ),
    ("GET", "/status", Role::Public),
    ("POST", "/v1/chat/completions", Role::Public),
    ("GET", "/v1/models", Role::Public),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Copy of `voidwatch::risk_class::tests::extract_registered_routes`'s manual
    /// balanced-paren scan — deliberately re-implemented here rather than imported, per this
    /// task's spec (`voidwatch/risk_class.rs` is a forbidden zone with no ADR grant for this
    /// task, so this file must not depend on anything inside it, including its test-only
    /// parser). Mirrors the `.route("path", verb(...).verb(...))` shape used throughout
    /// `api::router()`.
    fn extract_registered_routes(src: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut i = 0usize;
        while let Some(rel) = src[i..].find(".route(\"") {
            let start = i + rel;
            let path_start = start + ".route(\"".len();
            let path_len = src[path_start..]
                .find('"')
                .expect("unterminated route path string");
            let path = &src[path_start..path_start + path_len];

            let open = start + ".route".len();
            debug_assert_eq!(&src[open..open + 1], "(");
            let mut depth = 0i32;
            let mut inner_start = 0usize;
            let mut close = open;
            for (offset, ch) in src[open..].char_indices() {
                match ch {
                    '(' => {
                        depth += 1;
                        if depth == 1 {
                            inner_start = open + offset + 1;
                        }
                    }
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = open + offset;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let inner = &src[inner_start..close];

            for method in ["GET", "POST", "PUT", "PATCH", "DELETE"] {
                let needle = format!("{}(", method.to_ascii_lowercase());
                if inner.contains(&needle) {
                    out.push((method.to_string(), path.to_string()));
                }
            }
            i = close + 1;
        }
        out
    }

    #[test]
    fn session_role_matrix_has_no_duplicate_entries() {
        let mut seen = HashSet::new();
        for (method, path, _) in SESSION_ROLE_MATRIX {
            assert!(
                seen.insert((*method, *path)),
                "duplicate SESSION_ROLE_MATRIX entry: {method} {path}"
            );
        }
    }

    /// The core deliverable (gap-analysis P1 table row 2): every route registered in
    /// `api::router()` must have a `SESSION_ROLE_MATRIX` entry, and vice versa. Parses
    /// `api/mod.rs` directly rather than trusting this table, so a new endpoint shipped
    /// without a matching classification fails the build — "catches the 'new endpoint forgot
    /// auth' class mechanically, forever."
    #[test]
    fn every_registered_route_declares_a_required_role() {
        let src = include_str!("mod.rs");
        let registered = extract_registered_routes(src);
        assert!(
            registered.len() > 200,
            "sanity check: route extraction found suspiciously few routes ({}) — the parser \
             may have broken against a source formatting change",
            registered.len()
        );

        let table: HashSet<(&str, &str)> = SESSION_ROLE_MATRIX
            .iter()
            .map(|(m, p, _)| (*m, *p))
            .collect();
        let missing: Vec<String> = registered
            .iter()
            .filter(|(m, p)| !table.contains(&(m.as_str(), p.as_str())))
            .map(|(m, p)| format!("{m} {p}"))
            .collect();
        assert!(
            missing.is_empty(),
            "routes registered in api/mod.rs with no SESSION_ROLE_MATRIX entry (new endpoint \
             shipped without a declared session role): {missing:?}"
        );

        let registered_set: HashSet<(&str, &str)> = registered
            .iter()
            .map(|(m, p)| (m.as_str(), p.as_str()))
            .collect();
        let stale: Vec<String> = SESSION_ROLE_MATRIX
            .iter()
            .filter(|(m, p, _)| !registered_set.contains(&(*m, *p)))
            .map(|(m, p, _)| format!("{m} {p}"))
            .collect();
        assert!(
            stale.is_empty(),
            "SESSION_ROLE_MATRIX entries with no corresponding route in api/mod.rs (route was \
             removed or renamed — update the table): {stale:?}"
        );
    }
}
