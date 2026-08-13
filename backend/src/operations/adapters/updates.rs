//! Durable host, application, image, and operating-system update adapter.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::mcp::action_registry::{self, RiskClass},
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
    updates::{
        self as update_service, ExecutionResult, ReconciliationResult, RollbackPoint,
        UpdateRequest, UpdateSnapshot, UpdateTarget,
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;

const ACTIONS: &[&str] = &[
    "update.voidtower.check",
    "update.voidtower.apply",
    "update.voidtower.rollback",
    "update.odysseus.apply",
    "update.docker.check",
    "update.docker.apply",
    "update.os.apply",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RollbackInput {
    tag: String,
}

#[async_trait]
pub trait UpdateProvider: Send + Sync {
    async fn snapshot(&self, target: &UpdateTarget) -> Result<UpdateSnapshot>;
    async fn prepare_rollback(
        &self,
        target: &UpdateTarget,
        operation_id: &str,
    ) -> Result<RollbackPoint>;
    async fn rollback_prepared(&self, target: &UpdateTarget, operation_id: &str) -> Result<bool>;
    async fn execute(
        &self,
        target: &UpdateTarget,
        request: &UpdateRequest,
        operation_id: &str,
    ) -> Result<ExecutionResult>;
    async fn reconcile(
        &self,
        target: &UpdateTarget,
        request: &UpdateRequest,
        operation_id: &str,
    ) -> Result<ReconciliationResult>;
}

#[derive(Default)]
pub struct HostUpdateProvider;

#[async_trait]
impl UpdateProvider for HostUpdateProvider {
    async fn snapshot(&self, target: &UpdateTarget) -> Result<UpdateSnapshot> {
        update_service::snapshot(target).await
    }

    async fn prepare_rollback(
        &self,
        target: &UpdateTarget,
        operation_id: &str,
    ) -> Result<RollbackPoint> {
        update_service::prepare_rollback(target, operation_id).await
    }

    async fn rollback_prepared(&self, target: &UpdateTarget, operation_id: &str) -> Result<bool> {
        update_service::rollback_prepared(target, operation_id).await
    }

    async fn execute(
        &self,
        target: &UpdateTarget,
        request: &UpdateRequest,
        operation_id: &str,
    ) -> Result<ExecutionResult> {
        update_service::execute(target, request, operation_id).await
    }

    async fn reconcile(
        &self,
        target: &UpdateTarget,
        request: &UpdateRequest,
        operation_id: &str,
    ) -> Result<ReconciliationResult> {
        update_service::reconcile(target, request, operation_id).await
    }
}

pub struct UpdatesAdapter {
    pool: SqlitePool,
    provider: Arc<dyn UpdateProvider>,
}

impl UpdatesAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            provider: Arc::new(HostUpdateProvider),
        }
    }

    #[cfg(test)]
    fn with_provider(pool: SqlitePool, provider: Arc<dyn UpdateProvider>) -> Self {
        Self { pool, provider }
    }

    async fn target(&self, request: &PlanRequest) -> Result<UpdateTarget> {
        validate_resource_kind(&request.action, &request.resource.kind)?;
        Ok(match request.action.as_str() {
            "update.voidtower.check" | "update.voidtower.apply" | "update.voidtower.rollback" => {
                self.require_alias(&request.resource.id, "voidtower.update_target", "voidtower")
                    .await?;
                UpdateTarget::VoidTower
            }
            "update.odysseus.apply" => {
                self.require_alias(&request.resource.id, "voidtower.update_target", "odysseus")
                    .await?;
                UpdateTarget::Odysseus
            }
            "update.os.apply" => {
                self.require_alias(&request.resource.id, "voidtower.update_target", "os")
                    .await?;
                UpdateTarget::OperatingSystem
            }
            "update.docker.check" => {
                self.require_alias(&request.resource.id, "voidtower.singleton", "docker")
                    .await?;
                UpdateTarget::DockerEngine
            }
            "update.docker.apply" => UpdateTarget::DockerImage {
                container_id: self
                    .single_alias(&request.resource.id, "docker.container_image")
                    .await?,
            },
            _ => bail!("unsupported update action"),
        })
    }

    async fn require_alias(
        &self,
        resource_id: &str,
        namespace: &str,
        expected: &str,
    ) -> Result<()> {
        let actual = self.single_alias(resource_id, namespace).await?;
        ensure!(
            actual == expected,
            "update resource alias does not match action target"
        );
        Ok(())
    }

    async fn single_alias(&self, resource_id: &str, namespace: &str) -> Result<String> {
        let aliases: Vec<String> = sqlx::query_scalar(
            "SELECT value FROM resource_aliases WHERE resource_id = ? AND namespace = ? \
             ORDER BY scope_key, value",
        )
        .bind(resource_id)
        .bind(namespace)
        .fetch_all(&self.pool)
        .await?;
        ensure!(
            aliases.len() == 1,
            "update resource must have exactly one {namespace} alias"
        );
        Ok(aliases.into_iter().next().expect("length checked"))
    }
}

#[async_trait]
impl OperationAdapter for UpdatesAdapter {
    fn key(&self) -> &'static str {
        "updates"
    }

    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        let target = self.target(&request).await?;
        let update_request = parse_request(&request.action, &request.input)?;
        let snapshot = self.provider.snapshot(&target).await?;
        validate_snapshot(&target, &snapshot)?;
        validate_request_snapshot(&update_request, &snapshot)?;
        let metadata = action_registry::action(&request.action)
            .context("update action is absent from the action registry")?;
        let steps = if matches!(update_request, UpdateRequest::Check) {
            vec![planned_step("check", &request.action, metadata)?]
        } else {
            vec![
                planned_step("prepare_rollback", "Prepare rollback point", metadata)?,
                planned_step("apply", &request.action, metadata)?,
            ]
        };
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: plan_title(&request.action)?.into(),
            risk: risk_name(metadata.risk).into(),
            changes: plan_changes(&request.action, &update_request, &snapshot),
            preview: plan_preview(&snapshot),
            external_fingerprint: fingerprint(&snapshot)?,
            steps,
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        let target = self.target(request).await?;
        let update_request = parse_request(&request.action, &request.input)?;
        let snapshot = self.provider.snapshot(&target).await?;
        validate_snapshot(&target, &snapshot)?;
        validate_request_snapshot(&update_request, &snapshot)?;
        fingerprint(&snapshot)
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        let plan_request = PlanRequest {
            action: request.action.clone(),
            resource: request.resource,
            input: request.input,
        };
        let target = self.target(&plan_request).await?;
        let update_request = parse_request(&plan_request.action, &plan_request.input)?;
        match request.step.kind.as_str() {
            "prepare_rollback" => match self
                .provider
                .prepare_rollback(&target, &request.job_id)
                .await
            {
                Ok(point) => Ok(StepOutcome::Succeeded {
                    result: serde_json::json!({
                        "action": request.action,
                        "rollback_kind": point.kind,
                        "rollback_prepared": true,
                    }),
                    external_operation_id: Some(point.operation_id),
                }),
                Err(error) => Ok(StepOutcome::Failed {
                    code: "update_rollback_preparation_failed".into(),
                    message: safe_text(&format!(
                        "Could not prepare an update rollback point: {error}"
                    )),
                    retryable: false,
                    diagnostic: None,
                }),
            },
            "check" | "apply" => {
                ensure!(
                    (request.step.kind == "check")
                        == matches!(update_request, UpdateRequest::Check),
                    "update step/request mismatch"
                );
                match self
                    .provider
                    .execute(&target, &update_request, &request.job_id)
                    .await
                {
                    Ok(ExecutionResult::Completed { message }) => Ok(StepOutcome::Succeeded {
                        result: serde_json::json!({
                            "action": request.action,
                            "message": safe_text(&message),
                        }),
                        external_operation_id: Some(request.job_id),
                    }),
                    Ok(ExecutionResult::RestartInitiated { message }) => {
                        Ok(StepOutcome::Uncertain {
                            code: "update_restart_pending".into(),
                            message: safe_text(&message),
                            external_operation_id: Some(request.job_id),
                            diagnostic: None,
                        })
                    }
                    Err(error) if matches!(update_request, UpdateRequest::Check) => {
                        Ok(StepOutcome::Failed {
                            code: "update_check_failed".into(),
                            message: safe_text(&format!("Update check failed: {error}")),
                            retryable: true,
                            diagnostic: None,
                        })
                    }
                    Err(error) => Ok(StepOutcome::Uncertain {
                        code: "update_execution_uncertain".into(),
                        message: safe_text(&format!(
                            "The update provider did not report a conclusive outcome: {error}"
                        )),
                        external_operation_id: Some(request.job_id),
                        diagnostic: None,
                    }),
                }
            }
            _ => bail!("unsupported update step kind"),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        let plan_request = PlanRequest {
            action: request.action.clone(),
            resource: request.resource,
            input: request.input,
        };
        let target = self.target(&plan_request).await?;
        let update_request = parse_request(&plan_request.action, &plan_request.input)?;
        if request.step.kind == "prepare_rollback" {
            return match self
                .provider
                .rollback_prepared(&target, &request.job_id)
                .await?
            {
                true => Ok(ReconcileOutcome::Failed {
                    code: "update_interrupted_before_apply".into(),
                    message: "Rollback preparation completed, but the update was interrupted before apply; submit a new operation".into(),
                }),
                false => Ok(ReconcileOutcome::StillUncertain {
                    message: "The provider cannot prove that rollback preparation completed".into(),
                }),
            };
        }
        match self
            .provider
            .reconcile(&target, &update_request, &request.job_id)
            .await?
        {
            ReconciliationResult::Succeeded { message } => Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({
                    "action": request.action,
                    "message": safe_text(&message),
                    "verified": true,
                }),
            }),
            ReconciliationResult::Failed { message } => Ok(ReconcileOutcome::Failed {
                code: "update_state_mismatch".into(),
                message: safe_text(&message),
            }),
            ReconciliationResult::StillUncertain { message } => {
                Ok(ReconcileOutcome::StillUncertain {
                    message: safe_text(&message),
                })
            }
        }
    }
}

fn planned_step(
    kind: &str,
    name: &str,
    metadata: &action_registry::ActionMetadata,
) -> Result<PlannedStepV1> {
    Ok(PlannedStepV1 {
        kind: kind.into(),
        name: name.into(),
        retry_class: metadata
            .retry
            .context("update action has no retry metadata")?
            .class
            .as_str()
            .into(),
        recovery_class: metadata
            .recovery
            .context("update action has no recovery metadata")?
            .as_str()
            .into(),
    })
}

fn parse_request(action: &str, input: &Value) -> Result<UpdateRequest> {
    match action {
        "update.voidtower.check" | "update.docker.check" => {
            require_empty_input(input)?;
            Ok(UpdateRequest::Check)
        }
        "update.voidtower.apply"
        | "update.odysseus.apply"
        | "update.docker.apply"
        | "update.os.apply" => {
            require_empty_input(input)?;
            Ok(UpdateRequest::Apply)
        }
        "update.voidtower.rollback" => {
            let input: RollbackInput = serde_json::from_value(input.clone())?;
            update_service::validate_backup_tag(&input.tag)?;
            Ok(UpdateRequest::Rollback { tag: input.tag })
        }
        _ => bail!("unsupported update action"),
    }
}

fn require_empty_input(input: &Value) -> Result<()> {
    ensure!(
        input.as_object().is_some_and(serde_json::Map::is_empty),
        "update action input must be an empty object"
    );
    Ok(())
}

fn validate_resource_kind(action: &str, actual: &str) -> Result<()> {
    ensure!(ACTIONS.contains(&action), "unsupported update action");
    let expected = action_registry::action(action)
        .context("update action is absent from the action registry")?
        .resource_kind
        .context("update action has no resource kind")?;
    ensure!(
        actual == expected,
        "update action {action} requires resource kind {expected}, not {actual}"
    );
    Ok(())
}

fn validate_snapshot(target: &UpdateTarget, snapshot: &UpdateSnapshot) -> Result<()> {
    ensure!(
        matches!(
            (target, snapshot),
            (UpdateTarget::VoidTower, UpdateSnapshot::VoidTowerGit(_))
                | (
                    UpdateTarget::VoidTower,
                    UpdateSnapshot::VoidTowerBinary { .. }
                )
                | (UpdateTarget::VoidTower, UpdateSnapshot::VoidTowerDocker(_))
                | (UpdateTarget::Odysseus, UpdateSnapshot::Odysseus(_))
                | (
                    UpdateTarget::DockerEngine,
                    UpdateSnapshot::DockerEngine { .. }
                )
                | (
                    UpdateTarget::DockerImage { .. },
                    UpdateSnapshot::DockerImage(_)
                )
                | (
                    UpdateTarget::OperatingSystem,
                    UpdateSnapshot::OperatingSystem { .. }
                )
        ),
        "update provider returned a snapshot for another target"
    );
    Ok(())
}

fn validate_request_snapshot(request: &UpdateRequest, snapshot: &UpdateSnapshot) -> Result<()> {
    match (request, snapshot) {
        (UpdateRequest::Rollback { tag }, UpdateSnapshot::VoidTowerGit(snapshot)) => {
            ensure!(
                snapshot
                    .backup_tags
                    .iter()
                    .any(|candidate| candidate == tag),
                "VoidTower backup tag is not present"
            );
            Ok(())
        }
        (UpdateRequest::Rollback { .. }, _) => {
            bail!("VoidTower rollback is only available for source installations")
        }
        (UpdateRequest::Apply, UpdateSnapshot::Odysseus(snapshot)) => {
            ensure!(snapshot.installed, "Odysseus is not installed");
            Ok(())
        }
        _ => Ok(()),
    }
}

fn fingerprint(snapshot: &UpdateSnapshot) -> Result<String> {
    let mut value = serde_json::to_value(snapshot)?;
    remove_key(&mut value, "backup_tags");
    canonical_json::digest(&value)
}

fn remove_key(value: &mut Value, key: &str) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(|value| remove_key(value, key)),
        Value::Object(values) => {
            values.remove(key);
            values.values_mut().for_each(|value| remove_key(value, key));
        }
        _ => {}
    }
}

fn plan_title(action: &str) -> Result<&'static str> {
    Ok(match action {
        "update.voidtower.check" => "Check for VoidTower updates",
        "update.voidtower.apply" => "Update VoidTower",
        "update.voidtower.rollback" => "Rollback VoidTower",
        "update.odysseus.apply" => "Update Odysseus",
        "update.docker.check" => "Check Docker images for updates",
        "update.docker.apply" => "Update container image",
        "update.os.apply" => "Apply operating-system updates",
        _ => bail!("unsupported update action"),
    })
}

fn plan_changes(
    action: &str,
    request: &UpdateRequest,
    snapshot: &UpdateSnapshot,
) -> Vec<PlanChange> {
    let mut changes = vec![PlanChange {
        label: "Action".into(),
        value: plan_title(action).unwrap_or("Update").into(),
    }];
    match (request, snapshot) {
        (UpdateRequest::Rollback { tag }, _) => changes.push(PlanChange {
            label: "Target backup".into(),
            value: tag.clone(),
        }),
        (_, UpdateSnapshot::VoidTowerGit(snapshot) | UpdateSnapshot::Odysseus(snapshot)) => {
            changes.push(PlanChange {
                label: "Current commit".into(),
                value: short_commit(&snapshot.current_commit),
            });
            changes.push(PlanChange {
                label: "Target commit".into(),
                value: short_commit(&snapshot.remote_commit),
            });
        }
        (
            _,
            UpdateSnapshot::VoidTowerBinary {
                current_version,
                remote_version,
            },
        ) => {
            changes.push(PlanChange {
                label: "Current version".into(),
                value: safe_text(current_version),
            });
            changes.push(PlanChange {
                label: "Target version".into(),
                value: safe_text(remote_version),
            });
        }
        (_, UpdateSnapshot::VoidTowerDocker(snapshot) | UpdateSnapshot::DockerImage(snapshot)) => {
            changes.push(PlanChange {
                label: "Image".into(),
                value: safe_text(&snapshot.image),
            });
            changes.push(PlanChange {
                label: "Container".into(),
                value: safe_text(&snapshot.container_name),
            });
        }
        (_, UpdateSnapshot::DockerEngine { containers }) => changes.push(PlanChange {
            label: "Running containers".into(),
            value: containers.len().to_string(),
        }),
        (
            _,
            UpdateSnapshot::OperatingSystem {
                package_manager,
                packages,
            },
        ) => {
            changes.push(PlanChange {
                label: "Package manager".into(),
                value: package_manager.clone(),
            });
            changes.push(PlanChange {
                label: "Packages".into(),
                value: packages.len().to_string(),
            });
        }
    }
    if !matches!(request, UpdateRequest::Check) {
        changes.push(PlanChange {
            label: "Safety".into(),
            value: "Persist rollback/snapshot point before apply".into(),
        });
    }
    changes
}

fn plan_preview(snapshot: &UpdateSnapshot) -> Option<String> {
    let values: Vec<String> = match snapshot {
        UpdateSnapshot::OperatingSystem { packages, .. } => packages.clone(),
        UpdateSnapshot::DockerEngine { containers } => containers
            .iter()
            .map(|container| format!("{} ({})", container.container_name, container.image))
            .collect(),
        _ => Vec::new(),
    };
    (!values.is_empty()).then(|| safe_text(&values.join("\n")))
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "read",
        RiskClass::Mutate => "mutate",
        RiskClass::Destructive => "destructive",
        RiskClass::Irreversible => "irreversible",
    }
}

fn safe_text(value: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(value.trim());
    let mut characters = redacted.chars();
    let mut bounded: String = characters.by_ref().take(4 * 1024).collect();
    if characters.next().is_some() {
        bounded.push_str("…[truncated]");
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        contracts::ResourceRef,
        resources::{self, ObserveResource},
    };
    use std::sync::Mutex;

    struct FakeProvider {
        snapshot: Mutex<UpdateSnapshot>,
        calls: Mutex<Vec<String>>,
        restart: bool,
        prepared: bool,
    }

    #[async_trait]
    impl UpdateProvider for FakeProvider {
        async fn snapshot(&self, _target: &UpdateTarget) -> Result<UpdateSnapshot> {
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn prepare_rollback(
            &self,
            _target: &UpdateTarget,
            operation_id: &str,
        ) -> Result<RollbackPoint> {
            self.calls.lock().unwrap().push("prepare".into());
            Ok(RollbackPoint {
                operation_id: operation_id.into(),
                kind: "git_tag".into(),
                reference: "vt-backup-job-reference".into(),
            })
        }

        async fn rollback_prepared(
            &self,
            _target: &UpdateTarget,
            _operation_id: &str,
        ) -> Result<bool> {
            Ok(self.prepared)
        }

        async fn execute(
            &self,
            _target: &UpdateTarget,
            _request: &UpdateRequest,
            _operation_id: &str,
        ) -> Result<ExecutionResult> {
            self.calls.lock().unwrap().push("execute".into());
            if self.restart {
                Ok(ExecutionResult::RestartInitiated {
                    message: "restart started".into(),
                })
            } else {
                Ok(ExecutionResult::Completed {
                    message: "completed".into(),
                })
            }
        }

        async fn reconcile(
            &self,
            _target: &UpdateTarget,
            _request: &UpdateRequest,
            _operation_id: &str,
        ) -> Result<ReconciliationResult> {
            Ok(ReconciliationResult::Succeeded {
                message: "verified".into(),
            })
        }
    }

    async fn setup(
        action: &str,
        snapshot: UpdateSnapshot,
        restart: bool,
    ) -> (UpdatesAdapter, Arc<FakeProvider>, ResourceRef) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let (kind, namespace, alias) = match action {
            "update.docker.check" => ("docker_engine", "voidtower.singleton", "docker"),
            "update.docker.apply" => ("container_image", "docker.container_image", "abcdef012345"),
            "update.odysseus.apply" => ("update_target", "voidtower.update_target", "odysseus"),
            "update.os.apply" => ("update_target", "voidtower.update_target", "os"),
            _ => ("update_target", "voidtower.update_target", "voidtower"),
        };
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind,
                display_name: "Update target",
                node_id: None,
                provider: Some("local"),
                namespace,
                scope_key: "local",
                alias,
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(snapshot),
            calls: Mutex::new(Vec::new()),
            restart,
            prepared: true,
        });
        (
            UpdatesAdapter::with_provider(pool, provider.clone()),
            provider,
            resource,
        )
    }

    fn git_snapshot() -> UpdateSnapshot {
        UpdateSnapshot::VoidTowerGit(crate::updates::GitSnapshot {
            installed: true,
            branch: "dev".into(),
            current_commit: "1111111111111111111111111111111111111111".into(),
            remote_commit: "2222222222222222222222222222222222222222".into(),
            behind: 1,
            ahead: 0,
            backup_tags: vec![],
        })
    }

    #[tokio::test]
    async fn mutating_plan_persists_rollback_preparation_before_apply() {
        let (adapter, provider, resource) =
            setup("update.voidtower.apply", git_snapshot(), true).await;
        let request = PlanRequest {
            action: "update.voidtower.apply".into(),
            resource,
            input: serde_json::json!({}),
        };
        let plan = adapter.plan(request.clone()).await.unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].kind, "prepare_rollback");
        assert_eq!(plan.steps[1].kind, "apply");

        let prepare = adapter
            .execute_step(StepRequest {
                job_id: "00000000-0000-4000-8000-000000000001".into(),
                action: request.action.clone(),
                resource: request.resource.clone(),
                input: request.input.clone(),
                step: plan.steps[0].clone(),
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(prepare, StepOutcome::Succeeded { .. }));

        let apply = adapter
            .execute_step(StepRequest {
                job_id: "00000000-0000-4000-8000-000000000001".into(),
                action: request.action,
                resource: request.resource,
                input: request.input,
                step: plan.steps[1].clone(),
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(apply, StepOutcome::Uncertain { .. }));
        assert_eq!(
            provider.calls.lock().unwrap().as_slice(),
            &["prepare", "execute"]
        );
    }

    #[tokio::test]
    async fn interrupted_preparation_never_resumes_or_claims_the_update_applied() {
        let (adapter, _, resource) = setup("update.voidtower.apply", git_snapshot(), true).await;
        let outcome = adapter
            .reconcile(StepRequest {
                job_id: "00000000-0000-4000-8000-000000000001".into(),
                action: "update.voidtower.apply".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "prepare_rollback".into(),
                    name: "Prepare rollback point".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 2,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            ReconcileOutcome::Failed { ref code, .. }
                if code == "update_interrupted_before_apply"
        ));
    }

    #[tokio::test]
    async fn persisted_backup_tag_does_not_stale_the_following_apply_step() {
        let (adapter, provider, resource) =
            setup("update.voidtower.apply", git_snapshot(), true).await;
        let request = PlanRequest {
            action: "update.voidtower.apply".into(),
            resource,
            input: serde_json::json!({}),
        };
        let plan = adapter.plan(request.clone()).await.unwrap();
        let mut updated = match git_snapshot() {
            UpdateSnapshot::VoidTowerGit(snapshot) => snapshot,
            _ => unreachable!(),
        };
        updated.backup_tags.push("vt-backup-job-new".into());
        *provider.snapshot.lock().unwrap() = UpdateSnapshot::VoidTowerGit(updated);
        assert_eq!(
            adapter.external_fingerprint(&request).await.unwrap(),
            plan.external_fingerprint
        );
    }

    #[tokio::test]
    async fn rollback_tags_are_validated_before_provider_access() {
        let (adapter, provider, resource) =
            setup("update.voidtower.rollback", git_snapshot(), true).await;
        let error = adapter
            .plan(PlanRequest {
                action: "update.voidtower.rollback".into(),
                resource,
                input: serde_json::json!({"tag": "../../main"}),
            })
            .await
            .unwrap_err();
        assert!(error.to_string().contains("backup tag"));
        assert!(provider.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn checks_are_retryable_but_self_restart_never_claims_success() {
        let docker_snapshot = UpdateSnapshot::DockerEngine { containers: vec![] };
        let (adapter, _, resource) = setup("update.docker.check", docker_snapshot, false).await;
        let plan = adapter
            .plan(PlanRequest {
                action: "update.docker.check".into(),
                resource,
                input: serde_json::json!({}),
            })
            .await
            .unwrap();
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].retry_class, "transient");

        let (adapter, _, resource) = setup("update.voidtower.apply", git_snapshot(), true).await;
        let reconciled = adapter
            .reconcile(StepRequest {
                job_id: "00000000-0000-4000-8000-000000000001".into(),
                action: "update.voidtower.apply".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "apply".into(),
                    name: "update.voidtower.apply".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 2,
                external_operation_id: Some("00000000-0000-4000-8000-000000000001".into()),
            })
            .await
            .unwrap();
        assert!(matches!(reconciled, ReconcileOutcome::Succeeded { .. }));
    }

    #[test]
    fn plans_and_provider_messages_are_bounded_and_redacted() {
        let value = format!("api_key=known-secret-value {}", "x".repeat(5000));
        let safe = safe_text(&value);
        assert!(!safe.contains("known-secret-value"));
        assert!(safe.ends_with("[truncated]"));
    }
}
