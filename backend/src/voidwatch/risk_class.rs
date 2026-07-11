//! Compile-time risk classification for the API surface (EDD §3.2, gap-analysis P0.3).
//!
//! Two separate tables, deliberately not unified, because they classify two different
//! things:
//!
//! - [`ROUTE_RISK_CLASSES`] classifies every HTTP route registered in `api::router()`
//!   (`backend/src/api/mod.rs`) by `(method, path)`. This is the literal "every action
//!   name in the API surface" requirement — enforced exhaustively (both directions) by
//!   `every_registered_route_has_a_risk_class` below, which parses `api/mod.rs` at test
//!   time (the same source-grep-regression-test style already used by
//!   `voidwatch::tests::evaluate_is_the_only_caller_of_policy_check_from_ai_ingress`) so a
//!   route added to the router without a matching entry here fails the build. It is *not*
//!   wired into `evaluate()`'s mode pre-pass, because most of these handlers
//!   (`containers.rs`, `firewall.rs`, `proxmox.rs`, `backups.rs`, ...) are not
//!   AI-ingress files under this task's ADR-001 grant and are explicitly out of scope for
//!   the voidwatch choke point (see `voidwatch/mod.rs`'s own doc comment) — this table
//!   exists as the risk-classification ledger for the whole surface (and the hook P0-04
//!   hangs its irreversibility denylist off, per the P0-03 task spec's coordination note),
//!   independent of which handlers currently route through voidwatch.
//! - [`for_action`] classifies the much smaller vocabulary of *action names* actually
//!   passed into `voidwatch::evaluate()` by the three AI/automation ingress files
//!   (`api/mcp.rs`'s MCP tool dispatch, `api/integrations.rs`'s webhook structured
//!   actions and `run_automation_job`). This is what the mode ladder pre-pass in
//!   `evaluate()` actually consults, since those call sites pass tool/action names
//!   (`"restart"`, `"automation.run"`, MCP tool names), not HTTP routes.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskClass {
    /// Reserved for completeness of the enum (mirrors `ActorKind::Ai`'s precedent) —
    /// `for_action()` never returns it, since `evaluate()` already short-circuits `Read`-
    /// classified actions via `ActionKind` before risk_class is ever consulted, and no
    /// `ROUTE_RISK_CLASSES` entry using it is read at runtime (see below).
    #[allow(dead_code)]
    Read,
    Mutate,
    Destructive,
    Irreversible,
}

/// Exhaustive `(method, path, RiskClass)` table for every route in `api::router()`. Kept
/// sorted by path then method for reviewability. See module doc comment — deliberately
/// not read by any runtime code path today (`#[allow(dead_code)]`: this crate has no lib
/// target, so a table consumed only by its own test, same as `ActorKind::Ai`, still trips
/// the bin-target dead-code lint), only by `every_registered_route_has_a_risk_class`.
#[allow(dead_code)]
pub const ROUTE_RISK_CLASSES: &[(&str, &str, RiskClass)] = &[
    ("GET", "/api/agents", RiskClass::Read),
    ("POST", "/api/agents", RiskClass::Mutate),
    ("DELETE", "/api/agents/:id", RiskClass::Destructive),
    ("PUT", "/api/agents/:id", RiskClass::Mutate),
    ("GET", "/api/agents/:id/status", RiskClass::Read),
    ("POST", "/api/agents/:id/status", RiskClass::Mutate),
    ("GET", "/api/agents/export", RiskClass::Read),
    ("POST", "/api/agents/import", RiskClass::Mutate),
    ("GET", "/api/agents/ws", RiskClass::Read),
    ("POST", "/api/ai/ask", RiskClass::Mutate),
    ("GET", "/api/ai/context", RiskClass::Read),
    ("GET", "/api/ai/llama", RiskClass::Read),
    ("POST", "/api/ai/llama/unload", RiskClass::Mutate),
    ("GET", "/api/ai/providers", RiskClass::Read),
    ("POST", "/api/ai/providers", RiskClass::Mutate),
    ("DELETE", "/api/ai/providers/:id", RiskClass::Destructive),
    ("PUT", "/api/ai/providers/:id", RiskClass::Mutate),
    ("GET", "/api/ai/providers/:id/health", RiskClass::Read),
    ("GET", "/api/alerts", RiskClass::Read),
    ("DELETE", "/api/alerts/:id", RiskClass::Destructive),
    ("POST", "/api/alerts/:id/acknowledge", RiskClass::Mutate),
    ("POST", "/api/alerts/:id/resolve", RiskClass::Mutate),
    ("DELETE", "/api/apps/:project_name", RiskClass::Destructive),
    ("GET", "/api/apps/:project_name/compose", RiskClass::Read),
    ("POST", "/api/apps/:project_name/compose", RiskClass::Mutate),
    ("POST", "/api/apps/:project_name/convert", RiskClass::Mutate),
    (
        "POST",
        "/api/apps/:project_name/delete-volumes",
        // Reclassified from Destructive per ADR-004 reconciled denylist item 11
        // (`keep_data=false` app removal, `voidwatch::denylist` — no literal `keep_data`
        // flag exists in source, this is the unconditional volume-destroying analog).
        RiskClass::Irreversible,
    ),
    ("POST", "/api/apps/:project_name/env", RiskClass::Mutate),
    ("POST", "/api/apps/:project_name/expose", RiskClass::Mutate),
    ("GET", "/api/apps/:project_name/logs", RiskClass::Read),
    ("POST", "/api/apps/:project_name/pull", RiskClass::Mutate),
    (
        "POST",
        "/api/apps/:project_name/purge",
        // Reclassified from Destructive per ADR-004 reconciled denylist item 11 (see
        // `voidwatch::denylist` module doc comment for the full rationale).
        RiskClass::Irreversible,
    ),
    (
        "POST",
        "/api/apps/:project_name/redeploy",
        RiskClass::Mutate,
    ),
    ("POST", "/api/apps/:project_name/restart", RiskClass::Mutate),
    ("POST", "/api/apps/:project_name/start", RiskClass::Mutate),
    ("GET", "/api/apps/:project_name/status", RiskClass::Read),
    ("POST", "/api/apps/:project_name/stop", RiskClass::Mutate),
    ("POST", "/api/apps/adopt", RiskClass::Mutate),
    ("GET", "/api/apps/catalog", RiskClass::Read),
    ("POST", "/api/apps/deploy", RiskClass::Mutate),
    ("POST", "/api/apps/deploy-custom", RiskClass::Mutate),
    (
        "POST",
        "/api/apps/deploy/cancel/:project_name",
        RiskClass::Mutate,
    ),
    ("GET", "/api/apps/deployed", RiskClass::Read),
    ("GET", "/api/apps/detect-env", RiskClass::Read),
    ("GET", "/api/apps/detect-external", RiskClass::Read),
    (
        "GET",
        "/api/apps/embed/:project_name/*path",
        RiskClass::Read,
    ),
    ("POST", "/api/apps/open-ui", RiskClass::Mutate),
    ("GET", "/api/audit", RiskClass::Read),
    ("POST", "/api/auth/bootstrap", RiskClass::Mutate),
    ("POST", "/api/auth/login", RiskClass::Mutate),
    ("POST", "/api/auth/logout", RiskClass::Mutate),
    ("GET", "/api/auth/me", RiskClass::Read),
    ("GET", "/api/auth/oidc/callback", RiskClass::Read),
    ("GET", "/api/auth/oidc/login", RiskClass::Read),
    ("GET", "/api/auth/oidc/status", RiskClass::Read),
    ("POST", "/api/auth/totp/disable", RiskClass::Mutate),
    ("POST", "/api/auth/totp/enable", RiskClass::Mutate),
    ("POST", "/api/auth/totp/setup", RiskClass::Mutate),
    ("GET", "/api/automation", RiskClass::Read),
    ("POST", "/api/automation", RiskClass::Mutate),
    ("DELETE", "/api/automation/:id", RiskClass::Destructive),
    ("PATCH", "/api/automation/:id", RiskClass::Mutate),
    ("POST", "/api/automation/:id/run", RiskClass::Mutate),
    ("GET", "/api/automation/:id/runs", RiskClass::Read),
    ("GET", "/api/backups", RiskClass::Read),
    ("POST", "/api/backups", RiskClass::Mutate),
    ("DELETE", "/api/backups/:id", RiskClass::Destructive),
    ("POST", "/api/backups/:id/check", RiskClass::Mutate),
    ("POST", "/api/backups/:id/delete-plan", RiskClass::Mutate),
    ("POST", "/api/backups/:id/restore-test", RiskClass::Mutate),
    ("POST", "/api/backups/:id/run", RiskClass::Mutate),
    ("GET", "/api/capabilities", RiskClass::Read),
    ("GET", "/api/containers", RiskClass::Read),
    ("POST", "/api/containers/:id/action", RiskClass::Destructive),
    ("GET", "/api/containers/:id/compose", RiskClass::Read),
    (
        "POST",
        "/api/containers/:id/compose/apply",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/containers/:id/compose/propose",
        RiskClass::Mutate,
    ),
    ("GET", "/api/containers/:id/exec", RiskClass::Read),
    ("GET", "/api/containers/:id/logs", RiskClass::Read),
    ("GET", "/api/containers/:id/logs/stream", RiskClass::Read),
    ("GET", "/api/containers/images", RiskClass::Read),
    ("GET", "/api/diagnostics", RiskClass::Read),
    (
        "POST",
        "/api/disaster/emergency-disable",
        RiskClass::Irreversible,
    ),
    (
        "POST",
        "/api/disaster/emergency-reset-admin",
        RiskClass::Irreversible,
    ),
    (
        "POST",
        "/api/disaster/export-config",
        RiskClass::Irreversible,
    ),
    (
        "POST",
        "/api/disaster/import-config",
        RiskClass::Irreversible,
    ),
    ("GET", "/api/events/stream", RiskClass::Read),
    ("GET", "/api/files/activity", RiskClass::Read),
    ("DELETE", "/api/files/delete", RiskClass::Destructive),
    ("GET", "/api/files/list", RiskClass::Read),
    ("POST", "/api/files/mkdir", RiskClass::Mutate),
    ("GET", "/api/files/raw", RiskClass::Read),
    ("GET", "/api/files/read", RiskClass::Read),
    ("POST", "/api/files/rename", RiskClass::Mutate),
    ("GET", "/api/files/roots", RiskClass::Read),
    ("POST", "/api/files/write", RiskClass::Mutate),
    ("GET", "/api/firewall", RiskClass::Read),
    ("POST", "/api/firewall/action", RiskClass::Irreversible),
    ("POST", "/api/firewall/rules", RiskClass::Mutate),
    ("POST", "/api/firewall/rules/delete", RiskClass::Mutate),
    ("GET", "/api/health", RiskClass::Read),
    ("GET", "/api/integrations/actions", RiskClass::Read),
    ("GET", "/api/integrations/events", RiskClass::Read),
    ("GET", "/api/integrations/odysseus/config", RiskClass::Read),
    (
        "POST",
        "/api/integrations/odysseus/config",
        RiskClass::Mutate,
    ),
    (
        "GET",
        "/api/integrations/odysseus/manifest",
        RiskClass::Read,
    ),
    ("GET", "/api/integrations/odysseus/theme", RiskClass::Read),
    ("GET", "/api/integrations/scopes", RiskClass::Read),
    ("GET", "/api/integrations/tokens", RiskClass::Read),
    ("POST", "/api/integrations/tokens", RiskClass::Mutate),
    (
        "DELETE",
        "/api/integrations/tokens/:id",
        RiskClass::Destructive,
    ),
    ("POST", "/api/integrations/webhooks", RiskClass::Mutate),
    ("GET", "/api/lxc", RiskClass::Read),
    ("POST", "/api/lxc/:vmid/action", RiskClass::Mutate),
    ("GET", "/api/lxc/:vmid/config", RiskClass::Read),
    ("GET", "/api/mcp", RiskClass::Read),
    ("POST", "/api/mcp/message", RiskClass::Mutate),
    ("GET", "/api/members", RiskClass::Read),
    ("GET", "/api/members/:user_id/access", RiskClass::Read),
    ("POST", "/api/members/:user_id/access", RiskClass::Mutate),
    (
        "DELETE",
        "/api/members/:user_id/access/:app_id",
        RiskClass::Destructive,
    ),
    (
        "POST",
        "/api/members/:user_id/custom-deploy",
        RiskClass::Mutate,
    ),
    ("POST", "/api/members/:user_id/drives", RiskClass::Mutate),
    ("POST", "/api/members/:user_id/storage", RiskClass::Mutate),
    (
        "DELETE",
        "/api/members/drives/:drive_id",
        RiskClass::Destructive,
    ),
    ("GET", "/api/members/me/access", RiskClass::Read),
    ("GET", "/api/members/me/nodes", RiskClass::Read),
    ("GET", "/api/metrics/current", RiskClass::Read),
    ("GET", "/api/metrics/ws", RiskClass::Read),
    ("GET", "/api/models", RiskClass::Read),
    ("DELETE", "/api/models/:filename", RiskClass::Destructive),
    ("GET", "/api/models/active", RiskClass::Read),
    ("POST", "/api/models/download", RiskClass::Mutate),
    ("GET", "/api/models/download/:id", RiskClass::Read),
    ("GET", "/api/models/llama-config", RiskClass::Read),
    ("POST", "/api/models/llama-config", RiskClass::Mutate),
    ("POST", "/api/models/load", RiskClass::Mutate),
    ("GET", "/api/models/ollama", RiskClass::Read),
    ("GET", "/api/models/ollama-config", RiskClass::Read),
    ("POST", "/api/models/ollama-config", RiskClass::Mutate),
    ("POST", "/api/models/ollama/create", RiskClass::Mutate),
    ("GET", "/api/models/ollama/create/:id", RiskClass::Read),
    ("POST", "/api/models/ollama/pull", RiskClass::Mutate),
    ("GET", "/api/models/ollama/pull/:id", RiskClass::Read),
    ("GET", "/api/mods", RiskClass::Read),
    ("POST", "/api/mods/apply", RiskClass::Mutate),
    ("POST", "/api/mods/config", RiskClass::Mutate),
    ("GET", "/api/mods/diff", RiskClass::Read),
    ("POST", "/api/mods/fetch", RiskClass::Mutate),
    ("POST", "/api/mods/rollback", RiskClass::Destructive),
    ("DELETE", "/api/nav-config", RiskClass::Destructive),
    ("GET", "/api/nav-config", RiskClass::Read),
    ("POST", "/api/nav-config", RiskClass::Mutate),
    ("DELETE", "/api/nav-config/default", RiskClass::Destructive),
    ("GET", "/api/nav-config/default", RiskClass::Read),
    ("POST", "/api/nav-config/default", RiskClass::Mutate),
    ("GET", "/api/network/neighbors", RiskClass::Read),
    ("GET", "/api/nodes", RiskClass::Read),
    ("DELETE", "/api/nodes/:id", RiskClass::Destructive),
    ("POST", "/api/nodes/:id/heartbeat", RiskClass::Mutate),
    ("POST", "/api/nodes/enroll", RiskClass::Mutate),
    ("POST", "/api/nodes/pairing-code", RiskClass::Mutate),
    ("GET", "/api/oidc/config", RiskClass::Read),
    ("PUT", "/api/oidc/config", RiskClass::Mutate),
    ("GET", "/api/plugins", RiskClass::Read),
    ("POST", "/api/plugins", RiskClass::Mutate),
    ("DELETE", "/api/plugins/:id", RiskClass::Destructive),
    ("PATCH", "/api/plugins/:id", RiskClass::Mutate),
    ("POST", "/api/policy/check", RiskClass::Mutate),
    ("GET", "/api/policy/rules", RiskClass::Read),
    ("POST", "/api/policy/rules", RiskClass::Irreversible),
    ("DELETE", "/api/policy/rules/:id", RiskClass::Irreversible),
    ("PATCH", "/api/policy/rules/:id", RiskClass::Irreversible),
    ("GET", "/api/proxmox/:host_id/backup-jobs", RiskClass::Read),
    (
        "POST",
        "/api/proxmox/:host_id/lxc/deploy",
        RiskClass::Mutate,
    ),
    ("GET", "/api/proxmox/:host_id/nodes", RiskClass::Read),
    (
        "GET",
        "/api/proxmox/:host_id/nodes/:node/disks",
        RiskClass::Read,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/nodes/:node/disks/init",
        RiskClass::Irreversible,
    ),
    (
        "GET",
        "/api/proxmox/:host_id/nodes/:node/disks/smart",
        RiskClass::Read,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/nodes/:node/disks/wipe",
        RiskClass::Irreversible,
    ),
    (
        "DELETE",
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
        RiskClass::Destructive,
    ),
    (
        "GET",
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
        RiskClass::Read,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
        RiskClass::Mutate,
    ),
    ("GET", "/api/proxmox/:host_id/storage", RiskClass::Read),
    ("GET", "/api/proxmox/:host_id/tasks", RiskClass::Read),
    ("GET", "/api/proxmox/:host_id/vms", RiskClass::Read),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/disk-passthrough",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/reboot",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/reset",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/resume",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/rollback/:snapname",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/shutdown",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/snapshot",
        RiskClass::Mutate,
    ),
    (
        "DELETE",
        "/api/proxmox/:host_id/vms/:vmid/snapshot/:snapname",
        // Reclassified from Destructive to Irreversible per ADR-004 item 10 (deletion of the
        // last remaining snapshot of a resource). A static route table can't express the
        // count-dependent condition ("last remaining"), so this coarsely gates every snapshot
        // deletion in YOLO mode — the same accepted-false-positive tradeoff already documented
        // for item 6 (firewall_disable) in denylist.rs.
        RiskClass::Irreversible,
    ),
    (
        "GET",
        "/api/proxmox/:host_id/vms/:vmid/snapshots",
        RiskClass::Read,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/start",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/stop",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/suspend",
        RiskClass::Mutate,
    ),
    (
        "POST",
        "/api/proxmox/:host_id/vms/:vmid/vncproxy",
        RiskClass::Mutate,
    ),
    ("GET", "/api/proxmox/hosts", RiskClass::Read),
    ("POST", "/api/proxmox/hosts", RiskClass::Mutate),
    (
        "DELETE",
        "/api/proxmox/hosts/:host_id",
        RiskClass::Destructive,
    ),
    ("GET", "/api/proxy", RiskClass::Read),
    ("POST", "/api/proxy", RiskClass::Mutate),
    ("DELETE", "/api/proxy/:id", RiskClass::Destructive),
    ("PUT", "/api/proxy/:id", RiskClass::Mutate),
    ("GET", "/api/proxy/:id/health", RiskClass::Read),
    ("POST", "/api/proxy/:id/toggle", RiskClass::Mutate),
    ("POST", "/api/proxy/ai-auto", RiskClass::Mutate),
    ("GET", "/api/proxy/nginx-setup", RiskClass::Read),
    ("POST", "/api/proxy/nginx/action", RiskClass::Mutate),
    ("GET", "/api/proxy/nginx/logs", RiskClass::Read),
    ("GET", "/api/proxy/nginx/status", RiskClass::Read),
    ("GET", "/api/secrets", RiskClass::Read),
    ("POST", "/api/secrets", RiskClass::Mutate),
    ("DELETE", "/api/secrets/:id", RiskClass::Destructive),
    ("PATCH", "/api/secrets/:id", RiskClass::Mutate),
    ("GET", "/api/secrets/:id/reveal", RiskClass::Irreversible),
    ("POST", "/api/secrets/:id/rotate", RiskClass::Mutate),
    ("GET", "/api/security/sessions", RiskClass::Read),
    (
        "DELETE",
        "/api/security/sessions/:id",
        RiskClass::Destructive,
    ),
    (
        "POST",
        "/api/security/sessions/revoke-others",
        RiskClass::Mutate,
    ),
    ("GET", "/api/services", RiskClass::Read),
    ("GET", "/api/services/:name", RiskClass::Read),
    ("POST", "/api/services/:name/action", RiskClass::Mutate),
    ("GET", "/api/services/:name/logs", RiskClass::Read),
    ("GET", "/api/settings/ai-url", RiskClass::Read),
    ("POST", "/api/settings/ai-url", RiskClass::Mutate),
    ("GET", "/api/settings/general", RiskClass::Read),
    ("POST", "/api/settings/general", RiskClass::Mutate),
    ("GET", "/api/settings/mfa-policy", RiskClass::Read),
    ("POST", "/api/settings/mfa-policy", RiskClass::Mutate),
    ("GET", "/api/settings/notifications", RiskClass::Read),
    ("POST", "/api/settings/notifications", RiskClass::Mutate),
    (
        "POST",
        "/api/settings/notifications/test",
        RiskClass::Mutate,
    ),
    ("GET", "/api/settings/public", RiskClass::Read),
    ("GET", "/api/status-checks", RiskClass::Read),
    ("POST", "/api/status-checks", RiskClass::Mutate),
    ("DELETE", "/api/status-checks/:id", RiskClass::Destructive),
    ("GET", "/api/storage/devices", RiskClass::Read),
    ("POST", "/api/storage/format", RiskClass::Irreversible),
    ("GET", "/api/storage/fstab", RiskClass::Read),
    ("POST", "/api/storage/fstab", RiskClass::Mutate),
    ("DELETE", "/api/storage/fstab/:idx", RiskClass::Destructive),
    ("POST", "/api/storage/mount", RiskClass::Mutate),
    ("GET", "/api/storage/mounts", RiskClass::Read),
    ("GET", "/api/storage/paths", RiskClass::Read),
    ("POST", "/api/storage/paths", RiskClass::Mutate),
    ("GET", "/api/storage/raid", RiskClass::Read),
    ("POST", "/api/storage/raid/create", RiskClass::Mutate),
    ("POST", "/api/storage/raid/stop", RiskClass::Destructive),
    ("GET", "/api/storage/smart/:dev", RiskClass::Read),
    ("POST", "/api/storage/umount", RiskClass::Destructive),
    ("GET", "/api/studio/audio/:filename", RiskClass::Read),
    ("GET", "/api/studio/gallery", RiskClass::Read),
    (
        "DELETE",
        "/api/studio/gallery/:kind/:filename",
        RiskClass::Destructive,
    ),
    ("POST", "/api/studio/image/generate", RiskClass::Mutate),
    ("GET", "/api/studio/images/:filename", RiskClass::Read),
    ("POST", "/api/studio/mcp/invoke", RiskClass::Mutate),
    ("GET", "/api/studio/mcp/tools", RiskClass::Read),
    ("GET", "/api/studio/status", RiskClass::Read),
    ("POST", "/api/studio/stt/transcribe", RiskClass::Mutate),
    ("POST", "/api/studio/tts/generate", RiskClass::Mutate),
    ("POST", "/api/system/restart", RiskClass::Mutate),
    ("POST", "/api/system/update", RiskClass::Irreversible),
    ("GET", "/api/system/update-check", RiskClass::Read),
    ("GET", "/api/system/version", RiskClass::Read),
    ("GET", "/api/tabs", RiskClass::Read),
    ("POST", "/api/tabs", RiskClass::Mutate),
    ("DELETE", "/api/tabs/:id", RiskClass::Destructive),
    ("PUT", "/api/tabs/:id", RiskClass::Mutate),
    ("GET", "/api/tabs/export", RiskClass::Read),
    ("POST", "/api/tabs/import", RiskClass::Mutate),
    ("PUT", "/api/tabs/order", RiskClass::Mutate),
    ("GET", "/api/tags", RiskClass::Read),
    ("POST", "/api/tags", RiskClass::Mutate),
    ("DELETE", "/api/tags/:id", RiskClass::Destructive),
    ("PATCH", "/api/tags/:id", RiskClass::Mutate),
    ("POST", "/api/tags/assign", RiskClass::Mutate),
    ("GET", "/api/tags/for", RiskClass::Read),
    ("GET", "/api/tags/map", RiskClass::Read),
    ("POST", "/api/tags/unassign", RiskClass::Mutate),
    ("GET", "/api/terminal/local/sessions", RiskClass::Read),
    ("POST", "/api/terminal/local/sessions", RiskClass::Mutate),
    (
        "DELETE",
        "/api/terminal/local/sessions/:id",
        RiskClass::Destructive,
    ),
    ("PUT", "/api/terminal/local/sessions/:id", RiskClass::Mutate),
    ("GET", "/api/terminal/ssh/sessions", RiskClass::Read),
    ("POST", "/api/terminal/ssh/sessions", RiskClass::Mutate),
    (
        "DELETE",
        "/api/terminal/ssh/sessions/:id",
        RiskClass::Destructive,
    ),
    ("PUT", "/api/terminal/ssh/sessions/:id", RiskClass::Mutate),
    ("GET", "/api/terminal/ssh/ws", RiskClass::Read),
    ("GET", "/api/terminal/ws", RiskClass::Read),
    ("GET", "/api/timeline", RiskClass::Read),
    ("GET", "/api/updates/docker", RiskClass::Read),
    (
        "POST",
        "/api/updates/docker/:id/apply",
        RiskClass::Irreversible,
    ),
    ("POST", "/api/updates/docker/check", RiskClass::Mutate),
    ("GET", "/api/updates/odysseus", RiskClass::Read),
    (
        "POST",
        "/api/updates/odysseus/apply",
        RiskClass::Irreversible,
    ),
    ("GET", "/api/updates/os", RiskClass::Read),
    ("POST", "/api/updates/os/apply", RiskClass::Irreversible),
    ("GET", "/api/updates/voidtower", RiskClass::Read),
    (
        "POST",
        "/api/updates/voidtower/apply",
        RiskClass::Irreversible,
    ),
    ("POST", "/api/updates/voidtower/check", RiskClass::Mutate),
    (
        "POST",
        "/api/updates/voidtower/rollback",
        RiskClass::Irreversible,
    ),
    ("GET", "/api/users", RiskClass::Read),
    ("POST", "/api/users", RiskClass::Mutate),
    ("DELETE", "/api/users/:id", RiskClass::Destructive),
    ("POST", "/api/users/me/password", RiskClass::Mutate),
    ("GET", "/api/vms/local", RiskClass::Read),
    ("POST", "/api/vms/local/action", RiskClass::Destructive),
    ("POST", "/api/vms/proxmox/action", RiskClass::Mutate),
    ("GET", "/api/vms/proxmox/config", RiskClass::Read),
    ("POST", "/api/vms/proxmox/config", RiskClass::Mutate),
    ("POST", "/api/vms/proxmox/test", RiskClass::Mutate),
    ("GET", "/api/vms/proxmox/vms", RiskClass::Read),
    ("GET", "/api/webhooks", RiskClass::Read),
    ("POST", "/api/webhooks", RiskClass::Mutate),
    ("DELETE", "/api/webhooks/:id", RiskClass::Destructive),
    ("PATCH", "/api/webhooks/:id", RiskClass::Mutate),
    ("POST", "/api/webhooks/:id/test", RiskClass::Mutate),
    ("GET", "/api/wireguard", RiskClass::Read),
    ("POST", "/api/wireguard/peers", RiskClass::Mutate),
    ("DELETE", "/api/wireguard/peers/:id", RiskClass::Destructive),
    ("GET", "/plugin-assets/:id/*path", RiskClass::Read),
    ("GET", "/status", RiskClass::Read),
    ("POST", "/v1/chat/completions", RiskClass::Mutate),
    ("GET", "/v1/models", RiskClass::Read),
];

/// The known AI/automation-ingress action-name vocabulary (see module doc comment).
/// `"remove"` and `"voidwatch.mode.set"` are reserved entries not reachable through any
/// ingress point today (no mutating MCP tool is registered, and no webhook structured
/// action supports `remove` — verified against `api/mcp.rs`'s `READ_ONLY_TOOLS`/match
/// arms and `api/integrations.rs`'s `action_str` match respectively) — present so the
/// classification exists the day one is wired, mirroring the `ActorKind::Ai` reserved-
/// variant precedent from P0-02.
pub fn for_action(action: &str) -> RiskClass {
    match action {
        "start" | "stop" | "restart" | "automation.run" => RiskClass::Mutate,
        "remove" => RiskClass::Destructive,
        // Voidwatch policy/mode edits are item 5 of ADR-004's reconciled irreversibility
        // denylist — always requires approval, including in YOLO mode.
        "voidwatch.mode.set" => RiskClass::Irreversible,
        // Fail safe, not fail open: an action name this table has never seen is treated
        // as maximally risky rather than silently allowed through.
        _ => RiskClass::Irreversible,
    }
}

/// Resource types whose mutations Trusted mode must snapshot before applying (EDD §3.2:
/// "Snapshot-before-apply mandatory where the target supports it (Proxmox snapshot,
/// Btrfs snapshot, compose config backup)"). Keyed on `Resource::resource_type` as seen
/// by `voidwatch::evaluate()`.
pub const SNAPSHOT_CAPABLE_RESOURCE_TYPES: &[&str] = &["vm", "container", "app"];

pub fn requires_snapshot_before_apply(resource_type: &str) -> bool {
    SNAPSHOT_CAPABLE_RESOURCE_TYPES.contains(&resource_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Mirrors the `.route("path", verb(...).verb(...))` shape used throughout
    /// `api::router()`: manual balanced-paren scan (no regex dependency — this crate
    /// doesn't otherwise depend on `regex`, and CLAUDE.md asks that new dependencies not
    /// be introduced without an ADR) that pulls every `(method, path)` pair actually
    /// registered.
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
    fn route_risk_classes_has_no_duplicate_entries() {
        let mut seen = HashSet::new();
        for (method, path, _) in ROUTE_RISK_CLASSES {
            assert!(
                seen.insert((*method, *path)),
                "duplicate ROUTE_RISK_CLASSES entry: {method} {path}"
            );
        }
    }

    /// Clippy-style exhaustiveness (gap-analysis P0.3 / ADR-001 constraint 3): every
    /// route registered in `api::router()` must have a `ROUTE_RISK_CLASSES` entry, and
    /// vice versa (catches routes removed from the router whose table entry became
    /// stale). Parses `api/mod.rs` directly rather than trusting this table, so a
    /// forgotten classification on a new route fails the build instead of silently
    /// shipping unclassified.
    #[test]
    fn every_registered_route_has_a_risk_class() {
        let src = include_str!("../api/mod.rs");
        let registered = extract_registered_routes(src);
        assert!(
            registered.len() > 200,
            "sanity check: route extraction found suspiciously few routes ({}) — \
             the parser may have broken against a source formatting change",
            registered.len()
        );

        let table: HashSet<(&str, &str)> = ROUTE_RISK_CLASSES
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
            "routes registered in api/mod.rs with no ROUTE_RISK_CLASSES entry \
             (new endpoint shipped unclassified): {missing:?}"
        );

        let registered_set: HashSet<(&str, &str)> = registered
            .iter()
            .map(|(m, p)| (m.as_str(), p.as_str()))
            .collect();
        let stale: Vec<String> = ROUTE_RISK_CLASSES
            .iter()
            .filter(|(m, p, _)| !registered_set.contains(&(*m, *p)))
            .map(|(m, p, _)| format!("{m} {p}"))
            .collect();
        assert!(
            stale.is_empty(),
            "ROUTE_RISK_CLASSES entries with no corresponding route in api/mod.rs \
             (route was removed or renamed — update the table): {stale:?}"
        );
    }

    /// The AI-ingress action vocabulary (see `for_action`'s doc comment) must stay in
    /// sync with what `api/mcp.rs` and `api/integrations.rs` actually pass to
    /// `voidwatch::evaluate()`. This doesn't (and can't, for an open `&str` match) get
    /// Rust's own exhaustiveness checking, so it's enforced the same source-grep way as
    /// `every_registered_route_has_a_risk_class` above: every action-name literal found
    /// at a known evaluate()-feeding call site must classify to something other than the
    /// fail-safe default, i.e. must have a real, deliberate entry in `for_action`.
    #[test]
    fn known_ai_ingress_actions_are_explicitly_classified() {
        // From api/integrations.rs's structured webhook action match + automation.run.
        const KNOWN_SOURCE_ACTIONS: &[&str] = &["restart", "start", "stop", "automation.run"];
        for action in KNOWN_SOURCE_ACTIONS {
            assert_ne!(
                for_action(action),
                RiskClass::Irreversible,
                "action {action:?} is reachable from AI ingress but for_action() falls \
                 through to the fail-safe default — add an explicit match arm"
            );
        }
    }

    #[test]
    fn for_action_fails_safe_for_unknown_actions() {
        assert_eq!(
            for_action("some_future_unclassified_action"),
            RiskClass::Irreversible
        );
    }
}
