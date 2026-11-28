use crate::{
    api::mcp::action_registry,
    backups::{self, BackupConfig},
    operations::{
        contracts::{CapabilityAvailability, ResourceRef},
        invocation::{self, InvocationContext, InvocationError},
        resources::{self, ObserveResource},
    },
};
use sqlx::SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum BackupAdoptionError {
    #[error(transparent)]
    Invocation(#[from] InvocationError),
    #[error("backup configuration not found")]
    ConfigNotFound,
    #[error("restic is not installed")]
    ResticUnavailable,
    #[error("backup target adoption failed")]
    Internal(#[source] anyhow::Error),
}

#[derive(Debug, Clone, Copy)]
pub enum BackupSelector<'a> {
    Id(&'a str),
    Name(&'a str),
}

#[derive(Debug, Clone)]
pub struct AdoptedBackupConfig {
    pub resource: ResourceRef,
    pub config: BackupConfig,
}

pub fn authorize(
    context: &InvocationContext,
    action_name: &str,
) -> Result<(), BackupAdoptionError> {
    let action = action_registry::action(action_name).ok_or(InvocationError::UnknownAction)?;
    invocation::authorize_action(action, context)?;
    Ok(())
}

pub async fn resolve_create_target(
    pool: &SqlitePool,
    context: &InvocationContext,
) -> Result<ResourceRef, BackupAdoptionError> {
    const ACTION: &str = "backup.config.create";
    authorize(context, ACTION)?;
    let resource = resources::resolve_alias(pool, "voidtower.singleton", "local", "system")
        .await
        .map_err(BackupAdoptionError::Internal)?
        .ok_or(InvocationError::ResourceNotFound)?;
    if resource.kind != "system" {
        return Err(InvocationError::ResourceKindMismatch.into());
    }
    publish_capability(pool, &resource.id, ACTION).await?;
    Ok(resource)
}

pub async fn resolve_config_target(
    pool: &SqlitePool,
    context: &InvocationContext,
    selector: BackupSelector<'_>,
    action_name: &str,
) -> Result<AdoptedBackupConfig, BackupAdoptionError> {
    authorize(context, action_name)?;
    if requires_restic(action_name) && !backups::is_restic_available() {
        return Err(BackupAdoptionError::ResticUnavailable);
    }
    let config = match selector {
        BackupSelector::Id(id) => backups::get_config(pool, id)
            .await
            .map_err(BackupAdoptionError::Internal)?,
        BackupSelector::Name(name) => sqlx::query_as::<_, BackupConfig>(&format!(
            "SELECT {} FROM backup_configs WHERE name = ?",
            backups::SELECT_COLS
        ))
        .bind(name)
        .fetch_optional(pool)
        .await
        .map_err(|error| BackupAdoptionError::Internal(error.into()))?,
    }
    .ok_or(BackupAdoptionError::ConfigNotFound)?;

    let correlation_id = uuid::Uuid::new_v4().to_string();
    let resource = resources::observe(
        pool,
        ObserveResource {
            kind: "backup_config",
            display_name: &config.name,
            node_id: None,
            provider: Some("restic"),
            namespace: "voidtower.backup_config",
            scope_key: "local",
            alias: &config.id,
        },
        Some(context.actor()),
        &correlation_id,
    )
    .await
    .map_err(BackupAdoptionError::Internal)?;
    resources::set_capability(
        pool,
        &resource.id,
        action_name,
        CapabilityAvailability::Available,
        None,
        None,
        &correlation_id,
    )
    .await
    .map_err(BackupAdoptionError::Internal)?;
    Ok(AdoptedBackupConfig { resource, config })
}

async fn publish_capability(
    pool: &SqlitePool,
    resource_id: &str,
    action_name: &str,
) -> Result<(), BackupAdoptionError> {
    resources::set_capability(
        pool,
        resource_id,
        action_name,
        CapabilityAvailability::Available,
        None,
        None,
        &uuid::Uuid::new_v4().to_string(),
    )
    .await
    .map(|_| ())
    .map_err(BackupAdoptionError::Internal)
}

fn requires_restic(action_name: &str) -> bool {
    matches!(
        action_name,
        "backup.run" | "backup.check" | "backup.restore_test"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = crate::api::mcp::test_support::setup_db().await;
        resources::observe(
            &pool,
            ObserveResource {
                kind: "system",
                display_name: "This VoidTower",
                node_id: None,
                provider: Some("local"),
                namespace: "voidtower.singleton",
                scope_key: "local",
                alias: "system",
            },
            None,
            "seed",
        )
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn create_resolves_seeded_system_and_publishes_capability() {
        let pool = pool().await;
        let resource = resolve_create_target(&pool, &InvocationContext::LocalCli)
            .await
            .unwrap();
        assert_eq!(resource.kind, "system");
        let capability = resources::capability(&pool, &resource.id, "backup.config.create")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(capability.availability, "available");
    }

    #[tokio::test]
    async fn existing_config_is_observed_under_normalized_alias_after_authorization() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO backup_configs \
             (id, name, source_path, repo_path, retention_days, enabled, created_at) \
             VALUES ('config-1', 'Daily backup', '/srv/data', '/srv/restic', 30, 1, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let adopted = resolve_config_target(
            &pool,
            &InvocationContext::LocalCli,
            BackupSelector::Id("config-1"),
            "backup.config.delete",
        )
        .await
        .unwrap();
        assert_eq!(adopted.config.id, "config-1");
        assert_eq!(adopted.resource.kind, "backup_config");
        let resolved =
            resources::resolve_alias(&pool, "voidtower.backup_config", "local", "config-1")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(resolved.id, adopted.resource.id);
        let capability = resources::capability(&pool, &adopted.resource.id, "backup.config.delete")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(capability.availability, "available");
    }

    #[tokio::test]
    async fn ingress_denial_precedes_config_lookup_and_observation() {
        let pool = pool().await;
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources")
            .fetch_one(&pool)
            .await
            .unwrap();
        let error = resolve_config_target(
            &pool,
            &InvocationContext::Scheduler,
            BackupSelector::Id("missing"),
            "backup.config.delete",
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            BackupAdoptionError::Invocation(InvocationError::IngressDenied)
        ));
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, after);
    }
}
