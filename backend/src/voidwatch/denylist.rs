//! The hardcoded irreversibility denylist (EDD §10.3, gap-analysis P0.4), reconciled from
//! the two source documents' non-overlapping item lists by `docs/adr/ADR-004`. This module
//! is the named, auditable artifact ADR-004 asked for: a compile-time constant enumerating
//! every route this instance must always require human approval for, regardless of
//! Voidwatch mode (including YOLO) and regardless of actor class (ADR-004 constraint 2).
//!
//! This is deliberately a *separate* ledger from [`super::risk_class::ROUTE_RISK_CLASSES`],
//! not a duplicate of it: `ROUTE_RISK_CLASSES` classifies the entire API surface (including
//! plain `Mutate`/`Destructive` routes with no relationship to irreversibility), while this
//! module exists to give the twelve ADR-004 items an explicit, named, documented home with
//! their own rationale — and a test (`denylist_routes_are_classified_irreversible`) that
//! ties the two together so they can't silently drift apart. `ROUTE_RISK_CLASSES` is the
//! table `voidwatch::mode_pre_pass`'s YOLO branch actually consults at runtime (via
//! `risk_class::for_action` for the AI/automation ingress vocabulary); this module is the
//! documentation-grade source of truth that table's `Irreversible` entries must satisfy for
//! every item below.
//!
//! ## Compile-time-only, per ADR-004 constraint 1
//!
//! No `CREATE TABLE` backs this list (see `denylist_has_no_api_mutation_path` below) and no
//! route can write to it — it is a `&'static` slice, full stop. Changing it requires a code
//! change, a review, and a merge, the same as any other compiled behavior in this crate.
//!
//! ## Scope note: which handlers this list can gate today
//!
//! ADR-001's grant for this task covers `backend/src/policy.rs` and `backend/src/voidwatch/**`
//! only — not the admin HTTP handlers that actually implement these routes (`api/disaster.rs`,
//! `api/secrets.rs`, `api/storage.rs`, `api/firewall.rs`, `api/system.rs`, `api/updates.rs`,
//! `api/proxmox.rs`, `api/policy.rs`, `api/apps.rs`). Wiring those handlers to consult this
//! list directly is out of this task's grant. What *is* already wired and enforced today is
//! the AI/automation ingress path: `voidwatch::evaluate()`'s YOLO branch requires approval
//! for any action `risk_class::for_action` classifies `Irreversible` — which includes its
//! fail-safe default for any action name it has never seen (`risk_class.rs`'s
//! `for_action_fails_safe_for_unknown_actions`), so an AI ingress action reaching any of
//! these routes' semantics is already blocked in YOLO mode even before this module existed.
//! This module's job is to make the *list* explicit, named, and reconciled per ADR-004 — not
//! to invent new enforcement plumbing into handler files this task has no grant to touch.
//!
//! ## Items with no corresponding endpoint yet (verified against source, not assumed)
//!
//! ADR-004 flagged items 4, 9, 10, and 12 as unmapped in its own pass and asked the
//! implementer to verify each before writing its acceptance test:
//!
//! - **Item 4, secrets master-key operations**: no master-key rotation/access endpoint
//!   exists anywhere in `backend/src/api/secrets.rs` or `backend/src/` (`rg -n
//!   "master.key|master_key|MASTER_KEY"` across the crate returns nothing). N/A — nothing to
//!   gate yet.
//! - **Item 9, host power actions on the control-plane's own host**: `api/system.rs` has
//!   `restart` (soft process restart via `kill -TERM` + re-exec, not a host power action) and
//!   `update` (already covered by item 1); there is no `poweroff`/`reboot`/`shutdown -h`
//!   endpoint for the physical/VM host VoidTower itself runs on. N/A — nothing to gate yet.
//! - **Item 10, deletion of the last remaining snapshot/backup of a resource**: unlike 4/9/12,
//!   this one has *two* plausible, non-equivalent code paths, and picking the wrong one would
//!   be worse than picking neither. `api/backups.rs`'s `DELETE /api/backups/:id` (`delete`)
//!   only removes a `backup_configs` schedule row — verified in source, its sibling
//!   `delete_plan` handler even says so explicitly ("existing backup data on disk is NOT
//!   deleted"), and `delete` does the same `DELETE FROM backup_configs` with no data-path
//!   difference. Actual backup/snapshot *data* only gets destroyed by
//!   `api/proxmox.rs`'s `vm_delete_snapshot` (`DELETE
//!   /api/proxmox/:host_id/vms/:vmid/snapshot/:snapname`), a genuinely different resource
//!   model (Proxmox VM snapshots, countable via `list_snapshots`) than restic backup
//!   *configs* (which don't track underlying data at this layer at all). ADR-004's own text
//!   ("count-dependent condition... add a check in the backup-deletion path
//!   (`backend/src/api/backups.rs`)") assumed the former without verifying it deletes data —
//!   it doesn't. This is a genuine design question (which resource type "backup" refers to
//!   here, and whether it's one item or two), not a missing grant — `api/backups.rs` and
//!   `api/proxmox.rs` are both outside CLAUDE.md's actual forbidden-zone list (confirmed
//!   against `.devteam/escalations/P0-03-mode-ladder-risk-classes.md`'s own correction of the
//!   same mistaken assumption). Not included in [`IRREVERSIBILITY_DENYLIST`] below; escalated
//!   narrowly rather than guessed — see
//!   `.devteam/escalations/P0-04-irreversibility-denylist.md`.
//! - **Item 12, device decommission**: this repo has no multi-node agent yet (that's P3
//!   scope per the task spec) — `api/node_enroll.rs`'s `delete_node` deletes an *enrolled*
//!   node record (and its WireGuard peer), which is ordinary resource deletion, not
//!   "decommission" in the EDD's sense of retiring a live agent-managed device. N/A per the
//!   task spec's own explicit guidance for this item ("note it as N/A ... rather than
//!   inventing one").
//!
//! ## Item 11, `keep_data=false` app removal — judgment call, documented per CLAUDE.md
//!
//! No literal `keep_data` parameter exists anywhere in this crate (`rg -n
//! "keep_data|keepData"` returns nothing) or the frontend. The closest source-verified
//! analogs are `api/apps.rs`'s `purge_app` (`POST /api/apps/:project_name/purge` — deletes
//! the compose project directory from disk *and* the `deployed_apps` row; there is no
//! "keep data" alternative, i.e. it always operates in the `keep_data=false` sense) and
//! `delete_app_volumes` (`POST /api/apps/:project_name/delete-volumes` — explicitly destroys
//! volumes/data while leaving the app entry registered). Both routes were previously
//! classified `RiskClass::Destructive` in `ROUTE_RISK_CLASSES`; this task reclassifies both
//! to `Irreversible` to reflect that they are the concrete, unconditional data-destroying
//! app-removal paths ADR-004's item 11 describes. Noted here rather than filed as a fresh
//! ADR: this is applying ADR-004's own explicit instruction to "locate the exact route" for
//! this item, not a new architectural decision.

/// One reconciled ADR-004 denylist item and the concrete route(s) it maps to. `id` mirrors
/// the acceptance-test naming convention in the task spec
/// (`yolo_mode_still_requires_approval_for_<id>`). `description` is read only by this
/// module's own tests/doc tooling today (`#[allow(dead_code)]`: this crate has no lib
/// target, so a field consumed only by `cfg(test)` code still trips the bin-target dead-code
/// lint, same precedent as `risk_class::RiskClass::Read`).
#[allow(dead_code)]
pub struct DenylistItem {
    pub id: &'static str,
    pub description: &'static str,
    pub routes: &'static [(&'static str, &'static str)],
}

/// ADR-004's reconciled irreversibility denylist, source-verified routes only. Items 4, 9,
/// 10, and 12 have no corresponding endpoint yet (or, for item 10, an endpoint this task has
/// no grant to modify) and are intentionally absent — see the module doc comment above, not
/// silently dropped.
pub const IRREVERSIBILITY_DENYLIST: &[DenylistItem] = &[
    DenylistItem {
        id: "self_update",
        description: "Self-update / agent-update trigger — VoidTower, Odysseus, Docker, and OS update targets",
        routes: &[
            ("POST", "/api/system/update"),
            ("POST", "/api/updates/voidtower/apply"),
            ("POST", "/api/updates/odysseus/apply"),
            ("POST", "/api/updates/docker/:id/apply"),
            ("POST", "/api/updates/os/apply"),
        ],
    },
    DenylistItem {
        id: "disaster_reset",
        description: "Disaster-recovery import/reset, including the emergency-disable candidate ADR-004 added",
        routes: &[
            ("POST", "/api/disaster/import-config"),
            ("POST", "/api/disaster/emergency-reset-admin"),
            ("POST", "/api/disaster/emergency-disable"),
        ],
    },
    DenylistItem {
        id: "secrets_reveal",
        description: "Secrets export/reveal — single-secret plaintext reveal (bulk export-config was \
                       verified in source to carry no decrypted secret material, so it is not listed here)",
        routes: &[("GET", "/api/secrets/:id/reveal")],
    },
    DenylistItem {
        id: "policy_edit",
        description: "Policy/mode edits — policy_rules CRUD (the mode-setting action itself is gated \
                       via risk_class::for_action's \"voidwatch.mode.set\" entry, not a route yet)",
        routes: &[
            ("POST", "/api/policy/rules"),
            ("PATCH", "/api/policy/rules/:id"),
            ("DELETE", "/api/policy/rules/:id"),
        ],
    },
    DenylistItem {
        id: "disk_format_or_wipe",
        description: "Disk/storage wipe or format — local storage format plus Proxmox node disk wipe/init",
        routes: &[
            ("POST", "/api/storage/format"),
            ("POST", "/api/proxmox/:host_id/nodes/:node/disks/wipe"),
            ("POST", "/api/proxmox/:host_id/nodes/:node/disks/init"),
        ],
    },
    DenylistItem {
        id: "firewall_disable",
        description: "Firewall disable — gates the whole /api/firewall/action route; \"enable\"/\"reload\" \
                       ride along as false positives since body-level action granularity isn't \
                       achievable without touching api/firewall.rs, which is outside this task's grant",
        routes: &[("POST", "/api/firewall/action")],
    },
    DenylistItem {
        id: "app_removal_without_keep_data",
        description: "keep_data=false app removal — no literal keep_data flag exists in source; mapped \
                       to the two unconditional data-destroying app-removal routes (see module doc comment)",
        routes: &[
            ("POST", "/api/apps/:project_name/purge"),
            ("POST", "/api/apps/:project_name/delete-volumes"),
        ],
    },
];

/// Whether a given `(method, path)` route pair is on the hardcoded irreversibility denylist.
/// Not called from any production code path yet — no HTTP handler for these routes is in
/// this task's grant to wire up (see module doc comment's scope note) — same
/// reserved-until-wired precedent as `ActorKind::Ai` and `mode::set_mode`.
#[allow(dead_code)]
pub fn is_route_denylisted(method: &str, path: &str) -> bool {
    IRREVERSIBILITY_DENYLIST
        .iter()
        .any(|item| item.routes.iter().any(|(m, p)| *m == method && *p == path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voidwatch::risk_class::{RiskClass, ROUTE_RISK_CLASSES};

    /// Ties this module's named ADR-004 list to the exhaustive `ROUTE_RISK_CLASSES` ledger:
    /// every route named here must be classified `Irreversible` there. Catches the two
    /// ledgers drifting apart (e.g. someone reclassifying a route in `risk_class.rs` without
    /// checking whether it's on this denylist).
    fn assert_item_routes_are_irreversible(item_id: &str) {
        let item = IRREVERSIBILITY_DENYLIST
            .iter()
            .find(|i| i.id == item_id)
            .unwrap_or_else(|| panic!("no denylist item with id {item_id:?}"));
        assert!(
            !item.routes.is_empty(),
            "denylist item {item_id:?} has no routes"
        );
        for (method, path) in item.routes {
            let classified = ROUTE_RISK_CLASSES
                .iter()
                .find(|(m, p, _)| m == method && p == path);
            match classified {
                Some((_, _, RiskClass::Irreversible)) => {}
                Some((_, _, other)) => panic!(
                    "denylist item {item_id:?}'s route {method} {path} is classified \
                     {other:?} in ROUTE_RISK_CLASSES, not Irreversible"
                ),
                None => panic!(
                    "denylist item {item_id:?}'s route {method} {path} has no \
                     ROUTE_RISK_CLASSES entry at all"
                ),
            }
        }
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_self_update() {
        assert_item_routes_are_irreversible("self_update");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_disaster_reset() {
        assert_item_routes_are_irreversible("disaster_reset");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_emergency_disable() {
        // ADR-004's candidate addition — called out as its own test per the task spec's
        // acceptance-test list, even though it's one of disaster_reset's three routes.
        let item = IRREVERSIBILITY_DENYLIST
            .iter()
            .find(|i| i.id == "disaster_reset")
            .unwrap();
        assert!(
            item.routes
                .contains(&("POST", "/api/disaster/emergency-disable")),
            "emergency-disable must be on the disaster_reset denylist item"
        );
        assert_item_routes_are_irreversible("disaster_reset");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_secrets_reveal() {
        assert_item_routes_are_irreversible("secrets_reveal");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_policy_edit() {
        assert_item_routes_are_irreversible("policy_edit");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_disk_format() {
        assert_item_routes_are_irreversible("disk_format_or_wipe");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_firewall_disable() {
        assert_item_routes_are_irreversible("firewall_disable");
    }

    #[test]
    fn yolo_mode_still_requires_approval_for_app_removal_without_keep_data() {
        assert_item_routes_are_irreversible("app_removal_without_keep_data");
    }

    /// ADR-004 constraint 4: assert *structurally* that no route can alter the constant, not
    /// just that today's routes happen not to. Two checks: (1) no route registered in
    /// `api::router()` mentions "denylist" in its path (there is no mutation endpoint for
    /// it), and (2) no `CREATE TABLE` in `db/mod.rs` backs it (it is not persisted state an
    /// admin token or DB write could flip).
    #[test]
    fn denylist_has_no_api_mutation_path() {
        let router_src = include_str!("../api/mod.rs");
        assert!(
            !router_src.to_lowercase().contains("denylist"),
            "api::router() must not register any route mentioning \"denylist\" — the \
             irreversibility denylist is a compile-time constant, not an API-mutable resource"
        );

        let db_src = include_str!("../db/mod.rs");
        assert!(
            !db_src.to_lowercase().contains("denylist"),
            "db/mod.rs must not define a table backing the irreversibility denylist — it is \
             a compile-time constant per ADR-004 constraint 1, not persisted state"
        );
    }

    /// ADR-004 constraint 2: the denylist applies to every actor class, not just `ai`/
    /// `automation`. Exercised against the one denylist-equivalent action already wired
    /// through `voidwatch::evaluate()` today (item 5's mode-change action, classified
    /// `Irreversible` by `risk_class::for_action`) across all four `ActorKind` variants — the
    /// YOLO branch's actual code path (`voidwatch::mod::mode_pre_pass`) never inspects
    /// `actor.kind` before returning `RequireApproval` for an `Irreversible`-classified
    /// action, so this proves the mechanism itself is actor-agnostic by construction, not
    /// just for the one action name available to test today.
    #[tokio::test]
    async fn denylist_applies_regardless_of_actor_class() {
        use crate::voidwatch::{self, mode, tests::create_policy_tables, ActionKind, Actor, ActorKind, Resource, Verdict};
        use sqlx::sqlite::SqlitePoolOptions;

        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        create_policy_tables(&pool).await;
        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) VALUES (?, 'yolo', 0)",
        )
        .bind(mode::GLOBAL_SCOPE)
        .execute(&pool)
        .await
        .unwrap();

        for actor_kind in [
            ActorKind::ApiToken,
            ActorKind::Automation,
            ActorKind::User,
            ActorKind::Ai,
        ] {
            let verdict = voidwatch::evaluate(
                &pool,
                Actor { kind: actor_kind },
                ActionKind::Mutating,
                "voidwatch.mode.set",
                Resource {
                    resource_type: "voidwatch_mode",
                    resource_id: "global",
                },
            )
            .await;
            assert!(
                matches!(verdict, Verdict::RequireApproval(_)),
                "actor kind {actor_kind:?} was not blocked by the irreversibility denylist \
                 in YOLO mode: got {verdict:?}"
            );
        }
    }

    /// Item 6: no action verb may skip Voidwatch by dropping to the raw Docker API. Verified
    /// structurally (ADR-004: this is an invariant, not a route to gate) — walks every `.rs`
    /// file under `backend/src` and asserts the only call sites of
    /// `bollard::Docker::connect_with_unix_defaults` are the typed-verb helper
    /// (`containers/mod.rs`) and the documented read-only log-stream exception
    /// (`api/containers.rs`'s `logs_ws`, gated by `require_user`, never a mutation). A new
    /// call site anywhere else would mean some future handler bypasses the typed verb layer
    /// entirely, which this test exists to catch.
    #[test]
    fn docker_sock_no_bypass_invariant() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits = Vec::new();
        let mut stack = vec![src_root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read_dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name().is_some_and(|n| n != "denylist.rs")
                {
                    // Excludes this file itself: its own doc comments and assertion
                    // message name the symbol in prose without ever calling it.
                    let contents = std::fs::read_to_string(&path).expect("read file");
                    if contents.contains("connect_with_unix_defaults") {
                        hits.push(
                            path.strip_prefix(&src_root)
                                .unwrap()
                                .to_string_lossy()
                                .replace(std::path::MAIN_SEPARATOR, "/"),
                        );
                    }
                }
            }
        }
        hits.sort();
        assert_eq!(
            hits,
            vec![
                "api/containers.rs".to_string(),
                "containers/mod.rs".to_string(),
            ],
            "unexpected (or missing) call site(s) for connect_with_unix_defaults — every \
             mutating container action must route through the typed verb layer in \
             containers/mod.rs, never a raw socket call reachable outside it"
        );
    }
}
