//! The single choke point every AI/automation-reachable mutating action must pass
//! through before it can reach an action handler (EDD §3.2, gap-analysis P0.1).
//!
//! P0-01 landed this as a **no-op by default** (ADR-001, sequencing clarification):
//! `evaluate()` composed with the existing `policy::check` call convention unchanged,
//! so the verdict for every then-observed action was identical to pre-choke-point
//! behavior. The only new behavior was structural — ingress points that previously
//! never consulted `policy_rules` at all (`api/mcp.rs`, `api/studio.rs`'s `mcp_invoke`)
//! now do, so a rule added later actually takes effect for them.
//!
//! P0-02 (gap-analysis P0.2) ends the no-op: `policy::check` now default-**denies**
//! `api_token` / `automation` / `ai` actors when no rule matches (an explicit
//! `voidwatch_default_allowlist` entry, seeded from pre-existing usage on upgrade,
//! is required — see [`allowlist_seed::seed_default_allowlist_if_empty`]). `user`
//! sessions are unaffected — they're RBAC-governed, not `policy_rules`-governed
//! (see `ActorKind`'s doc comment below). The risk_class/mode-ladder table (P0-03)
//! is still out of scope here.

pub(crate) mod allowlist_seed;

use crate::policy;
use sqlx::SqlitePool;

/// The originator of an action being evaluated at the choke point.
///
/// Mirrors `policy::PolicyRule::actor_type`'s existing string convention
/// (`"api_token" | "automation" | "*"`); `User` is new here and intentionally
/// matches only `"*"` rules, preserving the existing convention elsewhere in the
/// codebase (e.g. `api/containers.rs`) that session-authenticated users are governed
/// by RBAC (`auth::User::role`), not `policy_rules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    /// Bearer-token-authenticated automated actor (MCP client, Odysseus direct calls).
    ApiToken,
    /// Server-initiated automation job (webhook `automation_id` trigger, structured
    /// webhook resource actions).
    Automation,
    /// A logged-in human session (e.g. the Studio MCP tool panel).
    User,
    /// An AI actor distinguishable from a human-held API token (gap-analysis P0.2).
    /// No ingress point constructs this yet — per the P0-02 task spec's
    /// scope-bypass caveat, today's `ApiToken` requests can't reliably tell a human
    /// using a personal token from an AI acting on the god-token (see ADR-003 /
    /// P0-06). This variant exists so `policy_rules` and `voidwatch_default_allowlist`
    /// already have a distinct `"ai"` actor class to target once that signal lands.
    /// Only constructed in tests until an ingress point wires it up — allowed here
    /// rather than deleting the reserved variant.
    #[allow(dead_code)]
    Ai,
}

impl ActorKind {
    fn as_policy_str(self) -> &'static str {
        match self {
            ActorKind::ApiToken => "api_token",
            ActorKind::Automation => "automation",
            ActorKind::User => "user",
            ActorKind::Ai => "ai",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub kind: ActorKind,
}

/// The resource an action targets, used for tag-scoped policy rules.
pub struct Resource<'a> {
    pub resource_type: &'a str,
    pub resource_id: &'a str,
}

/// Whether an action reads state (never gated) or mutates it (always routed through
/// `policy::check`). This is a minimal, local classification for this choke point —
/// not the exhaustive, compile-time-enforced `risk_class` table that P0.3 adds on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionKind {
    Read,
    Mutating,
}

/// `evaluate()`'s verdict. P0-01 only ever produces `Allow`/`Deny`, mirroring
/// `policy::check` exactly (no-op by default, ADR-001). EDD §3.2's third rung,
/// `RequireApproval`, lands with the approval-queue work in P0-02/P0-03.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(String),
}

/// Evaluate an action against Voidwatch policy. This is the *only* function that may
/// call `policy::check` from an AI/automation-reachable ingress handler (`api/mcp.rs`,
/// `api/studio.rs`, `api/integrations.rs`) — see
/// `evaluate_is_the_only_caller_of_policy_check_from_ai_ingress` below.
///
/// Read actions are never gated (nothing to protect). Mutating actions are checked
/// against `policy_rules` and, for `ApiToken`/`Automation`/`Ai` actors, the
/// `voidwatch_default_allowlist` no-matching-rule fallback (gap-analysis P0.2) —
/// see `policy::check`'s doc comment. `User` actors keep the original
/// no-matching-rule-means-`Allow` behavior; they're RBAC-governed, not
/// `policy_rules`-governed.
pub async fn evaluate(
    db: &SqlitePool,
    actor: Actor,
    action_kind: ActionKind,
    action: &str,
    resource: Resource<'_>,
) -> Verdict {
    if action_kind == ActionKind::Read {
        return Verdict::Allow;
    }

    match policy::check(
        db,
        actor.kind.as_policy_str(),
        action,
        resource.resource_type,
        resource.resource_id,
    )
    .await
    {
        policy::PolicyVerdict::Allow => Verdict::Allow,
        policy::PolicyVerdict::Deny(reason) => Verdict::Deny(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// `db::run_migrations` doesn't create `policy_rules` (that happens in
    /// `db::init_pool`, outside the migration path tests use elsewhere in this
    /// crate — see `api/agents.rs`'s `setup_db`), so tests here create it directly.
    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS policy_rules (
                id            TEXT PRIMARY KEY,
                name          TEXT NOT NULL,
                actor_type    TEXT NOT NULL DEFAULT 'api_token',
                action        TEXT NOT NULL DEFAULT '*',
                resource_type TEXT NOT NULL DEFAULT '*',
                resource_tag  TEXT,
                effect        TEXT NOT NULL DEFAULT 'deny',
                priority      INTEGER NOT NULL DEFAULT 100,
                enabled       INTEGER NOT NULL DEFAULT 1,
                created_at    INTEGER NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        // P0.2: `policy::check`'s default-deny path for non-`user` actors consults
        // this table (see `policy.rs::check`, `db::seed_default_allowlist_if_empty`).
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS voidwatch_default_allowlist (
                id            TEXT PRIMARY KEY,
                actor_type    TEXT NOT NULL,
                action        TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                created_at    INTEGER NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn insert_rule(
        pool: &SqlitePool,
        actor_type: &str,
        action: &str,
        resource_type: &str,
        effect: &str,
        priority: i64,
    ) {
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES (?, 'test rule', ?, ?, ?, NULL, ?, ?, 1, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(actor_type)
        .bind(action)
        .bind(resource_type)
        .bind(effect)
        .bind(priority)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn evaluate_read_action_is_always_allowed_without_policy_lookup() {
        let pool = setup_db().await;
        // A deny-everything rule is present, but the action is classified Read, so
        // evaluate() must short-circuit before ever consulting it.
        insert_rule(&pool, "*", "*", "*", "deny", 1).await;

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::ApiToken,
            },
            ActionKind::Read,
            "list_containers",
            Resource {
                resource_type: "mcp_tool",
                resource_id: "list_containers",
            },
        )
        .await;

        assert_eq!(verdict, Verdict::Allow);
    }

    /// Supersedes the P0-01 placeholder `evaluate_no_rule_defaults_to_allow_preserving_current_behavior`:
    /// per gap-analysis P0.2, `api_token` (and `automation`/`ai`) actors now
    /// default-deny when no rule matches and no `voidwatch_default_allowlist` entry
    /// covers the action. `user` actors are unaffected — see `evaluate_no_rule_still_allows_user_actor` below.
    #[tokio::test]
    async fn evaluate_no_rule_defaults_to_deny_for_non_user_actor() {
        let pool = setup_db().await;

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::ApiToken,
            },
            ActionKind::Mutating,
            "restart_container",
            Resource {
                resource_type: "mcp_tool",
                resource_id: "restart_container",
            },
        )
        .await;

        assert!(matches!(verdict, Verdict::Deny(_)));
    }

    /// Companion to the above: `user` sessions keep the pre-existing default-allow
    /// behavior (gap-analysis P0.2 — RBAC governs users, not `policy_rules`).
    #[tokio::test]
    async fn evaluate_no_rule_still_allows_user_actor() {
        let pool = setup_db().await;

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::User,
            },
            ActionKind::Mutating,
            "restart_container",
            Resource {
                resource_type: "mcp_tool",
                resource_id: "restart_container",
            },
        )
        .await;

        assert_eq!(verdict, Verdict::Allow);
    }

    /// The new `ai` actor class (gap-analysis P0.2) default-denies just like
    /// `ApiToken`/`Automation` — no ingress point constructs `ActorKind::Ai` yet
    /// (see its doc comment), but `evaluate()` must already handle it correctly
    /// for the day one does.
    #[tokio::test]
    async fn evaluate_no_rule_defaults_to_deny_for_ai_actor() {
        let pool = setup_db().await;

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::Ai,
            },
            ActionKind::Mutating,
            "restart_container",
            Resource {
                resource_type: "mcp_tool",
                resource_id: "restart_container",
            },
        )
        .await;

        assert!(matches!(verdict, Verdict::Deny(_)));
    }

    /// A `voidwatch_default_allowlist` entry grandfathers a non-`user` actor's
    /// pre-existing action back to `Allow` — the mechanism the P0.2 upgrade
    /// migration relies on (`db::seed_default_allowlist_if_empty`).
    #[tokio::test]
    async fn evaluate_allows_non_user_actor_when_allowlisted() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a1', 'api_token', 'restart_container', 'mcp_tool', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::ApiToken,
            },
            ActionKind::Mutating,
            "restart_container",
            Resource {
                resource_type: "mcp_tool",
                resource_id: "restart_container",
            },
        )
        .await;

        assert_eq!(verdict, Verdict::Allow);
    }

    /// Regression guard for the day a mutating MCP tool is added: today `api/mcp.rs`
    /// calls `policy::check` zero times, so even an admin-authored deny rule matching
    /// a future tool's action name would have no effect. Once routed through
    /// `evaluate()`, it does.
    #[tokio::test]
    async fn mcp_tools_call_denies_mutating_tool_without_approval() {
        let pool = setup_db().await;
        insert_rule(&pool, "api_token", "restart_container", "*", "deny", 1).await;

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::ApiToken,
            },
            ActionKind::Mutating,
            "restart_container",
            Resource {
                resource_type: "mcp_tool",
                resource_id: "restart_container",
            },
        )
        .await;

        assert!(matches!(verdict, Verdict::Deny(_)));
    }

    #[tokio::test]
    async fn evaluate_denies_when_explicit_rule_matches_regardless_of_actor_kind() {
        let pool = setup_db().await;
        insert_rule(&pool, "automation", "run", "automation_job", "deny", 1).await;

        let verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::Automation,
            },
            ActionKind::Mutating,
            "run",
            Resource {
                resource_type: "automation_job",
                resource_id: "job-1",
            },
        )
        .await;

        assert!(matches!(verdict, Verdict::Deny(_)));
    }

    /// EDD §3.2: "There is no code path from the AI runtime to an adapter that
    /// bypasses evaluate()." Enforced here for the three ingress points this task
    /// wires — `containers.rs`/`services.rs` keep their pre-existing, out-of-scope
    /// direct `policy::check` calls (see task spec's "Files to touch").
    #[test]
    fn evaluate_is_the_only_caller_of_policy_check_from_ai_ingress() {
        const GATED_INGRESS_FILES: &[(&str, &str)] = &[
            ("api/mcp.rs", include_str!("../api/mcp.rs")),
            ("api/studio.rs", include_str!("../api/studio.rs")),
            (
                "api/integrations.rs",
                include_str!("../api/integrations.rs"),
            ),
        ];

        for (path, src) in GATED_INGRESS_FILES {
            assert!(
                !src.contains("policy::check("),
                "{path} calls policy::check() directly — AI ingress must route through voidwatch::evaluate() instead (P0.1)"
            );
        }
    }
}
