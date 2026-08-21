//! Durable container lifecycle and Docker Compose adapter.
//!
//! Compose content crosses the durable boundary only through an opaque controlled artifact. Plans
//! never persist Compose YAML or caller-selected host paths, and uncertain applies are reconciled
//! from installed content and observed Docker state without replaying `docker compose up`.

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
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncWriteExt;

const ACTIONS: &[&str] = &[
    "container.start",
    "container.stop",
    "container.restart",
    "container.remove",
    "container.compose.apply",
];

const COMPOSE_ACTION: &str = "container.compose.apply";
const COMPOSE_ARTIFACT_DIR: &str = "compose-artifacts";
const COMPOSE_ROLLBACK_DIR: &str = "compose-rollbacks";
const MAX_COMPOSE_BYTES: usize = 1024 * 1024;
const MAX_DOCKER_INSPECT_BYTES: usize = 64 * 1024;
const MAX_PROVIDER_OUTPUT_CHARS: usize = 8 * 1024;
const COMPOSE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeApplyInput {
    pub artifact_ref: String,
    pub artifact_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComposeArtifactMetadata {
    pub artifact_ref: String,
    pub sha256: String,
    pub length: usize,
}

#[derive(Debug, Clone)]
struct ComposeArtifact {
    metadata: ComposeArtifactMetadata,
    content: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComposeProjectSnapshot {
    pub container_id: String,
    pub container_name: String,
    pub container_created: i64,
    pub container_state: String,
    pub project_name: String,
    pub working_dir: PathBuf,
    pub config_path: PathBuf,
    pub current_sha256: String,
    pub current_length: usize,
    pub current_lines: usize,
    #[serde(skip)]
    pub current_content: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComposePreview {
    pub added: usize,
    pub removed: usize,
    pub current_lines: usize,
    pub proposed_lines: usize,
    pub current_bytes: usize,
    pub proposed_bytes: usize,
    pub current_sha256: String,
    pub proposed_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeCommandOutcome {
    Succeeded { output: String },
    Failed { output: String },
    Uncertain { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ComposeFingerprint {
    resource_revision: i64,
    container_id: String,
    container_name: String,
    container_created: i64,
    container_state: String,
    project_name: String,
    config_identity: String,
    current_sha256: String,
    artifact_ref: String,
    artifact_sha256: String,
    artifact_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ComposeRecoveryToken {
    schema_version: u16,
    container_id: String,
    container_created: i64,
    project_name: String,
    original_sha256: String,
    proposed_sha256: String,
}

#[derive(Debug, Clone)]
pub struct ComposeArtifactStore {
    data_root: PathBuf,
    root: PathBuf,
}

impl ComposeArtifactStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_root: data_dir.to_path_buf(),
            root: data_dir.join(COMPOSE_ARTIFACT_DIR),
        }
    }

    pub async fn stage(&self, content: &[u8]) -> Result<ComposeArtifactMetadata> {
        validate_compose_content(content)?;
        let root = controlled_directory(&self.data_root, &self.root, true).await?;
        let artifact_ref = uuid::Uuid::new_v4().to_string();
        let path = root.join(format!("{artifact_ref}.yaml"));
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .await
            .context("cannot create controlled Compose artifact")?;
        restrict_file_permissions(&path).await?;
        file.write_all(content)
            .await
            .context("cannot write controlled Compose artifact")?;
        file.sync_all()
            .await
            .context("cannot sync controlled Compose artifact")?;
        Ok(ComposeArtifactMetadata {
            artifact_ref,
            sha256: sha256_bytes(content),
            length: content.len(),
        })
    }

    async fn resolve(&self, input: &ComposeApplyInput) -> Result<ComposeArtifact> {
        validate_artifact_ref(&input.artifact_ref)?;
        validate_sha256(&input.artifact_sha256)?;
        let root = controlled_directory(&self.data_root, &self.root, false).await?;
        let joined = root.join(format!("{}.yaml", input.artifact_ref));
        let link_metadata = tokio::fs::symlink_metadata(&joined)
            .await
            .context("Compose artifact is missing")?;
        ensure!(
            !link_metadata.file_type().is_symlink(),
            "Compose artifact cannot be a symlink"
        );
        ensure!(
            link_metadata.is_file(),
            "Compose artifact is not a regular file"
        );
        let path = tokio::fs::canonicalize(&joined)
            .await
            .context("cannot resolve Compose artifact")?;
        ensure!(
            path.starts_with(&root),
            "Compose artifact escaped its controlled root"
        );
        ensure_yaml_path(&path)?;
        let content = read_bounded(&path, MAX_COMPOSE_BYTES, "Compose artifact").await?;
        validate_compose_content(&content)?;
        let sha256 = sha256_bytes(&content);
        ensure!(
            sha256 == input.artifact_sha256,
            "Compose artifact digest does not match the submitted digest"
        );
        Ok(ComposeArtifact {
            metadata: ComposeArtifactMetadata {
                artifact_ref: input.artifact_ref.clone(),
                sha256,
                length: content.len(),
            },
            content,
        })
    }
}

#[async_trait]
pub trait ContainerProvider: Send + Sync {
    async fn inspect(&self, native_id: &str) -> Result<Option<ContainerSnapshot>>;
    async fn execute(&self, native_id: &str, action: ContainerAction) -> Result<()>;

    async fn inspect_compose(&self, _native_id: &str) -> Result<Option<ComposeProjectSnapshot>> {
        bail!("Compose inspection is unavailable")
    }

    async fn prepare_compose_rollback(
        &self,
        _job_id: &str,
        _snapshot: &ComposeProjectSnapshot,
    ) -> Result<()> {
        bail!("Compose rollback preparation is unavailable")
    }

    async fn reconcile_compose_rollback(
        &self,
        _job_id: &str,
        _snapshot: &ComposeProjectSnapshot,
    ) -> Result<bool> {
        bail!("Compose rollback reconciliation is unavailable")
    }

    async fn apply_compose(
        &self,
        _snapshot: &ComposeProjectSnapshot,
        _content: &[u8],
    ) -> Result<ComposeCommandOutcome> {
        bail!("Compose apply is unavailable")
    }
}

pub struct DockerContainerProvider {
    data_root: PathBuf,
    rollback_root: PathBuf,
}

impl DockerContainerProvider {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_root: data_dir.to_path_buf(),
            rollback_root: data_dir.join(COMPOSE_ROLLBACK_DIR),
        }
    }
}

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

    async fn inspect_compose(&self, native_id: &str) -> Result<Option<ComposeProjectSnapshot>> {
        let Some(container) = self.inspect(native_id).await? else {
            return Ok(None);
        };
        let output = tokio::process::Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{json .Config.Labels}}",
                "--",
                native_id,
            ])
            .stdin(Stdio::null())
            .output()
            .await
            .context("cannot inspect Docker Compose labels")?;
        ensure!(
            output.status.success(),
            "Docker Compose labels are unavailable"
        );
        ensure!(
            output.stdout.len() <= MAX_DOCKER_INSPECT_BYTES,
            "Docker Compose label response exceeds the size limit"
        );
        let labels: HashMap<String, String> = serde_json::from_slice(&output.stdout)
            .context("Docker Compose labels are malformed")?;
        let (project_name, working_dir, config_path) = compose_identity_from_labels(&labels)?;
        let (working_dir, config_path, content) =
            validate_live_compose_path(&working_dir, &config_path).await?;
        Ok(Some(ComposeProjectSnapshot {
            container_id: container.id,
            container_name: container.name,
            container_created: container.created,
            container_state: container.state,
            project_name,
            working_dir,
            config_path,
            current_sha256: sha256_bytes(&content),
            current_length: content.len(),
            current_lines: count_lines(&content),
            current_content: content,
        }))
    }

    async fn prepare_compose_rollback(
        &self,
        job_id: &str,
        snapshot: &ComposeProjectSnapshot,
    ) -> Result<()> {
        validate_opaque_id(job_id, "job id")?;
        let content = read_bounded(
            &snapshot.config_path,
            MAX_COMPOSE_BYTES,
            "live Compose config",
        )
        .await?;
        ensure!(
            sha256_bytes(&content) == snapshot.current_sha256,
            "live Compose config changed before rollback preparation"
        );
        let root = controlled_directory(&self.data_root, &self.rollback_root, true).await?;
        let path = root.join(format!("{job_id}.yaml"));
        match tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .await
        {
            Ok(mut file) => {
                restrict_file_permissions(&path).await?;
                file.write_all(&content)
                    .await
                    .context("cannot write controlled Compose rollback")?;
                file.sync_all()
                    .await
                    .context("cannot sync controlled Compose rollback")?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = read_bounded(&path, MAX_COMPOSE_BYTES, "Compose rollback").await?;
                ensure!(
                    sha256_bytes(&existing) == snapshot.current_sha256,
                    "existing Compose rollback does not match the planned config"
                );
            }
            Err(error) => return Err(error).context("cannot create controlled Compose rollback"),
        }
        Ok(())
    }

    async fn reconcile_compose_rollback(
        &self,
        job_id: &str,
        snapshot: &ComposeProjectSnapshot,
    ) -> Result<bool> {
        validate_opaque_id(job_id, "job id")?;
        if !tokio::fs::try_exists(&self.rollback_root)
            .await
            .context("cannot inspect controlled Compose rollback directory")?
        {
            return Ok(false);
        }
        let root = controlled_directory(&self.data_root, &self.rollback_root, false).await?;
        let joined = root.join(format!("{job_id}.yaml"));
        let metadata = match tokio::fs::symlink_metadata(&joined).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error).context("cannot inspect Compose rollback"),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Compose rollback is not a controlled regular file"
        );
        let path = tokio::fs::canonicalize(&joined)
            .await
            .context("cannot resolve Compose rollback")?;
        ensure!(
            path.starts_with(&root),
            "Compose rollback escaped its controlled root"
        );
        let content = read_bounded(&path, MAX_COMPOSE_BYTES, "Compose rollback").await?;
        Ok(sha256_bytes(&content) == snapshot.current_sha256)
    }

    async fn apply_compose(
        &self,
        snapshot: &ComposeProjectSnapshot,
        content: &[u8],
    ) -> Result<ComposeCommandOutcome> {
        validate_compose_content(content)?;
        let current = tokio::fs::canonicalize(&snapshot.config_path)
            .await
            .context("cannot revalidate live Compose config")?;
        ensure!(
            current == snapshot.config_path,
            "live Compose config identity changed before apply"
        );
        let parent = snapshot
            .config_path
            .parent()
            .context("live Compose config has no parent directory")?;
        let temporary = parent.join(format!(".voidtower-compose-{}.tmp", uuid::Uuid::new_v4()));
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .await
            .context("cannot create atomic Compose replacement")?;
        file.write_all(content)
            .await
            .context("cannot write atomic Compose replacement")?;
        file.sync_all()
            .await
            .context("cannot sync atomic Compose replacement")?;
        let permissions = tokio::fs::metadata(&snapshot.config_path)
            .await
            .context("cannot inspect live Compose permissions")?
            .permissions();
        tokio::fs::set_permissions(&temporary, permissions)
            .await
            .context("cannot preserve live Compose permissions")?;
        if let Err(error) = tokio::fs::rename(&temporary, &snapshot.config_path).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(error).context("cannot atomically install Compose artifact");
        }

        let mut command = tokio::process::Command::new("docker");
        command
            .args(["compose", "-p"])
            .arg(&snapshot.project_name)
            .arg("-f")
            .arg(&snapshot.config_path)
            .args(["up", "-d", "--remove-orphans"])
            .current_dir(&snapshot.working_dir)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let output = match tokio::time::timeout(COMPOSE_TIMEOUT, command.output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => {
                return Ok(ComposeCommandOutcome::Uncertain {
                    message: "Docker Compose could not report an execution result".into(),
                });
            }
            Err(_) => {
                return Ok(ComposeCommandOutcome::Uncertain {
                    message: "Docker Compose execution timed out with an unknown outcome".into(),
                });
            }
        };
        let safe_output = safe_provider_output(&output.stdout, &output.stderr);
        if output.status.success() {
            Ok(ComposeCommandOutcome::Succeeded {
                output: safe_output,
            })
        } else {
            Ok(ComposeCommandOutcome::Failed {
                output: safe_output,
            })
        }
    }
}

pub struct ContainerAdapter {
    pool: SqlitePool,
    provider: Arc<dyn ContainerProvider>,
    artifacts: ComposeArtifactStore,
    secrets_key: Arc<[u8; 32]>,
}

impl ContainerAdapter {
    pub fn new(pool: SqlitePool, secrets_key: Arc<[u8; 32]>, data_dir: PathBuf) -> Self {
        Self {
            pool,
            provider: Arc::new(DockerContainerProvider::new(&data_dir)),
            artifacts: ComposeArtifactStore::new(&data_dir),
            secrets_key,
        }
    }

    #[cfg(test)]
    fn with_provider(
        pool: SqlitePool,
        provider: Arc<dyn ContainerProvider>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            pool,
            provider,
            artifacts: ComposeArtifactStore::new(&data_dir),
            secrets_key: Arc::new([0u8; 32]),
        }
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

    async fn compose_context(
        &self,
        request: &PlanRequest,
    ) -> Result<(
        String,
        ComposeApplyInput,
        ComposeArtifact,
        ComposeProjectSnapshot,
    )> {
        ensure!(
            request.resource.kind == "container",
            "container adapter received resource kind {}",
            request.resource.kind
        );
        ensure!(
            request.action == COMPOSE_ACTION,
            "unsupported Compose action"
        );
        let input: ComposeApplyInput = serde_json::from_value(request.input.clone())
            .context("invalid container Compose apply input")?;
        validate_artifact_ref(&input.artifact_ref)?;
        validate_sha256(&input.artifact_sha256)?;
        let native_id = self.native_id(&request.resource.id).await?;
        let snapshot = self
            .provider
            .inspect_compose(&native_id)
            .await?
            .context("container is not attached to one supported Docker Compose project")?;
        ensure!(
            snapshot.container_id == native_id,
            "Docker Compose observation did not match the selected container"
        );
        let artifact = self.artifacts.resolve(&input).await?;
        Ok((native_id, input, artifact, snapshot))
    }

    fn compose_fingerprint(
        request: &PlanRequest,
        artifact: &ComposeArtifact,
        snapshot: &ComposeProjectSnapshot,
    ) -> Result<String> {
        let config_identity =
            canonical_json::digest(&snapshot.config_path.to_string_lossy().into_owned())?;
        canonical_json::digest(&ComposeFingerprint {
            resource_revision: request.resource.revision,
            container_id: snapshot.container_id.clone(),
            container_name: snapshot.container_name.clone(),
            container_created: snapshot.container_created,
            container_state: snapshot.container_state.clone(),
            project_name: snapshot.project_name.clone(),
            config_identity,
            current_sha256: snapshot.current_sha256.clone(),
            artifact_ref: artifact.metadata.artifact_ref.clone(),
            artifact_sha256: artifact.metadata.sha256.clone(),
            artifact_length: artifact.metadata.length,
        })
    }

    async fn plan_compose(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        let (_, _, artifact, snapshot) = self.compose_context(&request).await?;
        let metadata = action_registry::action(&request.action)
            .context("Compose action is absent from the action registry")?;
        let preview = compose_preview(&snapshot.current_content, &artifact.content)?;
        let retry_class = metadata
            .retry
            .context("Compose action has no retry metadata")?
            .class
            .as_str()
            .to_string();
        let recovery_class = metadata
            .recovery
            .context("Compose action has no recovery metadata")?
            .as_str()
            .to_string();
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: format!("Apply Compose project {}", snapshot.project_name),
            risk: risk_name(metadata.risk).into(),
            changes: vec![
                PlanChange {
                    label: "Container".into(),
                    value: snapshot.container_name.clone(),
                },
                PlanChange {
                    label: "Compose project".into(),
                    value: snapshot.project_name.clone(),
                },
                PlanChange {
                    label: "Current config".into(),
                    value: format!(
                        "{} bytes, {} lines, sha256 {}",
                        preview.current_bytes, preview.current_lines, preview.current_sha256
                    ),
                },
                PlanChange {
                    label: "Proposed config".into(),
                    value: format!(
                        "{} bytes, {} lines, sha256 {}",
                        preview.proposed_bytes, preview.proposed_lines, preview.proposed_sha256
                    ),
                },
                PlanChange {
                    label: "Line changes".into(),
                    value: format!("+{} -{}", preview.added, preview.removed),
                },
            ],
            preview: None,
            external_fingerprint: Self::compose_fingerprint(&request, &artifact, &snapshot)?,
            steps: vec![
                PlannedStepV1 {
                    kind: "prepare".into(),
                    name: "prepare_rollback".into(),
                    retry_class: retry_class.clone(),
                    recovery_class: recovery_class.clone(),
                },
                PlannedStepV1 {
                    kind: "execute".into(),
                    name: "apply_compose".into(),
                    retry_class,
                    recovery_class,
                },
            ],
        })
    }

    async fn redact_provider_output(&self, output: &str) -> String {
        let encrypted: Vec<String> = sqlx::query_scalar("SELECT value_enc FROM secrets")
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        let known_values: Vec<String> = encrypted
            .into_iter()
            .filter_map(|value| crate::api::secrets::decrypt(&self.secrets_key, &value).ok())
            .collect();
        crate::api::mcp::redact::redact(output, &known_values)
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
        if request.action == COMPOSE_ACTION {
            return self.plan_compose(request).await;
        }
        ensure!(
            request
                .input
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
            "container lifecycle input must be an empty object"
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
        if request.action == COMPOSE_ACTION {
            let (_, _, artifact, snapshot) = self.compose_context(request).await?;
            return Self::compose_fingerprint(request, &artifact, &snapshot);
        }
        let (_, snapshot) = self.snapshot(request).await?;
        canonical_json::digest(&snapshot)
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        ensure!(
            ACTIONS.contains(&request.action.as_str()),
            "unsupported container action"
        );
        if request.action == COMPOSE_ACTION {
            let plan_request = PlanRequest {
                action: request.action.clone(),
                resource: request.resource,
                input: request.input,
            };
            let (_, _, artifact, snapshot) = self.compose_context(&plan_request).await?;
            return match (request.step.kind.as_str(), request.step.name.as_str()) {
                ("prepare", "prepare_rollback") => {
                    match self
                        .provider
                        .prepare_compose_rollback(&request.job_id, &snapshot)
                        .await
                    {
                        Ok(()) => Ok(StepOutcome::Succeeded {
                            result: serde_json::json!({"rollback_prepared": true}),
                            external_operation_id: None,
                        }),
                        Err(_) => Ok(StepOutcome::Failed {
                            code: "compose_rollback_preparation_failed".into(),
                            message: "Unable to prepare the controlled Compose rollback".into(),
                            retryable: false,
                            diagnostic: None,
                        }),
                    }
                }
                ("execute", "apply_compose") => {
                    let recovery_token = serde_json::to_string(&ComposeRecoveryToken {
                        schema_version: 1,
                        container_id: snapshot.container_id.clone(),
                        container_created: snapshot.container_created,
                        project_name: snapshot.project_name.clone(),
                        original_sha256: snapshot.current_sha256.clone(),
                        proposed_sha256: artifact.metadata.sha256.clone(),
                    })?;
                    match self
                        .provider
                        .apply_compose(&snapshot, &artifact.content)
                        .await
                    {
                        Ok(ComposeCommandOutcome::Succeeded { output }) => {
                            let output = self.redact_provider_output(&output).await;
                            Ok(StepOutcome::Succeeded {
                                result: serde_json::json!({
                                    "action": COMPOSE_ACTION,
                                    "container_name": snapshot.container_name,
                                    "project_name": snapshot.project_name,
                                    "provider_output": output,
                                }),
                                external_operation_id: None,
                            })
                        }
                        Ok(ComposeCommandOutcome::Failed { output }) => {
                            let output = self.redact_provider_output(&output).await;
                            Ok(StepOutcome::Failed {
                                code: "compose_provider_failed".into(),
                                message: "Docker Compose reported that the approved apply failed"
                                    .into(),
                                retryable: false,
                                diagnostic: Some(serde_json::json!({"provider_output": output})),
                            })
                        }
                        Ok(ComposeCommandOutcome::Uncertain { message }) => {
                            Ok(StepOutcome::Uncertain {
                                code: "compose_apply_uncertain".into(),
                                message,
                                external_operation_id: Some(recovery_token),
                                diagnostic: None,
                            })
                        }
                        Err(_) => Ok(StepOutcome::Uncertain {
                            code: "compose_apply_uncertain".into(),
                            message: "Docker Compose did not report a conclusive outcome".into(),
                            external_operation_id: Some(recovery_token),
                            diagnostic: None,
                        }),
                    }
                }
                _ => bail!("unsupported Compose step"),
            };
        }
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
        if request.action == COMPOSE_ACTION {
            let input: ComposeApplyInput = serde_json::from_value(request.input)
                .context("invalid container Compose apply input")?;
            validate_sha256(&input.artifact_sha256)?;
            let native_id = self.native_id(&request.resource.id).await?;
            let Some(snapshot) = self.provider.inspect_compose(&native_id).await? else {
                return Ok(ReconcileOutcome::StillUncertain {
                    message: "Docker Compose state is unavailable for reconciliation".into(),
                });
            };
            if request.step.kind == "prepare" && request.step.name == "prepare_rollback" {
                return match self
                    .provider
                    .reconcile_compose_rollback(&request.job_id, &snapshot)
                    .await
                {
                    Ok(true) => Ok(ReconcileOutcome::Succeeded {
                        result: serde_json::json!({"rollback_prepared": true}),
                    }),
                    Ok(false) => Ok(ReconcileOutcome::Failed {
                        code: "compose_rollback_missing".into(),
                        message: "The controlled Compose rollback was not prepared".into(),
                    }),
                    Err(_) => Ok(ReconcileOutcome::StillUncertain {
                        message: "The controlled Compose rollback could not be verified".into(),
                    }),
                };
            }
            ensure!(
                request.step.kind == "execute" && request.step.name == "apply_compose",
                "unsupported Compose reconciliation step"
            );
            if snapshot.current_sha256 == input.artifact_sha256 {
                if let Some(encoded) = request.external_operation_id {
                    let token: ComposeRecoveryToken = serde_json::from_str(&encoded)
                        .context("invalid Compose recovery identity")?;
                    if token.container_id != snapshot.container_id
                        || token.container_created != snapshot.container_created
                        || token.project_name != snapshot.project_name
                        || token.proposed_sha256 != input.artifact_sha256
                    {
                        return Ok(ReconcileOutcome::StillUncertain {
                            message: "Observed Compose identity differs from the attempted apply"
                                .into(),
                        });
                    }
                }
                return Ok(ReconcileOutcome::Succeeded {
                    result: serde_json::json!({
                        "action": COMPOSE_ACTION,
                        "container_name": snapshot.container_name,
                        "project_name": snapshot.project_name,
                    }),
                });
            }
            if let Some(encoded) = request.external_operation_id {
                let token: ComposeRecoveryToken =
                    serde_json::from_str(&encoded).context("invalid Compose recovery identity")?;
                if token.container_id == snapshot.container_id
                    && token.container_created == snapshot.container_created
                    && token.project_name == snapshot.project_name
                    && token.original_sha256 == snapshot.current_sha256
                {
                    return Ok(ReconcileOutcome::Failed {
                        code: "compose_artifact_not_installed".into(),
                        message: "The approved Compose artifact was not installed".into(),
                    });
                }
            }
            return Ok(ReconcileOutcome::StillUncertain {
                message: "Installed Compose content cannot prove the apply outcome".into(),
            });
        }
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

pub async fn preview_compose_change(
    data_dir: &Path,
    native_id: &str,
    asserted_path: &Path,
    proposed: &[u8],
) -> Result<ComposePreview> {
    let provider = DockerContainerProvider::new(data_dir);
    let snapshot = provider
        .inspect_compose(native_id)
        .await?
        .context("container is not attached to one supported Docker Compose project")?;
    validate_asserted_compose_path(asserted_path, &snapshot.config_path).await?;
    compose_preview(&snapshot.current_content, proposed)
}

fn compose_preview(current: &[u8], proposed: &[u8]) -> Result<ComposePreview> {
    validate_compose_content(current)?;
    validate_compose_content(proposed)?;
    let current_text = std::str::from_utf8(current).context("current Compose YAML is not UTF-8")?;
    let proposed_text =
        std::str::from_utf8(proposed).context("proposed Compose YAML is not UTF-8")?;
    let current_lines: Vec<&str> = current_text.lines().collect();
    let proposed_lines: Vec<&str> = proposed_text.lines().collect();
    Ok(ComposePreview {
        added: proposed_lines
            .iter()
            .filter(|line| !current_lines.contains(line))
            .count(),
        removed: current_lines
            .iter()
            .filter(|line| !proposed_lines.contains(line))
            .count(),
        current_lines: current_lines.len(),
        proposed_lines: proposed_lines.len(),
        current_bytes: current.len(),
        proposed_bytes: proposed.len(),
        current_sha256: sha256_bytes(current),
        proposed_sha256: sha256_bytes(proposed),
    })
}

async fn validate_live_compose_path(
    working_dir: &Path,
    config_path: &Path,
) -> Result<(PathBuf, PathBuf, Vec<u8>)> {
    ensure!(
        working_dir.is_absolute(),
        "Docker Compose working directory must be absolute"
    );
    ensure!(
        config_path.is_absolute(),
        "Docker Compose config path must be absolute"
    );
    ensure_yaml_path(config_path)?;
    let working_dir = tokio::fs::canonicalize(working_dir)
        .await
        .context("cannot resolve Docker Compose working directory")?;
    let metadata = tokio::fs::symlink_metadata(config_path)
        .await
        .context("Docker Compose config is missing")?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "Docker Compose config cannot be a symlink"
    );
    ensure!(
        metadata.is_file(),
        "Docker Compose config is not a regular file"
    );
    let config_path = tokio::fs::canonicalize(config_path)
        .await
        .context("cannot resolve Docker Compose config")?;
    ensure!(
        config_path.starts_with(&working_dir),
        "Docker Compose config escaped its observed working directory"
    );
    ensure_yaml_path(&config_path)?;
    let content = read_bounded(&config_path, MAX_COMPOSE_BYTES, "live Compose config").await?;
    validate_compose_content(&content)?;
    Ok((working_dir, config_path, content))
}

async fn validate_asserted_compose_path(asserted_path: &Path, observed_path: &Path) -> Result<()> {
    ensure!(asserted_path.is_absolute(), "Compose path must be absolute");
    let asserted = tokio::fs::canonicalize(asserted_path)
        .await
        .context("cannot resolve asserted Compose path")?;
    ensure!(
        asserted == observed_path,
        "asserted Compose path does not match the selected container"
    );
    Ok(())
}

async fn controlled_directory(data_dir: &Path, directory: &Path, create: bool) -> Result<PathBuf> {
    let data_root = tokio::fs::canonicalize(data_dir)
        .await
        .context("cannot resolve VoidTower data directory")?;
    if create {
        tokio::fs::create_dir_all(directory)
            .await
            .context("cannot create controlled Compose directory")?;
    }
    let directory = tokio::fs::canonicalize(directory)
        .await
        .context("controlled Compose directory is unavailable")?;
    ensure!(
        directory.starts_with(&data_root),
        "controlled Compose directory escaped the VoidTower data directory"
    );
    Ok(directory)
}

async fn read_bounded(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>> {
    let metadata = tokio::fs::metadata(path)
        .await
        .with_context(|| format!("cannot inspect {label}"))?;
    ensure!(metadata.is_file(), "{label} is not a regular file");
    let length = usize::try_from(metadata.len()).context("file length exceeds platform limits")?;
    ensure!(length <= limit, "{label} exceeds the size limit");
    let content = tokio::fs::read(path)
        .await
        .with_context(|| format!("cannot read {label}"))?;
    ensure!(content.len() <= limit, "{label} exceeds the size limit");
    Ok(content)
}

#[cfg(unix)]
async fn restrict_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .await
        .context("cannot restrict controlled Compose file permissions")
}

#[cfg(not(unix))]
async fn restrict_file_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn validate_compose_content(content: &[u8]) -> Result<()> {
    ensure!(!content.is_empty(), "Compose YAML is empty");
    ensure!(
        content.len() <= MAX_COMPOSE_BYTES,
        "Compose YAML exceeds the size limit"
    );
    let document: serde_yaml::Value =
        serde_yaml::from_slice(content).context("Compose YAML is invalid")?;
    let mapping = document
        .as_mapping()
        .context("Compose YAML must be a mapping")?;
    let services = mapping
        .get(serde_yaml::Value::String("services".into()))
        .context("Compose YAML must define services")?;
    ensure!(
        services.as_mapping().is_some(),
        "Compose services must be a mapping"
    );
    reject_literal_secret_values(&document, false)?;
    Ok(())
}

fn reject_literal_secret_values(
    value: &serde_yaml::Value,
    secret_names_are_ids: bool,
) -> Result<()> {
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let key = key.as_str().unwrap_or_default();
                if !secret_names_are_ids && is_secret_key(key) && !is_secret_reference_value(value)
                {
                    bail!("Compose YAML contains a literal secret value");
                }
                reject_literal_secret_values(value, key.eq_ignore_ascii_case("secrets"))?;
            }
        }
        serde_yaml::Value::Sequence(values) => {
            for value in values {
                reject_literal_secret_values(value, false)?;
            }
        }
        serde_yaml::Value::String(value) => {
            if let Some((key, candidate)) = value.split_once('=') {
                if is_secret_key(key) && !is_variable_reference(candidate) {
                    bail!("Compose YAML contains a literal secret value");
                }
            }
            if !is_variable_reference(value)
                && crate::api::mcp::redact::redact_patterns(value) != *value
            {
                bail!("Compose YAML contains secret-shaped literal content");
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_secret_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', '.'], "_");
    [
        "password",
        "passwd",
        "api_key",
        "secret_key",
        "secret",
        "access_key",
        "private_key",
        "auth_token",
        "token",
        "credential",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn is_secret_reference_value(value: &serde_yaml::Value) -> bool {
    match value {
        serde_yaml::Value::Null => true,
        serde_yaml::Value::String(value) => is_variable_reference(value),
        _ => false,
    }
}

fn is_variable_reference(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("${") && value.ends_with('}') && value.len() > 3
}

fn ensure_yaml_path(path: &Path) -> Result<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    ensure!(
        matches!(extension, "yml" | "yaml"),
        "Compose config must be a YAML file"
    );
    Ok(())
}

fn validate_artifact_ref(value: &str) -> Result<()> {
    validate_opaque_id(value, "Compose artifact reference")
}

fn validate_opaque_id(value: &str, label: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(value).with_context(|| format!("invalid {label}"))?;
    ensure!(parsed.to_string() == value, "invalid {label}");
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "Compose artifact digest must be lowercase SHA-256"
    );
    Ok(())
}

fn sha256_bytes(content: &[u8]) -> String {
    hex::encode(Sha256::digest(content))
}

fn count_lines(content: &[u8]) -> usize {
    String::from_utf8_lossy(content).lines().count()
}

fn required_label(labels: &HashMap<String, String>, name: &str) -> Result<String> {
    let value = labels
        .get(name)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("Docker Compose label {name} is missing"))?;
    Ok(value.to_string())
}

fn compose_identity_from_labels(
    labels: &HashMap<String, String>,
) -> Result<(String, PathBuf, PathBuf)> {
    let project_name = required_label(labels, "com.docker.compose.project")?;
    let working_dir = PathBuf::from(required_label(
        labels,
        "com.docker.compose.project.working_dir",
    )?);
    let config_files = required_label(labels, "com.docker.compose.project.config_files")?;
    let paths: Vec<&str> = config_files
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .collect();
    ensure!(
        paths.len() == 1,
        "exactly one Docker Compose config file is required"
    );
    Ok((project_name, working_dir, PathBuf::from(paths[0])))
}

fn safe_provider_output(stdout: &[u8], stderr: &[u8]) -> String {
    let combined = match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!(
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(stdout),
            String::from_utf8_lossy(stderr)
        ),
        (false, true) => String::from_utf8_lossy(stdout).into_owned(),
        (true, false) => String::from_utf8_lossy(stderr).into_owned(),
        (true, true) => String::new(),
    };
    let redacted = crate::api::mcp::redact::redact_patterns(&combined);
    let mut chars = redacted.chars();
    let mut bounded: String = chars.by_ref().take(MAX_PROVIDER_OUTPUT_CHARS).collect();
    if chars.next().is_some() {
        bounded.push_str("…[truncated]");
    }
    bounded
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
            ContainerAdapter::with_provider(
                pool,
                provider.clone(),
                std::env::temp_dir()
                    .join(format!("voidtower-container-test-{}", uuid::Uuid::new_v4())),
            ),
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
    async fn lifecycle_planning_rejects_nonempty_input() {
        let (adapter, _, resource) = setup().await;
        let result = adapter
            .plan(PlanRequest {
                action: "container.start".into(),
                resource,
                input: serde_json::json!({"password": "must-not-persist"}),
            })
            .await;

        assert!(result.is_err());
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

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let path = std::env::temp_dir()
                .join(format!("voidtower-compose-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct FakeComposeProvider {
        snapshot: Mutex<Option<ComposeProjectSnapshot>>,
        calls: Mutex<Vec<String>>,
        rollback_ready: Mutex<bool>,
        outcome: Mutex<ComposeCommandOutcome>,
    }

    #[async_trait]
    impl ContainerProvider for FakeComposeProvider {
        async fn inspect(&self, _native_id: &str) -> Result<Option<ContainerSnapshot>> {
            Ok(self
                .snapshot
                .lock()
                .unwrap()
                .as_ref()
                .map(|snapshot| ContainerSnapshot {
                    id: snapshot.container_id.clone(),
                    name: snapshot.container_name.clone(),
                    image: "fixture:stable".into(),
                    state: snapshot.container_state.clone(),
                    created: snapshot.container_created,
                }))
        }

        async fn execute(&self, _native_id: &str, _action: ContainerAction) -> Result<()> {
            bail!("lifecycle execution is not used by Compose tests")
        }

        async fn inspect_compose(
            &self,
            _native_id: &str,
        ) -> Result<Option<ComposeProjectSnapshot>> {
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn prepare_compose_rollback(
            &self,
            _job_id: &str,
            _snapshot: &ComposeProjectSnapshot,
        ) -> Result<()> {
            self.calls.lock().unwrap().push("prepare_rollback".into());
            *self.rollback_ready.lock().unwrap() = true;
            Ok(())
        }

        async fn reconcile_compose_rollback(
            &self,
            _job_id: &str,
            _snapshot: &ComposeProjectSnapshot,
        ) -> Result<bool> {
            Ok(*self.rollback_ready.lock().unwrap())
        }

        async fn apply_compose(
            &self,
            _snapshot: &ComposeProjectSnapshot,
            _content: &[u8],
        ) -> Result<ComposeCommandOutcome> {
            self.calls.lock().unwrap().push("apply_compose".into());
            Ok(self.outcome.lock().unwrap().clone())
        }
    }

    async fn setup_compose() -> (
        ContainerAdapter,
        Arc<FakeComposeProvider>,
        crate::operations::contracts::ResourceRef,
        ComposeApplyInput,
        TestDir,
    ) {
        let directory = TestDir::new();
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
                alias: "compose-container-id",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let current = b"services:\n  web:\n    image: fixture:one\n".to_vec();
        let working_dir = directory.0.join("live");
        std::fs::create_dir_all(&working_dir).unwrap();
        let config_path = working_dir.join("compose.yaml");
        std::fs::write(&config_path, &current).unwrap();
        let provider = Arc::new(FakeComposeProvider {
            snapshot: Mutex::new(Some(ComposeProjectSnapshot {
                container_id: "compose-container-id".into(),
                container_name: "web".into(),
                container_created: 11,
                container_state: "running".into(),
                project_name: "fixture-project".into(),
                working_dir,
                config_path,
                current_sha256: sha256_bytes(&current),
                current_length: current.len(),
                current_lines: count_lines(&current),
                current_content: current,
            })),
            calls: Mutex::new(vec![]),
            rollback_ready: Mutex::new(false),
            outcome: Mutex::new(ComposeCommandOutcome::Succeeded {
                output: "updated".into(),
            }),
        });
        let adapter = ContainerAdapter::with_provider(pool, provider.clone(), directory.0.clone());
        let proposed = b"services:\n  web:\n    image: fixture:two\n";
        let metadata = adapter.artifacts.stage(proposed).await.unwrap();
        let input = ComposeApplyInput {
            artifact_ref: metadata.artifact_ref,
            artifact_sha256: metadata.sha256,
        };
        (adapter, provider, resource, input, directory)
    }

    fn compose_request(
        resource: crate::operations::contracts::ResourceRef,
        input: &ComposeApplyInput,
    ) -> PlanRequest {
        PlanRequest {
            action: COMPOSE_ACTION.into(),
            resource,
            input: serde_json::to_value(input).unwrap(),
        }
    }

    #[tokio::test]
    async fn compose_plan_is_stable_redacted_and_has_ordered_steps() {
        let (adapter, _, resource, input, _directory) = setup_compose().await;
        let request = compose_request(resource, &input);
        let first = adapter.plan(request.clone()).await.unwrap();
        let second = adapter.plan(request.clone()).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.external_fingerprint,
            adapter.external_fingerprint(&request).await.unwrap()
        );
        assert_eq!(first.steps.len(), 2);
        assert_eq!(first.steps[0].name, "prepare_rollback");
        assert_eq!(first.steps[1].name, "apply_compose");
        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains("fixture:two"));
        assert!(!encoded.contains("compose.yaml"));
    }

    #[tokio::test]
    async fn compose_plan_rejects_ambiguous_scoped_container_aliases() {
        let (adapter, _, resource, input, _directory) = setup_compose().await;
        sqlx::query(
            "INSERT INTO resource_aliases \
             (resource_id, namespace, scope_key, value, created_at, last_seen_at) \
             VALUES (?, 'docker.container', 'other-engine', 'other-container-id', 0, 0)",
        )
        .bind(&resource.id)
        .execute(&adapter.pool)
        .await
        .unwrap();
        let error = adapter
            .plan(compose_request(resource, &input))
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("exactly one docker.container alias"));
    }

    #[tokio::test]
    async fn compose_execution_prepares_rollback_before_apply() {
        let (adapter, provider, resource, input, _directory) = setup_compose().await;
        let request = compose_request(resource.clone(), &input);
        let plan = adapter.plan(request.clone()).await.unwrap();
        let job_id = uuid::Uuid::new_v4().to_string();
        for step in plan.steps {
            let outcome = adapter
                .execute_step(StepRequest {
                    job_id: job_id.clone(),
                    action: COMPOSE_ACTION.into(),
                    resource: resource.clone(),
                    input: request.input.clone(),
                    step,
                    attempt: 1,
                    external_operation_id: None,
                })
                .await
                .unwrap();
            assert!(matches!(outcome, StepOutcome::Succeeded { .. }));
        }
        assert_eq!(
            provider.calls.lock().unwrap().as_slice(),
            &["prepare_rollback", "apply_compose"]
        );
    }

    #[tokio::test]
    async fn compose_provider_failure_and_uncertainty_are_classified_without_retry() {
        let (adapter, provider, resource, input, _directory) = setup_compose().await;
        let request = compose_request(resource.clone(), &input);
        let step = adapter.plan(request.clone()).await.unwrap().steps[1].clone();
        *provider.outcome.lock().unwrap() = ComposeCommandOutcome::Failed {
            output: "provider rejected update".into(),
        };
        let failed = adapter
            .execute_step(StepRequest {
                job_id: uuid::Uuid::new_v4().to_string(),
                action: COMPOSE_ACTION.into(),
                resource: resource.clone(),
                input: request.input.clone(),
                step: step.clone(),
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(
            failed,
            StepOutcome::Failed {
                retryable: false,
                ..
            }
        ));

        *provider.outcome.lock().unwrap() = ComposeCommandOutcome::Uncertain {
            message: "result lost".into(),
        };
        let uncertain = adapter
            .execute_step(StepRequest {
                job_id: uuid::Uuid::new_v4().to_string(),
                action: COMPOSE_ACTION.into(),
                resource,
                input: request.input,
                step,
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(
            uncertain,
            StepOutcome::Uncertain {
                external_operation_id: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn compose_provider_output_redacts_values_from_the_secret_store() {
        let (adapter, _, _, _, _directory) = setup_compose().await;
        let secret_value = ["known", "fixture", "value"].join("-");
        let encrypted = crate::api::secrets::encrypt(&[0u8; 32], &secret_value).unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) \
             VALUES ('compose-secret-fixture', 'compose-output-fixture', ?, 0, 0)",
        )
        .bind(encrypted)
        .execute(&adapter.pool)
        .await
        .unwrap();
        let safe = adapter
            .redact_provider_output(&format!("provider echoed {secret_value}"))
            .await;
        assert!(!safe.contains(&secret_value));
        assert!(safe.contains("[REDACTED]"));
    }

    #[tokio::test]
    async fn compose_reconciliation_proves_success_failure_or_uncertainty_without_apply() {
        let (adapter, provider, resource, input, _directory) = setup_compose().await;
        let original = provider.snapshot.lock().unwrap().clone().unwrap();
        let token = serde_json::to_string(&ComposeRecoveryToken {
            schema_version: 1,
            container_id: original.container_id.clone(),
            container_created: original.container_created,
            project_name: original.project_name.clone(),
            original_sha256: original.current_sha256.clone(),
            proposed_sha256: input.artifact_sha256.clone(),
        })
        .unwrap();
        let request = |external_operation_id: Option<String>| StepRequest {
            job_id: uuid::Uuid::new_v4().to_string(),
            action: COMPOSE_ACTION.into(),
            resource: resource.clone(),
            input: serde_json::to_value(&input).unwrap(),
            step: PlannedStepV1 {
                kind: "execute".into(),
                name: "apply_compose".into(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
            },
            attempt: 2,
            external_operation_id,
        };

        let failed = adapter
            .reconcile(request(Some(token.clone())))
            .await
            .unwrap();
        assert!(matches!(failed, ReconcileOutcome::Failed { .. }));

        {
            let mut snapshot = provider.snapshot.lock().unwrap();
            let snapshot = snapshot.as_mut().unwrap();
            snapshot.current_sha256 = input.artifact_sha256.clone();
            snapshot.current_content = b"services:\n  web:\n    image: fixture:two\n".to_vec();
        }
        let succeeded = adapter.reconcile(request(None)).await.unwrap();
        assert!(matches!(succeeded, ReconcileOutcome::Succeeded { .. }));

        provider
            .snapshot
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .current_sha256 = sha256_bytes(b"diverged");
        let uncertain = adapter.reconcile(request(Some(token))).await.unwrap();
        assert!(matches!(uncertain, ReconcileOutcome::StillUncertain { .. }));
        assert!(provider.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rollback_reconciliation_verifies_preparation_without_replaying_it() {
        let (adapter, provider, resource, input, _directory) = setup_compose().await;
        let step = PlannedStepV1 {
            kind: "prepare".into(),
            name: "prepare_rollback".into(),
            retry_class: "never".into(),
            recovery_class: "reconcile".into(),
        };
        let request = || StepRequest {
            job_id: uuid::Uuid::new_v4().to_string(),
            action: COMPOSE_ACTION.into(),
            resource: resource.clone(),
            input: serde_json::to_value(&input).unwrap(),
            step: step.clone(),
            attempt: 2,
            external_operation_id: None,
        };
        let missing = adapter.reconcile(request()).await.unwrap();
        assert!(matches!(missing, ReconcileOutcome::Failed { .. }));
        *provider.rollback_ready.lock().unwrap() = true;
        let present = adapter.reconcile(request()).await.unwrap();
        assert!(matches!(present, ReconcileOutcome::Succeeded { .. }));
        assert!(provider.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn artifact_store_rejects_changed_missing_traversal_and_oversized_content() {
        let directory = TestDir::new();
        let store = ComposeArtifactStore::new(&directory.0);
        let content = b"services:\n  web:\n    image: fixture:one\n";
        let metadata = store.stage(content).await.unwrap();
        let input = ComposeApplyInput {
            artifact_ref: metadata.artifact_ref.clone(),
            artifact_sha256: metadata.sha256,
        };
        assert!(store.resolve(&input).await.is_ok());
        let path = store.root.join(format!("{}.yaml", metadata.artifact_ref));
        tokio::fs::write(&path, b"services:\n  web:\n    image: changed\n")
            .await
            .unwrap();
        assert!(store.resolve(&input).await.is_err());
        tokio::fs::remove_file(path).await.unwrap();
        assert!(store.resolve(&input).await.is_err());

        let traversal = ComposeApplyInput {
            artifact_ref: "../outside".into(),
            artifact_sha256: sha256_bytes(content),
        };
        assert!(store.resolve(&traversal).await.is_err());
        let oversized = vec![b'x'; MAX_COMPOSE_BYTES + 1];
        assert!(store.stage(&oversized).await.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn artifact_store_and_live_config_reject_symlinks_and_escape() {
        use std::os::unix::fs::symlink;

        let escaped_data = TestDir::new();
        let outside_root = TestDir::new();
        symlink(&outside_root.0, escaped_data.0.join(COMPOSE_ARTIFACT_DIR)).unwrap();
        let escaped_store = ComposeArtifactStore::new(&escaped_data.0);
        assert!(escaped_store
            .stage(b"services:\n  web: {}\n")
            .await
            .is_err());

        let directory = TestDir::new();
        let store = ComposeArtifactStore::new(&directory.0);
        tokio::fs::create_dir_all(&store.root).await.unwrap();
        let reference = uuid::Uuid::new_v4().to_string();
        let outside = directory.0.join("outside.yaml");
        tokio::fs::write(&outside, b"services:\n  web: {}\n")
            .await
            .unwrap();
        symlink(&outside, store.root.join(format!("{reference}.yaml"))).unwrap();
        assert!(store
            .resolve(&ComposeApplyInput {
                artifact_ref: reference,
                artifact_sha256: sha256_bytes(b"services:\n  web: {}\n"),
            })
            .await
            .is_err());

        let working = directory.0.join("working");
        tokio::fs::create_dir_all(&working).await.unwrap();
        let linked = working.join("compose.yaml");
        symlink(&outside, &linked).unwrap();
        assert!(validate_live_compose_path(&working, &linked).await.is_err());
        assert!(validate_live_compose_path(&working, &outside)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn compose_labels_and_asserted_path_are_bound_to_one_observed_config() {
        let directory = TestDir::new();
        let working = directory.0.join("working");
        tokio::fs::create_dir_all(&working).await.unwrap();
        let observed = working.join("compose.yaml");
        let other = working.join("other.yaml");
        tokio::fs::write(&observed, b"services:\n  web: {}\n")
            .await
            .unwrap();
        tokio::fs::write(&other, b"services:\n  other: {}\n")
            .await
            .unwrap();
        let mut labels = HashMap::from([
            ("com.docker.compose.project".into(), "fixture".into()),
            (
                "com.docker.compose.project.working_dir".into(),
                working.to_string_lossy().into_owned(),
            ),
            (
                "com.docker.compose.project.config_files".into(),
                observed.to_string_lossy().into_owned(),
            ),
        ]);
        let (_, _, config) = compose_identity_from_labels(&labels).unwrap();
        assert_eq!(config, observed);
        let canonical = tokio::fs::canonicalize(&observed).await.unwrap();
        validate_asserted_compose_path(&observed, &canonical)
            .await
            .unwrap();
        assert!(validate_asserted_compose_path(&other, &canonical)
            .await
            .is_err());

        labels.insert(
            "com.docker.compose.project.config_files".into(),
            format!("{},{}", observed.display(), other.display()),
        );
        assert!(compose_identity_from_labels(&labels).is_err());
    }

    #[test]
    fn compose_validation_and_provider_output_are_bounded_and_redacted() {
        assert!(validate_compose_content(b"not-a-mapping").is_err());
        assert!(validate_compose_content(b"services: []\n").is_err());
        let secret_key = ["api", "key"].join("_");
        let literal =
            format!("services:\n  web:\n    environment:\n      {secret_key}: fixture-value\n");
        assert!(validate_compose_content(literal.as_bytes()).is_err());
        let reference = format!(
            "services:\n  web:\n    environment:\n      {secret_key}: ${{FIXTURE_VALUE}}\n"
        );
        assert!(validate_compose_content(reference.as_bytes()).is_ok());
        let secret_value = ["fixture", "value"].join("-");
        let raw = format!(
            "{}={} {}",
            ["api", "key"].join("_"),
            secret_value,
            "x".repeat(MAX_PROVIDER_OUTPUT_CHARS + 10)
        );
        let safe = safe_provider_output(raw.as_bytes(), &[]);
        assert!(!safe.contains(&secret_value));
        assert!(safe.ends_with("[truncated]"));
        assert!(safe.chars().count() <= MAX_PROVIDER_OUTPUT_CHARS + "…[truncated]".chars().count());
    }

    #[test]
    fn compatibility_compose_routes_cannot_stage_or_apply_outside_the_adapter() {
        let source = include_str!("../../api/containers.rs");
        assert!(!source.contains("tokio::fs::write(&proposed_path"));
        assert!(!source.contains("tokio::fs::rename(proposed_path"));
        assert!(!source.contains(".args([\"compose\", \"up\""));
        assert!(source.contains("preview_compose_change("));
        assert!(source.contains("Durable Compose submission is not enabled yet"));
    }
}
