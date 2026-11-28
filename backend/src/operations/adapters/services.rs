//! Durable systemd service restart adapter.
//!
//! The compatibility route only observes and submits. This adapter owns the bounded provider
//! boundary, immutable plan/fingerprint, fixed-argument restart invocation, and reconciliation
//! token used after an uncertain external result.

use super::{
    CompatibilityObservation, CompatibilityObservationRequest, OperationAdapter, PlanRequest,
    ReconcileOutcome, StepOutcome, StepRequest,
};
use crate::{
    api::mcp::action_registry,
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
    services,
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    time::timeout,
};

const ACTIONS: &[&str] = &["service.restart"];
const MAX_SYSTEMCTL_OUTPUT_BYTES: usize = 32 * 1024;
const INSPECT_TIMEOUT: Duration = Duration::from_secs(5);
const RESTART_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub unit: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub unit_file_state: String,
    pub active_enter_timestamp_monotonic: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartOutcome {
    Succeeded,
    Failed,
    Uncertain,
}

#[allow(clippy::double_must_use)] // async-trait adds a redundant must_use marker to boxed futures.
#[async_trait]
pub trait ServiceProvider: std::fmt::Debug + Send + Sync {
    async fn inspect(&self, unit: &str) -> Result<Option<ServiceSnapshot>>;
    async fn restart(&self, unit: &str) -> Result<RestartOutcome>;
}

#[derive(Debug, Default)]
pub struct SystemdServiceProvider;

#[async_trait]
impl ServiceProvider for SystemdServiceProvider {
    async fn inspect(&self, unit: &str) -> Result<Option<ServiceSnapshot>> {
        let unit = services::canonical_unit_name(unit)?;
        let output = run_systemctl(
            [
                "show",
                unit.as_str(),
                "--no-pager",
                "--property=LoadState,ActiveState,SubState,UnitFileState,ActiveEnterTimestampMonotonic",
            ],
            INSPECT_TIMEOUT,
        )
        .await
        .context("systemd service inspection failed")?;
        if !output.status.success() {
            return Ok(None);
        }
        let props = parse_properties(&output.stdout)?;
        if props
            .get("LoadState")
            .is_none_or(|state| state == "not-found")
        {
            return Ok(None);
        }
        Ok(Some(ServiceSnapshot {
            unit,
            load_state: bounded_field(props.get("LoadState").unwrap_or(&String::new()))?,
            active_state: bounded_field(props.get("ActiveState").unwrap_or(&String::new()))?,
            sub_state: bounded_field(props.get("SubState").unwrap_or(&String::new()))?,
            unit_file_state: bounded_field(props.get("UnitFileState").unwrap_or(&String::new()))?,
            active_enter_timestamp_monotonic: bounded_field(
                props
                    .get("ActiveEnterTimestampMonotonic")
                    .unwrap_or(&String::new()),
            )?,
        }))
    }

    async fn restart(&self, unit: &str) -> Result<RestartOutcome> {
        let unit = services::canonical_unit_name(unit)?;
        let output = match run_systemctl(["restart", unit.as_str()], RESTART_TIMEOUT).await {
            Ok(output) => output,
            Err(_) => return Ok(RestartOutcome::Uncertain),
        };
        if output.status.success() {
            Ok(RestartOutcome::Succeeded)
        } else {
            Ok(RestartOutcome::Failed)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecoveryToken {
    unit: String,
    active_enter_timestamp_monotonic: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ServiceFingerprint<'a> {
    resource_revision: i64,
    snapshot: &'a ServiceSnapshot,
}

pub struct ServicesAdapter {
    pool: SqlitePool,
    provider: Arc<dyn ServiceProvider>,
}

impl ServicesAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            provider: Arc::new(SystemdServiceProvider),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_provider(pool: SqlitePool, provider: Arc<dyn ServiceProvider>) -> Self {
        Self { pool, provider }
    }

    async fn native_unit(&self, resource_id: &str) -> Result<String> {
        let aliases: Vec<String> = sqlx::query_scalar(
            "SELECT value FROM resource_aliases WHERE resource_id = ? \
             AND namespace = 'systemd.service' ORDER BY scope_key, value",
        )
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await?;
        ensure!(
            aliases.len() == 1,
            "service resource must have exactly one systemd.service alias"
        );
        services::canonical_unit_name(&aliases[0])
    }

    async fn snapshot(&self, request: &PlanRequest) -> Result<(String, ServiceSnapshot)> {
        ensure!(
            request.resource.kind == "service",
            "service adapter received resource kind {}",
            request.resource.kind
        );
        ensure!(
            request.action == "service.restart",
            "unsupported service action"
        );
        ensure!(
            request
                .input
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
            "service restart input must be an empty object"
        );
        let unit = self.native_unit(&request.resource.id).await?;
        let snapshot = self
            .provider
            .inspect(&unit)
            .await?
            .context("systemd service is unavailable")?;
        ensure!(
            snapshot.unit == unit,
            "systemd observation did not match service alias"
        );
        Ok((unit, snapshot))
    }

    fn fingerprint(request: &PlanRequest, snapshot: &ServiceSnapshot) -> Result<String> {
        canonical_json::digest(&ServiceFingerprint {
            resource_revision: request.resource.revision,
            snapshot,
        })
    }

    fn recovery_token(snapshot: &ServiceSnapshot) -> Result<String> {
        serde_json::to_string(&RecoveryToken {
            unit: snapshot.unit.clone(),
            active_enter_timestamp_monotonic: snapshot.active_enter_timestamp_monotonic.clone(),
        })
        .context("cannot encode service recovery token")
    }

    fn service_result(snapshot: Option<ServiceSnapshot>) -> Value {
        match snapshot {
            Some(snapshot) => serde_json::json!({
                "unit": snapshot.unit,
                "restarted": true,
                "active_state": snapshot.active_state,
                "sub_state": snapshot.sub_state,
            }),
            None => serde_json::json!({"restarted": true}),
        }
    }
}

#[async_trait]
impl OperationAdapter for ServicesAdapter {
    fn key(&self) -> &'static str {
        "services"
    }

    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        let (_, snapshot) = self.snapshot(&request).await?;
        let metadata = action_registry::action(&request.action)
            .context("service action is absent from the action registry")?;
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: format!("Restart service {}", snapshot.unit),
            risk: risk_name(metadata.risk).into(),
            changes: vec![
                PlanChange {
                    label: "Action".into(),
                    value: "Restart".into(),
                },
                PlanChange {
                    label: "Service".into(),
                    value: snapshot.unit.clone(),
                },
                PlanChange {
                    label: "Current state".into(),
                    value: format!("{} / {}", snapshot.active_state, snapshot.sub_state),
                },
            ],
            preview: None,
            external_fingerprint: Self::fingerprint(&request, &snapshot)?,
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: "restart".into(),
                retry_class: metadata
                    .retry
                    .context("service action has no retry metadata")?
                    .class
                    .as_str()
                    .into(),
                recovery_class: metadata
                    .recovery
                    .context("service action has no recovery metadata")?
                    .as_str()
                    .into(),
            }],
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        let (_, snapshot) = self.snapshot(request).await?;
        Self::fingerprint(request, &snapshot)
    }

    async fn observe_compatibility(
        &self,
        request: CompatibilityObservationRequest<'_>,
    ) -> Result<CompatibilityObservation> {
        ensure!(
            request.action == "service.restart",
            "unsupported service action"
        );
        ensure!(
            request.kind == "service",
            "unsupported compatibility resource kind"
        );
        ensure!(
            request.namespace == "systemd.service" && request.scope_key == "local",
            "unsupported service resource namespace"
        );
        let unit = services::canonical_unit_name(request.alias)?;
        let snapshot = self
            .provider
            .inspect(&unit)
            .await?
            .context("systemd service is unavailable")?;
        ensure!(
            snapshot.unit == unit,
            "systemd observation did not match service alias"
        );
        Ok(CompatibilityObservation {
            display_name: snapshot.unit,
            canonical_alias: unit,
        })
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        ensure!(
            request.action == "service.restart",
            "unsupported service action"
        );
        ensure!(
            request.step.kind == "execute" && request.step.name == "restart",
            "invalid service restart step"
        );
        let plan_request = PlanRequest {
            action: request.action.clone(),
            resource: request.resource.clone(),
            input: request.input.clone(),
        };
        let (unit, before) = self.snapshot(&plan_request).await?;
        let token = Self::recovery_token(&before)?;
        let restart = match self.provider.restart(&unit).await {
            Ok(outcome) => outcome,
            Err(_) => {
                return Ok(StepOutcome::Uncertain {
                    code: "service_restart_uncertain".into(),
                    message: "The service restart outcome could not be confirmed.".into(),
                    external_operation_id: Some(token),
                    diagnostic: None,
                });
            }
        };
        match restart {
            RestartOutcome::Succeeded => match self.provider.inspect(&unit).await {
                Ok(Some(after))
                    if after.active_enter_timestamp_monotonic
                        != before.active_enter_timestamp_monotonic
                        && stable_service_state(&after) =>
                {
                    Ok(StepOutcome::Succeeded {
                        result: Self::service_result(Some(after)),
                        external_operation_id: None,
                    })
                }
                Ok(_) | Err(_) => Ok(StepOutcome::Uncertain {
                    code: "service_restart_uncertain".into(),
                    message: "The service restart outcome could not be confirmed.".into(),
                    external_operation_id: Some(token),
                    diagnostic: None,
                }),
            },
            // A nonzero systemctl exit cannot prove that no effect occurred.
            // The unit may have stopped before systemd failed to start it again.
            RestartOutcome::Failed => Ok(StepOutcome::Uncertain {
                code: "service_restart_uncertain".into(),
                message: "systemctl returned failure; the restart may have partially taken effect and requires operator reconciliation.".into(),
                external_operation_id: Some(token),
                diagnostic: None,
            }),
            RestartOutcome::Uncertain => Ok(StepOutcome::Uncertain {
                code: "service_restart_uncertain".into(),
                message: "The service restart outcome could not be confirmed.".into(),
                external_operation_id: Some(token),
                diagnostic: None,
            }),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        ensure!(
            request.action == "service.restart",
            "unsupported service action"
        );
        let token = request
            .external_operation_id
            .as_deref()
            .context("service restart recovery token is missing")?;
        let token: RecoveryToken =
            serde_json::from_str(token).context("service restart recovery token is invalid")?;
        let current = self
            .provider
            .inspect(&token.unit)
            .await?
            .context("systemd service is unavailable during reconciliation")?;
        // A changed active-enter timestamp proves a restart-like transition, but
        // systemd does not expose which actor caused it here. Do not attribute an
        // unrelated restart to this durable job; retain uncertainty for operator
        // reconciliation until the provider offers a job-correlated operation ID.
        let _changed_from_before =
            current.active_enter_timestamp_monotonic != token.active_enter_timestamp_monotonic;
        let _is_stable = stable_service_state(&current);
        Ok(ReconcileOutcome::StillUncertain {
            message: "The service restart outcome is not attributable to this job and requires operator reconciliation.".into(),
        })
    }
}

fn risk_name(risk: action_registry::RiskClass) -> &'static str {
    match risk {
        action_registry::RiskClass::Read => "read",
        action_registry::RiskClass::Mutate => "mutate",
        action_registry::RiskClass::Destructive => "destructive",
        action_registry::RiskClass::Irreversible => "irreversible",
    }
}

fn stable_service_state(snapshot: &ServiceSnapshot) -> bool {
    snapshot.active_state == "active"
        && !matches!(snapshot.sub_state.as_str(), "activating" | "deactivating")
}

async fn read_bounded<R>(mut reader: R) -> Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Ok(output);
        }
        if output.len().saturating_add(read) > MAX_SYSTEMCTL_OUTPUT_BYTES {
            anyhow::bail!("systemd provider output exceeded the bounded limit");
        }
        output.extend_from_slice(&chunk[..read]);
    }
}

async fn terminate_child(child: &mut Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn run_systemctl<const N: usize>(
    args: [&str; N],
    deadline: Duration,
) -> Result<std::process::Output> {
    let mut command = Command::new("systemctl");
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .context("systemd provider could not start")?;
    let stdout = child.stdout.take().context("systemd stdout unavailable")?;
    let stderr = child.stderr.take().context("systemd stderr unavailable")?;
    let result = timeout(deadline, async {
        let (stdout, stderr) = tokio::try_join!(read_bounded(stdout), read_bounded(stderr))?;
        let status = child.wait().await?;
        Ok::<_, anyhow::Error>(std::process::Output {
            status,
            stdout,
            stderr,
        })
    })
    .await;
    match result {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(error)) => {
            terminate_child(&mut child).await;
            Err(error)
        }
        Err(_) => {
            terminate_child(&mut child).await;
            anyhow::bail!("systemd provider command timed out")
        }
    }
}

fn bounded_field(value: &str) -> Result<String> {
    ensure!(
        value.chars().count() <= 256,
        "systemd provider field exceeded the bounded limit"
    );
    Ok(value.to_owned())
}

fn parse_properties(output: &[u8]) -> Result<HashMap<String, String>> {
    let text = std::str::from_utf8(output).context("systemd provider returned invalid text")?;
    let mut properties = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        properties.insert(key.to_owned(), value.to_owned());
    }
    Ok(properties)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::resources::{self, ObserveResource};
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FakeProvider {
        snapshot: Mutex<Option<ServiceSnapshot>>,
        post_restart_snapshot: Mutex<Option<ServiceSnapshot>>,
        restart_outcome: RestartOutcome,
        restart_error: Mutex<bool>,
        inspect_fail_after: Mutex<Option<usize>>,
        inspect_calls: Mutex<usize>,
        calls: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl ServiceProvider for FakeProvider {
        async fn inspect(&self, _unit: &str) -> Result<Option<ServiceSnapshot>> {
            let mut calls = self.inspect_calls.lock().unwrap();
            *calls += 1;
            if *self.inspect_fail_after.lock().unwrap() == Some(*calls) {
                anyhow::bail!("fixture inspection failed");
            }
            if *calls >= 2 {
                if let Some(snapshot) = self.post_restart_snapshot.lock().unwrap().clone() {
                    return Ok(Some(snapshot));
                }
            }
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn restart(&self, unit: &str) -> Result<RestartOutcome> {
            self.calls.lock().unwrap().push(unit.to_owned());
            if *self.restart_error.lock().unwrap() {
                anyhow::bail!("fixture restart failed");
            }
            Ok(self.restart_outcome)
        }
    }

    fn snapshot(timestamp: &str) -> ServiceSnapshot {
        ServiceSnapshot {
            unit: "fixture.service".into(),
            load_state: "loaded".into(),
            active_state: "active".into(),
            sub_state: "running".into(),
            unit_file_state: "enabled".into(),
            active_enter_timestamp_monotonic: timestamp.into(),
        }
    }

    async fn setup(
        outcome: RestartOutcome,
    ) -> (
        ServicesAdapter,
        Arc<FakeProvider>,
        crate::operations::contracts::ResourceRef,
    ) {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(Some(snapshot("100"))),
            post_restart_snapshot: Mutex::new(None),
            restart_outcome: outcome,
            restart_error: Mutex::new(false),
            inspect_fail_after: Mutex::new(None),
            inspect_calls: Mutex::new(0),
            calls: Mutex::new(Vec::new()),
        });
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: "service",
                display_name: "fixture",
                node_id: None,
                provider: Some("systemd"),
                namespace: "systemd.service",
                scope_key: "local",
                alias: "fixture.service",
            },
            None,
            "service-test",
        )
        .await
        .unwrap();
        let adapter = ServicesAdapter::with_provider(pool, provider.clone());
        (adapter, provider, resource)
    }

    #[tokio::test]
    async fn planning_is_side_effect_free_and_fingerprinted() {
        let (adapter, provider, resource) = setup(RestartOutcome::Succeeded).await;
        let plan = adapter
            .plan(PlanRequest {
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
            })
            .await
            .unwrap();
        assert_eq!(plan.steps[0].name, "restart");
        assert!(!plan.external_fingerprint.is_empty());
        assert!(provider.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn nonzero_restart_result_is_uncertain_with_operator_recovery_token() {
        let (adapter, _provider, resource) = setup(RestartOutcome::Failed).await;
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-nonzero-restart".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            StepOutcome::Uncertain {
                code,
                external_operation_id: Some(_),
                ..
            } if code == "service_restart_uncertain"
        ));
    }

    #[tokio::test]
    async fn unchanged_restart_timestamp_remains_uncertain_after_provider_success() {
        let (adapter, provider, resource) = setup(RestartOutcome::Succeeded).await;
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-unchanged-timestamp".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            StepOutcome::Uncertain {
                external_operation_id: Some(_),
                ..
            }
        ));
        assert_eq!(
            provider.calls.lock().unwrap().as_slice(),
            &["fixture.service"]
        );
    }

    #[tokio::test]
    async fn changed_restart_timestamp_and_stable_state_succeed() {
        let (adapter, provider, resource) = setup(RestartOutcome::Succeeded).await;
        *provider.post_restart_snapshot.lock().unwrap() = Some(snapshot("101"));
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-changed-timestamp".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Succeeded { .. }));
        assert_eq!(
            provider.calls.lock().unwrap().as_slice(),
            &["fixture.service"]
        );
    }

    #[tokio::test]
    async fn timestamp_change_from_an_unattributed_restart_remains_uncertain() {
        let (adapter, provider, resource) = setup(RestartOutcome::Uncertain).await;
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-1".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        let token = match outcome {
            StepOutcome::Uncertain {
                external_operation_id: Some(token),
                ..
            } => token,
            other => panic!("expected uncertain outcome, got {other:?}"),
        };
        *provider.snapshot.lock().unwrap() = Some(snapshot("101"));
        let reconciled = adapter
            .reconcile(StepRequest {
                job_id: "job-1".into(),
                action: "service.restart".into(),
                resource: crate::operations::contracts::ResourceRef {
                    id: "unused".into(),
                    kind: "service".into(),
                    display_name: "fixture".into(),
                    revision: 1,
                },
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: Some(token),
            })
            .await
            .unwrap();
        assert!(matches!(
            reconciled,
            ReconcileOutcome::StillUncertain { .. }
        ));
        assert_eq!(
            provider.calls.lock().unwrap().as_slice(),
            &["fixture.service"]
        );
    }

    #[tokio::test]
    async fn restart_provider_error_retains_recovery_token() {
        let (adapter, provider, resource) = setup(RestartOutcome::Succeeded).await;
        *provider.restart_error.lock().unwrap() = true;
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-1".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            StepOutcome::Uncertain {
                external_operation_id: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn post_restart_inspection_error_retains_recovery_token() {
        let (adapter, provider, resource) = setup(RestartOutcome::Succeeded).await;
        *provider.inspect_fail_after.lock().unwrap() = Some(2);
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-1".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            StepOutcome::Uncertain {
                external_operation_id: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn activating_state_remains_uncertain_during_reconciliation() {
        let (adapter, provider, resource) = setup(RestartOutcome::Uncertain).await;
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-1".into(),
                action: "service.restart".into(),
                resource,
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        let token = match outcome {
            StepOutcome::Uncertain {
                external_operation_id: Some(token),
                ..
            } => token,
            other => panic!("expected uncertain outcome, got {other:?}"),
        };
        let mut activating = snapshot("101");
        activating.active_state = "activating".into();
        activating.sub_state = "start".into();
        *provider.snapshot.lock().unwrap() = Some(activating);
        let reconciled = adapter
            .reconcile(StepRequest {
                job_id: "job-1".into(),
                action: "service.restart".into(),
                resource: crate::operations::contracts::ResourceRef {
                    id: "unused".into(),
                    kind: "service".into(),
                    display_name: "fixture".into(),
                    revision: 1,
                },
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "restart".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: Some(token),
            })
            .await
            .unwrap();
        assert!(matches!(
            reconciled,
            ReconcileOutcome::StillUncertain { .. }
        ));
    }
}
