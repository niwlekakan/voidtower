use sqlx::SqlitePool;

#[derive(Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize, Clone)]
pub struct PolicyRule {
    pub id: String,
    pub name: String,
    /// "api_token" | "automation" | "*"
    pub actor_type: String,
    /// "restart" | "stop" | "remove" | "deploy" | "run" | "*"
    pub action: String,
    /// "container" | "service" | "app" | "backup" | "vm" | "*"
    pub resource_type: String,
    /// If set, rule only applies when the resource has this tag name
    pub resource_tag: Option<String>,
    /// "allow" | "deny"
    pub effect: String,
    pub priority: i64,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug)]
pub enum PolicyVerdict {
    Allow,
    Deny(String),
}

/// Check policy rules for an automated actor performing an action on a resource.
/// Returns `Allow` if no deny rule matches (default-allow after scope check).
/// Returns `Deny(reason)` if a matching deny rule fires.
///
/// `actor_type`: "api_token" | "automation"
/// `action`:     "restart" | "stop" | "remove" | "deploy" | "run" | etc.
/// `resource_type`: "container" | "service" | "app" | "backup" | "vm"
/// `resource_id`: used to look up the resource's tags
pub async fn check(
    db: &SqlitePool,
    actor_type: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) -> PolicyVerdict {
    // Fetch tag names for this resource
    let tag_names: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM tags t
         JOIN resource_tags rt ON rt.tag_id = t.id
         WHERE rt.resource_type = ? AND rt.resource_id = ?",
    )
    .bind(resource_type)
    .bind(resource_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    // Fetch all enabled rules in priority order (lowest number = highest priority)
    let rules: Vec<PolicyRule> = sqlx::query_as(
        "SELECT id, name, actor_type, action, resource_type, resource_tag,
                effect, priority, enabled, created_at
         FROM policy_rules WHERE enabled = 1 ORDER BY priority ASC",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    for rule in &rules {
        if !matches_actor(&rule.actor_type, actor_type) {
            continue;
        }
        if !matches_field(&rule.action, action) {
            continue;
        }
        if !matches_field(&rule.resource_type, resource_type) {
            continue;
        }
        if let Some(required_tag) = &rule.resource_tag {
            if !tag_names.iter().any(|t| t == required_tag) {
                continue;
            }
        }

        return if rule.effect == "deny" {
            PolicyVerdict::Deny(format!("Blocked by policy rule \"{}\"", rule.name))
        } else {
            PolicyVerdict::Allow
        };
    }

    // No matching rule (gap-analysis P0.2): `user` sessions are RBAC-governed, not
    // policy_rules-governed (see `voidwatch::ActorKind`'s doc comment), so their
    // default-allow behavior is unchanged. `api_token` / `automation` / `ai` flip to
    // default-deny — allowed only via an explicit entry in
    // `voidwatch_default_allowlist`, seeded on upgrade by
    // `db::seed_default_allowlist_if_empty` so pre-existing usage doesn't break.
    if actor_type == "user" {
        return PolicyVerdict::Allow;
    }

    let allowlisted: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM voidwatch_default_allowlist
         WHERE actor_type = ? AND action = ? AND resource_type = ? LIMIT 1",
    )
    .bind(actor_type)
    .bind(action)
    .bind(resource_type)
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    if allowlisted.is_some() {
        PolicyVerdict::Allow
    } else {
        PolicyVerdict::Deny(format!(
            "No policy rule or default allowlist entry permits \"{action}\" on \"{resource_type}\" for actor type \"{actor_type}\" (default-deny)"
        ))
    }
}

fn matches_actor(rule_actor: &str, request_actor: &str) -> bool {
    rule_actor == "*" || rule_actor == request_actor
}

fn matches_field(rule_val: &str, request_val: &str) -> bool {
    rule_val == "*" || rule_val == request_val
}

/// Marker extension injected by bearer_auth middleware when a request arrives
/// via API token rather than a browser session cookie.
#[derive(Clone)]
pub struct ApiTokenActor;

/// Axum extractor that reads `true` when the request came via API token.
/// Returns `false` (never fails) for normal session-cookie requests.
pub struct MaybeTokenActor(pub bool);

#[async_trait::async_trait]
impl<S> axum::extract::FromRequestParts<S> for MaybeTokenActor
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(MaybeTokenActor(
            parts.extensions.get::<ApiTokenActor>().is_some(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Mirrors `voidwatch::tests::setup_db` — `db::run_migrations` doesn't create
    /// `policy_rules` or `voidwatch_default_allowlist` (those live in `db::init_pool`
    /// / `db::seed_default_allowlist_if_empty`, outside the migration path tests use
    /// elsewhere in this crate), so tests here create them directly.
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

    /// user sessions are RBAC-governed, not policy_rules-governed (see
    /// `voidwatch::ActorKind` doc comment) — this is a pure regression guard that
    /// `check()`'s default verdict for `"user"` never changes.
    #[tokio::test]
    async fn check_defaults_to_allow_for_user_actor_with_no_matching_rule() {
        let pool = setup_db().await;

        let verdict = check(&pool, "user", "restart", "container", "c1").await;

        assert!(matches!(verdict, PolicyVerdict::Allow));
    }

    #[tokio::test]
    async fn check_defaults_to_deny_for_ai_actor_with_no_matching_rule() {
        let pool = setup_db().await;

        let verdict = check(&pool, "ai", "restart", "container", "c1").await;

        assert!(matches!(verdict, PolicyVerdict::Deny(_)));
    }

    #[tokio::test]
    async fn check_defaults_to_deny_for_automation_actor_with_no_matching_rule() {
        let pool = setup_db().await;

        let verdict = check(&pool, "automation", "run", "automation_job", "job-1").await;

        assert!(matches!(verdict, PolicyVerdict::Deny(_)));
    }

    /// api_token also flips to default-deny (gap-analysis P0.2) — a plain regression
    /// guard alongside the ai/automation cases above.
    #[tokio::test]
    async fn check_defaults_to_deny_for_api_token_actor_with_no_matching_rule() {
        let pool = setup_db().await;

        let verdict = check(&pool, "api_token", "restart", "container", "c1").await;

        assert!(matches!(verdict, PolicyVerdict::Deny(_)));
    }

    /// An entry in `voidwatch_default_allowlist` overrides the new default-deny for
    /// non-`user` actor classes — this is the mechanism the P0.2 upgrade migration
    /// relies on to avoid breaking currently-observed usage.
    #[tokio::test]
    async fn check_allows_default_deny_actor_when_allowlist_entry_matches() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES ('a1', 'automation', 'run', 'automation_job', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let verdict = check(&pool, "automation", "run", "automation_job", "job-1").await;

        assert!(matches!(verdict, PolicyVerdict::Allow));
    }

    /// Seed of the P1 "exhaustive mode×risk×actor matrix" requirement (gap-analysis
    /// §2 exit criteria) — written as a data-driven table over actor classes so P1 can
    /// extend it with mode/risk dimensions instead of writing one-off tests per class.
    #[tokio::test]
    async fn check_default_verdict_matrix_by_actor_class() {
        let pool = setup_db().await;

        const CASES: &[(&str, bool)] = &[
            ("user", true),
            ("api_token", false),
            ("automation", false),
            ("ai", false),
        ];

        for (actor_type, expect_allow) in CASES {
            let verdict = check(&pool, actor_type, "some_new_action", "container", "c1").await;
            let allowed = matches!(verdict, PolicyVerdict::Allow);
            assert_eq!(
                allowed, *expect_allow,
                "actor_type {actor_type:?}: expected allow={expect_allow}, got {verdict:?}"
            );
        }
    }

    /// An explicit deny rule still fires regardless of the actor-class default —
    /// existing behavior, unchanged by the P0.2 default-deny flip.
    #[tokio::test]
    async fn check_explicit_deny_rule_still_fires_for_user_actor() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('r1', 'test', 'user', 'remove', '*', NULL, 'deny', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let verdict = check(&pool, "user", "remove", "container", "c1").await;

        assert!(matches!(verdict, PolicyVerdict::Deny(_)));
    }

    /// P1-03 mutation-testing finding: no existing test exercised a `"*"` wildcard
    /// `actor_type` reaching this function (every prior test's wildcard-actor rule was
    /// either short-circuited by `ActionKind::Read` or intercepted by a mode-ladder
    /// mode before ever reaching `policy::check`), so `matches_actor`'s
    /// `rule_actor == "*"` branch had no test that would fail if that comparison were
    /// flipped to `!=`. Calling `check()` directly bypasses the mode ladder entirely,
    /// closing the gap: a wildcard-actor rule must fire for every concrete actor type.
    #[tokio::test]
    async fn check_wildcard_actor_rule_applies_to_any_actor_type() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('r1', 'test', '*', 'restart', 'container', NULL, 'deny', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        assert!(matches!(
            check(&pool, "user", "restart", "container", "c1").await,
            PolicyVerdict::Deny(_)
        ));
        assert!(matches!(
            check(&pool, "api_token", "restart", "container", "c1").await,
            PolicyVerdict::Deny(_)
        ));
    }

    /// P1-03 mutation-testing finding: every existing rule/actor pair in this module's
    /// tests happened to match, so `matches_actor` returning unconditional `true`
    /// (rather than actually comparing) never changed a verdict. This pins the
    /// opposite case: a rule scoped to one specific actor type must not apply to a
    /// request from a different one.
    #[tokio::test]
    async fn check_specific_actor_rule_does_not_apply_to_a_different_actor() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('r1', 'test', 'user', 'restart', 'container', NULL, 'allow', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // The rule is scoped to "user" and must not apply to "api_token" — which then
        // falls through to the P0.2 default-deny fallback (no allowlist entry here).
        assert!(matches!(
            check(&pool, "api_token", "restart", "container", "c1").await,
            PolicyVerdict::Deny(_)
        ));
    }

    /// P1-03 mutation-testing finding: same gap as `matches_actor` above, for
    /// `matches_field` (shared by both the `action` and `resource_type` comparisons):
    /// every existing rule's action/resource_type happened to equal the request's, so
    /// `matches_field` returning unconditional `true` never changed a verdict either.
    #[tokio::test]
    async fn check_specific_action_rule_does_not_match_a_different_action() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('r1', 'test', '*', 'remove', '*', NULL, 'deny', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // The rule only matches action "remove" — "restart" must fall through
        // unmatched. "user" default-allows with no matching rule (RBAC-governed).
        assert!(matches!(
            check(&pool, "user", "restart", "container", "c1").await,
            PolicyVerdict::Allow
        ));
    }

    /// P1-03 mutation-testing finding: `resource_tag`-scoped rules had no test
    /// coverage at all (every rule inserted across this crate's tests passes `NULL`),
    /// so the `if !tag_names.iter().any(...) { continue; }` guard's `!` and `==` had
    /// nothing that would fail if deleted/flipped. Exercises both directions: a
    /// tag-scoped rule applies to a resource carrying that tag, and is skipped for one
    /// that doesn't.
    #[tokio::test]
    async fn check_resource_tag_scoped_rule_only_applies_to_tagged_resources() {
        let pool = setup_db().await;
        sqlx::query("INSERT INTO tags (id, name) VALUES ('t1', 'prod')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO resource_tags (resource_type, resource_id, tag_id) VALUES ('container', 'c-tagged', 't1')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO policy_rules (id, name, actor_type, action, resource_type, resource_tag, effect, priority, enabled, created_at)
             VALUES ('r1', 'test', '*', 'restart', 'container', 'prod', 'deny', 1, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Carries the "prod" tag — the tag-scoped rule applies.
        assert!(matches!(
            check(&pool, "user", "restart", "container", "c-tagged").await,
            PolicyVerdict::Deny(_)
        ));
        // Does not carry the "prod" tag — the rule must be skipped; "user" then
        // default-allows with no other matching rule.
        assert!(matches!(
            check(&pool, "user", "restart", "container", "c-untagged").await,
            PolicyVerdict::Allow
        ));
    }
}
