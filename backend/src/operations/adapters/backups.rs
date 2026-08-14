//! Durable restic configuration, backup, integrity-check, and restore-test adapter.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::mcp::action_registry::{self, RiskClass},
    backups::{self as backup_service, BackupConfig, BackupConfigInput},
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;

const ACTIONS: &[&str] = &[
    "backup.config.create",
    "backup.config.delete",
    "backup.run",
    "backup.check",
    "backup.restore_test",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BackupConfigSnapshot {
    id: String,
    name: String,
    source_path: String,
    repo_path: String,
    schedule: Option<String>,
    retention_days: i64,
    enabled: bool,
    restore_test_schedule: Option<String>,
}

impl From<BackupConfig> for BackupConfigSnapshot {
    fn from(config: BackupConfig) -> Self {
        Self {
            id: config.id,
            name: config.name,
            source_path: config.source_path,
            repo_path: config.repo_path,
            schedule: config.schedule,
            retention_days: config.retention_days,
            enabled: config.enabled,
            restore_test_schedule: config.restore_test_schedule,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct BackupSnapshot {
    configs: Vec<BackupConfigSnapshot>,
    restic_available: Option<bool>,
}

#[derive(Clone)]
enum BackupOperation {
    Create {
        id: String,
        input: BackupConfigInput,
    },
    Delete {
        id: String,
    },
    PrepareRepository {
        id: String,
    },
    Run {
        id: String,
        tag: String,
    },
    Check {
        id: String,
    },
    RestoreTest {
        id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BackupOperationResult {
    config_id: String,
    resource_id: Option<String>,
    status: String,
    snapshot_id: Option<String>,
    output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BackupVerification {
    Succeeded,
    Failed,
    Indeterminate,
}

#[async_trait]
trait BackupProvider: Send + Sync {
    async fn snapshot(&self, config_id: Option<&str>, probe_restic: bool)
        -> Result<BackupSnapshot>;
    async fn execute(
        &self,
        operation: BackupOperation,
        correlation_id: &str,
    ) -> Result<BackupOperationResult>;
    async fn verify(&self, operation: &BackupOperation) -> Result<BackupVerification>;
}

struct LocalBackupProvider {
    pool: SqlitePool,
}

impl LocalBackupProvider {
    fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn config(&self, id: &str) -> Result<BackupConfig> {
        backup_service::get_config(&self.pool, id)
            .await?
            .context("backup configuration is not present")
    }

    async fn resource_is_active(&self, config_id: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM resources r JOIN resource_aliases a \
             ON a.resource_id = r.id WHERE a.namespace = 'voidtower.backup_config' \
             AND a.scope_key = 'local' AND a.value = ? AND r.lifecycle_state = 'active')",
        )
        .bind(config_id)
        .fetch_one(&self.pool)
        .await?)
    }
}

#[async_trait]
impl BackupProvider for LocalBackupProvider {
    async fn snapshot(
        &self,
        config_id: Option<&str>,
        probe_restic: bool,
    ) -> Result<BackupSnapshot> {
        let mut configs: Vec<BackupConfigSnapshot> = if let Some(id) = config_id {
            backup_service::get_config(&self.pool, id)
                .await?
                .into_iter()
                .map(BackupConfigSnapshot::from)
                .collect()
        } else {
            backup_service::list_configs(&self.pool)
                .await?
                .into_iter()
                .map(BackupConfigSnapshot::from)
                .collect()
        };
        configs.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(BackupSnapshot {
            configs,
            restic_available: probe_restic.then(backup_service::is_restic_available),
        })
    }

    async fn execute(
        &self,
        operation: BackupOperation,
        correlation_id: &str,
    ) -> Result<BackupOperationResult> {
        match operation {
            BackupOperation::Create { id, input } => {
                let resource_id =
                    backup_service::create_config(&self.pool, &id, &input, correlation_id).await?;
                Ok(BackupOperationResult {
                    config_id: id,
                    resource_id: Some(resource_id),
                    status: "created".into(),
                    snapshot_id: None,
                    output: None,
                })
            }
            BackupOperation::Delete { id } => {
                ensure!(
                    backup_service::delete_config(&self.pool, &id).await?,
                    "backup configuration is not present"
                );
                Ok(BackupOperationResult {
                    config_id: id,
                    resource_id: None,
                    status: "deleted".into(),
                    snapshot_id: None,
                    output: None,
                })
            }
            BackupOperation::PrepareRepository { id } => {
                let config = self.config(&id).await?;
                backup_service::prepare_config_repository(
                    &config,
                    &backup_service::restic_password(),
                )
                .await?;
                Ok(BackupOperationResult {
                    config_id: id,
                    resource_id: None,
                    status: "ready".into(),
                    snapshot_id: None,
                    output: None,
                })
            }
            BackupOperation::Run { id, tag } => {
                let config = self.config(&id).await?;
                let run = backup_service::run_config_backup(
                    &self.pool,
                    &config,
                    &backup_service::restic_password(),
                    Some(&tag),
                )
                .await?;
                Ok(BackupOperationResult {
                    config_id: id,
                    resource_id: None,
                    status: run.status,
                    snapshot_id: run.snapshot_id,
                    output: Some(run.output),
                })
            }
            BackupOperation::Check { id } => {
                let config = self.config(&id).await?;
                let probe = backup_service::check_config(
                    &self.pool,
                    &config,
                    &backup_service::restic_password(),
                )
                .await?;
                Ok(BackupOperationResult {
                    config_id: id,
                    resource_id: None,
                    status: probe.status,
                    snapshot_id: None,
                    output: probe.message,
                })
            }
            BackupOperation::RestoreTest { id } => {
                let config = self.config(&id).await?;
                let probe = backup_service::restore_test_config(
                    &self.pool,
                    &config,
                    &backup_service::restic_password(),
                )
                .await?;
                Ok(BackupOperationResult {
                    config_id: id,
                    resource_id: None,
                    status: probe.status,
                    snapshot_id: None,
                    output: probe.message,
                })
            }
        }
    }

    async fn verify(&self, operation: &BackupOperation) -> Result<BackupVerification> {
        match operation {
            BackupOperation::Create { id, input } => {
                let Some(config) = backup_service::get_config(&self.pool, id).await? else {
                    return Ok(BackupVerification::Failed);
                };
                let expected = normalized_snapshot(id, input);
                if BackupConfigSnapshot::from(config) == expected
                    && self.resource_is_active(id).await?
                {
                    Ok(BackupVerification::Succeeded)
                } else {
                    Ok(BackupVerification::Failed)
                }
            }
            BackupOperation::Delete { id } => Ok(
                if backup_service::get_config(&self.pool, id).await?.is_none() {
                    BackupVerification::Succeeded
                } else {
                    BackupVerification::Failed
                },
            ),
            BackupOperation::Run { id, tag } => {
                let Some(config) = backup_service::get_config(&self.pool, id).await? else {
                    return Ok(BackupVerification::Failed);
                };
                if backup_service::has_snapshot_tag(
                    &config.repo_path,
                    &backup_service::restic_password(),
                    tag,
                )
                .await?
                {
                    Ok(BackupVerification::Succeeded)
                } else {
                    Ok(BackupVerification::Indeterminate)
                }
            }
            BackupOperation::PrepareRepository { .. }
            | BackupOperation::Check { .. }
            | BackupOperation::RestoreTest { .. } => Ok(BackupVerification::Indeterminate),
        }
    }
}

pub struct BackupsAdapter {
    pool: SqlitePool,
    provider: Arc<dyn BackupProvider>,
}

impl BackupsAdapter {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            provider: Arc::new(LocalBackupProvider::new(pool.clone())),
            pool,
        }
    }

    #[cfg(test)]
    fn with_provider(pool: SqlitePool, provider: Arc<dyn BackupProvider>) -> Self {
        Self { pool, provider }
    }

    async fn require_system_resource(&self, resource_id: &str) -> Result<()> {
        let alias = single_alias(&self.pool, resource_id, "voidtower.singleton").await?;
        ensure!(
            alias == "system",
            "backup config creation must target the local system resource"
        );
        Ok(())
    }

    async fn config_id(&self, request: &PlanRequest) -> Result<Option<String>> {
        validate_target(&request.action, &request.resource.kind)?;
        if request.action == "backup.config.create" {
            self.require_system_resource(&request.resource.id).await?;
            Ok(None)
        } else {
            Ok(Some(
                single_alias(&self.pool, &request.resource.id, "voidtower.backup_config").await?,
            ))
        }
    }

    async fn snapshot(
        &self,
        request: &PlanRequest,
    ) -> Result<(ParsedInput, Option<String>, BackupSnapshot)> {
        let parsed = parse_input(&request.action, &request.input)?;
        let config_id = self.config_id(request).await?;
        let snapshot = self
            .provider
            .snapshot(config_id.as_deref(), requires_restic(&request.action))
            .await?;
        validate_snapshot(&request.action, &parsed, &snapshot)?;
        Ok((parsed, config_id, snapshot))
    }
}

#[async_trait]
impl OperationAdapter for BackupsAdapter {
    fn key(&self) -> &'static str {
        "backups"
    }

    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        ensure!(
            ACTIONS.contains(&request.action.as_str()),
            "unsupported backup action"
        );
        let (parsed, _, snapshot) = self.snapshot(&request).await?;
        let metadata = action_registry::action(&request.action)
            .context("backup action is absent from the action registry")?;
        let mut steps = Vec::new();
        if request.action == "backup.run" {
            steps.push(planned_step(
                "prepare_repository",
                "Initialize or verify restic repository",
                metadata,
            )?);
        }
        steps.push(planned_step("execute", &request.action, metadata)?);
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: plan_title(&request.action)?.into(),
            risk: risk_name(metadata.risk).into(),
            changes: plan_changes(&request.action, &parsed, &snapshot)?,
            preview: None,
            external_fingerprint: canonical_json::digest(&snapshot)?,
            steps,
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        let (_, _, snapshot) = self.snapshot(request).await?;
        canonical_json::digest(&snapshot)
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        ensure!(
            ACTIONS.contains(&request.action.as_str()),
            "unsupported backup action"
        );
        let plan_request = PlanRequest {
            action: request.action.clone(),
            resource: request.resource,
            input: request.input,
        };
        let parsed = parse_input(&plan_request.action, &plan_request.input)?;
        let config_id = self.config_id(&plan_request).await?;
        let operation = operation_for(
            &plan_request.action,
            parsed,
            config_id,
            &request.job_id,
            &request.step.kind,
        )?;
        let run_tag = match &operation {
            BackupOperation::Run { tag, .. } => Some(tag.clone()),
            _ => None,
        };
        match self.provider.execute(operation, &request.job_id).await {
            Ok(result) if result.status == "failed" => Ok(StepOutcome::Failed {
                code: failure_code(&request.action).into(),
                message: safe_text(
                    result
                        .output
                        .as_deref()
                        .unwrap_or("restic operation failed"),
                ),
                retryable: matches!(
                    request.action.as_str(),
                    "backup.check" | "backup.restore_test"
                ),
                diagnostic: None,
            }),
            Ok(result) => Ok(StepOutcome::Succeeded {
                result: serde_json::json!({
                    "action": request.action,
                    "config_id": result.config_id,
                    "resource_id": result.resource_id,
                    "status": result.status,
                    "snapshot_id": result.snapshot_id,
                    "output": result.output.as_deref().map(safe_text),
                }),
                external_operation_id: run_tag,
            }),
            Err(error) if request.step.kind == "prepare_repository" => Ok(StepOutcome::Failed {
                code: "backup_repository_preparation_failed".into(),
                message: safe_text(&format!("Could not prepare the backup repository: {error}")),
                retryable: false,
                diagnostic: None,
            }),
            Err(error) => Ok(StepOutcome::Uncertain {
                code: "backup_execution_uncertain".into(),
                message: safe_text(&format!(
                    "The backup provider did not report a conclusive outcome: {error}"
                )),
                external_operation_id: run_tag,
                diagnostic: None,
            }),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        let plan_request = PlanRequest {
            action: request.action.clone(),
            resource: request.resource,
            input: request.input,
        };
        let parsed = parse_input(&plan_request.action, &plan_request.input)?;
        let config_id = self.config_id(&plan_request).await?;
        let operation = operation_for(
            &plan_request.action,
            parsed,
            config_id,
            &request.job_id,
            &request.step.kind,
        )?;
        match self.provider.verify(&operation).await? {
            BackupVerification::Succeeded => Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({
                    "action": request.action,
                    "verified": true,
                    "external_operation_id": request.external_operation_id,
                }),
            }),
            BackupVerification::Failed => Ok(ReconcileOutcome::Failed {
                code: "backup_state_mismatch".into(),
                message: "Observed backup state does not match the requested outcome".into(),
            }),
            BackupVerification::Indeterminate => Ok(ReconcileOutcome::StillUncertain {
                message: "Current backup state cannot prove that this operation completed".into(),
            }),
        }
    }
}

#[derive(Clone)]
enum ParsedInput {
    Create(BackupConfigInput),
    Unit,
}

fn parse_input(action: &str, input: &Value) -> Result<ParsedInput> {
    match action {
        "backup.config.create" => {
            let parsed: BackupConfigInput = serde_json::from_value(input.clone())?;
            parsed.validate()?;
            Ok(ParsedInput::Create(parsed))
        }
        "backup.config.delete" | "backup.run" | "backup.check" | "backup.restore_test" => {
            ensure!(
                input.as_object().is_some_and(serde_json::Map::is_empty),
                "backup action input must be an empty object"
            );
            Ok(ParsedInput::Unit)
        }
        _ => bail!("unsupported backup action"),
    }
}

fn operation_for(
    action: &str,
    input: ParsedInput,
    config_id: Option<String>,
    job_id: &str,
    step_kind: &str,
) -> Result<BackupOperation> {
    if step_kind == "prepare_repository" {
        ensure!(
            action == "backup.run",
            "only backup.run prepares a repository"
        );
        return Ok(BackupOperation::PrepareRepository {
            id: config_id.context("backup action has no config ID")?,
        });
    }
    ensure!(step_kind == "execute", "unsupported backup step kind");
    Ok(match (action, input) {
        ("backup.config.create", ParsedInput::Create(input)) => BackupOperation::Create {
            id: job_id.into(),
            input,
        },
        ("backup.config.delete", ParsedInput::Unit) => BackupOperation::Delete {
            id: config_id.context("backup delete has no config ID")?,
        },
        ("backup.run", ParsedInput::Unit) => BackupOperation::Run {
            id: config_id.context("backup run has no config ID")?,
            tag: operation_tag(job_id)?,
        },
        ("backup.check", ParsedInput::Unit) => BackupOperation::Check {
            id: config_id.context("backup check has no config ID")?,
        },
        ("backup.restore_test", ParsedInput::Unit) => BackupOperation::RestoreTest {
            id: config_id.context("backup restore test has no config ID")?,
        },
        _ => bail!("backup action/input mismatch"),
    })
}

fn operation_tag(job_id: &str) -> Result<String> {
    uuid::Uuid::parse_str(job_id).context("backup job ID must be a UUID")?;
    Ok(format!("voidtower-job-{job_id}"))
}

fn validate_target(action: &str, kind: &str) -> Result<()> {
    let expected = if action == "backup.config.create" {
        "system"
    } else {
        "backup_config"
    };
    ensure!(
        kind == expected,
        "backup action {action} requires resource kind {expected}"
    );
    Ok(())
}

fn validate_snapshot(action: &str, input: &ParsedInput, snapshot: &BackupSnapshot) -> Result<()> {
    if action == "backup.config.create" {
        ensure!(
            matches!(input, ParsedInput::Create(_)),
            "backup create input is missing"
        );
    } else {
        ensure!(
            snapshot.configs.len() == 1,
            "backup resource must resolve to exactly one configuration"
        );
    }
    if requires_restic(action) {
        ensure!(
            snapshot.restic_available == Some(true),
            "restic is not installed"
        );
    }
    Ok(())
}

fn plan_changes(
    action: &str,
    input: &ParsedInput,
    snapshot: &BackupSnapshot,
) -> Result<Vec<PlanChange>> {
    let (name, source_path, repo_path, retention) = match input {
        ParsedInput::Create(input) => (
            input.name.as_str(),
            input.source_path.as_str(),
            input.repo_path.as_str(),
            input.retention_days,
        ),
        ParsedInput::Unit => {
            let config = snapshot
                .configs
                .first()
                .context("backup configuration is not present")?;
            (
                config.name.as_str(),
                config.source_path.as_str(),
                config.repo_path.as_str(),
                config.retention_days,
            )
        }
    };
    let mut changes = vec![
        PlanChange {
            label: "Action".into(),
            value: action_label(action)?.into(),
        },
        PlanChange {
            label: "Backup".into(),
            value: safe_text(name),
        },
    ];
    if action != "backup.config.delete" {
        changes.push(PlanChange {
            label: "Source path".into(),
            value: safe_text(source_path),
        });
        changes.push(PlanChange {
            label: "Repository".into(),
            value: safe_text(repo_path),
        });
    }
    if action == "backup.config.create" {
        changes.push(PlanChange {
            label: "Retention".into(),
            value: format!("{retention} days"),
        });
    }
    if action == "backup.config.delete" {
        changes.push(PlanChange {
            label: "Effect".into(),
            value: "Delete configuration only; repository data is preserved".into(),
        });
    }
    Ok(changes)
}

fn normalized_snapshot(id: &str, input: &BackupConfigInput) -> BackupConfigSnapshot {
    BackupConfigSnapshot {
        id: id.into(),
        name: input.name.trim().into(),
        source_path: input.source_path.trim().into(),
        repo_path: input.repo_path.trim().into(),
        schedule: input.schedule.as_deref().map(str::trim).map(str::to_owned),
        retention_days: input.retention_days,
        enabled: true,
        restore_test_schedule: input
            .restore_test_schedule
            .as_deref()
            .map(str::trim)
            .map(str::to_owned),
    }
}

async fn single_alias(pool: &SqlitePool, resource_id: &str, namespace: &str) -> Result<String> {
    let aliases: Vec<String> = sqlx::query_scalar(
        "SELECT value FROM resource_aliases WHERE resource_id = ? AND namespace = ? \
         ORDER BY scope_key, value",
    )
    .bind(resource_id)
    .bind(namespace)
    .fetch_all(pool)
    .await?;
    ensure!(
        aliases.len() == 1,
        "backup resource must have exactly one {namespace} alias"
    );
    Ok(aliases.into_iter().next().expect("length checked"))
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
            .context("backup action has no retry metadata")?
            .class
            .as_str()
            .into(),
        recovery_class: metadata
            .recovery
            .context("backup action has no recovery metadata")?
            .as_str()
            .into(),
    })
}

fn requires_restic(action: &str) -> bool {
    matches!(
        action,
        "backup.run" | "backup.check" | "backup.restore_test"
    )
}

fn plan_title(action: &str) -> Result<&'static str> {
    Ok(match action {
        "backup.config.create" => "Create backup configuration",
        "backup.config.delete" => "Delete backup configuration",
        "backup.run" => "Run backup",
        "backup.check" => "Check backup repository integrity",
        "backup.restore_test" => "Test backup restoration",
        _ => bail!("unsupported backup action"),
    })
}

fn action_label(action: &str) -> Result<&'static str> {
    Ok(match action {
        "backup.config.create" => "Create configuration",
        "backup.config.delete" => "Delete configuration",
        "backup.run" => "Create snapshot",
        "backup.check" => "Integrity check",
        "backup.restore_test" => "Restore test",
        _ => bail!("unsupported backup action"),
    })
}

fn failure_code(action: &str) -> &'static str {
    match action {
        "backup.run" => "backup_run_failed",
        "backup.check" => "backup_check_failed",
        "backup.restore_test" => "backup_restore_test_failed",
        "backup.config.create" => "backup_config_create_failed",
        "backup.config.delete" => "backup_config_delete_failed",
        _ => "backup_operation_failed",
    }
}

fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "read",
        RiskClass::Mutate => "mutate",
        RiskClass::Destructive => "destructive",
        RiskClass::Irreversible => "irreversible",
    }
}

fn safe_text(text: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(text.trim());
    let mut characters = redacted.chars();
    let mut bounded: String = characters.by_ref().take(4 * 1024).collect();
    if characters.next().is_some() {
        bounded.push_str("\n[truncated]");
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{contracts::ResourceRef, resources};
    use std::sync::Mutex;

    struct FakeProvider {
        snapshot: Mutex<BackupSnapshot>,
        calls: Mutex<Vec<String>>,
        result: Mutex<Result<BackupOperationResult, String>>,
        verification: Mutex<BackupVerification>,
    }

    #[async_trait]
    impl BackupProvider for FakeProvider {
        async fn snapshot(
            &self,
            _config_id: Option<&str>,
            _probe_restic: bool,
        ) -> Result<BackupSnapshot> {
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn execute(
            &self,
            operation: BackupOperation,
            _correlation_id: &str,
        ) -> Result<BackupOperationResult> {
            self.calls.lock().unwrap().push(
                match operation {
                    BackupOperation::Create { .. } => "create",
                    BackupOperation::Delete { .. } => "delete",
                    BackupOperation::PrepareRepository { .. } => "prepare_repository",
                    BackupOperation::Run { .. } => "run",
                    BackupOperation::Check { .. } => "check",
                    BackupOperation::RestoreTest { .. } => "restore_test",
                }
                .into(),
            );
            match &*self.result.lock().unwrap() {
                Ok(result) => Ok(result.clone()),
                Err(error) => bail!(error.clone()),
            }
        }

        async fn verify(&self, _operation: &BackupOperation) -> Result<BackupVerification> {
            Ok(self.verification.lock().unwrap().clone())
        }
    }

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    async fn setup(action: &str) -> (BackupsAdapter, Arc<FakeProvider>, ResourceRef) {
        let pool = pool().await;
        let (kind, namespace, alias) = if action == "backup.config.create" {
            ("system", "voidtower.singleton", "system")
        } else {
            ("backup_config", "voidtower.backup_config", "config-1")
        };
        let resource = resources::observe(
            &pool,
            resources::ObserveResource {
                kind,
                display_name: "Daily backup",
                node_id: None,
                provider: Some("test"),
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
            snapshot: Mutex::new(BackupSnapshot {
                configs: if action == "backup.config.create" {
                    vec![]
                } else {
                    vec![BackupConfigSnapshot {
                        id: "config-1".into(),
                        name: "Daily backup".into(),
                        source_path: "/srv/data".into(),
                        repo_path: "/srv/restic".into(),
                        schedule: None,
                        retention_days: 30,
                        enabled: true,
                        restore_test_schedule: None,
                    }]
                },
                restic_available: requires_restic(action).then_some(true),
            }),
            calls: Mutex::new(vec![]),
            result: Mutex::new(Ok(BackupOperationResult {
                config_id: "config-1".into(),
                resource_id: None,
                status: "ok".into(),
                snapshot_id: None,
                output: None,
            })),
            verification: Mutex::new(BackupVerification::Succeeded),
        });
        (
            BackupsAdapter::with_provider(pool, provider.clone()),
            provider,
            resource,
        )
    }

    fn request(action: &str, resource: ResourceRef) -> PlanRequest {
        PlanRequest {
            action: action.into(),
            resource,
            input: if action == "backup.config.create" {
                serde_json::json!({
                    "name": "Daily backup",
                    "source_path": "/srv/data",
                    "repo_path": "/srv/restic"
                })
            } else {
                serde_json::json!({})
            },
        }
    }

    fn step_request(request: &PlanRequest, step: PlannedStepV1) -> StepRequest {
        StepRequest {
            job_id: "00000000-0000-4000-8000-000000000001".into(),
            action: request.action.clone(),
            resource: request.resource.clone(),
            input: request.input.clone(),
            step,
            attempt: 1,
            external_operation_id: None,
        }
    }

    #[tokio::test]
    async fn run_plan_persists_repository_preparation_before_tagged_backup() {
        let (adapter, provider, resource) = setup("backup.run").await;
        let request = request("backup.run", resource);
        let plan = adapter.plan(request.clone()).await.unwrap();
        assert_eq!(
            plan.steps
                .iter()
                .map(|step| step.kind.as_str())
                .collect::<Vec<_>>(),
            ["prepare_repository", "execute"]
        );

        for step in plan.steps {
            adapter
                .execute_step(step_request(&request, step))
                .await
                .unwrap();
        }
        assert_eq!(
            *provider.calls.lock().unwrap(),
            ["prepare_repository", "run"]
        );
    }

    #[tokio::test]
    async fn every_backup_action_plans_and_executes_through_typed_provider() {
        for action in ACTIONS {
            let (adapter, provider, resource) = setup(action).await;
            let request = request(action, resource);
            let plan = adapter.plan(request.clone()).await.unwrap();
            let last = plan.steps.last().unwrap().clone();
            adapter
                .execute_step(step_request(&request, last))
                .await
                .unwrap();
            assert!(!provider.calls.lock().unwrap().is_empty(), "{action}");
        }
    }

    #[tokio::test]
    async fn failed_and_uncertain_provider_results_are_not_reported_as_success() {
        let (adapter, provider, resource) = setup("backup.check").await;
        let request = request("backup.check", resource);
        let step = adapter.plan(request.clone()).await.unwrap().steps.remove(0);
        *provider.result.lock().unwrap() = Ok(BackupOperationResult {
            config_id: "config-1".into(),
            resource_id: None,
            status: "failed".into(),
            snapshot_id: None,
            output: Some("repository is damaged".into()),
        });
        assert!(matches!(
            adapter
                .execute_step(step_request(&request, step.clone()))
                .await
                .unwrap(),
            StepOutcome::Failed {
                retryable: true,
                ..
            }
        ));
        *provider.result.lock().unwrap() = Err("connection disappeared".into());
        assert!(matches!(
            adapter
                .execute_step(step_request(&request, step))
                .await
                .unwrap(),
            StepOutcome::Uncertain { .. }
        ));
    }

    #[tokio::test]
    async fn reconciliation_uses_provider_evidence_without_replaying() {
        let (adapter, provider, resource) = setup("backup.run").await;
        let request = request("backup.run", resource);
        let step = adapter.plan(request.clone()).await.unwrap().steps.remove(1);
        assert!(matches!(
            adapter
                .reconcile(step_request(&request, step.clone()))
                .await
                .unwrap(),
            ReconcileOutcome::Succeeded { .. }
        ));
        *provider.verification.lock().unwrap() = BackupVerification::Failed;
        assert!(matches!(
            adapter
                .reconcile(step_request(&request, step))
                .await
                .unwrap(),
            ReconcileOutcome::Failed { .. }
        ));
        assert!(provider.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn planning_rejects_missing_restic_and_wrong_resource_kind() {
        let (adapter, provider, resource) = setup("backup.check").await;
        provider.snapshot.lock().unwrap().restic_available = Some(false);
        assert!(adapter
            .plan(request("backup.check", resource))
            .await
            .is_err());

        let (adapter, _, mut resource) = setup("backup.config.delete").await;
        resource.kind = "system".into();
        assert!(adapter
            .plan(request("backup.config.delete", resource))
            .await
            .is_err());
    }

    #[test]
    fn job_tags_are_shell_safe_and_messages_are_bounded_and_redacted() {
        assert_eq!(
            operation_tag("00000000-0000-4000-8000-000000000001").unwrap(),
            "voidtower-job-00000000-0000-4000-8000-000000000001"
        );
        assert!(operation_tag("../../unsafe").is_err());
        let value = format!("api_key=known-secret-value {}", "x".repeat(5000));
        let safe = safe_text(&value);
        assert!(!safe.contains("known-secret-value"));
        assert!(safe.ends_with("[truncated]"));
    }

    #[test]
    fn operational_status_does_not_stale_a_backup_definition_fingerprint() {
        let base = BackupConfig {
            id: "config-1".into(),
            name: "Daily backup".into(),
            source_path: "/srv/data".into(),
            repo_path: "/srv/restic".into(),
            schedule: None,
            retention_days: 30,
            enabled: true,
            last_run_at: None,
            last_status: None,
            created_at: 1,
            last_check_at: None,
            last_check_status: None,
            last_restore_test_at: None,
            last_restore_test_status: None,
            restore_test_schedule: None,
        };
        let mut observed = base.clone();
        observed.last_run_at = Some(2);
        observed.last_status = Some("success".into());
        observed.last_check_at = Some(3);
        observed.last_check_status = Some("ok".into());
        assert_eq!(
            BackupConfigSnapshot::from(base),
            BackupConfigSnapshot::from(observed)
        );
    }

    #[test]
    fn compatibility_and_cli_callers_use_the_typed_backup_service_boundary() {
        let callers = format!(
            "{}\n{}",
            include_str!("../../api/backups.rs"),
            include_str!("../../main.rs")
        );
        for low_level in [
            "backups::run_backup(",
            "backups::run_check(",
            "backups::run_restore_test(",
            "backups::init_repo(",
        ] {
            assert!(!callers.contains(low_level), "direct call: {low_level}");
        }
    }
}
