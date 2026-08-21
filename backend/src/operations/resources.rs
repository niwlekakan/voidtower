use super::{
    contracts::{ActorRef, CapabilityAvailability, ResourceAlias, ResourceCapability, ResourceRef},
    events::{self, PendingEvent},
    unix_now,
};
use anyhow::{bail, Result};
use sqlx::SqlitePool;

type ObservedResourceRow = (String, String, String, i64, Option<String>, Option<String>);

#[derive(Debug, Clone)]
pub struct ObserveResource<'a> {
    pub kind: &'a str,
    pub display_name: &'a str,
    pub node_id: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub namespace: &'a str,
    pub scope_key: &'a str,
    pub alias: &'a str,
}

pub async fn observe(
    pool: &SqlitePool,
    observed: ObserveResource<'_>,
    actor: Option<ActorRef>,
    correlation_id: &str,
) -> Result<ResourceRef> {
    let mut transaction = pool.begin().await?;
    let existing: Option<ObservedResourceRow> = sqlx::query_as(
        "SELECT r.id, r.kind, r.display_name, r.revision, r.node_id, r.provider \
             FROM resources r JOIN resource_aliases a ON a.resource_id = r.id \
             WHERE a.namespace = ? AND a.scope_key = ? AND a.value = ?",
    )
    .bind(observed.namespace)
    .bind(observed.scope_key)
    .bind(observed.alias)
    .fetch_optional(&mut *transaction)
    .await?;
    let now = unix_now();

    let resource = if let Some((id, kind, display_name, revision, node_id, provider)) = existing {
        if kind != observed.kind {
            bail!(
                "resource alias {}:{}:{} already belongs to kind {kind}",
                observed.namespace,
                observed.scope_key,
                observed.alias
            );
        }
        let changed = display_name != observed.display_name
            || node_id.as_deref() != observed.node_id
            || provider.as_deref() != observed.provider;
        let next_revision = revision + i64::from(changed);
        sqlx::query(
            "UPDATE resources SET display_name = ?, node_id = ?, provider = ?, \
             lifecycle_state = 'active', revision = ?, updated_at = ? WHERE id = ?",
        )
        .bind(observed.display_name)
        .bind(observed.node_id)
        .bind(observed.provider)
        .bind(next_revision)
        .bind(now)
        .bind(&id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE resource_aliases SET last_seen_at = ? \
             WHERE resource_id = ? AND namespace = ? AND scope_key = ? AND value = ?",
        )
        .bind(now)
        .bind(&id)
        .bind(observed.namespace)
        .bind(observed.scope_key)
        .bind(observed.alias)
        .execute(&mut *transaction)
        .await?;
        if changed {
            events::append(
                &mut transaction,
                PendingEvent {
                    event_type: "resource.updated.v1".into(),
                    actor: actor.clone(),
                    resource_id: Some(id.clone()),
                    job_id: None,
                    approval_id: None,
                    correlation_id: correlation_id.into(),
                    causation_id: None,
                    payload: serde_json::json!({"revision": next_revision}),
                },
            )
            .await?;
        }
        ResourceRef {
            id,
            kind,
            display_name: observed.display_name.into(),
            revision: next_revision,
        }
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO resources \
             (id, kind, display_name, node_id, provider, lifecycle_state, revision, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'active', 0, ?, ?)",
        )
        .bind(&id)
        .bind(observed.kind)
        .bind(observed.display_name)
        .bind(observed.node_id)
        .bind(observed.provider)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO resource_aliases \
             (resource_id, namespace, scope_key, value, created_at, last_seen_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(observed.namespace)
        .bind(observed.scope_key)
        .bind(observed.alias)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        events::append(
            &mut transaction,
            PendingEvent {
                event_type: "resource.registered.v1".into(),
                actor,
                resource_id: Some(id.clone()),
                job_id: None,
                approval_id: None,
                correlation_id: correlation_id.into(),
                causation_id: None,
                payload: serde_json::json!({"kind": observed.kind}),
            },
        )
        .await?;
        ResourceRef {
            id,
            kind: observed.kind.into(),
            display_name: observed.display_name.into(),
            revision: 0,
        }
    };

    transaction.commit().await?;
    Ok(resource)
}

pub async fn resolve_alias(
    pool: &SqlitePool,
    namespace: &str,
    scope_key: &str,
    value: &str,
) -> Result<Option<ResourceRef>> {
    Ok(sqlx::query_as(
        "SELECT r.id, r.kind, r.display_name, r.revision \
         FROM resources r JOIN resource_aliases a ON a.resource_id = r.id \
         WHERE a.namespace = ? AND a.scope_key = ? AND a.value = ?",
    )
    .bind(namespace)
    .bind(scope_key)
    .bind(value)
    .fetch_optional(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool, limit: i64) -> Result<Vec<ResourceRef>> {
    Ok(sqlx::query_as(
        "SELECT id, kind, display_name, revision FROM resources \
         WHERE lifecycle_state != 'retired' ORDER BY kind, display_name, id LIMIT ?",
    )
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, resource_id: &str) -> Result<Option<ResourceRef>> {
    Ok(
        sqlx::query_as("SELECT id, kind, display_name, revision FROM resources WHERE id = ?")
            .bind(resource_id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn get_active(pool: &SqlitePool, resource_id: &str) -> Result<Option<ResourceRef>> {
    Ok(sqlx::query_as(
        "SELECT id, kind, display_name, revision FROM resources \
         WHERE id = ? AND lifecycle_state = 'active'",
    )
    .bind(resource_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn capability(
    pool: &SqlitePool,
    resource_id: &str,
    action: &str,
) -> Result<Option<ResourceCapability>> {
    Ok(sqlx::query_as(
        "SELECT resource_id, action, availability, reason_code, detail, schema_version, observed_at \
         FROM resource_capabilities WHERE resource_id = ? AND action = ?",
    )
    .bind(resource_id)
    .bind(action)
    .fetch_optional(pool)
    .await?)
}

pub async fn aliases(pool: &SqlitePool, resource_id: &str) -> Result<Vec<ResourceAlias>> {
    Ok(sqlx::query_as(
        "SELECT resource_id, namespace, scope_key, value FROM resource_aliases \
         WHERE resource_id = ? ORDER BY namespace, scope_key, value",
    )
    .bind(resource_id)
    .fetch_all(pool)
    .await?)
}

pub async fn capabilities(pool: &SqlitePool, resource_id: &str) -> Result<Vec<ResourceCapability>> {
    Ok(sqlx::query_as(
        "SELECT resource_id, action, availability, reason_code, detail, schema_version, observed_at \
         FROM resource_capabilities WHERE resource_id = ? ORDER BY action",
    )
    .bind(resource_id)
    .fetch_all(pool)
    .await?)
}

pub async fn set_capability(
    pool: &SqlitePool,
    resource_id: &str,
    action: &str,
    availability: CapabilityAvailability,
    reason_code: Option<&str>,
    detail: Option<&str>,
    correlation_id: &str,
) -> Result<ResourceCapability> {
    let availability = match availability {
        CapabilityAvailability::Available => "available",
        CapabilityAvailability::Unavailable => "unavailable",
        CapabilityAvailability::Unknown => "unknown",
    };
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO resource_capabilities \
         (resource_id, action, availability, reason_code, detail, schema_version, observed_at) \
         VALUES (?, ?, ?, ?, ?, 1, ?) \
         ON CONFLICT(resource_id, action) DO UPDATE SET availability = excluded.availability, \
         reason_code = excluded.reason_code, detail = excluded.detail, observed_at = excluded.observed_at",
    )
    .bind(resource_id)
    .bind(action)
    .bind(availability)
    .bind(reason_code)
    .bind(detail)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: "capability.changed.v1".into(),
            actor: None,
            resource_id: Some(resource_id.into()),
            job_id: None,
            approval_id: None,
            correlation_id: correlation_id.into(),
            causation_id: None,
            payload: serde_json::json!({
                "action": action,
                "availability": availability,
                "reason_code": reason_code,
            }),
        },
    )
    .await?;
    transaction.commit().await?;

    Ok(ResourceCapability {
        resource_id: resource_id.into(),
        action: action.into(),
        availability: availability.into(),
        reason_code: reason_code.map(str::to_owned),
        detail: detail.map(str::to_owned),
        schema_version: 1,
        observed_at: now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn observation_preserves_uuid_and_scopes_aliases() {
        let pool = pool().await;
        let first = observe(
            &pool,
            ObserveResource {
                kind: "container",
                display_name: "one",
                node_id: None,
                provider: Some("docker"),
                namespace: "docker.container",
                scope_key: "engine-a",
                alias: "same-native-id",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let again = observe(
            &pool,
            ObserveResource {
                display_name: "renamed",
                ..ObserveResource {
                    kind: "container",
                    display_name: "one",
                    node_id: None,
                    provider: Some("docker"),
                    namespace: "docker.container",
                    scope_key: "engine-a",
                    alias: "same-native-id",
                }
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let other_scope = observe(
            &pool,
            ObserveResource {
                kind: "container",
                display_name: "two",
                node_id: None,
                provider: Some("docker"),
                namespace: "docker.container",
                scope_key: "engine-b",
                alias: "same-native-id",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        assert_eq!(first.id, again.id);
        assert_eq!(again.revision, 1);
        assert_ne!(first.id, other_scope.id);
    }
}
