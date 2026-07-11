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
//! (see `ActorKind`'s doc comment below).
//!
//! P0-03 (gap-analysis P0.3) adds the Observer/Assisted/Trusted/YOLO mode ladder as a
//! pre-pass **in front of** everything above (EDD §3.2's `evaluate()` diagram: mode
//! ladder first, `policy_rules` second) — see [`mode`] for per-scope mode storage and
//! [`risk_class`] for the `read | mutate | destructive | irreversible` classification
//! the ladder consults for Trusted/YOLO's risk-sensitive branches. Rollout is
//! deliberately no-op-safe, same precedent as P0-01: a scope (or the whole instance)
//! with no mode ever configured skips the ladder entirely and falls straight through to
//! the P0-01/P0-02 behavior described above — see [`mode::get_mode`]'s doc comment.

pub(crate) mod allowlist_seed;
pub mod denylist;
pub mod mode;
pub mod risk_class;

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

/// `evaluate()`'s verdict. P0-01 only ever produced `Allow`/`Deny`, mirroring
/// `policy::check` exactly (no-op by default, ADR-001). P0-03 adds the two remaining
/// rungs EDD §3.2 describes:
/// - `RequireApproval` — Assisted mode's blanket rule, Trusted mode's
///   destructive/not-allowlisted case, and YOLO's irreversibility-denylist exception
///   (`risk_class::RiskClass::Irreversible`, coordinated with P0-04's hardcoded list).
/// - `AllowRequireSnapshot` — Trusted mode's "snapshot-before-apply mandatory where the
///   target supports it" clause (`risk_class::requires_snapshot_before_apply`). The
///   action is authorized, but the caller must take the target's snapshot before
///   actually executing it — deliberately a distinct variant from a plain `Allow`
///   rather than a side channel, so call sites can't silently skip it.
///
/// Adding either variant is a breaking match-exhaustiveness change on purpose: every
/// existing `evaluate()` caller (`api/mcp.rs`, `api/integrations.rs`) must be updated to
/// decide what it does with the new rungs rather than silently falling through the old
/// two-armed match (which would have treated `RequireApproval` as unhandled/`Allow`-like
/// — the exact bug this exhaustiveness is here to prevent).
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    AllowRequireSnapshot(String),
    RequireApproval(String),
    Deny(String),
}

/// Evaluate an action against Voidwatch policy. This is the *only* function that may
/// call `policy::check` from an AI/automation-reachable ingress handler (`api/mcp.rs`,
/// `api/studio.rs`, `api/integrations.rs`) — see
/// `evaluate_is_the_only_caller_of_policy_check_from_ai_ingress` below.
///
/// Read actions are never gated (nothing to protect). Mutating actions first pass
/// through the P0-03 mode-ladder pre-pass (skipped entirely if no mode is configured for
/// the resource's scope or globally — see [`mode::get_mode`]), then, if the pre-pass
/// doesn't return a terminal verdict, `policy_rules` and the `voidwatch_default_allowlist`
/// no-matching-rule fallback (gap-analysis P0.2) exactly as before P0-03 — see
/// `policy::check`'s doc comment. `User` actors keep the original
/// no-matching-rule-means-`Allow` behavior at the `policy::check` stage; they're
/// RBAC-governed, not `policy_rules`-governed. The mode ladder itself applies to every
/// actor class equally (ADR-004 constraint 2: the ladder isn't an AI-only control).
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

    if let Some(verdict) = mode_pre_pass(db, actor, action, &resource).await {
        return verdict;
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

/// The EDD §3.2 mode ladder, evaluated ahead of `policy_rules`. Returns `Some(verdict)`
/// for a terminal decision (nothing more to check), or `None` to fall through to
/// `policy::check` as usual. `None` is also what an unconfigured scope/instance gets —
/// see [`mode::get_mode`]'s doc comment for why that's the safe rollout default.
async fn mode_pre_pass(
    db: &SqlitePool,
    actor: Actor,
    action: &str,
    resource: &Resource<'_>,
) -> Option<Verdict> {
    let scope = mode::scope_for_resource(resource);
    let current_mode = mode::get_mode(db, &scope).await?;
    let risk = risk_class::for_action(action);

    match current_mode {
        mode::Mode::Observer => Some(Verdict::Deny(
            "Observer mode denies all mutating actions".to_string(),
        )),
        mode::Mode::Assisted => Some(Verdict::RequireApproval(
            "Assisted mode requires approval for every mutating action".to_string(),
        )),
        mode::Mode::Trusted => trusted_mode_verdict(db, actor, action, resource, risk).await,
        mode::Mode::Yolo => {
            if risk == risk_class::RiskClass::Irreversible {
                Some(Verdict::RequireApproval(
                    "Irreversible actions always require approval, even in YOLO mode".to_string(),
                ))
            } else {
                None
            }
        }
    }
}

/// Trusted mode: destructive/irreversible actions always require approval; everything
/// else is auto-approved only if it's on the per-resource/per-action allowlist (reusing
/// `voidwatch_default_allowlist` — the same "(actor_type, action, resource_type) is
/// pre-approved" shape P0-02 already introduced for a different fallback, see the PR
/// description for why this task reuses rather than duplicates it); non-allowlisted
/// mutate-class actions still require approval — Trusted mode narrows Assisted mode's
/// blanket approval requirement, it doesn't remove it. Allowlisted mutations targeting a
/// snapshot-capable resource type get `AllowRequireSnapshot` instead of a plain `Allow`.
async fn trusted_mode_verdict(
    db: &SqlitePool,
    actor: Actor,
    action: &str,
    resource: &Resource<'_>,
    risk: risk_class::RiskClass,
) -> Option<Verdict> {
    if matches!(
        risk,
        risk_class::RiskClass::Destructive | risk_class::RiskClass::Irreversible
    ) {
        return Some(Verdict::RequireApproval(format!(
            "Trusted mode still requires approval for {risk:?}-class actions"
        )));
    }

    let allowlisted: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM voidwatch_default_allowlist
         WHERE actor_type = ? AND action = ? AND resource_type = ? LIMIT 1",
    )
    .bind(actor.kind.as_policy_str())
    .bind(action)
    .bind(resource.resource_type)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    if allowlisted.is_none() {
        return Some(Verdict::RequireApproval(
            "Trusted mode requires approval for actions not on the per-resource/per-action \
             allowlist"
                .to_string(),
        ));
    }

    if risk_class::requires_snapshot_before_apply(resource.resource_type) {
        return Some(Verdict::AllowRequireSnapshot(format!(
            "Trusted mode requires a pre-apply snapshot of {} before proceeding",
            resource.resource_type
        )));
    }

    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// `db::run_migrations` doesn't create `policy_rules`/`voidwatch_default_allowlist`/
    /// `voidwatch_mode_settings` (those live in `db::init_pool`, outside the migration
    /// path tests use elsewhere in this crate — see `api/agents.rs`'s `setup_db`), so
    /// tests create them directly. `pub(crate)` so `voidwatch::mode`'s own test module
    /// can reuse it rather than duplicating the same three `CREATE TABLE` statements.
    pub(crate) async fn create_policy_tables(pool: &SqlitePool) {
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
        .execute(pool)
        .await
        .unwrap();
        // P0.2: `policy::check`'s default-deny path for non-`user` actors consults
        // this table (see `policy.rs::check`, `db::seed_default_allowlist_if_empty`).
        // P0.3: Trusted mode's per-resource/per-action allowlist also reuses it (see
        // `trusted_mode_verdict`'s doc comment).
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS voidwatch_default_allowlist (
                id            TEXT PRIMARY KEY,
                actor_type    TEXT NOT NULL,
                action        TEXT NOT NULL,
                resource_type TEXT NOT NULL,
                created_at    INTEGER NOT NULL
            )"#,
        )
        .execute(pool)
        .await
        .unwrap();
        // P0.3: per-scope mode storage (`mode::get_mode`/`mode::set_mode`).
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS voidwatch_mode_settings (
                scope         TEXT PRIMARY KEY,
                mode          TEXT NOT NULL DEFAULT 'observer',
                updated_at    INTEGER NOT NULL,
                updated_by    TEXT
            )"#,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        create_policy_tables(&pool).await;
        pool
    }

    async fn set_mode_row(pool: &SqlitePool, scope: &str, mode: mode::Mode) {
        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) VALUES (?, ?, 0)
             ON CONFLICT(scope) DO UPDATE SET mode = excluded.mode",
        )
        .bind(scope)
        .bind(mode.as_str())
        .execute(pool)
        .await
        .unwrap();
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

    // ------------------------------------------------------------------------------
    // P0-03: mode ladder acceptance tests (named in the task spec)
    // ------------------------------------------------------------------------------

    fn api_token_actor() -> Actor {
        Actor {
            kind: ActorKind::ApiToken,
        }
    }

    #[tokio::test]
    async fn mode_pre_pass_is_a_noop_when_no_mode_configured() {
        // Rollout-safety regression (see `mode::get_mode`'s doc comment): an instance
        // that has never configured a mode must behave exactly as pre-P0-03 — a
        // mutating action with no matching policy_rules row and no allowlist entry
        // still hits the P0.2 default-deny fallback, not a mode-ladder Deny/Observer.
        let pool = setup_db().await;

        let verdict = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;

        // Reaches policy::check's P0.2 fallback (Deny, not the mode ladder's Observer
        // Deny) — same outcome, but for the pre-existing reason, proven by contrast
        // with `observer_mode_denies_all_mutating_actions` below using a different
        // resource so the two tests can't pass for the same coincidental reason.
        assert!(matches!(verdict, Verdict::Deny(reason) if !reason.contains("Observer")));
    }

    #[tokio::test]
    async fn observer_mode_denies_all_mutating_actions() {
        let pool = setup_db().await;
        set_mode_row(&pool, mode::GLOBAL_SCOPE, mode::Mode::Observer).await;
        // Even an explicit allow rule and an allowlist entry must not matter — Observer
        // denies at the pre-pass, before policy_rules is ever consulted.
        insert_rule(&pool, "*", "*", "*", "allow", 1).await;

        let verdict = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;

        assert!(matches!(verdict, Verdict::Deny(reason) if reason.contains("Observer")));

        // Read actions remain unaffected by Observer mode.
        let read_verdict = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Read,
            "list",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;
        assert_eq!(read_verdict, Verdict::Allow);
    }

    #[tokio::test]
    async fn assisted_mode_requires_approval_for_every_mutating_action() {
        let pool = setup_db().await;
        set_mode_row(&pool, mode::GLOBAL_SCOPE, mode::Mode::Assisted).await;
        // An explicit allow rule and an allowlist entry must not matter either — every
        // mutating call requires approval in Assisted mode, no exceptions.
        insert_rule(&pool, "*", "*", "*", "allow", 1).await;
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a1', 'api_token', 'start', 'service', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let verdict = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;

        assert!(matches!(verdict, Verdict::RequireApproval(_)));
    }

    #[tokio::test]
    async fn trusted_mode_auto_approves_allowlisted_mutate_but_not_destructive() {
        let pool = setup_db().await;
        set_mode_row(&pool, mode::GLOBAL_SCOPE, mode::Mode::Trusted).await;
        // Allowlist both a Mutate-class and a Destructive-class action — the
        // destructive one must still require approval despite being allowlisted.
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a1', 'api_token', 'start', 'service', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a2', 'api_token', 'remove', 'service', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let allowed = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;
        assert_eq!(allowed, Verdict::Allow);

        let destructive = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "remove",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;
        assert!(matches!(destructive, Verdict::RequireApproval(_)));

        // Not-allowlisted mutate-class actions still require approval too — the
        // allowlist narrows Assisted's blanket rule, it doesn't remove it.
        let not_allowlisted = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "stop",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;
        assert!(matches!(not_allowlisted, Verdict::RequireApproval(_)));
    }

    #[tokio::test]
    async fn trusted_mode_requires_snapshot_before_apply_where_supported() {
        let pool = setup_db().await;
        set_mode_row(&pool, mode::GLOBAL_SCOPE, mode::Mode::Trusted).await;
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a1', 'api_token', 'start', 'vm', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // "vm" is snapshot-capable (Proxmox/local VM snapshot) per
        // `risk_class::SNAPSHOT_CAPABLE_RESOURCE_TYPES`.
        let verdict = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "vm",
                resource_id: "v1",
            },
        )
        .await;
        assert!(matches!(verdict, Verdict::AllowRequireSnapshot(_)));

        // "service" is not snapshot-capable — same allowlisted mutate action, plain
        // Allow instead (already covered above, re-asserted here for contrast).
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a2', 'api_token', 'start', 'service', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let non_snapshot_verdict = evaluate(
            &pool,
            api_token_actor(),
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;
        assert_eq!(non_snapshot_verdict, Verdict::Allow);
    }

    #[tokio::test]
    async fn yolo_mode_auto_approves_except_denylist() {
        let pool = setup_db().await;
        set_mode_row(&pool, mode::GLOBAL_SCOPE, mode::Mode::Yolo).await;

        // `user` actors default-allow at the policy::check stage with no matching rule
        // (RBAC-governed, unaffected by P0.2), isolating the assertion to what YOLO
        // mode's pre-pass itself does rather than conflating it with P0.2's allowlist.
        let mutate_verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::User,
            },
            ActionKind::Mutating,
            "start",
            Resource {
                resource_type: "service",
                resource_id: "s1",
            },
        )
        .await;
        assert_eq!(mutate_verdict, Verdict::Allow);

        // The irreversibility-denylist exception: risk_class::Irreversible actions
        // always require approval, even in YOLO mode (this task stubs the actual
        // hardcoded list as the Irreversible risk class, coordinated with P0-04).
        let irreversible_verdict = evaluate(
            &pool,
            Actor {
                kind: ActorKind::User,
            },
            ActionKind::Mutating,
            "voidwatch.mode.set",
            Resource {
                resource_type: "voidwatch_mode",
                resource_id: "global",
            },
        )
        .await;
        assert!(matches!(irreversible_verdict, Verdict::RequireApproval(_)));
    }

    #[tokio::test]
    async fn mode_change_is_itself_audited_and_approval_gated() {
        let pool = setup_db().await;
        // Any mode other than a plain pass-through demonstrates the gate — Trusted
        // still requires approval for "voidwatch.mode.set" because it's classified
        // Irreversible (destructive/irreversible actions are never auto-approved even
        // when allowlisted, see `trusted_mode_verdict`).
        set_mode_row(&pool, mode::GLOBAL_SCOPE, mode::Mode::Trusted).await;

        let verdict =
            mode::set_mode(&pool, "container:c1", mode::Mode::Yolo, api_token_actor()).await;

        assert!(matches!(verdict, Verdict::RequireApproval(_)));
        // Not persisted — a RequireApproval verdict must not silently take effect. The
        // scope still inherits the pre-existing global Trusted mode, not the requested
        // Yolo override.
        assert_eq!(
            mode::get_mode(&pool, "container:c1").await,
            Some(mode::Mode::Trusted)
        );

        let audited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'voidwatch.mode.set' AND outcome = 'pending_approval'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audited, 1);
    }

    #[tokio::test]
    async fn exhaustive_mode_by_risk_class_matrix() {
        use mode::Mode::*;
        use risk_class::RiskClass::*;

        #[derive(Debug, PartialEq, Eq)]
        enum Expect {
            Allow,
            Snapshot,
            Approval,
            Deny,
        }

        // Representative action name per risk class (see `risk_class::for_action`).
        fn action_for(risk: risk_class::RiskClass) -> &'static str {
            match risk {
                Mutate => "start",
                Destructive => "remove",
                Irreversible => "voidwatch.mode.set",
                Read => unreachable!("risk_class is only consulted for Mutating actions"),
            }
        }

        // "service" is deliberately not in SNAPSHOT_CAPABLE_RESOURCE_TYPES, isolating
        // this matrix from the separate snapshot test above. Uses a `User` actor
        // throughout rather than `ApiToken`: `User` always default-allows at the
        // `policy::check` stage when the mode pre-pass lets an action through (RBAC-
        // governed, unaffected by P0.2's separate default-deny-for-automated-actors
        // mechanic), so a row reaching `Expect::Allow` here is proof the *mode ladder*
        // let it through, not an artifact of also happening to satisfy P0.2's unrelated
        // allowlist gate (which `trusted_mode_auto_approves_allowlisted_mutate_but_not_destructive`
        // and the `policy::tests` module already cover for `ApiToken`/`Automation`/`Ai`).
        const CASES: &[(mode::Mode, risk_class::RiskClass, bool, Expect)] = &[
            (Observer, Mutate, false, Expect::Deny),
            (Observer, Destructive, false, Expect::Deny),
            (Observer, Irreversible, false, Expect::Deny),
            (Assisted, Mutate, true, Expect::Approval),
            (Assisted, Destructive, true, Expect::Approval),
            (Assisted, Irreversible, true, Expect::Approval),
            (Trusted, Mutate, true, Expect::Allow),
            (Trusted, Mutate, false, Expect::Approval),
            (Trusted, Destructive, true, Expect::Approval),
            (Trusted, Destructive, false, Expect::Approval),
            (Trusted, Irreversible, true, Expect::Approval),
            (Trusted, Irreversible, false, Expect::Approval),
            (Yolo, Mutate, false, Expect::Allow),
            (Yolo, Destructive, false, Expect::Allow),
            (Yolo, Irreversible, false, Expect::Approval),
            (Yolo, Irreversible, true, Expect::Approval),
        ];

        for (mode_under_test, risk, allowlisted, expect) in CASES {
            let pool = setup_db().await;
            set_mode_row(&pool, mode::GLOBAL_SCOPE, *mode_under_test).await;
            let action = action_for(*risk);
            if *allowlisted {
                sqlx::query(
                    "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
                     VALUES (?, 'user', ?, 'service', 0)",
                )
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(action)
                .execute(&pool)
                .await
                .unwrap();
            }

            let verdict = evaluate(
                &pool,
                Actor {
                    kind: ActorKind::User,
                },
                ActionKind::Mutating,
                action,
                Resource {
                    resource_type: "service",
                    resource_id: "s1",
                },
            )
            .await;

            let got = match verdict {
                Verdict::Allow => Expect::Allow,
                Verdict::AllowRequireSnapshot(_) => Expect::Snapshot,
                Verdict::RequireApproval(_) => Expect::Approval,
                Verdict::Deny(_) => Expect::Deny,
            };
            assert_eq!(
                got, *expect,
                "mode={mode_under_test:?} risk={risk:?} allowlisted={allowlisted}: \
                 expected {expect:?}, got {got:?}"
            );
        }
    }
}
