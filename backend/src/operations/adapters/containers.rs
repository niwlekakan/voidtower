//! Durable container lifecycle adapter.
//!
//! This adapter is intentionally not installed in `main` yet. It is the first concrete runtime
//! implementation and covers lifecycle actions only; Compose apply remains outside the runtime
//! registry until its staged-file plan and rollback semantics are represented faithfully.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::mcp::action_registry::{self, RiskClass},
    containers::{self as container_service, ContainerAction, ContainerInfo},
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::Serialize;
use sqlx::SqlitePool;
use std::sync::Arc;

const ACTIONS: &[&str] = &[
    "container.start",
    "container.stop",
    "container.restart",
    "container.remove",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerSnapshot {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub created: i64,
}

impl From<ContainerInfo> for ContainerSnapshot {
    fn from(container: ContainerInfo) -> Self {
        Self {
            id: container.id,
            name: container.name,
            image: container.image,
            state: container.state,
            created: container.created,
        }
    }
}

#[async_trait]
pub trait ContainerProvider: Send + Sync {
    async fn inspect(&self, native_id: &str) -> Result<Option<ContainerSnapshot>>;
    async fn execute(&self, native_id: &str, action: ContainerAction) -> Result<()>;
}

#[derive(Default)]
pub struct DockerContainerProvider;

#[async_trait]
impl ContainerProvider for DockerContainerProvider {
    async fn inspect(&self, native_id: &str) -> Result<Option<ContainerSnapshot>> {
        Ok(container_service::list_containers()
            .await?
            .into_iter()
            .find(|container| container.id == native_id)
            .map(ContainerSnapshot::from))
    }

    async fn execute(&self, native_id: &str, action: ContainerAction) -> Result<()> {
        container_service::container_action(native_id, action).await
    }
}

pub struct ContainerAdapter {
    pool: SqlitePool,
    provider: Arc<dyn ContainerProvider>,
}

impl ContainerAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            provider: Arc::new(DockerContainerProvider),
        }
    }

    #[cfg(test)]
    fn with_provider(pool: SqlitePool, provider: Arc<dyn ContainerProvider>) -> Self {
        Self { pool, provider }
    }

    async fn native_id(&self, resource_id: &str) -> Result<String> {
        let aliases: Vec<String> = sqlx::query_scalar(
            "SELECT value FROM resource_aliases WHERE resource_id = ? \
             AND namespace = 'docker.container' ORDER BY scope_key, value",
        )
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await?;
        ensure!(
            aliases.len() == 1,
            "container resource must have exactly one docker.container alias"
        );
        Ok(aliases.into_iter().next().expect("length checked"))
    }

    async fn snapshot(&self, request: &PlanRequest) -> Result<(String, ContainerSnapshot)> {
        ensure!(
            request.resource.kind == "container",
            "container adapter received resource kind {}",
            request.resource.kind
        );
        ensure!(
            request.input.as_object().is_some(),
            "container lifecycle input must be an object"
        );
        let native_id = self.native_id(&request.resource.id).await?;
        let snapshot = self
            .provider
            .inspect(&native_id)
            .await?
            .context("container is not present on the Docker engine")?;
        Ok((native_id, snapshot))
    }
}

#[async_trait]
impl OperationAdapter for ContainerAdapter {
    fn key(&self) -> &'static str {
        "containers"
    }

    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        ensure!(
            ACTIONS.contains(&request.action.as_str()),
            "unsupported container action"
        );
        let (_, snapshot) = self.snapshot(&request).await?;
        let metadata = action_registry::action(&request.action)
            .context("container action is absent from the action registry")?;
        let verb = action_verb(&request.action)?;
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: format!("{verb} container {}", snapshot.name),
            risk: risk_name(metadata.risk).into(),
            changes: vec![
                PlanChange {
                    label: "Action".into(),
                    value: verb.into(),
                },
                PlanChange {
                    label: "Container".into(),
                    value: snapshot.name.clone(),
                },
                PlanChange {
                    label: "Current state".into(),
                    value: snapshot.state.clone(),
                },
            ],
            preview: None,
            external_fingerprint: canonical_json::digest(&snapshot)?,
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: request.action,
                retry_class: metadata
                    .retry
                    .context("container action has no retry metadata")?
                    .class
                    .as_str()
                    .into(),
                recovery_class: metadata
                    .recovery
                    .context("container action has no recovery metadata")?
                    .as_str()
                    .into(),
            }],
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        let (_, snapshot) = self.snapshot(request).await?;
        canonical_json::digest(&snapshot)
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        ensure!(
            ACTIONS.contains(&request.action.as_str()),
            "unsupported container action"
        );
        ensure!(
            request.step.kind == "execute",
            "unsupported container step kind"
        );
        ensure!(
            request.step.name == request.action,
            "container step/action mismatch"
        );
        let plan_request = PlanRequest {
            action: request.action.clone(),
            resource: request.resource,
            input: request.input,
        };
        let (native_id, snapshot) = self.snapshot(&plan_request).await?;
        let action = parse_action(&request.action)?;
        match self.provider.execute(&native_id, action).await {
            Ok(()) => Ok(StepOutcome::Succeeded {
                result: serde_json::json!({
                    "action": request.action,
                    "container_id": native_id,
                    "container_name": snapshot.name,
                }),
                external_operation_id: None,
            }),
            Err(error) => Ok(StepOutcome::Uncertain {
                code: "container_execution_uncertain".into(),
                message: crate::api::mcp::redact::redact_patterns(&format!(
                    "Docker did not report a conclusive outcome: {error}"
                )),
                external_operation_id: None,
                diagnostic: None,
            }),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        let native_id = self.native_id(&request.resource.id).await?;
        let snapshot = self.provider.inspect(&native_id).await?;
        let succeeded = match request.action.as_str() {
            "container.start" => snapshot
                .as_ref()
                .is_some_and(|value| value.state == "running"),
            "container.stop" => snapshot
                .as_ref()
                .is_some_and(|value| value.state != "running"),
            "container.remove" => snapshot.is_none(),
            "container.restart" => {
                return Ok(ReconcileOutcome::StillUncertain {
                    message: "Running state alone cannot prove that a restart completed".into(),
                });
            }
            _ => bail!("unsupported container action"),
        };
        if succeeded {
            Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({"action": request.action, "container_id": native_id}),
            })
        } else {
            Ok(ReconcileOutcome::Failed {
                code: "container_state_mismatch".into(),
                message: "Container state does not match the requested lifecycle outcome".into(),
            })
        }
    }
}

fn parse_action(action: &str) -> Result<ContainerAction> {
    Ok(match action {
        "container.start" => ContainerAction::Start,
        "container.stop" => ContainerAction::Stop,
        "container.restart" => ContainerAction::Restart,
        "container.remove" => ContainerAction::Remove,
        _ => bail!("unsupported container action"),
    })
}

fn action_verb(action: &str) -> Result<&'static str> {
    Ok(match action {
        "container.start" => "Start",
        "container.stop" => "Stop",
        "container.restart" => "Restart",
        "container.remove" => "Remove",
        _ => bail!("unsupported container action"),
    })
}

fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "read",
        RiskClass::Mutate => "mutate",
        RiskClass::Destructive => "destructive",
        RiskClass::Irreversible => "irreversible",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::resources::{self, ObserveResource};
    use std::sync::Mutex;

    struct FakeProvider {
        snapshot: Mutex<Option<ContainerSnapshot>>,
        actions: Mutex<Vec<ContainerAction>>,
    }

    #[async_trait]
    impl ContainerProvider for FakeProvider {
        async fn inspect(&self, _native_id: &str) -> Result<Option<ContainerSnapshot>> {
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn execute(&self, _native_id: &str, action: ContainerAction) -> Result<()> {
            self.actions.lock().unwrap().push(action);
            Ok(())
        }
    }

    async fn setup() -> (
        ContainerAdapter,
        Arc<FakeProvider>,
        crate::operations::contracts::ResourceRef,
    ) {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: "container",
                display_name: "web",
                node_id: None,
                provider: Some("docker"),
                namespace: "docker.container",
                scope_key: "local-engine",
                alias: "full-container-id",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(Some(ContainerSnapshot {
                id: "full-container-id".into(),
                name: "web".into(),
                image: "nginx:stable".into(),
                state: "exited".into(),
                created: 7,
            })),
            actions: Mutex::new(vec![]),
        });
        (
            ContainerAdapter::with_provider(pool, provider.clone()),
            provider,
            resource,
        )
    }

    #[tokio::test]
    async fn plan_is_stable_and_execution_uses_scoped_native_alias() {
        let (adapter, provider, resource) = setup().await;
        let request = PlanRequest {
            action: "container.start".into(),
            resource: resource.clone(),
            input: serde_json::json!({}),
        };
        let plan = adapter.plan(request.clone()).await.unwrap();
        assert_eq!(plan.steps[0].name, "container.start");
        assert_eq!(
            plan.external_fingerprint,
            adapter.external_fingerprint(&request).await.unwrap()
        );
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job".into(),
                action: request.action,
                resource,
                input: request.input,
                step: plan.steps[0].clone(),
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Succeeded { .. }));
        assert_eq!(
            provider.actions.lock().unwrap().as_slice(),
            &[ContainerAction::Start]
        );
    }

    #[tokio::test]
    async fn reconciliation_never_guesses_that_restart_completed() {
        let (adapter, _, resource) = setup().await;
        let outcome = adapter
            .reconcile(StepRequest {
                job_id: "job".into(),
                action: "container.restart".into(),
                resource,
                input: serde_json::Value::Object(Default::default()),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "container.restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, ReconcileOutcome::StillUncertain { .. }));
    }
}
