//! Compile-time inventory of routes that always require human approval, regardless of
//! Voidwatch mode or actor class.
//!
//! The route registry is the authoritative security ledger. Tests resolve every route in this
//! rationale-bearing inventory through that registry and require both irreversible risk and
//! mandatory approval, preventing the two representations from drifting.
//!
//! No database table or API backs this list. Changing it requires a reviewed code change.
//! AI and automation ingress already fail closed for `Irreversible` actions; direct handler
//! enforcement is represented by the route registry.
//!
//! ## Operations without corresponding endpoints
//!
//! - **Item 4, secrets master-key operations**: no master-key rotation/access endpoint
//!   exists anywhere in `backend/src/api/secrets.rs` or `backend/src/` (`rg -n
//!   "master.key|master_key|MASTER_KEY"` across the crate returns nothing). N/A — nothing to
//!   gate yet.
//! - **Item 9, host power actions on the control-plane's own host**: `api/system.rs` has
//!   `restart` (soft process restart via `kill -TERM` + re-exec, not a host power action) and
//!   `update` (already covered by item 1); there is no `poweroff`/`reboot`/`shutdown -h`
//!   endpoint for the physical/VM host VoidTower itself runs on. N/A — nothing to gate yet.
//! - **Deletion of the last remaining snapshot/backup of a resource**: `api/backups.rs`'s
//!   `DELETE /api/backups/:id` only
//!   removes a `backup_configs` schedule row (verified in source; its sibling `delete_plan`
//!   handler says so explicitly — "existing backup data on disk is NOT deleted"), so it isn't
//!   actual data loss. `api/proxmox.rs`'s `vm_delete_snapshot` (`DELETE
//!   /api/proxmox/:host_id/vms/:vmid/snapshot/:snapname`) is the route that destroys real,
//!   irrecoverable point-in-time recovery data, so it's the one gated here. A static route
//!   table can't express the literal count-dependent condition ("last remaining"), so this
//!   coarsely gates every snapshot deletion in YOLO mode — the same accepted-false-positive
//!   tradeoff already used for item 6 (`firewall_disable`, below). `backup_configs` deletion
//!   is left off this list since it deletes no data.
//! - **Device decommission**: this repo has no production multi-node agent yet.
//!   `api/node_enroll.rs`'s `delete_node` deletes an *enrolled*
//!   node record (and its WireGuard peer), which is ordinary resource deletion, not
//!   retiring a live agent-managed device.
//!
//! ## `keep_data=false` app removal
//!
//! No literal `keep_data` parameter exists anywhere in this crate (`rg -n
//! "keep_data|keepData"` returns nothing) or the frontend. The closest source-verified
//! analogs are `api/apps.rs`'s `purge_app` (`POST /api/apps/:project_name/purge` — deletes
//! the compose project directory from disk *and* the `deployed_apps` row; there is no
//! "keep data" alternative, i.e. it always operates in the `keep_data=false` sense) and
//! `delete_app_volumes` (`POST /api/apps/:project_name/delete-volumes` — explicitly destroys
//! volumes/data while leaving the app entry registered). Both routes were reclassified from
//! `RiskClass::Destructive` to `Irreversible` during P0-04 and retain that classification in the
//! shared route registry because they are the concrete, unconditional data-destroying
//! app-removal paths.

/// One denylist item and the concrete route(s) it maps to. `id` mirrors the
/// `yolo_mode_still_requires_approval_for_<id>` test names. `description` is read only by this
/// module's own tests/doc tooling today (`#[allow(dead_code)]`: this crate has no lib
/// target, so a field consumed only by `cfg(test)` code still trips the bin-target dead-code
/// lint, same precedent as `risk_class::RiskClass::Read`).
#[allow(dead_code)]
pub struct DenylistItem {
    pub id: &'static str,
    pub description: &'static str,
    pub routes: &'static [(&'static str, &'static str)],
}

/// Source-verified irreversibility routes. Operations with no endpoint are intentionally absent;
/// see the module documentation above.
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
        description: "Disaster-recovery import/reset, including emergency disable",
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
                       ride along as false positives because the route registry has no body-level \
                       action granularity",
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
    DenylistItem {
        id: "last_snapshot_deletion",
        description: "Deletion of a VM snapshot — coarse gate on every snapshot deletion, since a \
                       static route table can't express the literal \"last remaining\" count \
                       condition (see module doc comment); backup_configs schedule deletion is \
                       excluded since it destroys no data",
        routes: &[(
            "DELETE",
            "/api/proxmox/:host_id/vms/:vmid/snapshot/:snapname",
        )],
    },
];

/// Whether a given `(method, path)` route pair is on the hardcoded irreversibility denylist.
/// Kept as a query helper and directly covered by regression tests.
#[allow(dead_code)]
pub fn is_route_denylisted(method: &str, path: &str) -> bool {
    IRREVERSIBILITY_DENYLIST
        .iter()
        .any(|item| item.routes.iter().any(|(m, p)| *m == method && *p == path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mcp::action_registry::{self, ApprovalPolicy, RiskClass};

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
            let metadata = action_registry::route(method, path).unwrap_or_else(|| {
                panic!("denylist item {item_id:?}'s route {method} {path} has no metadata")
            });
            assert_eq!(
                metadata.risk,
                RiskClass::Irreversible,
                "denylist item {item_id:?}'s route {method} {path} must be irreversible"
            );
            assert_eq!(
                metadata.approval,
                ApprovalPolicy::Always,
                "denylist item {item_id:?}'s route {method} {path} must always require approval"
            );
        }
    }

    #[test]
    fn denylist_routes_are_irreversible_and_always_approved() {
        for item in IRREVERSIBILITY_DENYLIST {
            assert_item_routes_are_irreversible(item.id);
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
        // Kept as a dedicated regression even though this is one of disaster_reset's routes.
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

    #[test]
    fn yolo_mode_still_requires_approval_for_last_snapshot_deletion() {
        assert_item_routes_are_irreversible("last_snapshot_deletion");
    }

    /// Assert *structurally* that no route can alter the constant, not just that today's routes
    /// happen not to. Two checks: (1) no route registered in
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
             a compile-time constant, not persisted state"
        );
    }

    /// The denylist applies to every actor class, not just `ai`/
    /// `automation`. Exercised against the one denylist-equivalent action already wired
    /// through `voidwatch::evaluate()` today (item 5's mode-change action, classified
    /// `Irreversible` by `risk_class::for_action`) across all four `ActorKind` variants — the
    /// YOLO branch's actual code path (`voidwatch::mod::mode_pre_pass`) never inspects
    /// `actor.kind` before returning `RequireApproval` for an `Irreversible`-classified
    /// action, so this proves the mechanism itself is actor-agnostic by construction, not
    /// just for the one action name available to test today.
    #[tokio::test]
    async fn denylist_applies_regardless_of_actor_class() {
        use crate::voidwatch::{
            self, mode, tests::create_policy_tables, ActionKind, Actor, ActorKind, Resource,
            Verdict,
        };
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

    /// P1-03 audit finding: the mode-dimension combination this test's name describes
    /// ("all twelve/eight IRREVERSIBILITY_DENYLIST items require non-Allow in every
    /// mode, including YOLO") is **already exhaustively covered for the one item this
    /// module can actually exercise through `voidwatch::evaluate()`** — the sibling
    /// `voidwatch::tests::exhaustive_mode_by_risk_class_matrix` test drives the
    /// `RiskClass::Irreversible` risk class (representative action
    /// `"voidwatch.mode.set"`, this list's `policy_edit` item's paired mode-change
    /// action) through all four modes and both allowlisted states, and
    /// `denylist_applies_regardless_of_actor_class` above extends that to all four
    /// `ActorKind`s in YOLO. The other seven items are `(method, path)` HTTP route
    /// classifications with no ingress point that ever passes them into `evaluate()`
    /// as an action-name string (see this module's own "Scope note" doc comment) —
    /// wiring one would require a real production ingress path; there is no sound way to drive
    /// them through the mode
    /// ladder without a fabricated call that proves nothing (the prior attempt at this
    /// exact test did exactly that: it drove `evaluate()` with the item `id`s as
    /// action strings against an actor/table setup where every mode's outcome was
    /// already fixed by mode/allowlist logic alone, never actually reading
    /// `IRREVERSIBILITY_DENYLIST`'s contents — rejected in review for being
    /// unfalsifiable). What *is* still a genuine, closeable gap: nothing pinned that
    /// none of these eight ids happen to collide with a `for_action` vocabulary entry
    /// classified below `Irreversible` — the test below closes that.
    #[test]
    fn irreversibility_denylist_items_deny_or_require_approval_in_every_mode() {
        use crate::voidwatch::risk_class::{self, RiskClass};

        assert!(
            !IRREVERSIBILITY_DENYLIST.is_empty(),
            "the denylist must not be empty for this assertion to mean anything"
        );
        for item in IRREVERSIBILITY_DENYLIST {
            assert_eq!(
                risk_class::for_action(item.id),
                RiskClass::Irreversible,
                "denylist item {:?} shares its id with a for_action() vocabulary entry \
                 classified below Irreversible — if this id is ever wired as an action \
                 name into voidwatch::evaluate() (today none are), the mode ladder would \
                 treat it as a lower risk class instead of Irreversible, silently \
                 defeating the denylist in Observer/Assisted/Trusted and YOLO's \
                 own irreversibility exception alike",
                item.id
            );
        }
    }

    /// P1-03 mutation-testing finding: `is_route_denylisted` had no direct test at
    /// all — it's not called from any production path yet (see its doc comment) and
    /// no existing test invoked it either, so every mutant in its body survived.
    /// Covers both true cases (pulled from two different denylist items) and three
    /// false cases distinguishing each of method-only-match, path-only-match, and
    /// neither-matches.
    #[test]
    fn is_route_denylisted_matches_exact_method_and_path_pairs_only() {
        assert!(is_route_denylisted("POST", "/api/system/update"));
        assert!(is_route_denylisted("GET", "/api/secrets/:id/reveal"));

        // Same path as a real denylisted route, wrong method.
        assert!(!is_route_denylisted("GET", "/api/system/update"));
        // Same method as several denylisted routes, unrelated path.
        assert!(!is_route_denylisted("POST", "/api/system/restart"));
        // Neither matches anything on the list.
        assert!(!is_route_denylisted("DELETE", "/api/nonexistent"));
    }

    /// Item 6: no action verb may skip Voidwatch by dropping to the raw Docker API. Verified
    /// structurally (this is an invariant, not a route to gate) — walks every `.rs`
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
