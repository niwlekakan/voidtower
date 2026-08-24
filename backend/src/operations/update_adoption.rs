use crate::{
    api::mcp::action_registry,
    operations::{
        contracts::{CapabilityAvailability, ResourceRef},
        invocation::{self, InvocationContext, InvocationError},
        resources::{self, ObserveResource},
    },
    updates::{self, UpdateSnapshot, UpdateTarget},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use sqlx::SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum UpdateAdoptionError {
    #[error(transparent)]
    Invocation(#[from] InvocationError),
    #[error("update target is unavailable: {0}")]
    Unavailable(String),
    #[error("update target adoption failed")]
    Internal(#[source] anyhow::Error),
}

#[derive(Debug, Clone)]
pub struct AdoptedUpdateTarget {
    pub resource: ResourceRef,
}

#[async_trait]
trait UpdateEvidenceProvider: Send + Sync {
    async fn snapshot(&self, target: &UpdateTarget) -> Result<UpdateSnapshot>;
}

struct HostUpdateEvidenceProvider;

#[async_trait]
impl UpdateEvidenceProvider for HostUpdateEvidenceProvider {
    async fn snapshot(&self, target: &UpdateTarget) -> Result<UpdateSnapshot> {
        updates::snapshot(target).await
    }
}

pub fn authorize(
    context: &InvocationContext,
    action_name: &str,
) -> Result<(), UpdateAdoptionError> {
    let action = action_registry::action(action_name).ok_or(InvocationError::UnknownAction)?;
    invocation::authorize_action(action, context)?;
    Ok(())
}

pub async fn resolve_target(
    pool: &SqlitePool,
    context: &InvocationContext,
    action_name: &str,
    container_selector: Option<&str>,
) -> Result<AdoptedUpdateTarget, UpdateAdoptionError> {
    resolve_target_with_provider(
        pool,
        context,
        action_name,
        container_selector,
        &HostUpdateEvidenceProvider,
    )
    .await
}

async fn resolve_target_with_provider(
    pool: &SqlitePool,
    context: &InvocationContext,
    action_name: &str,
    container_selector: Option<&str>,
    provider: &dyn UpdateEvidenceProvider,
) -> Result<AdoptedUpdateTarget, UpdateAdoptionError> {
    authorize(context, action_name)?;
    let target = target_for_action(action_name, container_selector)?;
    let snapshot = provider
        .snapshot(&target)
        .await
        .map_err(|error| UpdateAdoptionError::Unavailable(safe_text(&error.to_string())))?;
    let resource = adopt_snapshot(pool, context, action_name, &target, &snapshot).await?;
    Ok(AdoptedUpdateTarget { resource })
}

fn target_for_action(
    action_name: &str,
    container_selector: Option<&str>,
) -> Result<UpdateTarget, UpdateAdoptionError> {
    Ok(match action_name {
        "update.voidtower.check" | "update.voidtower.apply" | "update.voidtower.rollback" => {
            if container_selector.is_some() {
                return Err(InvocationError::ResourceKindMismatch.into());
            }
            UpdateTarget::VoidTower
        }
        "update.odysseus.apply" => {
            if container_selector.is_some() {
                return Err(InvocationError::ResourceKindMismatch.into());
            }
            UpdateTarget::Odysseus
        }
        "update.docker.check" => {
            if container_selector.is_some() {
                return Err(InvocationError::ResourceKindMismatch.into());
            }
            UpdateTarget::DockerEngine
        }
        "update.docker.apply" => UpdateTarget::DockerImage {
            container_id: container_selector
                .filter(|selector| !selector.trim().is_empty())
                .ok_or(InvocationError::ResourceNotFound)?
                .to_owned(),
        },
        "update.os.apply" => {
            if container_selector.is_some() {
                return Err(InvocationError::ResourceKindMismatch.into());
            }
            UpdateTarget::OperatingSystem
        }
        _ => return Err(InvocationError::UnknownAction.into()),
    })
}

async fn adopt_snapshot(
    pool: &SqlitePool,
    context: &InvocationContext,
    action_name: &str,
    target: &UpdateTarget,
    snapshot: &UpdateSnapshot,
) -> Result<ResourceRef, UpdateAdoptionError> {
    if action_name == "update.voidtower.rollback"
        && !matches!(snapshot, UpdateSnapshot::VoidTowerGit(_))
    {
        return Err(UpdateAdoptionError::Unavailable(
            "VoidTower rollback is only available for source installations".into(),
        ));
    }
    let (kind, namespace, alias) = match (target, snapshot) {
        (
            UpdateTarget::VoidTower,
            UpdateSnapshot::VoidTowerGit(_)
            | UpdateSnapshot::VoidTowerBinary { .. }
            | UpdateSnapshot::VoidTowerDocker(_),
        ) => ("update_target", "voidtower.update_target", "voidtower"),
        (UpdateTarget::Odysseus, UpdateSnapshot::Odysseus(snapshot)) => {
            if !snapshot.installed {
                return Err(UpdateAdoptionError::Unavailable(
                    "Odysseus is not installed".into(),
                ));
            }
            ("update_target", "voidtower.update_target", "odysseus")
        }
        (UpdateTarget::DockerEngine, UpdateSnapshot::DockerEngine { .. }) => {
            ("docker_engine", "voidtower.singleton", "docker")
        }
        (UpdateTarget::OperatingSystem, UpdateSnapshot::OperatingSystem { .. }) => {
            ("update_target", "voidtower.update_target", "os")
        }
        (UpdateTarget::DockerImage { .. }, UpdateSnapshot::DockerImage(container)) => {
            let correlation_id = uuid::Uuid::new_v4().to_string();
            let resource = resources::observe(
                pool,
                ObserveResource {
                    kind: "container_image",
                    display_name: &container.container_name,
                    node_id: None,
                    provider: Some("docker"),
                    namespace: "docker.container_image",
                    scope_key: "local",
                    alias: &container.container_id,
                },
                Some(context.actor()),
                &correlation_id,
            )
            .await
            .map_err(UpdateAdoptionError::Internal)?;
            publish_capability(pool, &resource.id, action_name, &correlation_id).await?;
            return Ok(resource);
        }
        _ => {
            return Err(UpdateAdoptionError::Internal(anyhow!(
                "update provider returned a snapshot for another target"
            )))
        }
    };

    let resource = resources::resolve_alias(pool, namespace, "local", alias)
        .await
        .map_err(UpdateAdoptionError::Internal)?
        .ok_or(InvocationError::ResourceNotFound)?;
    if resource.kind != kind {
        return Err(InvocationError::ResourceKindMismatch.into());
    }
    publish_capability(
        pool,
        &resource.id,
        action_name,
        &uuid::Uuid::new_v4().to_string(),
    )
    .await?;
    Ok(resource)
}

async fn publish_capability(
    pool: &SqlitePool,
    resource_id: &str,
    action_name: &str,
    correlation_id: &str,
) -> Result<(), UpdateAdoptionError> {
    resources::set_capability(
        pool,
        resource_id,
        action_name,
        CapabilityAvailability::Available,
        None,
        None,
        correlation_id,
    )
    .await
    .map(|_| ())
    .map_err(UpdateAdoptionError::Internal)
}

fn safe_text(value: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(value.trim());
    redacted.chars().take(512).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updates::{DockerContainerSnapshot, GitSnapshot};
    use std::sync::Mutex;

    struct FakeProvider {
        snapshot: UpdateSnapshot,
        calls: Mutex<Vec<UpdateTarget>>,
    }

    #[async_trait]
    impl UpdateEvidenceProvider for FakeProvider {
        async fn snapshot(&self, target: &UpdateTarget) -> Result<UpdateSnapshot> {
            self.calls.lock().unwrap().push(target.clone());
            Ok(self.snapshot.clone())
        }
    }

    async fn pool() -> SqlitePool {
        let pool = crate::api::mcp::test_support::setup_db().await;
        for (display_name, alias) in [
            ("VoidTower Update Target", "voidtower"),
            ("Odysseus Update Target", "odysseus"),
            ("Operating System Update Target", "os"),
        ] {
            resources::observe(
                &pool,
                ObserveResource {
                    kind: "update_target",
                    display_name,
                    node_id: None,
                    provider: Some("local"),
                    namespace: "voidtower.update_target",
                    scope_key: "local",
                    alias,
                },
                None,
                "seed",
            )
            .await
            .unwrap();
        }
        resources::observe(
            &pool,
            ObserveResource {
                kind: "docker_engine",
                display_name: "Local Docker",
                node_id: None,
                provider: Some("local"),
                namespace: "voidtower.singleton",
                scope_key: "local",
                alias: "docker",
            },
            None,
            "seed",
        )
        .await
        .unwrap();
        pool
    }

    fn admin() -> InvocationContext {
        InvocationContext::Session {
            user_id: "admin-1".into(),
            role: "admin".into(),
        }
    }

    fn git_snapshot(installed: bool) -> GitSnapshot {
        GitSnapshot {
            installed,
            branch: "main".into(),
            current_commit: "1111111".into(),
            remote_commit: "2222222".into(),
            behind: 1,
            ahead: 0,
            backup_tags: vec![],
        }
    }

    #[tokio::test]
    async fn seeded_target_is_resolved_and_only_requested_capability_is_published() {
        let pool = pool().await;
        let provider = FakeProvider {
            snapshot: UpdateSnapshot::VoidTowerGit(git_snapshot(true)),
            calls: Mutex::new(Vec::new()),
        };
        let adopted = resolve_target_with_provider(
            &pool,
            &admin(),
            "update.voidtower.apply",
            None,
            &provider,
        )
        .await
        .unwrap();
        assert_eq!(adopted.resource.kind, "update_target");
        let capability =
            resources::capability(&pool, &adopted.resource.id, "update.voidtower.apply")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(capability.availability, "available");
        assert!(
            resources::capability(&pool, &adopted.resource.id, "update.voidtower.rollback")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn docker_selector_is_normalized_to_full_container_image_alias() {
        let pool = pool().await;
        let provider = FakeProvider {
            snapshot: UpdateSnapshot::DockerImage(DockerContainerSnapshot {
                container_id: "abcdef0123456789".into(),
                container_name: "media".into(),
                image: "example/media:latest".into(),
                container_image_id: "sha256:old".into(),
                local_image_id: "sha256:new".into(),
                compose_project: String::new(),
                compose_file: String::new(),
                compose_service: String::new(),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let adopted = resolve_target_with_provider(
            &pool,
            &admin(),
            "update.docker.apply",
            Some("abcdef"),
            &provider,
        )
        .await
        .unwrap();
        let resolved =
            resources::resolve_alias(&pool, "docker.container_image", "local", "abcdef0123456789")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(resolved.id, adopted.resource.id);
        assert_eq!(resolved.kind, "container_image");
    }

    #[tokio::test]
    async fn authorization_denial_precedes_provider_and_resource_observation() {
        let pool = pool().await;
        let provider = FakeProvider {
            snapshot: UpdateSnapshot::DockerImage(DockerContainerSnapshot {
                container_id: "abcdef0123456789".into(),
                container_name: "media".into(),
                image: "example/media:latest".into(),
                container_image_id: "sha256:old".into(),
                local_image_id: "sha256:new".into(),
                compose_project: String::new(),
                compose_file: String::new(),
                compose_service: String::new(),
            }),
            calls: Mutex::new(Vec::new()),
        };
        let viewer = InvocationContext::Session {
            user_id: "viewer-1".into(),
            role: "viewer".into(),
        };
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources")
            .fetch_one(&pool)
            .await
            .unwrap();
        let error = resolve_target_with_provider(
            &pool,
            &viewer,
            "update.docker.apply",
            Some("abcdef"),
            &provider,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            UpdateAdoptionError::Invocation(InvocationError::Forbidden)
        ));
        assert!(provider.calls.lock().unwrap().is_empty());
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn absent_odysseus_never_publishes_apply_capability() {
        let pool = pool().await;
        let provider = FakeProvider {
            snapshot: UpdateSnapshot::Odysseus(git_snapshot(false)),
            calls: Mutex::new(Vec::new()),
        };
        let error =
            resolve_target_with_provider(&pool, &admin(), "update.odysseus.apply", None, &provider)
                .await
                .unwrap_err();
        assert!(matches!(error, UpdateAdoptionError::Unavailable(_)));
        let resource =
            resources::resolve_alias(&pool, "voidtower.update_target", "local", "odysseus")
                .await
                .unwrap()
                .unwrap();
        assert!(
            resources::capability(&pool, &resource.id, "update.odysseus.apply")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn rollback_capability_is_not_published_for_non_source_installations() {
        let pool = pool().await;
        let provider = FakeProvider {
            snapshot: UpdateSnapshot::VoidTowerBinary {
                current_version: "0.9.0".into(),
                remote_version: "1.0.0".into(),
            },
            calls: Mutex::new(Vec::new()),
        };
        let error = resolve_target_with_provider(
            &pool,
            &admin(),
            "update.voidtower.rollback",
            None,
            &provider,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, UpdateAdoptionError::Unavailable(_)));
        let resource =
            resources::resolve_alias(&pool, "voidtower.update_target", "local", "voidtower")
                .await
                .unwrap()
                .unwrap();
        assert!(
            resources::capability(&pool, &resource.id, "update.voidtower.rollback")
                .await
                .unwrap()
                .is_none()
        );
    }
}
