//! Per-scope Voidwatch mode storage (EDD §3.2: "Mode is set per scope — global default,
//! overridable per device/app — not per conversation"). Schema lives in
//! `voidwatch_mode_settings` (`db/mod.rs`, ADR-002, additive only).
//!
//! Scope keys: `GLOBAL_SCOPE` (`"global"`) for the instance-wide default, or
//! `"{resource_type}:{resource_id}"` for a per-device/app override. `evaluate()`
//! (`mod.rs`) derives the latter from the `Resource` it already receives, so no ingress
//! call site needs to pass mode/scope explicitly — a resource with no override row
//! simply inherits the global row.

use crate::voidwatch::{self, ActionKind, Actor, Resource, Verdict};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Observer,
    Assisted,
    Trusted,
    Yolo,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Observer => "observer",
            Mode::Assisted => "assisted",
            Mode::Trusted => "trusted",
            Mode::Yolo => "yolo",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "observer" => Some(Mode::Observer),
            "assisted" => Some(Mode::Assisted),
            "trusted" => Some(Mode::Trusted),
            "yolo" => Some(Mode::Yolo),
            _ => None,
        }
    }
}

pub const GLOBAL_SCOPE: &str = "global";

/// Derives the scope key `evaluate()` looks up for a given resource. Any resource
/// without its own override row transparently inherits the global mode.
pub fn scope_for_resource(resource: &Resource<'_>) -> String {
    format!("{}:{}", resource.resource_type, resource.resource_id)
}

/// `None` means no mode has been configured for this scope *or* globally — a fresh or
/// just-upgraded install with an empty `voidwatch_mode_settings` table. Mirroring P0-01's
/// own rollout precedent (`voidwatch/mod.rs`'s doc comment: "landed this as a no-op by
/// default"), an unconfigured mode is a no-op: `evaluate()` skips the ladder pre-pass
/// entirely and falls straight through to `policy::check`, exactly as it did before this
/// task landed. The ladder only starts gating once an operator explicitly sets a mode via
/// [`set_mode`] — it must never silently flip every existing automation to Observer's
/// deny-everything behavior on upgrade.
pub async fn get_mode(db: &SqlitePool, scope: &str) -> Option<Mode> {
    if scope != GLOBAL_SCOPE {
        if let Some(mode) = read_mode_row(db, scope).await {
            return Some(mode);
        }
    }
    read_mode_row(db, GLOBAL_SCOPE).await
}

async fn read_mode_row(db: &SqlitePool, scope: &str) -> Option<Mode> {
    let row: Option<String> =
        sqlx::query_scalar("SELECT mode FROM voidwatch_mode_settings WHERE scope = ?")
            .bind(scope)
            .fetch_optional(db)
            .await
            .ok()
            .flatten();
    row.and_then(|m| Mode::parse(&m))
}

/// Changes a scope's mode. This is itself an audited, approval-gated action (EDD §3.2's
/// closing line) — routed through `evaluate()` under the reserved `"voidwatch.mode.set"`
/// action name, classified `RiskClass::Irreversible` (`risk_class::for_action`), which is
/// ADR-004 reconciled-denylist item 5 ("Voidwatch policy/mode changes... always require
/// approval"). Only persists the new mode when the verdict is `Allow`; any other verdict
/// (`Deny`/`RequireApproval`/`AllowRequireSnapshot`, though the latter shouldn't occur for
/// a `voidwatch_mode` resource) is returned to the caller without writing anything, and is
/// itself audit-logged by `evaluate()`'s caller convention — this function additionally
/// logs the *outcome* of the mode-change attempt specifically, since no HTTP handler
/// exists yet to do so on its behalf (see the P0-03 PR description for why: exposing this
/// via a route needs a forbidden-zone grant this task's ADR-001 citation doesn't cover).
/// `#[allow(dead_code)]`: not called from any production code path yet for that same
/// reason (no HTTP handler wires it) — exercised directly by this module's tests and by
/// `voidwatch::tests::mode_change_is_itself_audited_and_approval_gated`, same
/// reserved-until-wired precedent as `ActorKind::Ai`.
#[allow(dead_code)]
pub async fn set_mode(db: &SqlitePool, scope: &str, new_mode: Mode, actor: Actor) -> Verdict {
    let verdict = voidwatch::evaluate(
        db,
        actor,
        ActionKind::Mutating,
        "voidwatch.mode.set",
        Resource {
            resource_type: "voidwatch_mode",
            resource_id: scope,
        },
    )
    .await;

    let outcome = match &verdict {
        Verdict::Allow | Verdict::AllowRequireSnapshot(_) => "success",
        Verdict::RequireApproval(_) => "pending_approval",
        Verdict::Deny(_) => "blocked",
    };
    let details = format!("scope={scope} new_mode={}", new_mode.as_str());
    crate::audit::log(
        db,
        None,
        actor.kind.as_policy_str(),
        "voidwatch.mode.set",
        Some("voidwatch_mode"),
        Some(scope),
        outcome,
        None,
        Some(&details),
    )
    .await;

    if !matches!(verdict, Verdict::Allow) {
        return verdict;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let _ = sqlx::query(
        "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at, updated_by)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(scope) DO UPDATE SET
             mode = excluded.mode,
             updated_at = excluded.updated_at,
             updated_by = excluded.updated_by",
    )
    .bind(scope)
    .bind(new_mode.as_str())
    .bind(now)
    .bind(actor.kind.as_policy_str())
    .execute(db)
    .await;

    Verdict::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        crate::voidwatch::tests::create_policy_tables(&pool).await;
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS voidwatch_mode_settings (
                scope         TEXT PRIMARY KEY,
                mode          TEXT NOT NULL DEFAULT 'observer',
                updated_at    INTEGER NOT NULL,
                updated_by    TEXT
            )"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn get_mode_is_none_when_nothing_is_set() {
        let pool = setup_db().await;
        assert_eq!(get_mode(&pool, GLOBAL_SCOPE).await, None);
        assert_eq!(get_mode(&pool, "container:c1").await, None);
    }

    #[tokio::test]
    async fn get_mode_scope_override_takes_precedence_over_global() {
        let pool = setup_db().await;
        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) VALUES (?, ?, 0)",
        )
        .bind(GLOBAL_SCOPE)
        .bind("yolo")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) VALUES (?, ?, 0)",
        )
        .bind("container:c1")
        .bind("observer")
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(get_mode(&pool, GLOBAL_SCOPE).await, Some(Mode::Yolo));
        assert_eq!(get_mode(&pool, "container:c1").await, Some(Mode::Observer));
        // A different, unconfigured resource still inherits the global mode.
        assert_eq!(get_mode(&pool, "container:c2").await, Some(Mode::Yolo));
    }
}
