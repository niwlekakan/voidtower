//! Durable local automation adapter.
//!
//! Automation commands are user-authored provider work. They are planned and submitted as durable
//! jobs so policy, approval, idempotency, audit, event, lease, and recovery boundaries remain the
//! same as other machine-capable mutations.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::mcp::action_registry::{self, RiskClass},
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
};
use anyhow::{ensure, Context, Result};
use async_trait::async_trait;
use serde::Serialize;
use sqlx::SqlitePool;
use std::{process::Stdio, sync::Arc, time::Duration};

const ACTION: &str = "automation.run";
const RESOURCE_KIND: &str = "automation_job";
const MAX_TIMEOUT_SECS: i64 = 60 * 60;
const MAX_OUTPUT_CHARS: usize = 16 * 1024;

#[derive(Debug, sqlx::FromRow)]
struct AutomationRecord {
    id: String,
    name: String,
    command: String,
    timeout_secs: i64,
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct AutomationSnapshot<'a> {
    id: &'a str,
    name: &'a str,
    command_digest: String,
    timeout_secs: i64,
    enabled: bool,
}

pub struct AutomationAdapter {
    pool: SqlitePool,
    secrets_key: Option<Arc<[u8; 32]>>,
}

impl AutomationAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            secrets_key: None,
        }
    }

    pub fn with_secrets_key(pool: SqlitePool, secrets_key: Arc<[u8; 32]>) -> Self {
        Self {
            pool,
            secrets_key: Some(secrets_key),
        }
    }

    async fn record(&self, resource_id: &str) -> Result<AutomationRecord> {
        sqlx::query_as::<_, AutomationRecord>(
            "SELECT j.id, j.name, j.command, j.timeout_secs, j.enabled
             FROM automation_jobs j
             JOIN resource_aliases a ON a.value = j.id
             WHERE a.resource_id = ? AND a.namespace = 'automation.job'
               AND a.scope_key = 'local'",
        )
        .bind(resource_id)
        .fetch_optional(&self.pool)
        .await?
        .context("automation resource has no backing job")
    }

    fn validate_request(request: &PlanRequest) -> Result<()> {
        ensure!(request.action == ACTION, "unsupported automation action");
        ensure!(
            request.resource.kind == RESOURCE_KIND,
            "automation.run requires an automation_job resource"
        );
        ensure!(
            request
                .input
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
            "automation.run input must be an empty object"
        );
        Ok(())
    }
}

#[async_trait]
impl OperationAdapter for AutomationAdapter {
    fn key(&self) -> &'static str {
        "automation"
    }

    fn actions(&self) -> &[&'static str] {
        &[ACTION]
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        Self::validate_request(&request)?;
        let record = self.record(&request.resource.id).await?;
        ensure!(record.enabled, "automation job is disabled");
        ensure!(
            (1..=MAX_TIMEOUT_SECS).contains(&record.timeout_secs),
            "automation timeout is outside the supported bound"
        );
        let command_digest = canonical_json::digest(&record.command)?;
        let snapshot = AutomationSnapshot {
            id: &record.id,
            name: &record.name,
            command_digest,
            timeout_secs: record.timeout_secs,
            enabled: record.enabled,
        };
        let metadata =
            action_registry::action(ACTION).context("automation action metadata missing")?;
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: format!("Run automation {}", record.name),
            risk: risk_name(metadata.risk).into(),
            changes: vec![
                PlanChange {
                    label: "Automation".into(),
                    value: record.name.clone(),
                },
                PlanChange {
                    label: "Timeout".into(),
                    value: format!("{} seconds", record.timeout_secs),
                },
            ],
            preview: None,
            external_fingerprint: canonical_json::digest(&snapshot)?,
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: ACTION.into(),
                retry_class: metadata
                    .retry
                    .context("automation action retry metadata missing")?
                    .class
                    .as_str()
                    .into(),
                recovery_class: metadata
                    .recovery
                    .context("automation action recovery metadata missing")?
                    .as_str()
                    .into(),
            }],
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        Self::validate_request(request)?;
        let record = self.record(&request.resource.id).await?;
        ensure!(record.enabled, "automation job is disabled");
        ensure!(
            (1..=MAX_TIMEOUT_SECS).contains(&record.timeout_secs),
            "automation timeout is outside the supported bound"
        );
        let command_digest = canonical_json::digest(&record.command)?;
        let snapshot = AutomationSnapshot {
            id: &record.id,
            name: &record.name,
            command_digest,
            timeout_secs: record.timeout_secs,
            enabled: record.enabled,
        };
        canonical_json::digest(&snapshot)
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        ensure!(request.action == ACTION, "unsupported automation action");
        ensure!(
            request.resource.kind == RESOURCE_KIND,
            "invalid automation resource kind"
        );
        ensure!(
            request.step.kind == "execute",
            "unsupported automation step kind"
        );
        ensure!(
            request.step.name == ACTION,
            "automation step/action mismatch"
        );
        let record = self.record(&request.resource.id).await?;
        ensure!(record.enabled, "automation job is disabled");
        ensure!(
            (1..=MAX_TIMEOUT_SECS).contains(&record.timeout_secs),
            "automation timeout is outside the supported bound"
        );

        let current_fingerprint = Self::fingerprint(&record)?;
        let planned_fingerprint: Option<String> =
            sqlx::query_scalar("SELECT external_fingerprint FROM jobs WHERE id = ?")
                .bind(&request.job_id)
                .fetch_optional(&self.pool)
                .await?;
        if let Some(planned_fingerprint) = planned_fingerprint {
            ensure!(
                planned_fingerprint == current_fingerprint,
                "automation plan is stale; refusing to execute changed command"
            );
        }

        let inserted = sqlx::query(
            "INSERT INTO automation_runs (id, job_id, started_at, status, output)
             VALUES (?, ?, ?, 'running', '') ON CONFLICT(id) DO NOTHING",
        )
        .bind(&request.job_id)
        .bind(&record.id)
        .bind(crate::unix_now())
        .execute(&self.pool)
        .await?
        .rows_affected();
        if inserted == 0 {
            return self.reconcile_existing_run(&request.job_id).await.map(
                |outcome| match outcome {
                    ReconcileOutcome::Succeeded { result } => StepOutcome::Succeeded {
                        result,
                        external_operation_id: None,
                    },
                    ReconcileOutcome::Failed { code, message } => StepOutcome::Failed {
                        code,
                        message,
                        retryable: false,
                        diagnostic: None,
                    },
                    ReconcileOutcome::StillUncertain { message } => StepOutcome::Uncertain {
                        code: "automation_run_already_exists".into(),
                        message,
                        external_operation_id: None,
                        diagnostic: None,
                    },
                },
            );
        }

        let timeout = Duration::from_secs(record.timeout_secs as u64);
        let mut command = tokio::process::Command::new("setsid");
        command
            .arg("bash")
            .arg("-c")
            .arg(&record.command)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let child = command
            .spawn()
            .context("failed to spawn automation process")?;
        let pid = child.id();
        let command_result = tokio::time::timeout(timeout, child.wait_with_output()).await;
        let (status, exit_code, output) = match command_result {
            Ok(Ok(output)) => {
                let exit_code = output.status.code().map(i64::from);
                let status = if output.status.success() {
                    "success"
                } else {
                    "failure"
                };
                let combined = format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
                (status, exit_code, self.redact_output(&combined).await)
            }
            Ok(Err(error)) => (
                "failure",
                None,
                self.redact_output(&format!("automation execution failed: {error}"))
                    .await,
            ),
            Err(_) => {
                terminate_process_group(pid);
                (
                    "timeout",
                    None,
                    format!("automation timed out after {} seconds", record.timeout_secs),
                )
            }
        };
        sqlx::query(
            "UPDATE automation_runs SET finished_at=?, status=?, exit_code=?, output=? WHERE id=?",
        )
        .bind(crate::unix_now())
        .bind(status)
        .bind(exit_code)
        .bind(&output)
        .bind(&request.job_id)
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "UPDATE automation_jobs SET last_run_at=?, last_status=?, last_exit_code=?, updated_at=? WHERE id=?",
        )
        .bind(crate::unix_now())
        .bind(status)
        .bind(exit_code)
        .bind(crate::unix_now())
        .bind(&record.id)
        .execute(&self.pool)
        .await?;

        if status == "success" {
            Ok(StepOutcome::Succeeded {
                result: serde_json::json!({
                    "automation_run_id": request.job_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": output,
                }),
                external_operation_id: None,
            })
        } else {
            Ok(StepOutcome::Failed {
                code: format!("automation_{status}"),
                message: format!("Automation completed with status {status}"),
                retryable: false,
                diagnostic: Some(serde_json::json!({"output": output, "exit_code": exit_code})),
            })
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        ensure!(request.action == ACTION, "unsupported automation action");
        self.reconcile_existing_run(&request.job_id).await
    }
}

impl AutomationAdapter {
    fn fingerprint(record: &AutomationRecord) -> Result<String> {
        let command_digest = canonical_json::digest(&record.command)?;
        let snapshot = AutomationSnapshot {
            id: &record.id,
            name: &record.name,
            command_digest,
            timeout_secs: record.timeout_secs,
            enabled: record.enabled,
        };
        canonical_json::digest(&snapshot)
    }

    async fn redact_output(&self, value: &str) -> String {
        let redacted = if let Some(key) = &self.secrets_key {
            let known = crate::api::mcp::redact::known_secret_values_from(&self.pool, key).await;
            if !known.complete {
                crate::api::mcp::redact::REDACTION_UNAVAILABLE.to_owned()
            } else {
                crate::api::mcp::redact::redact(value, &known.values)
            }
        } else {
            crate::api::mcp::redact::redact_patterns(value)
        };
        redacted.chars().take(MAX_OUTPUT_CHARS).collect()
    }

    async fn reconcile_existing_run(&self, run_id: &str) -> Result<ReconcileOutcome> {
        let row: Option<(String, Option<i64>, String)> =
            sqlx::query_as("SELECT status, exit_code, output FROM automation_runs WHERE id = ?")
                .bind(run_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((status, exit_code, output)) = row else {
            return Ok(ReconcileOutcome::Failed {
                code: "automation_run_missing".into(),
                message: "The automation execution record is missing".into(),
            });
        };
        match status.as_str() {
            "success" => Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({
                    "automation_run_id": run_id,
                    "status": status,
                    "exit_code": exit_code,
                    "output": self.redact_output(&output).await,
                }),
            }),
            "failure" | "timeout" => Ok(ReconcileOutcome::Failed {
                code: format!("automation_{status}"),
                message: format!("Automation completed with status {status}"),
            }),
            _ => Ok(ReconcileOutcome::StillUncertain {
                message: "The automation process has not reported a final outcome".into(),
            }),
        }
    }
}

#[cfg(unix)]
fn terminate_process_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        let _ = nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(-(pid as i32)),
            nix::sys::signal::Signal::SIGKILL,
        );
    }
}

#[cfg(not(unix))]
fn terminate_process_group(_pid: Option<u32>) {}

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
    use crate::operations::{
        contracts::ResourceRef,
        resources::{self, ObserveResource},
    };

    #[tokio::test]
    async fn plan_fingerprints_command_without_exposing_it() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO automation_jobs
             (id, name, command, enabled, timeout_secs, created_at, updated_at)
             VALUES ('automation-1', 'Safe automation', 'printf sensitive-command', 1, 30, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: RESOURCE_KIND,
                display_name: "Safe automation",
                node_id: None,
                provider: Some("local"),
                namespace: "automation.job",
                scope_key: "local",
                alias: "automation-1",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let adapter = AutomationAdapter::new(pool);
        let plan = adapter
            .plan(PlanRequest {
                action: ACTION.into(),
                resource: ResourceRef {
                    id: resource.id,
                    kind: RESOURCE_KIND.into(),
                    display_name: "Safe automation".into(),
                    revision: resource.revision,
                },
                input: serde_json::json!({}),
            })
            .await
            .unwrap();
        assert!(!plan.title.contains("sensitive-command"));
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("sensitive-command"));
    }

    #[tokio::test]
    async fn execute_step_persists_result_and_replays_completed_run() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO automation_jobs
             (id, name, command, enabled, timeout_secs, created_at, updated_at)
             VALUES ('automation-2', 'Executable automation', 'printf canonical-output', 1, 30, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: RESOURCE_KIND,
                display_name: "Executable automation",
                node_id: None,
                provider: Some("local"),
                namespace: "automation.job",
                scope_key: "local",
                alias: "automation-2",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let step = PlannedStepV1 {
            kind: "execute".into(),
            name: ACTION.into(),
            retry_class: "never".into(),
            recovery_class: "reconcile".into(),
        };
        let request = StepRequest {
            job_id: "operation-job-1".into(),
            action: ACTION.into(),
            resource: resource.clone(),
            input: serde_json::json!({}),
            step: step.clone(),
            attempt: 1,
            external_operation_id: None,
        };
        let adapter = AutomationAdapter::new(pool.clone());
        let outcome = adapter.execute_step(request.clone()).await.unwrap();
        let StepOutcome::Succeeded { result, .. } = outcome else {
            panic!("automation command should succeed");
        };
        assert_eq!(result["status"], "success");
        assert_eq!(result["output"], "canonical-output");
        let status: String =
            sqlx::query_scalar("SELECT status FROM automation_runs WHERE id = 'operation-job-1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "success");

        sqlx::query(
            "INSERT INTO automation_runs (id, job_id, started_at, status, output)
             VALUES ('operation-job-2', 'automation-2', 0, 'success', 'already-completed')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let replay = adapter
            .execute_step(StepRequest {
                job_id: "operation-job-2".into(),
                ..request
            })
            .await
            .unwrap();
        let StepOutcome::Succeeded { result, .. } = replay else {
            panic!("completed automation should replay its durable result");
        };
        assert_eq!(result["output"], "already-completed");
        let run_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM automation_runs WHERE job_id = 'automation-2'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(run_count, 2);
    }

    #[tokio::test]
    async fn exact_stored_secret_is_redacted_from_output() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let key = [7u8; 32];
        let secret = "automation-secret-value";
        let encrypted = crate::api::secrets::encrypt(&key, secret).unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at)
             VALUES ('automation-secret', 'automation-secret', NULL, ?, 0, 0)",
        )
        .bind(encrypted)
        .execute(&pool)
        .await
        .unwrap();

        let adapter = AutomationAdapter::with_secrets_key(pool, Arc::new(key));
        let output = adapter
            .redact_output(&format!("command output: {secret}"))
            .await;
        assert!(!output.contains(secret));
        assert!(output.contains("[REDACTED]"));
    }
}
