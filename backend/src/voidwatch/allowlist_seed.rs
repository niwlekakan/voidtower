//! One-shot backfill for `voidwatch_default_allowlist` (gap-analysis P0.2,
//! ADR-001 constraint 5: "A generated allowlist preserving currently-observed
//! automation behavior ships with the default-deny flip").
//!
//! Deliberately kept out of `backend/src/db/mod.rs`: that file's ADR-002 grant
//! covers additive `CREATE TABLE` calls only and explicitly excludes "data
//! migration or backfill logic" as its own category. This submodule is the
//! standalone home the P0-02 task spec's "Files to touch" section calls for —
//! `db::init_pool` calls out to [`seed_default_allowlist_if_empty`] rather than
//! inlining the derivation there. It lives under `voidwatch/` (not `db/`) because
//! both ADR-001 and ADR-002 explicitly pre-authorize `backend/src/voidwatch/**`,
//! and generating the policy-engine's default allowlist is squarely voidwatch's
//! own concern, not the schema layer's.

use sqlx::SqlitePool;

/// Backfills `voidwatch_default_allowlist` from pre-existing usage so the P0.2
/// default-deny flip for `api_token` / `automation` / `ai` actors doesn't break
/// upgrades. Only runs once — skipped if the table already has rows, since an
/// operator (or a later policy decision) may have edited entries since.
///
/// Sourcing, per the P0-02 task spec's ingress-ambiguity caveat:
/// - `automation`: derived from `audit_log` rows written by
///   `audit::log_sourced(..., Some("odysseus"))` (`api/integrations.rs`'s webhook
///   handlers) — the only actor class with a reliable historical signal in
///   `audit_log` today (`source = 'odysseus'`). The audit `action` column is a
///   compound string (e.g. `"integrations.webhook.container.restart"`); see
///   `derive_automation_policy_action` for the reverse mapping back to the bare
///   action name `policy::check` is actually called with.
/// - `api_token`: `audit_log` never records whether a container/service action came
///   from a token (`MaybeTokenActor` is a per-request marker, not a persisted
///   column), so mining it would either fabricate a signal or silently miss one.
///   Instead this grandfathers the exhaustive, currently-reachable action set for
///   the two handlers that call `policy::check(..., "api_token", ...)` today
///   (`api/containers.rs`'s `ContainerAction`, `api/services.rs`'s `ServiceAction`)
///   — verified in source, not guessed.
/// - `ai`: no code path passes `actor_type = "ai"` to `policy::check` in this
///   version, so there is no pre-existing usage to grandfather — it starts with an
///   empty allowlist. That's correct: default-deny for a brand-new actor class
///   doesn't break anything that wasn't already happening.
pub(crate) async fn seed_default_allowlist_if_empty(pool: &SqlitePool) {
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM voidwatch_default_allowlist")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    if existing != 0 {
        return;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut entries: std::collections::HashSet<(String, String, String)> =
        std::collections::HashSet::new();

    for action in ["start", "stop", "restart", "remove"] {
        entries.insert((
            "api_token".to_string(),
            action.to_string(),
            "container".to_string(),
        ));
    }
    for action in ["start", "stop", "restart", "enable", "disable"] {
        entries.insert((
            "api_token".to_string(),
            action.to_string(),
            "service".to_string(),
        ));
    }

    let odysseus_rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT action, resource_type FROM audit_log WHERE source = 'odysseus'")
            .fetch_all(pool)
            .await
            .unwrap_or_default();

    for (action, resource_type) in odysseus_rows {
        let resource_type = resource_type.unwrap_or_default();
        if let Some(bare_action) = derive_automation_policy_action(&action, &resource_type) {
            let policy_resource_type = if bare_action == "automation.run" {
                "automation_job".to_string()
            } else {
                resource_type
            };
            entries.insert(("automation".to_string(), bare_action, policy_resource_type));
        }
    }

    for (actor_type, action, resource_type) in entries {
        let _ = sqlx::query(
            "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at)
             VALUES (?,?,?,?,?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(actor_type)
        .bind(action)
        .bind(resource_type)
        .bind(now)
        .execute(pool)
        .await;
    }
}

/// Reverses `api/integrations.rs`'s webhook audit-log action naming
/// (`"integrations.webhook.{resource_type}.{action}"`, or the automation-job-run
/// special case `"integrations.webhook.automation_trigger"`) back to the bare action
/// name `policy::check` is called with. Returns `None` for anything that doesn't
/// match — a best-effort migration should skip unrecognized rows, not guess.
fn derive_automation_policy_action(audit_action: &str, resource_type: &str) -> Option<String> {
    let rest = audit_action.strip_prefix("integrations.webhook.")?;
    if rest == "automation_trigger" {
        return Some("automation.run".to_string());
    }
    let resource_prefix = format!("{resource_type}.");
    rest.strip_prefix(&resource_prefix).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_migrated_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        // `run_migrations` predates the `source` column (added later via `ALTER` in
        // `init_pool`, mirrored here for this v0.9.0-shaped fixture).
        sqlx::query("ALTER TABLE audit_log ADD COLUMN source TEXT")
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

    async fn insert_odysseus_audit_row(pool: &SqlitePool, action: &str, resource_type: &str) {
        sqlx::query(
            "INSERT INTO audit_log (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, source)
             VALUES (?, 0, NULL, 'agent', ?, ?, 'r1', 'success', 'odysseus')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action)
        .bind(resource_type)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Acceptance test named in the P0-02 task spec: seed a v0.9.0-shaped fixture
    /// (base schema + historical `audit_log` rows, no `voidwatch_default_allowlist`
    /// data yet) and assert the generated allowlist preserves every action that was
    /// already working, so the default-deny flip doesn't break upgrades.
    #[tokio::test]
    async fn upgrade_migration_allowlist_contains_all_pre_existing_observed_actions() {
        let pool = setup_migrated_db().await;
        insert_odysseus_audit_row(&pool, "integrations.webhook.container.restart", "container")
            .await;
        insert_odysseus_audit_row(&pool, "integrations.webhook.service.stop", "service").await;
        insert_odysseus_audit_row(
            &pool,
            "integrations.webhook.automation_trigger",
            "automation_job",
        )
        .await;

        seed_default_allowlist_if_empty(&pool).await;

        // Previously-observed automation-sourced usage must still resolve to Allow.
        assert!(matches!(
            crate::policy::check(&pool, "automation", "restart", "container", "c1").await,
            crate::policy::PolicyVerdict::Allow
        ));
        assert!(matches!(
            crate::policy::check(&pool, "automation", "stop", "service", "s1").await,
            crate::policy::PolicyVerdict::Allow
        ));
        assert!(matches!(
            crate::policy::check(
                &pool,
                "automation",
                "automation.run",
                "automation_job",
                "job-1"
            )
            .await,
            crate::policy::PolicyVerdict::Allow
        ));

        // Currently-reachable api_token actions (containers.rs/services.rs) are
        // grandfathered too, without needing an audit_log signal.
        assert!(matches!(
            crate::policy::check(&pool, "api_token", "remove", "container", "c1").await,
            crate::policy::PolicyVerdict::Allow
        ));

        // An action that was never observed and isn't in the static api_token set
        // stays denied — the migration must not become a blanket allowlist.
        assert!(matches!(
            crate::policy::check(&pool, "automation", "reboot_host", "vm", "v1").await,
            crate::policy::PolicyVerdict::Deny(_)
        ));
    }

    #[tokio::test]
    async fn seed_default_allowlist_if_empty_is_idempotent_and_does_not_duplicate() {
        let pool = setup_migrated_db().await;
        insert_odysseus_audit_row(&pool, "integrations.webhook.container.restart", "container")
            .await;
        insert_odysseus_audit_row(&pool, "integrations.webhook.container.restart", "container")
            .await;

        seed_default_allowlist_if_empty(&pool).await;
        let count_after_first: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM voidwatch_default_allowlist")
                .fetch_one(&pool)
                .await
                .unwrap();

        // Re-running (e.g. a restart of init_pool) must not duplicate or reset rows.
        seed_default_allowlist_if_empty(&pool).await;
        let count_after_second: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM voidwatch_default_allowlist")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(count_after_first, count_after_second);
    }

    #[test]
    fn derive_automation_policy_action_parses_structured_webhook_actions() {
        assert_eq!(
            derive_automation_policy_action("integrations.webhook.container.restart", "container"),
            Some("restart".to_string())
        );
        assert_eq!(
            derive_automation_policy_action("integrations.webhook.service.stop", "service"),
            Some("stop".to_string())
        );
    }

    #[test]
    fn derive_automation_policy_action_parses_automation_trigger_special_case() {
        assert_eq!(
            derive_automation_policy_action(
                "integrations.webhook.automation_trigger",
                "automation_job"
            ),
            Some("automation.run".to_string())
        );
    }

    #[test]
    fn derive_automation_policy_action_rejects_unrecognized_format() {
        assert_eq!(
            derive_automation_policy_action("container.restart", "container"),
            None
        );
        assert_eq!(
            derive_automation_policy_action("integrations.webhook.container.restart", "service"),
            None
        );
    }
}
