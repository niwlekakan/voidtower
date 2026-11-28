//! Household membership and resource-grant authorization.
//!
//! This module is deliberately additive to the existing Authentik/OIDC-backed user identity. A
//! household is a local authorization boundary; it never creates a second user identity or a
//! second resource identity.

use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantPermission {
    Read,
    Operate,
    Admin,
}

impl GrantPermission {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "read" => Some(Self::Read),
            "operate" => Some(Self::Operate),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    const fn rank(self) -> i64 {
        match self {
            Self::Read => 1,
            Self::Operate => 2,
            Self::Admin => 3,
        }
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn globally_unrestricted(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "operator")
}

pub async fn has_resource_permission(
    pool: &SqlitePool,
    user_id: &str,
    role: &str,
    resource_id: &str,
    permission: GrantPermission,
) -> Result<bool, sqlx::Error> {
    if globally_unrestricted(role) {
        return Ok(true);
    }
    let now = unix_now();
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM resource_grants g
             JOIN household_memberships m
               ON m.household_id = g.household_id AND m.user_id = g.user_id
             WHERE g.user_id = ?
               AND g.resource_id = ?
               AND m.status = 'active'
               AND m.revoked_at IS NULL
               AND (m.expires_at IS NULL OR m.expires_at > ?)
               AND g.revoked_at IS NULL
               AND (g.expires_at IS NULL OR g.expires_at > ?)
               AND CASE g.permission
                     WHEN 'admin' THEN 3
                     WHEN 'operate' THEN 2
                     WHEN 'read' THEN 1
                     ELSE 0
                   END >= ?
         )",
    )
    .bind(user_id)
    .bind(resource_id)
    .bind(now)
    .bind(now)
    .bind(permission.rank())
    .fetch_one(pool)
    .await?;
    Ok(allowed != 0)
}

pub async fn can_manage_household(
    pool: &SqlitePool,
    user_id: &str,
    role: &str,
    household_id: &str,
) -> Result<bool, sqlx::Error> {
    if matches!(role, "owner" | "admin") {
        return Ok(true);
    }
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM household_memberships
             WHERE household_id = ? AND user_id = ? AND role IN ('owner', 'admin')
               AND status = 'active' AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > ?)
         )",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(unix_now())
    .fetch_one(pool)
    .await?;
    Ok(allowed != 0)
}

pub async fn is_active_member(
    pool: &SqlitePool,
    user_id: &str,
    household_id: &str,
) -> Result<bool, sqlx::Error> {
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM household_memberships
             WHERE household_id = ? AND user_id = ? AND status = 'active'
               AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)
         )",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(unix_now())
    .fetch_one(pool)
    .await?;
    Ok(allowed != 0)
}

/// Revokes one resource grant and cancels the subject's non-terminal jobs for that resource.
///
/// New intent is refused by the canonical grant check, but work already accepted must not be
/// approved or executed after the authority that authorized it is gone, so revocation reconciles
/// those jobs in the same transaction. Only human actors can reach a grant-scoped action, so the
/// cancellation is scoped to the human subject rather than every token of the account.
pub async fn revoke_resource_grant(
    pool: &SqlitePool,
    household_id: &str,
    resource_id: &str,
    user_id: &str,
    grant_scoped_actions: &[&str],
) -> Result<bool, sqlx::Error> {
    let now = unix_now();
    let mut tx = pool.begin().await?;
    let changed = sqlx::query(
        "UPDATE resource_grants SET revoked_at = ?, updated_at = ?
         WHERE household_id = ? AND resource_id = ? AND user_id = ? AND revoked_at IS NULL",
    )
    .bind(now)
    .bind(now)
    .bind(household_id)
    .bind(resource_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(false);
    }
    // Only actions a grant could have authorized are reconciled: an operator's job on the same
    // resource never depended on this grant and must not be cancelled by its revocation.
    if !grant_scoped_actions.is_empty() {
        let placeholders = vec!["?"; grant_scoped_actions.len()].join(", ");
        let sql = format!(
            "UPDATE jobs SET state = 'cancelled', cancel_requested = 1,
                    error_code = 'household_grant_revoked',
                    error_message = 'The household resource grant was revoked',
                    finished_at = ?, updated_at = ?
             WHERE resource_id = ?
               AND state IN ('awaiting_approval', 'queued', 'running', 'needs_attention')
               AND actor_type = 'human' AND actor_id = ?
               AND action IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql)
            .bind(now)
            .bind(now)
            .bind(resource_id)
            .bind(user_id);
        for action in grant_scoped_actions {
            query = query.bind(*action);
        }
        query.execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(true)
}

pub async fn revoke_membership(
    pool: &SqlitePool,
    household_id: &str,
    user_id: &str,
) -> Result<bool, sqlx::Error> {
    let now = unix_now();
    let mut tx = pool.begin().await?;
    let token_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM api_tokens WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(&mut *tx)
        .await?;

    let changed = sqlx::query(
        "UPDATE household_memberships
         SET status = 'revoked', revoked_at = ?, updated_at = ?
         WHERE household_id = ? AND user_id = ? AND status = 'active'",
    )
    .bind(now)
    .bind(now)
    .bind(household_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(false);
    }

    sqlx::query(
        "UPDATE jobs SET state = 'cancelled', cancel_requested = 1,
                error_code = 'household_membership_revoked',
                error_message = 'Household membership was revoked',
                finished_at = ?, updated_at = ?
         WHERE state IN ('awaiting_approval', 'queued', 'running', 'needs_attention')
           AND ((actor_type = 'human' AND actor_id = ?)
             OR (actor_type = 'api_token' AND actor_id IN (
                 SELECT id FROM api_tokens WHERE user_id = ?
             )))",
    )
    .bind(now)
    .bind(now)
    .bind(user_id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?;

    // Token ids are intentionally queried before deletion for the audit-safe update above. The
    // token rows and all compatibility sessions are then removed so cached bearer credentials
    // cannot be used to recreate access after revocation.
    let _ = token_ids;
    sqlx::query("DELETE FROM api_tokens WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_have_a_monotonic_least_privilege_order() {
        assert!(GrantPermission::Read.rank() < GrantPermission::Operate.rank());
        assert!(GrantPermission::Operate.rank() < GrantPermission::Admin.rank());
        assert!(!globally_unrestricted("member"));
        assert!(globally_unrestricted("operator"));
    }
}
