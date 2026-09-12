//! Startup validation for the durable operation registry.
//!
//! The security registry remains the single action authority. This module owns the adapter-key
//! inventory and proves that adopted compatibility routes, canonical actions, schemas, and future
//! worker dispatch keys converge before the HTTP server starts. Concrete adapter implementations
//! attach to these keys during the domain-adoption slices; the generic invocation route remains
//! unmounted until that runtime registry has a real adapter.

use crate::api::mcp::action_registry::{
    self, ActionExecution, ActionIngress, ActionKind, AiExposure, ApprovalPolicy, BearerPolicy,
    HttpMethod, RecoveryClass, RetryClass, RiskClass, RoleTier, SessionPolicy, ACTIONS, ROUTES,
};
use anyhow::{bail, ensure, Result};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdapterMetadata {
    pub key: &'static str,
}

pub const ADAPTERS: &[AdapterMetadata] = &[
    AdapterMetadata { key: "automation" },
    AdapterMetadata { key: "backups" },
    AdapterMetadata { key: "containers" },
    AdapterMetadata { key: "firewall" },
    AdapterMetadata { key: "proxmox" },
    AdapterMetadata { key: "proxy" },
    AdapterMetadata { key: "updates" },
];

/// Compatibility routes included in the approved six-domain adoption. POST routes that only plan
/// and the ephemeral Proxmox VNC-ticket route are intentionally absent.
const ADOPTED_ROUTES: &[(HttpMethod, &str)] = &[
    (HttpMethod::Post, "/api/apps/:project_name/expose"),
    (HttpMethod::Post, "/api/automation/:id/run"),
    (HttpMethod::Post, "/api/backups"),
    (HttpMethod::Delete, "/api/backups/:id"),
    (HttpMethod::Post, "/api/backups/:id/check"),
    (HttpMethod::Post, "/api/backups/:id/restore-test"),
    (HttpMethod::Post, "/api/backups/:id/run"),
    (HttpMethod::Post, "/api/containers/:id/action"),
    (HttpMethod::Post, "/api/containers/:id/compose/apply"),
    (HttpMethod::Post, "/api/firewall/action"),
    (HttpMethod::Post, "/api/firewall/rules"),
    (HttpMethod::Post, "/api/firewall/rules/delete"),
    (HttpMethod::Post, "/api/integrations/webhooks"),
    (HttpMethod::Post, "/api/proxy"),
    (HttpMethod::Delete, "/api/proxy/:id"),
    (HttpMethod::Put, "/api/proxy/:id"),
    (HttpMethod::Post, "/api/proxy/:id/toggle"),
    (HttpMethod::Post, "/api/proxy/ai-auto"),
    (HttpMethod::Post, "/api/proxy/nginx/action"),
    (HttpMethod::Post, "/api/updates/docker/:id/apply"),
    (HttpMethod::Post, "/api/updates/docker/check"),
    (HttpMethod::Post, "/api/updates/odysseus/apply"),
    (HttpMethod::Post, "/api/updates/os/apply"),
    (HttpMethod::Post, "/api/updates/voidtower/apply"),
    (HttpMethod::Post, "/api/updates/voidtower/check"),
    (HttpMethod::Post, "/api/updates/voidtower/rollback"),
    (HttpMethod::Post, "/api/system/update"),
    (HttpMethod::Get, "/api/system/update-check"),
    (HttpMethod::Post, "/api/proxmox/:host_id/lxc/deploy"),
    (
        HttpMethod::Post,
        "/api/proxmox/:host_id/nodes/:node/disks/init",
    ),
    (
        HttpMethod::Post,
        "/api/proxmox/:host_id/nodes/:node/disks/wipe",
    ),
    (
        HttpMethod::Delete,
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
    ),
    (
        HttpMethod::Post,
        "/api/proxmox/:host_id/nodes/:node/storage/:storage/content",
    ),
    (
        HttpMethod::Post,
        "/api/proxmox/:host_id/vms/:vmid/disk-passthrough",
    ),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/reboot"),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/reset"),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/resume"),
    (
        HttpMethod::Post,
        "/api/proxmox/:host_id/vms/:vmid/rollback/:snapname",
    ),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/shutdown"),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/snapshot"),
    (
        HttpMethod::Delete,
        "/api/proxmox/:host_id/vms/:vmid/snapshot/:snapname",
    ),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/start"),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/stop"),
    (HttpMethod::Post, "/api/proxmox/:host_id/vms/:vmid/suspend"),
    (HttpMethod::Post, "/api/proxmox/hosts"),
    (HttpMethod::Delete, "/api/proxmox/hosts/:host_id"),
    (HttpMethod::Post, "/api/vms/proxmox/action"),
    (HttpMethod::Post, "/api/vms/proxmox/config"),
    (HttpMethod::Post, "/api/vms/proxmox/test"),
];

pub fn validate() -> Result<()> {
    validate_adapter_inventory()?;
    validate_durable_actions()?;
    validate_route_mappings()?;
    Ok(())
}

fn validate_adapter_inventory() -> Result<()> {
    let mut keys = HashSet::new();
    for adapter in ADAPTERS {
        ensure!(!adapter.key.is_empty(), "operation adapter key is empty");
        ensure!(
            keys.insert(adapter.key),
            "duplicate operation adapter key: {}",
            adapter.key
        );
        ensure!(
            ACTIONS.iter().any(|action| {
                action.execution == ActionExecution::DurableJob
                    && action.adapter_key == Some(adapter.key)
            }),
            "operation adapter {} has no registered durable action",
            adapter.key
        );
    }
    Ok(())
}

fn validate_durable_actions() -> Result<()> {
    let adapter_keys: HashSet<&str> = ADAPTERS.iter().map(|adapter| adapter.key).collect();
    let mut input_schemas = HashSet::new();
    let mut result_schemas = HashSet::new();

    for action in ACTIONS
        .iter()
        .filter(|action| action.execution == ActionExecution::Direct)
    {
        ensure!(
            action.resource_kind.is_none()
                && action.adapter_key.is_none()
                && action.input_schema_id.is_none()
                && action.result_schema_id.is_none()
                && action.concurrency.is_none()
                && action.retry.is_none()
                && action.recovery.is_none()
                && action.canonical_session_role.is_none()
                && action.canonical_bearer == BearerPolicy::Denied,
            "direct action {} has partial durable metadata",
            action.name
        );
    }

    for action in ACTIONS
        .iter()
        .filter(|action| action.execution == ActionExecution::DurableJob)
    {
        let resource_kind = required(action.resource_kind, action.name, "resource kind")?;
        let adapter_key = required(action.adapter_key, action.name, "adapter key")?;
        let input_schema = required(action.input_schema_id, action.name, "input schema")?;
        let result_schema = required(action.result_schema_id, action.name, "result schema")?;
        ensure!(
            !resource_kind.contains(char::is_whitespace),
            "action {} has invalid resource kind",
            action.name
        );
        ensure!(
            adapter_keys.contains(adapter_key),
            "action {} references unknown adapter {}",
            action.name,
            adapter_key
        );
        ensure!(
            input_schema.ends_with(".input.v1"),
            "action {} input schema is not versioned: {}",
            action.name,
            input_schema
        );
        ensure!(
            result_schema.ends_with(".result.v1"),
            "action {} result schema is not versioned: {}",
            action.name,
            result_schema
        );
        ensure!(
            input_schemas.insert(input_schema),
            "duplicate durable input schema: {}",
            input_schema
        );
        ensure!(
            result_schemas.insert(result_schema),
            "duplicate durable result schema: {}",
            result_schema
        );
        ensure!(
            action.concurrency.is_some(),
            "action {} has no concurrency policy",
            action.name
        );
        let retry = action
            .retry
            .ok_or_else(|| anyhow::anyhow!("action {} has no retry metadata", action.name))?;
        ensure!(
            retry.max_attempts > 0,
            "action {} permits zero attempts",
            action.name
        );
        match retry.class {
            RetryClass::Never => ensure!(
                retry.max_attempts == 1,
                "never-retry action {} must permit exactly one attempt",
                action.name
            ),
            RetryClass::Transient => ensure!(
                retry.max_attempts > 1,
                "transient-retry action {} must permit multiple attempts",
                action.name
            ),
        }
        ensure!(
            action.recovery == Some(RecoveryClass::Reconcile),
            "action {} has no supported recovery class",
            action.name
        );
        // Exercise the persisted spellings here so they cannot silently drift from the job schema.
        ensure!(
            !retry.class.as_str().is_empty(),
            "action {} has an empty retry class",
            action.name
        );
        ensure!(
            !action.recovery.expect("checked above").as_str().is_empty(),
            "action {} has an empty recovery class",
            action.name
        );
        ensure!(
            action.ingresses.contains(&ActionIngress::Http),
            "durable action {} is not reachable from HTTP",
            action.name
        );
        required_role(action.canonical_session_role, action.name)?;
        match action.canonical_bearer {
            BearerPolicy::Scope(scope) => {
                ensure!(
                    !scope.is_empty(),
                    "durable action {} has an empty bearer scope",
                    action.name
                );
                ensure!(
                    action.ai_exposure == AiExposure::Callable,
                    "bearer-scoped durable action {} is not AI-callable",
                    action.name
                );
            }
            BearerPolicy::Denied => ensure!(
                action.ai_exposure == AiExposure::None,
                "bearer-denied durable action {} is AI-exposed",
                action.name
            ),
            BearerPolicy::Public | BearerPolicy::Unscoped | BearerPolicy::ActionScoped => bail!(
                "durable action {} has unsafe bearer policy {:?}",
                action.name,
                action.canonical_bearer
            ),
        }

        match action.kind {
            ActionKind::Read => {
                ensure!(
                    action.risk == RiskClass::Read
                        && action.approval == ApprovalPolicy::NotApplicable,
                    "read-result job {} has inconsistent risk/approval metadata",
                    action.name
                );
            }
            ActionKind::Mutating => {
                ensure!(
                    action.risk != RiskClass::Read,
                    "mutating job {} is classified as read",
                    action.name
                );
                ensure!(
                    (action.risk == RiskClass::Irreversible)
                        == (action.approval == ApprovalPolicy::Always),
                    "mutating job {} has inconsistent irreversible approval metadata",
                    action.name
                );
                if matches!(
                    action.risk,
                    RiskClass::Destructive | RiskClass::Irreversible
                ) {
                    ensure!(
                        retry.class == RetryClass::Never,
                        "high-risk action {} cannot retry automatically",
                        action.name
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_route_mappings() -> Result<()> {
    let expected: HashSet<(HttpMethod, &str)> = ADOPTED_ROUTES.iter().copied().collect();
    ensure!(
        expected.len() == ADOPTED_ROUTES.len(),
        "duplicate adopted route key"
    );

    let mapped: HashSet<(HttpMethod, &str)> = ROUTES
        .iter()
        .filter(|route| !route.canonical_actions.is_empty())
        .map(|route| (route.method, route.path))
        .collect();
    ensure!(
        mapped == expected,
        "adopted and mapped route inventories diverge"
    );

    for route in ROUTES
        .iter()
        .filter(|route| !route.canonical_actions.is_empty())
    {
        let mut names = HashSet::new();
        for action_name in route.canonical_actions {
            ensure!(
                names.insert(*action_name),
                "route {} {} repeats action {}",
                route.method.as_str(),
                route.path,
                action_name
            );
            let action = action_registry::action(action_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "route {} {} maps unknown action {}",
                    route.method.as_str(),
                    route.path,
                    action_name
                )
            })?;
            ensure!(
                action.execution == ActionExecution::DurableJob,
                "route {} {} maps non-durable action {}",
                route.method.as_str(),
                route.path,
                action_name
            );
            ensure!(
                risk_rank(route.risk) >= risk_rank(action.risk),
                "route {} {} understates action {} risk",
                route.method.as_str(),
                route.path,
                action_name
            );
            if action.approval == ApprovalPolicy::Always {
                ensure!(
                    route.approval == ApprovalPolicy::Always,
                    "route {} {} weakens always-approval action {}",
                    route.method.as_str(),
                    route.path,
                    action_name
                );
            }
            match route.session {
                SessionPolicy::Required(route_role) => {
                    let action_role = required_role(action.canonical_session_role, action.name)?;
                    ensure!(
                        role_rank(route_role) >= role_rank(action_role),
                        "route {} {} weakens action {} session role",
                        route.method.as_str(),
                        route.path,
                        action.name
                    );
                }
                SessionPolicy::HandlerManaged => {
                    ensure!(
                        route.credential == crate::api::mcp::action_registry::CredentialPolicy::WebhookHmac
                            && action.ingresses.contains(&ActionIngress::Webhook),
                        "mapped handler-managed route {} {} is not bound to webhook action {}",
                        route.method.as_str(),
                        route.path,
                        action.name
                    );
                }
                other => bail!(
                    "mapped route {} {} has unsupported session policy {:?}",
                    route.method.as_str(),
                    route.path,
                    other
                ),
            }
            ensure!(
                bearer_at_least_as_restrictive(route.bearer, action.canonical_bearer),
                "route {} {} weakens action {} bearer policy",
                route.method.as_str(),
                route.path,
                action.name
            );
        }
    }
    Ok(())
}

fn required<'a>(value: Option<&'a str>, action: &str, field: &str) -> Result<&'a str> {
    match value {
        Some(value) if !value.is_empty() => Ok(value),
        _ => bail!("action {} has no {}", action, field),
    }
}

fn required_role(value: Option<RoleTier>, action: &str) -> Result<RoleTier> {
    value.ok_or_else(|| anyhow::anyhow!("action {action} has no canonical session role"))
}

const fn role_rank(role: RoleTier) -> u8 {
    match role {
        RoleTier::Session => 0,
        RoleTier::Operator => 1,
        RoleTier::Admin => 2,
        RoleTier::Owner => 3,
    }
}

fn bearer_at_least_as_restrictive(route: BearerPolicy, action: BearerPolicy) -> bool {
    match (route, action) {
        (BearerPolicy::Denied, _) => true,
        (BearerPolicy::Scope(route), BearerPolicy::Scope(action)) => route == action,
        _ => false,
    }
}

const fn risk_rank(risk: RiskClass) -> u8 {
    match risk {
        RiskClass::Read => 0,
        RiskClass::Mutate => 1,
        RiskClass::Destructive => 2,
        RiskClass::Irreversible => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occurrences(source: &str, needle: &str) -> usize {
        source.match_indices(needle).count()
    }

    #[test]
    fn operation_registry_is_complete_and_consistent() {
        validate().expect("operation registry should be valid");
        assert_eq!(ADOPTED_ROUTES.len(), 49, "the adopted route inventory drifted");
    }

    #[test]
    fn every_durable_action_has_explicit_canonical_access() {
        let durable: Vec<_> = ACTIONS
            .iter()
            .filter(|action| action.execution == ActionExecution::DurableJob)
            .collect();
        assert_eq!(durable.len(), 52, "the J0 durable action inventory drifted");

        for action in durable {
            assert!(
                action.canonical_session_role.is_some(),
                "durable action {} has no canonical session role",
                action.name
            );
            match action.canonical_bearer {
                crate::api::mcp::action_registry::BearerPolicy::Scope(_) => assert_eq!(
                    action.ai_exposure,
                    crate::api::mcp::action_registry::AiExposure::Callable,
                    "bearer-scoped durable action {} must be AI-callable",
                    action.name
                ),
                crate::api::mcp::action_registry::BearerPolicy::Denied => assert_eq!(
                    action.ai_exposure,
                    crate::api::mcp::action_registry::AiExposure::None,
                    "bearer-denied durable action {} must not be AI-callable",
                    action.name
                ),
                other => panic!(
                    "durable action {} has unsafe canonical bearer policy {other:?}",
                    action.name
                ),
            }
        }
    }

    #[test]
    fn plan_only_and_ephemeral_routes_are_not_job_mapped() {
        for (method, path) in [
            ("POST", "/api/backups/:id/delete-plan"),
            ("POST", "/api/containers/:id/compose/propose"),
            ("POST", "/api/proxmox/:host_id/vms/:vmid/vncproxy"),
        ] {
            let route = action_registry::route(method, path).expect("route metadata");
            assert!(route.canonical_actions.is_empty(), "{method} {path}");
        }
    }

    #[test]
    fn slow_checks_are_durable_reads_without_approval() {
        for name in [
            "backup.check",
            "backup.restore_test",
            "update.docker.check",
            "update.voidtower.check",
        ] {
            let action = action_registry::action(name).expect("action metadata");
            assert_eq!(action.execution, ActionExecution::DurableJob);
            assert_eq!(action.kind, ActionKind::Read);
            assert_eq!(action.risk, RiskClass::Read);
            assert_eq!(action.approval, ApprovalPolicy::NotApplicable);
        }
    }

    #[test]
    fn backup_actions_have_the_exact_non_http_ingress_matrix() {
        for name in [
            "backup.config.create",
            "backup.config.delete",
            "backup.run",
            "backup.check",
            "backup.restore_test",
        ] {
            let action = action_registry::action(name).unwrap();
            assert!(action.ingresses.contains(&ActionIngress::Http), "{name}");
            assert!(
                action.ingresses.contains(&ActionIngress::LocalCli),
                "{name}"
            );
            assert_eq!(
                action.ingresses.contains(&ActionIngress::Scheduler),
                name == "backup.restore_test",
                "{name}"
            );
        }
        for action in ACTIONS.iter().filter(|action| {
            !action.name.starts_with("backup.") && action.execution == ActionExecution::DurableJob
        }) {
            assert!(!action.ingresses.contains(&ActionIngress::LocalCli));
            assert!(
                !action.ingresses.contains(&ActionIngress::Scheduler)
                    || action.name == "automation.run"
            );
        }
    }

    #[test]
    fn container_webhook_actions_use_the_durable_submission_path() {
        for name in ["container.start", "container.stop", "container.restart"] {
            let action = action_registry::action(name).expect("container action metadata");
            assert_eq!(action.execution, ActionExecution::DurableJob, "{name}");
            let expected_ingresses = if name == "container.start" {
                &[
                    ActionIngress::Http,
                    ActionIngress::Webhook,
                    ActionIngress::Mcp,
                    ActionIngress::Studio,
                ][..]
            } else {
                &[ActionIngress::Http, ActionIngress::Webhook][..]
            };
            assert_eq!(action.ingresses, expected_ingresses, "{name}");
        }

        let integrations = include_str!("../api/integrations.rs");
        assert!(integrations.contains("operation_adoption::submit("));
        assert!(!integrations.contains("containers::container_action("));
        assert!(!integrations.contains("container_service::container_action("));

        let adapter = include_str!("adapters/containers.rs");
        assert_eq!(
            occurrences(adapter, "container_service::container_action("),
            1,
            "the container adapter must remain the only native execution boundary"
        );
    }

    #[test]
    fn app_exposure_is_durable_and_open_ui_is_read_only() {
        let apps = include_str!("../api/apps.rs");
        let expose = apps
            .split_once("pub async fn expose_app(")
            .expect("expose handler")
            .1
            .split_once("pub async fn delete_app_volumes(")
            .expect("handler boundary")
            .0;
        assert!(expose.contains("create_with_credential("));
        assert!(!expose.contains("audit::log("));
        assert!(!expose.contains("create_proxy_record"));

        let open_ui = apps
            .split_once("pub async fn open_ui(")
            .expect("open-ui handler")
            .1
            .split_once("pub async fn update_compose(")
            .expect("handler boundary")
            .0;
        for forbidden in [
            ".execute(&state.db)",
            "write_nginx_port_conf",
            "reload_nginx_pub",
            "open_firewall_port",
            "create_proxy_record",
        ] {
            assert!(!open_ui.contains(forbidden), "open-ui contains {forbidden}");
        }
        assert!(!apps.contains("create_proxy_record"));
        assert!(!apps.contains("write_nginx_port_conf"));
    }

    #[test]
    fn deferred_direct_execution_inventory_is_exact() {
        let apps = include_str!("../api/apps.rs");
        for (needle, expected) in [
            ("containers::deploy_compose(", 4),
            ("containers::deploy_compose_cancellable(", 1),
            ("containers::restart_compose(", 1),
            ("containers::remove_compose(", 2),
            ("containers::stop_compose(", 1),
            ("containers::pull_compose(", 0),
        ] {
            assert_eq!(occurrences(apps, needle), expected, "App Vault bypass drift: {needle}");
        }

        let models = include_str!("../api/models.rs");
        assert_eq!(occurrences(models, "crate::containers::deploy_compose("), 3);

        let settings = include_str!("../api/settings.rs");
        let production = settings
            .split("#[cfg(test)]")
            .next()
            .expect("settings tests must follow production code");
        for needle in [
            "reload_nginx_pub(",
            "open_firewall_port(",
            "close_firewall_port(",
            "write_ai_proxy_conf(",
            "patch_nginx_compose_port(",
        ] {
            assert!(!production.contains(needle), "settings bypass drift: {needle}");
        }
    }

    #[test]
    fn shipped_domain_clients_follow_accepted_jobs() {
        let clients = [
            ("Containers", include_str!("../../../frontend/src/pages/Containers.tsx")),
            ("Container detail", include_str!("../../../frontend/src/pages/ContainerDetail.tsx")),
            ("Firewall", include_str!("../../../frontend/src/pages/Firewall.tsx")),
            ("Proxies", include_str!("../../../frontend/src/pages/Proxies.tsx")),
            ("Backups", include_str!("../../../frontend/src/pages/Backups.tsx")),
            ("Dashboard", include_str!("../../../frontend/src/pages/Dashboard.tsx")),
            ("Updates", include_str!("../../../frontend/src/pages/Updates.tsx")),
            ("Settings", include_str!("../../../frontend/src/pages/Settings.tsx")),
            ("Proxmox", include_str!("../../../frontend/src/pages/ProxmoxPage.tsx")),
            ("VMs", include_str!("../../../frontend/src/pages/VMs.tsx")),
            ("native containers", include_str!("../../../frontend/src/aios/panels/containers.tsx")),
            ("native firewall", include_str!("../../../frontend/src/aios/panels/firewall.tsx")),
            ("native proxies", include_str!("../../../frontend/src/aios/panels/proxies.tsx")),
            ("native backups", include_str!("../../../frontend/src/aios/panels/backups.tsx")),
            ("native Proxmox", include_str!("../../../frontend/src/aios/panels/proxmox.tsx")),
            ("native VMs", include_str!("../../../frontend/src/aios/panels/vms.tsx")),
        ];
        for (name, source) in clients {
            assert!(source.contains("useDurableJobTracker"), "{name} does not track jobs");
            assert!(source.contains("DurableJobNotice"), "{name} does not expose job state");
        }
    }

    #[test]
    fn shared_jobs_and_approvals_surfaces_are_source_enforced() {
        let app = include_str!("../../../frontend/src/App.tsx");
        for route in [
            "path=\"jobs\"",
            "path=\"jobs/:id\"",
            "path=\"approvals\"",
            "path=\"approvals/:id\"",
        ] {
            assert!(app.contains(route), "missing shared operation route {route}");
        }
        assert!(app.contains("allowed={OPERATOR_ROLES}"));
        assert!(app.contains("allowed={ADMIN_ROLES}"));

        let sidebar = include_str!("../../../frontend/src/components/layout/Sidebar.tsx");
        assert!(sidebar.contains("to: '/jobs'"));
        assert!(sidebar.contains("roles: OPERATOR_ROLES"));
        assert!(sidebar.contains("to: '/approvals'"));
        assert!(sidebar.contains("roles: ADMIN_ROLES"));
        assert!(sidebar.contains("roleAllowed(user?.role, item.roles)"));

        let dock = include_str!("../../../frontend/src/aios/AiosDock.tsx");
        assert!(dock.contains("key: 'jobs'"));
        assert!(dock.contains("roles: OPERATOR_ROLES"));
        assert!(dock.contains("key: 'approvals'"));
        assert!(dock.contains("roles: ADMIN_ROLES"));
        assert!(dock.contains("roleAllowed(role, d.roles)"));

        let layout = include_str!("../../../frontend/src/aios/AiosLayout.tsx");
        assert!(layout.contains("jobs:         NativeJobsPanel"));
        assert!(layout.contains("approvals:    NativeApprovalsPanel"));
        assert!(layout.contains("/^\\/(jobs|approvals)(?:\\/|$)/"));

        let notice = include_str!("../../../frontend/src/components/ui/DurableJobNotice.tsx");
        assert!(notice.contains("<Link"));
        assert!(notice.contains("`/jobs/${encodeURIComponent(job.id)}`"));
    }

    #[test]
    fn durable_event_delivery_and_shared_invalidations_are_source_enforced() {
        let routes = include_str!("../api/mod.rs");
        assert!(routes.contains(
            ".route(\"/api/events/stream\", get(events::stream_handler))"
        ));
        assert!(routes.contains(
            ".route(\"/api/integrations/events\",                  get(events::stream_handler))"
        ));
        assert!(routes.contains(
            ".route(\"/api/integrations/events/legacy\",           get(integrations::legacy_event_stream))"
        ));

        let backend = include_str!("../api/events.rs");
        for contract in [
            "last-event-id",
            "DELIVERY_BATCH_SIZE: i64 = 100",
            ".event(\"stream.ready\")",
            ".event(\"stream.gap\")",
            ".event(\"durable_event\")",
            ".id(envelope.sequence.to_string())",
        ] {
            assert!(backend.contains(contract), "missing durable stream contract {contract}");
        }

        let client = include_str!("../../../frontend/src/operations/durableEvents.ts");
        assert!(client.contains("new EventSource(api.events.streamUrl(after)"));
        assert!(client.contains("event.sequence !== lastSequence + 1"));

        let hooks = include_str!("../../../frontend/src/hooks/useOperationRecords.ts");
        assert_eq!(occurrences(hooks, "const eventPredicate ="), 4);
        assert!(hooks.contains("event.job_id === id"));
        assert!(hooks.contains("event.approval_id === id"));

        let polling = include_str!("../../../frontend/src/hooks/useBoundedPolling.ts");
        assert!(polling.contains("subscribeDurableEvents"));
        assert!(polling.contains("streamReady = value !== null && transportReady"));
    }
}
