//! Durable Proxmox plans and restart-safe task reconciliation.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::{
        mcp::action_registry::{self, RiskClass},
        secrets,
    },
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::io::AsyncReadExt;

const ACTIONS: &[&str] = &[
    "proxmox.host.create",
    "proxmox.host.configure",
    "proxmox.host.delete",
    "proxmox.host.test",
    "proxmox.guest.start",
    "proxmox.guest.stop",
    "proxmox.guest.shutdown",
    "proxmox.guest.reboot",
    "proxmox.guest.reset",
    "proxmox.guest.suspend",
    "proxmox.guest.resume",
    "proxmox.snapshot.create",
    "proxmox.snapshot.rollback",
    "proxmox.snapshot.delete",
    "proxmox.disk.attach",
    "proxmox.lxc.deploy",
    "proxmox.storage.upload",
    "proxmox.storage.delete",
    "proxmox.disk.wipe",
    "proxmox.disk.initialize",
];

#[derive(Debug, Clone, Serialize)]
struct HostSnapshot {
    id: String,
    name: String,
    url: String,
    node: String,
    fingerprint: Option<String>,
    token_version: i64,
}

#[derive(Debug, Clone)]
struct HostAccess {
    id: String,
    name: String,
    url: String,
    node: String,
    fingerprint: Option<String>,
    token: String,
    token_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum GuestKind {
    Qemu,
    Lxc,
}

impl GuestKind {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "qemu" => Ok(Self::Qemu),
            "lxc" => Ok(Self::Lxc),
            _ => bail!("invalid Proxmox guest kind"),
        }
    }
    fn path(self) -> &'static str {
        match self {
            Self::Qemu => "qemu",
            Self::Lxc => "lxc",
        }
    }
}

#[derive(Debug, Clone)]
enum Operation {
    HostCreate {
        id: String,
        name: String,
        url: String,
        node: String,
        fingerprint: Option<String>,
        token_secret_id: String,
    },
    HostConfigure {
        id: String,
        name: Option<String>,
        url: Option<String>,
        node: Option<String>,
        fingerprint: Option<String>,
        token_secret_id: Option<String>,
    },
    HostDelete {
        host_id: String,
    },
    HostTest {
        host_id: String,
    },
    Guest {
        host_id: String,
        node: String,
        kind: GuestKind,
        vmid: u64,
        action: String,
    },
    Snapshot {
        host_id: String,
        node: String,
        kind: GuestKind,
        vmid: u64,
        action: String,
        name: String,
        description: Option<String>,
    },
    AttachDisk {
        host_id: String,
        node: String,
        vmid: u64,
        disk_path: String,
        bus: String,
    },
    DeployLxc {
        host_id: String,
        node: String,
        hostname: String,
        ostemplate: String,
        cores: u64,
        memory: u64,
        storage: String,
        disk_gb: u64,
    },
    StorageUpload {
        host_id: String,
        node: String,
        storage: String,
        content: String,
        staged_path: PathBuf,
    },
    StorageDelete {
        host_id: String,
        node: String,
        storage: String,
        volid: String,
    },
    DiskWipe {
        host_id: String,
        node: String,
        disk: String,
    },
    DiskInitialize {
        host_id: String,
        node: String,
        disk: String,
        fstype: String,
        name: String,
        raidlevel: Option<String>,
    },
}

impl Operation {
    fn host_id(&self) -> Option<&str> {
        match self {
            Self::HostCreate { .. } => None,
            Self::HostConfigure { id, .. } => Some(id),
            Self::HostDelete { host_id }
            | Self::HostTest { host_id }
            | Self::Guest { host_id, .. }
            | Self::Snapshot { host_id, .. }
            | Self::AttachDisk { host_id, .. }
            | Self::DeployLxc { host_id, .. }
            | Self::StorageUpload { host_id, .. }
            | Self::StorageDelete { host_id, .. }
            | Self::DiskWipe { host_id, .. }
            | Self::DiskInitialize { host_id, .. } => Some(host_id),
        }
    }
    fn node(&self) -> Option<&str> {
        match self {
            Self::Guest { node, .. }
            | Self::Snapshot { node, .. }
            | Self::AttachDisk { node, .. }
            | Self::DeployLxc { node, .. }
            | Self::StorageUpload { node, .. }
            | Self::StorageDelete { node, .. }
            | Self::DiskWipe { node, .. }
            | Self::DiskInitialize { node, .. } => Some(node),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Execution {
    Complete(Value),
    Submitted { upid: String, result: Value },
}

#[derive(Debug, Clone, PartialEq)]
enum TaskState {
    Running,
    Succeeded,
    Failed(String),
}

#[async_trait]
trait ProxmoxProvider: Send + Sync {
    async fn snapshot(&self, operation: &Operation) -> Result<Value>;
    async fn execute(&self, operation: Operation, correlation_id: &str) -> Result<Execution>;
    async fn task_status(&self, host_id: &str, node: &str, upid: &str) -> Result<TaskState>;
}

struct HttpProxmoxProvider {
    pool: SqlitePool,
    secrets_key: Arc<[u8; 32]>,
}

impl HttpProxmoxProvider {
    async fn secret_version(&self, id: &str) -> Result<i64> {
        sqlx::query_scalar("SELECT version FROM secrets WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .context("token_secret_id does not reference a secret")
    }

    async fn host(&self, id: &str) -> Result<HostAccess> {
        let row: (String, String, String, Option<String>) =
            sqlx::query_as("SELECT name, url, node, fingerprint FROM proxmox_hosts WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .context("Proxmox host is not configured")?;
        let secret_name = format!("proxmox_token_{id}");
        let (value_enc, version): (String, i64) =
            sqlx::query_as("SELECT value_enc, version FROM secrets WHERE name = ?")
                .bind(secret_name)
                .fetch_optional(&self.pool)
                .await?
                .context("Proxmox host token is not configured")?;
        let token = secrets::decrypt(&self.secrets_key, &value_enc)
            .context("Proxmox token decryption failed")?;
        Ok(HostAccess {
            id: id.into(),
            name: row.0,
            url: row.1,
            node: row.2,
            fingerprint: row.3,
            token,
            token_version: version,
        })
    }

    fn client() -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(30))
            .build()?)
    }

    async fn request(
        &self,
        host: &HostAccess,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}/api2/json{}", host.url.trim_end_matches('/'), path);
        let mut request = Self::client()?
            .request(method, url)
            .header("Authorization", format!("PVEAPIToken={}", host.token));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.context("Proxmox request failed")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("Proxmox returned invalid JSON")?;
        ensure!(status.is_success(), "Proxmox returned HTTP {status}");
        Ok(body.get("data").cloned().unwrap_or(Value::Null))
    }

    async fn copy_token(&self, host_id: &str, secret_id: &str, description: &str) -> Result<()> {
        let (value_enc, version): (String, i64) =
            sqlx::query_as("SELECT value_enc, version FROM secrets WHERE id = ?")
                .bind(secret_id)
                .fetch_optional(&self.pool)
                .await?
                .context("token_secret_id does not reference a secret")?;
        let now = crate::operations::unix_now();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at, version) \
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT(name) DO UPDATE SET value_enc = excluded.value_enc, \
             description = excluded.description, updated_at = excluded.updated_at, version = secrets.version + 1",
        ).bind(uuid::Uuid::new_v4().to_string()).bind(format!("proxmox_token_{host_id}"))
            .bind(description).bind(value_enc).bind(now).bind(now).bind(version).execute(&self.pool).await?;
        Ok(())
    }

    async fn remote_snapshot(&self, operation: &Operation) -> Result<Value> {
        let host = self
            .host(operation.host_id().context("operation has no host")?)
            .await?;
        let data = match operation {
            Operation::HostTest { .. } => {
                self.request(&host, reqwest::Method::GET, "/nodes", None)
                    .await?
            }
            Operation::Guest {
                node, kind, vmid, ..
            } => {
                self.request(
                    &host,
                    reqwest::Method::GET,
                    &format!("/nodes/{node}/{}/{vmid}/status/current", kind.path()),
                    None,
                )
                .await?
            }
            Operation::Snapshot {
                node, kind, vmid, ..
            } => {
                self.request(
                    &host,
                    reqwest::Method::GET,
                    &format!("/nodes/{node}/{}/{vmid}/snapshot", kind.path()),
                    None,
                )
                .await?
            }
            Operation::AttachDisk { node, vmid, .. } => {
                self.request(
                    &host,
                    reqwest::Method::GET,
                    &format!("/nodes/{node}/qemu/{vmid}/config"),
                    None,
                )
                .await?
            }
            Operation::DeployLxc { node, .. } => {
                self.request(
                    &host,
                    reqwest::Method::GET,
                    &format!("/nodes/{node}/status"),
                    None,
                )
                .await?
            }
            Operation::StorageUpload {
                node,
                storage,
                staged_path,
                ..
            } => {
                let storage_state = self
                    .request(
                        &host,
                        reqwest::Method::GET,
                        &format!("/nodes/{node}/storage/{storage}/status"),
                        None,
                    )
                    .await?;
                return Ok(json!({
                    "host": host_snapshot(&host),
                    "provider_state": bounded_state(storage_state),
                    "staged_file": staged_file_snapshot(staged_path).await?,
                }));
            }
            Operation::StorageDelete { node, storage, .. } => {
                self.request(
                    &host,
                    reqwest::Method::GET,
                    &format!("/nodes/{node}/storage/{storage}/content"),
                    None,
                )
                .await?
            }
            Operation::DiskWipe { node, .. } | Operation::DiskInitialize { node, .. } => {
                self.request(
                    &host,
                    reqwest::Method::GET,
                    &format!("/nodes/{node}/disks/list"),
                    None,
                )
                .await?
            }
            _ => Value::Null,
        };
        Ok(json!({"host": host_snapshot(&host), "provider_state": bounded_state(data)}))
    }
}

#[async_trait]
impl ProxmoxProvider for HttpProxmoxProvider {
    async fn snapshot(&self, operation: &Operation) -> Result<Value> {
        match operation {
            Operation::HostCreate {
                id,
                token_secret_id,
                ..
            } => {
                let exists: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM proxmox_hosts WHERE id = ?)")
                        .bind(id)
                        .fetch_one(&self.pool)
                        .await?;
                let token_version = self.secret_version(token_secret_id).await?;
                Ok(
                    json!({"host_id": id, "exists": exists, "token_secret_id": token_secret_id, "token_version": token_version}),
                )
            }
            Operation::HostConfigure {
                id,
                token_secret_id,
                ..
            } => {
                let host = self.host(id).await?;
                let replacement_token_version = match token_secret_id {
                    Some(secret_id) => Some(self.secret_version(secret_id).await?),
                    None => None,
                };
                Ok(
                    json!({"host": host_snapshot(&host), "replacement_token_version": replacement_token_version}),
                )
            }
            Operation::HostDelete { host_id: id } => {
                let host = self.host(id).await?;
                Ok(serde_json::to_value(host_snapshot(&host))?)
            }
            _ => self.remote_snapshot(operation).await,
        }
    }

    async fn execute(&self, operation: Operation, correlation_id: &str) -> Result<Execution> {
        match operation {
            Operation::HostCreate {
                id,
                name,
                url,
                node,
                fingerprint,
                token_secret_id,
            } => {
                let mut tx = self.pool.begin().await?;
                sqlx::query("INSERT INTO proxmox_hosts (id, name, url, node, fingerprint) VALUES (?, ?, ?, ?, ?)")
                    .bind(&id).bind(&name).bind(&url).bind(&node).bind(&fingerprint).execute(&mut *tx).await?;
                tx.commit().await?;
                if let Err(error) = self
                    .copy_token(
                        &id,
                        &token_secret_id,
                        &format!("Proxmox API token for host {name}"),
                    )
                    .await
                {
                    let _ = sqlx::query("DELETE FROM proxmox_hosts WHERE id = ?")
                        .bind(&id)
                        .execute(&self.pool)
                        .await;
                    return Err(error);
                }
                crate::operations::resources::observe(
                    &self.pool,
                    crate::operations::resources::ObserveResource {
                        kind: "proxmox_host",
                        display_name: &name,
                        node_id: None,
                        provider: Some("proxmox"),
                        namespace: "voidtower.proxmox_host",
                        scope_key: "local",
                        alias: &id,
                    },
                    None,
                    correlation_id,
                )
                .await?;
                Ok(Execution::Complete(
                    json!({"host_id": id, "status": "created"}),
                ))
            }
            Operation::HostConfigure {
                id,
                name,
                url,
                node,
                fingerprint,
                token_secret_id,
            } => {
                ensure!(
                    name.is_some()
                        || url.is_some()
                        || node.is_some()
                        || fingerprint.is_some()
                        || token_secret_id.is_some(),
                    "host configure input has no changes"
                );
                sqlx::query("UPDATE proxmox_hosts SET name = COALESCE(?, name), url = COALESCE(?, url), node = COALESCE(?, node), fingerprint = COALESCE(?, fingerprint) WHERE id = ?")
                    .bind(name.as_deref()).bind(url.as_deref()).bind(node.as_deref()).bind(fingerprint.as_deref()).bind(&id).execute(&self.pool).await?;
                if let Some(secret_id) = token_secret_id {
                    self.copy_token(&id, &secret_id, "Proxmox API token")
                        .await?;
                }
                Ok(Execution::Complete(
                    json!({"host_id": id, "status": "configured"}),
                ))
            }
            Operation::HostDelete { host_id } => {
                let mut tx = self.pool.begin().await?;
                sqlx::query("DELETE FROM proxmox_hosts WHERE id = ?")
                    .bind(&host_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM secrets WHERE name = ?")
                    .bind(format!("proxmox_token_{host_id}"))
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                sqlx::query(
                    "UPDATE resources SET lifecycle_state = 'retired', revision = revision + 1, \
                     updated_at = ? WHERE id IN (SELECT resource_id FROM resource_aliases WHERE \
                     namespace = 'voidtower.proxmox_host' AND scope_key = 'local' AND value = ?)",
                )
                .bind(crate::operations::unix_now())
                .bind(&host_id)
                .execute(&self.pool)
                .await?;
                Ok(Execution::Complete(
                    json!({"host_id": host_id, "status": "deleted"}),
                ))
            }
            other => self.execute_remote(other).await,
        }
    }

    async fn task_status(&self, host_id: &str, node: &str, upid: &str) -> Result<TaskState> {
        ensure!(
            !upid.is_empty() && upid.len() <= 1024,
            "invalid Proxmox task ID"
        );
        let host = self.host(host_id).await?;
        let data = self
            .request(
                &host,
                reqwest::Method::GET,
                &format!("/nodes/{node}/tasks/{}/status", percent_encode(upid)),
                None,
            )
            .await?;
        if data.get("status").and_then(Value::as_str) != Some("stopped") {
            return Ok(TaskState::Running);
        }
        match data.get("exitstatus").and_then(Value::as_str) {
            Some("OK") => Ok(TaskState::Succeeded),
            other => Ok(TaskState::Failed(
                other
                    .unwrap_or("unknown failure")
                    .chars()
                    .take(256)
                    .collect(),
            )),
        }
    }
}

impl HttpProxmoxProvider {
    async fn execute_remote(&self, operation: Operation) -> Result<Execution> {
        let host_id = operation
            .host_id()
            .context("operation has no host")?
            .to_owned();
        let host = self.host(&host_id).await?;
        if let Operation::HostTest { .. } = operation {
            let nodes = self
                .request(&host, reqwest::Method::GET, "/nodes", None)
                .await?;
            return Ok(Execution::Complete(
                json!({"host_id": host_id, "nodes": bounded_state(nodes)}),
            ));
        }
        let (method, path, body, immediate) = match &operation {
            Operation::Guest {
                node,
                kind,
                vmid,
                action,
                ..
            } => (
                reqwest::Method::POST,
                format!(
                    "/nodes/{node}/{}/{vmid}/status/{}",
                    kind.path(),
                    action.trim_start_matches("proxmox.guest.")
                ),
                Some(json!({})),
                false,
            ),
            Operation::Snapshot {
                node,
                kind,
                vmid,
                action,
                name,
                description,
                ..
            } => match action.as_str() {
                "proxmox.snapshot.create" => (
                    reqwest::Method::POST,
                    format!("/nodes/{node}/{}/{vmid}/snapshot", kind.path()),
                    Some(json!({"snapname": name, "description": description})),
                    false,
                ),
                "proxmox.snapshot.rollback" => (
                    reqwest::Method::POST,
                    format!(
                        "/nodes/{node}/{}/{vmid}/snapshot/{}/rollback",
                        kind.path(),
                        percent_encode(name)
                    ),
                    Some(json!({})),
                    false,
                ),
                _ => (
                    reqwest::Method::DELETE,
                    format!(
                        "/nodes/{node}/{}/{vmid}/snapshot/{}",
                        kind.path(),
                        percent_encode(name)
                    ),
                    None,
                    false,
                ),
            },
            Operation::AttachDisk {
                node,
                vmid,
                disk_path,
                bus,
                ..
            } => (
                reqwest::Method::POST,
                format!("/nodes/{node}/qemu/{vmid}/config"),
                Some(json!({bus: disk_path})),
                true,
            ),
            Operation::DeployLxc {
                node,
                hostname,
                ostemplate,
                cores,
                memory,
                storage,
                disk_gb,
                ..
            } => {
                let vmid = self
                    .request(&host, reqwest::Method::GET, "/cluster/nextid", None)
                    .await?
                    .as_str()
                    .context("Proxmox nextid response is invalid")?
                    .to_owned();
                (
                    reqwest::Method::POST,
                    format!("/nodes/{node}/lxc"),
                    Some(
                        json!({"vmid": vmid, "hostname": hostname, "ostemplate": ostemplate, "cores": cores, "memory": memory, "rootfs": format!("{storage}:{disk_gb}"), "start": 1}),
                    ),
                    false,
                )
            }
            Operation::StorageUpload {
                node,
                storage,
                content,
                staged_path,
                ..
            } => {
                return self
                    .upload(&host, node, storage, content, staged_path)
                    .await;
            }
            Operation::StorageDelete {
                node,
                storage,
                volid,
                ..
            } => (
                reqwest::Method::DELETE,
                format!(
                    "/nodes/{node}/storage/{storage}/content/{}",
                    percent_encode(volid)
                ),
                None,
                false,
            ),
            Operation::DiskWipe { node, disk, .. } => (
                reqwest::Method::POST,
                format!("/nodes/{node}/disks/wipedisk"),
                Some(json!({"disk": disk})),
                false,
            ),
            Operation::DiskInitialize {
                node,
                disk,
                fstype,
                name,
                raidlevel,
                ..
            } => {
                let body = match fstype.as_str() {
                    "directory" => {
                        json!({"device": disk, "name": name, "filesystem": "ext4", "add_storage": 1})
                    }
                    "lvm" | "lvmthin" => json!({"device": disk, "name": name, "add_storage": 1}),
                    "zfs" => {
                        json!({"devices": disk, "name": name, "raidlevel": raidlevel.as_deref().unwrap_or("single"), "add_storage": 1})
                    }
                    _ => bail!("unsupported disk filesystem type"),
                };
                (
                    reqwest::Method::POST,
                    format!("/nodes/{node}/disks/{fstype}"),
                    Some(body),
                    false,
                )
            }
            _ => bail!("unsupported remote Proxmox operation"),
        };
        let data = self.request(&host, method, &path, body).await?;
        if immediate {
            return Ok(Execution::Complete(
                json!({"host_id": host_id, "status": "applied"}),
            ));
        }
        submitted(data, &host_id)
    }

    async fn upload(
        &self,
        host: &HostAccess,
        node: &str,
        storage: &str,
        content: &str,
        staged_path: &Path,
    ) -> Result<Execution> {
        let length = tokio::fs::metadata(staged_path).await?.len();
        ensure!(
            length <= 16 * 1024 * 1024 * 1024u64,
            "staged Proxmox upload exceeds 16 GiB"
        );
        let filename = staged_path
            .file_name()
            .and_then(|value| value.to_str())
            .context("staged upload filename is invalid")?
            .to_owned();
        let file = tokio::fs::File::open(staged_path).await?;
        let stream = futures_util::stream::try_unfold(file, |mut file| async move {
            let mut chunk = vec![0u8; 64 * 1024];
            let read = file.read(&mut chunk).await?;
            if read == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            chunk.truncate(read);
            Ok(Some((chunk, file)))
        });
        let form = reqwest::multipart::Form::new()
            .text("content", content.to_owned())
            .part(
                "filename",
                reqwest::multipart::Part::stream_with_length(
                    reqwest::Body::wrap_stream(stream),
                    length,
                )
                .file_name(filename),
            );
        let url = format!(
            "{}/api2/json/nodes/{node}/storage/{storage}/upload",
            host.url.trim_end_matches('/')
        );
        let response = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(900))
            .build()?
            .post(url)
            .header("Authorization", format!("PVEAPIToken={}", host.token))
            .multipart(form)
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await?;
        ensure!(status.is_success(), "Proxmox returned HTTP {status}");
        submitted(body.get("data").cloned().unwrap_or(Value::Null), &host.id)
    }
}

fn submitted(data: Value, host_id: &str) -> Result<Execution> {
    let upid = data
        .as_str()
        .context("Proxmox mutation did not return a task ID")?
        .to_owned();
    ensure!(
        !upid.is_empty() && upid.len() <= 1024,
        "invalid Proxmox task ID"
    );
    Ok(Execution::Submitted {
        upid,
        result: json!({"host_id": host_id, "status": "submitted"}),
    })
}

pub struct ProxmoxAdapter {
    pool: SqlitePool,
    provider: Arc<dyn ProxmoxProvider>,
    upload_root: PathBuf,
}

impl ProxmoxAdapter {
    pub fn new(pool: SqlitePool, secrets_key: Arc<[u8; 32]>, data_dir: PathBuf) -> Self {
        Self {
            provider: Arc::new(HttpProxmoxProvider {
                pool: pool.clone(),
                secrets_key,
            }),
            pool,
            upload_root: data_dir.join("proxmox-uploads"),
        }
    }
    #[cfg(test)]
    fn with_provider(
        pool: SqlitePool,
        provider: Arc<dyn ProxmoxProvider>,
        upload_root: PathBuf,
    ) -> Self {
        Self {
            pool,
            provider,
            upload_root,
        }
    }

    async fn operation(&self, request: &PlanRequest) -> Result<Operation> {
        ensure!(
            ACTIONS.contains(&request.action.as_str()),
            "unsupported Proxmox action"
        );
        let input = object(&request.input)?;
        match request.action.as_str() {
            "proxmox.host.create" => {
                ensure_allowed(
                    input,
                    &[
                        "host_id",
                        "name",
                        "url",
                        "node",
                        "fingerprint",
                        "token_secret_id",
                    ],
                )?;
                ensure!(
                    request.resource.kind == "system",
                    "host creation requires the system resource"
                );
                Ok(Operation::HostCreate {
                    id: required(input, "host_id")?,
                    name: required(input, "name")?,
                    url: valid_url(&required(input, "url")?)?,
                    node: optional(input, "node").unwrap_or_else(|| "pve".into()),
                    fingerprint: optional(input, "fingerprint"),
                    token_secret_id: required(input, "token_secret_id")?,
                })
            }
            "proxmox.host.configure" => {
                ensure_allowed(
                    input,
                    &[
                        "host_id",
                        "name",
                        "url",
                        "node",
                        "fingerprint",
                        "token_secret_id",
                    ],
                )?;
                ensure!(
                    request.resource.kind == "system",
                    "host configuration requires the system resource"
                );
                Ok(Operation::HostConfigure {
                    id: required(input, "host_id")?,
                    name: optional(input, "name"),
                    url: optional(input, "url").map(|v| valid_url(&v)).transpose()?,
                    node: optional(input, "node"),
                    fingerprint: optional(input, "fingerprint"),
                    token_secret_id: optional(input, "token_secret_id"),
                })
            }
            "proxmox.host.delete" | "proxmox.host.test" => {
                ensure_allowed(input, &[])?;
                ensure!(
                    request.resource.kind == "proxmox_host",
                    "host action requires a Proxmox host resource"
                );
                let host_id = self
                    .single_alias(&request.resource.id, "voidtower.proxmox_host")
                    .await?;
                Ok(if request.action.ends_with("delete") {
                    Operation::HostDelete { host_id }
                } else {
                    Operation::HostTest { host_id }
                })
            }
            "proxmox.lxc.deploy" => {
                ensure_allowed(
                    input,
                    &[
                        "node",
                        "hostname",
                        "ostemplate",
                        "cores",
                        "memory",
                        "storage",
                        "disk_gb",
                    ],
                )?;
                let host_id = self.host_alias(&request.resource.id).await?;
                Ok(Operation::DeployLxc {
                    host_id,
                    node: valid_segment(&required(input, "node")?)?,
                    hostname: valid_segment(&required(input, "hostname")?)?,
                    ostemplate: bounded(required(input, "ostemplate")?, 512, "ostemplate")?,
                    cores: integer(input, "cores", 2, 1, 128)?,
                    memory: integer(input, "memory", 1024, 128, 1_048_576)?,
                    storage: valid_segment(
                        &optional(input, "storage").unwrap_or_else(|| "local-lvm".into()),
                    )?,
                    disk_gb: integer(input, "disk_gb", 20, 1, 65_536)?,
                })
            }
            _ => self.target_operation(request, input).await,
        }
    }

    async fn target_operation(
        &self,
        request: &PlanRequest,
        input: &Map<String, Value>,
    ) -> Result<Operation> {
        match request.resource.kind.as_str() {
            "proxmox_guest" => {
                let (host_id, node, value) = self
                    .scoped_alias(&request.resource.id, "proxmox.guest")
                    .await?;
                let (kind, vmid) = value
                    .split_once(':')
                    .context("invalid Proxmox guest alias")?;
                let kind = GuestKind::parse(kind)?;
                let vmid = vmid.parse::<u64>().context("invalid Proxmox VM ID")?;
                match request.action.as_str() {
                    action if action.starts_with("proxmox.guest.") => {
                        ensure_allowed(input, &[])?;
                        Ok(Operation::Guest {
                            host_id,
                            node,
                            kind,
                            vmid,
                            action: action.into(),
                        })
                    }
                    action if action.starts_with("proxmox.snapshot.") => {
                        ensure_allowed(input, &["name", "description"])?;
                        Ok(Operation::Snapshot {
                            host_id,
                            node,
                            kind,
                            vmid,
                            action: action.into(),
                            name: valid_segment(&required(input, "name")?)?,
                            description: optional(input, "description")
                                .map(|v| bounded(v, 1024, "description"))
                                .transpose()?,
                        })
                    }
                    "proxmox.disk.attach" => {
                        ensure_allowed(input, &["disk_path", "bus"])?;
                        Ok(Operation::AttachDisk {
                            host_id,
                            node,
                            vmid,
                            disk_path: valid_disk(&required(input, "disk_path")?)?,
                            bus: valid_bus(
                                &optional(input, "bus").unwrap_or_else(|| "scsi1".into()),
                            )?,
                        })
                    }
                    _ => bail!("action does not apply to a Proxmox guest"),
                }
            }
            "proxmox_storage" => {
                let (host_id, node, storage) = self
                    .scoped_alias(&request.resource.id, "proxmox.storage")
                    .await?;
                match request.action.as_str() {
                    "proxmox.storage.upload" => {
                        ensure_allowed(input, &["content", "staged_file"])?;
                        Ok(Operation::StorageUpload {
                            host_id,
                            node,
                            storage,
                            content: content_type(&required(input, "content")?)?,
                            staged_path: self.staged_path(&required(input, "staged_file")?).await?,
                        })
                    }
                    "proxmox.storage.delete" => {
                        ensure_allowed(input, &["volid"])?;
                        Ok(Operation::StorageDelete {
                            host_id,
                            node,
                            storage,
                            volid: bounded(required(input, "volid")?, 1024, "volid")?,
                        })
                    }
                    _ => bail!("action does not apply to Proxmox storage"),
                }
            }
            "proxmox_disk" => {
                let (host_id, node, disk) = self
                    .scoped_alias(&request.resource.id, "proxmox.disk")
                    .await?;
                match request.action.as_str() {
                    "proxmox.disk.wipe" => {
                        ensure_allowed(input, &[])?;
                        Ok(Operation::DiskWipe {
                            host_id,
                            node,
                            disk,
                        })
                    }
                    "proxmox.disk.initialize" => {
                        ensure_allowed(input, &["fstype", "name", "raidlevel"])?;
                        Ok(Operation::DiskInitialize {
                            host_id,
                            node,
                            disk,
                            fstype: filesystem(&required(input, "fstype")?)?,
                            name: valid_segment(&required(input, "name")?)?,
                            raidlevel: optional(input, "raidlevel")
                                .map(|v| valid_segment(&v))
                                .transpose()?,
                        })
                    }
                    _ => bail!("action does not apply to a Proxmox disk"),
                }
            }
            other => bail!("Proxmox adapter received resource kind {other}"),
        }
    }

    async fn host_alias(&self, resource_id: &str) -> Result<String> {
        self.single_alias(resource_id, "voidtower.proxmox_host")
            .await
    }
    async fn single_alias(&self, resource_id: &str, namespace: &str) -> Result<String> {
        let values: Vec<String> = sqlx::query_scalar("SELECT value FROM resource_aliases WHERE resource_id = ? AND namespace = ? ORDER BY value").bind(resource_id).bind(namespace).fetch_all(&self.pool).await?;
        ensure!(
            values.len() == 1,
            "resource must have exactly one {namespace} alias"
        );
        Ok(values.into_iter().next().expect("length checked"))
    }
    async fn scoped_alias(
        &self,
        resource_id: &str,
        namespace: &str,
    ) -> Result<(String, String, String)> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT scope_key, value FROM resource_aliases WHERE resource_id = ? AND namespace = ? ORDER BY scope_key, value").bind(resource_id).bind(namespace).fetch_all(&self.pool).await?;
        ensure!(
            rows.len() == 1,
            "resource must have exactly one {namespace} alias"
        );
        let (scope, value) = rows.into_iter().next().expect("length checked");
        let (host_id, node) = scope
            .split_once('/')
            .context("invalid scoped Proxmox alias")?;
        Ok((host_id.into(), node.into(), value))
    }
    async fn staged_path(&self, name: &str) -> Result<PathBuf> {
        ensure!(
            !name.contains('/') && !name.contains('\\') && name != "." && name != "..",
            "staged_file must be a filename"
        );
        let root = tokio::fs::canonicalize(&self.upload_root)
            .await
            .context("Proxmox upload staging directory is unavailable")?;
        let path = tokio::fs::canonicalize(root.join(name))
            .await
            .context("staged Proxmox upload is unavailable")?;
        ensure!(
            path.starts_with(&root),
            "staged upload escaped its controlled directory"
        );
        ensure!(
            tokio::fs::metadata(&path).await?.is_file(),
            "staged Proxmox upload is not a file"
        );
        Ok(path)
    }
}

#[async_trait]
impl OperationAdapter for ProxmoxAdapter {
    fn key(&self) -> &'static str {
        "proxmox"
    }
    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        let operation = self.operation(&request).await?;
        let snapshot = self.provider.snapshot(&operation).await?;
        let metadata = action_registry::action(&request.action)
            .context("Proxmox action is absent from registry")?;
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: title(&request.action, &request.resource.display_name),
            risk: risk_name(metadata.risk).into(),
            changes: changes(&operation),
            preview: None,
            external_fingerprint: canonical_json::digest(&snapshot)?,
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: request.action,
                retry_class: metadata
                    .retry
                    .context("Proxmox action has no retry metadata")?
                    .class
                    .as_str()
                    .into(),
                recovery_class: metadata
                    .recovery
                    .context("Proxmox action has no recovery metadata")?
                    .as_str()
                    .into(),
            }],
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        canonical_json::digest(
            &self
                .provider
                .snapshot(&self.operation(request).await?)
                .await?,
        )
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        ensure!(
            request.step.kind == "execute" && request.step.name == request.action,
            "Proxmox step/action mismatch"
        );
        let plan = PlanRequest {
            action: request.action,
            resource: request.resource,
            input: request.input,
        };
        let operation = self.operation(&plan).await?;
        match self.provider.execute(operation, &request.job_id).await {
            Ok(Execution::Complete(result)) => Ok(StepOutcome::Succeeded {
                result: bounded_state(result),
                external_operation_id: None,
            }),
            Ok(Execution::Submitted { upid, result }) => Ok(StepOutcome::Uncertain {
                code: "proxmox_task_pending".into(),
                message: "Proxmox accepted the operation; task completion is being reconciled"
                    .into(),
                external_operation_id: Some(upid),
                diagnostic: Some(bounded_state(result)),
            }),
            Err(error) => Ok(StepOutcome::Uncertain {
                code: "proxmox_execution_uncertain".into(),
                message: crate::api::mcp::redact::redact_patterns(&format!(
                    "Proxmox did not report a conclusive outcome: {error}"
                )),
                external_operation_id: None,
                diagnostic: None,
            }),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        let plan = PlanRequest {
            action: request.action,
            resource: request.resource,
            input: request.input,
        };
        let operation = self.operation(&plan).await?;
        let upid = request
            .external_operation_id
            .context("Proxmox reconciliation requires a task ID")?;
        let host_id = operation.host_id().context("Proxmox task has no host")?;
        let node = operation.node().context("Proxmox task has no node")?;
        match self.provider.task_status(host_id, node, &upid).await? {
            TaskState::Running => Ok(ReconcileOutcome::StillUncertain {
                message: "Proxmox task is still running".into(),
            }),
            TaskState::Succeeded => Ok(ReconcileOutcome::Succeeded {
                result: json!({"task_id": upid, "status": "succeeded"}),
            }),
            TaskState::Failed(message) => Ok(ReconcileOutcome::Failed {
                code: "proxmox_task_failed".into(),
                message: crate::api::mcp::redact::redact_patterns(&message),
            }),
        }
    }
}

fn host_snapshot(host: &HostAccess) -> HostSnapshot {
    HostSnapshot {
        id: host.id.clone(),
        name: host.name.clone(),
        url: public_url(&host.url),
        node: host.node.clone(),
        fingerprint: host.fingerprint.clone(),
        token_version: host.token_version,
    }
}
fn object(value: &Value) -> Result<&Map<String, Value>> {
    value
        .as_object()
        .context("Proxmox action input must be an object")
}
fn ensure_allowed(input: &Map<String, Value>, allowed: &[&str]) -> Result<()> {
    let mut unexpected: Vec<&str> = input
        .keys()
        .map(String::as_str)
        .filter(|key| !allowed.contains(key))
        .collect();
    unexpected.sort_unstable();
    ensure!(
        unexpected.is_empty(),
        "unsupported Proxmox input fields: {}",
        unexpected.join(", ")
    );
    Ok(())
}
fn required(input: &Map<String, Value>, key: &str) -> Result<String> {
    bounded(
        input
            .get(key)
            .and_then(Value::as_str)
            .context(format!("missing {key}"))?
            .to_owned(),
        4096,
        key,
    )
}
fn optional(input: &Map<String, Value>, key: &str) -> Option<String> {
    input
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|v| !v.is_empty())
}
fn bounded(value: String, max: usize, label: &str) -> Result<String> {
    ensure!(!value.is_empty() && value.len() <= max, "invalid {label}");
    ensure!(!value.chars().any(char::is_control), "invalid {label}");
    Ok(value)
}
fn integer(input: &Map<String, Value>, key: &str, default: u64, min: u64, max: u64) -> Result<u64> {
    let value = input.get(key).and_then(Value::as_u64).unwrap_or(default);
    ensure!((min..=max).contains(&value), "invalid {key}");
    Ok(value)
}
fn valid_segment(value: &str) -> Result<String> {
    ensure!(
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
        "invalid Proxmox identifier"
    );
    Ok(value.into())
}
fn valid_url(value: &str) -> Result<String> {
    let url = reqwest::Url::parse(value).context("invalid Proxmox URL")?;
    ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "invalid Proxmox URL"
    );
    Ok(value.trim_end_matches('/').into())
}
fn public_url(value: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(value) else {
        return "invalid-proxmox-url".into();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}
fn valid_disk(value: &str) -> Result<String> {
    ensure!(
        value.starts_with("/dev/")
            && value.len() <= 256
            && !value.contains("..")
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.')),
        "invalid disk path"
    );
    Ok(value.into())
}
fn valid_bus(value: &str) -> Result<String> {
    ensure!(
        ["scsi", "sata", "virtio", "ide"].iter().any(|prefix| value
            .strip_prefix(prefix)
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))),
        "invalid guest disk bus"
    );
    Ok(value.into())
}
fn filesystem(value: &str) -> Result<String> {
    ensure!(
        ["directory", "lvm", "lvmthin", "zfs"].contains(&value),
        "unsupported disk filesystem type"
    );
    Ok(value.into())
}
fn content_type(value: &str) -> Result<String> {
    ensure!(
        ["iso", "vztmpl", "backup", "snippets"].contains(&value),
        "unsupported storage content type"
    );
    Ok(value.into())
}
fn percent_encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}
fn bounded_state(value: Value) -> Value {
    let encoded = serde_json::to_string(&value).unwrap_or_default();
    if encoded.len() <= 16_384 {
        value
    } else {
        json!({"truncated": true, "digest": canonical_json::digest(&value).unwrap_or_default()})
    }
}
async fn staged_file_snapshot(path: &Path) -> Result<Value> {
    let mut file = tokio::fs::File::open(path)
        .await
        .context("staged Proxmox upload is unavailable")?;
    let mut hasher = Sha256::new();
    let mut length = 0u64;
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        length = length.saturating_add(read as u64);
        ensure!(
            length <= 16 * 1024 * 1024 * 1024u64,
            "staged Proxmox upload exceeds 16 GiB"
        );
        hasher.update(&buffer[..read]);
    }
    Ok(json!({"sha256": hex::encode(hasher.finalize()), "length": length}))
}
fn title(action: &str, target: &str) -> String {
    format!(
        "{} {target}",
        action.trim_start_matches("proxmox.").replace('.', " ")
    )
}
fn changes(operation: &Operation) -> Vec<PlanChange> {
    vec![
        PlanChange {
            label: "Action".into(),
            value: match operation {
                Operation::HostCreate { .. } => "Create host".into(),
                Operation::HostConfigure { .. } => "Configure host".into(),
                Operation::HostDelete { .. } => "Delete host".into(),
                Operation::HostTest { .. } => "Test host".into(),
                Operation::Guest { action, .. } | Operation::Snapshot { action, .. } => {
                    action.trim_start_matches("proxmox.").replace('.', " ")
                }
                Operation::AttachDisk { .. } => "Attach physical disk".into(),
                Operation::DeployLxc { .. } => "Deploy LXC".into(),
                Operation::StorageUpload { .. } => "Upload staged file".into(),
                Operation::StorageDelete { .. } => "Delete storage content".into(),
                Operation::DiskWipe { .. } => "Wipe disk".into(),
                Operation::DiskInitialize { .. } => "Initialize disk".into(),
            },
        },
        PlanChange {
            label: "Provider".into(),
            value: "Proxmox VE".into(),
        },
    ]
}
fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "low",
        RiskClass::Mutate => "medium",
        RiskClass::Destructive | RiskClass::Irreversible => "high",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::contracts::ResourceRef;
    use axum::{
        extract::Path as AxumPath,
        http::HeaderMap,
        routing::{get, post},
        Json, Router,
    };
    use std::sync::Mutex;

    struct MockProvider {
        snapshot: Mutex<Value>,
        execution: Mutex<Option<Result<Execution, String>>>,
        task: Mutex<TaskState>,
    }

    impl MockProvider {
        fn new(execution: Result<Execution, String>, task: TaskState) -> Self {
            Self {
                snapshot: Mutex::new(json!({"status": "running"})),
                execution: Mutex::new(Some(execution)),
                task: Mutex::new(task),
            }
        }

        fn set_snapshot(&self, value: Value) {
            *self.snapshot.lock().unwrap() = value;
        }
    }

    #[async_trait]
    impl ProxmoxProvider for MockProvider {
        async fn snapshot(&self, _operation: &Operation) -> Result<Value> {
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn execute(&self, _operation: Operation, _correlation_id: &str) -> Result<Execution> {
            match self
                .execution
                .lock()
                .unwrap()
                .take()
                .context("mock execution exhausted")?
            {
                Ok(value) => Ok(value),
                Err(message) => bail!(message),
            }
        }

        async fn task_status(&self, _host_id: &str, _node: &str, _upid: &str) -> Result<TaskState> {
            Ok(self.task.lock().unwrap().clone())
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

    async fn guest(pool: &SqlitePool) -> ResourceRef {
        sqlx::query("INSERT INTO resources (id, kind, display_name, provider, lifecycle_state, revision, created_at, updated_at) VALUES ('guest-r', 'proxmox_guest', 'web-100', 'proxmox', 'active', 0, 0, 0)")
            .execute(pool).await.unwrap();
        sqlx::query("INSERT INTO resource_aliases (resource_id, namespace, scope_key, value, created_at, last_seen_at) VALUES ('guest-r', 'proxmox.guest', 'host-1/pve', 'qemu:100', 0, 0)")
            .execute(pool).await.unwrap();
        ResourceRef {
            id: "guest-r".into(),
            kind: "proxmox_guest".into(),
            display_name: "web-100".into(),
            revision: 0,
        }
    }

    #[tokio::test]
    async fn http_provider_uses_upid_task_endpoint_for_reconciliation() {
        let app = Router::new()
            .route(
                "/api2/json/nodes/pve/qemu/100/status/current",
                get(|| async { Json(json!({"data": {"status": "stopped"}})) }),
            )
            .route(
                "/api2/json/nodes/pve/qemu/100/status/start",
                post(|headers: HeaderMap| async move {
                    assert_eq!(
                        headers
                            .get("authorization")
                            .and_then(|value| value.to_str().ok()),
                        Some("PVEAPIToken=root@pam!voidtower=secret")
                    );
                    Json(json!({"data": "UPID:pve:1"}))
                }),
            )
            .route(
                "/api2/json/nodes/pve/tasks/:upid/status",
                get(|AxumPath(upid): AxumPath<String>| async move {
                    assert_eq!(upid, "UPID:pve:1");
                    Json(json!({"data": {"status": "stopped", "exitstatus": "OK"}}))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let pool = pool().await;
        let key = Arc::new([7u8; 32]);
        let encrypted = secrets::encrypt(&key, "root@pam!voidtower=secret").unwrap();
        sqlx::query(
            "INSERT INTO proxmox_hosts (id, name, url, node) VALUES ('host-1', 'PVE', ?, 'pve')",
        )
        .bind(format!("http://{address}"))
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES ('secret-1', 'proxmox_token_host-1', ?, 0, 0)")
            .bind(encrypted).execute(&pool).await.unwrap();
        let provider = HttpProxmoxProvider {
            pool,
            secrets_key: key,
        };
        let operation = Operation::Guest {
            host_id: "host-1".into(),
            node: "pve".into(),
            kind: GuestKind::Qemu,
            vmid: 100,
            action: "proxmox.guest.start".into(),
        };
        let snapshot = provider.snapshot(&operation).await.unwrap();
        assert_eq!(snapshot["provider_state"]["status"], "stopped");
        let execution = provider.execute(operation, "job-1").await.unwrap();
        assert!(matches!(execution, Execution::Submitted { ref upid, .. } if upid == "UPID:pve:1"));
        assert_eq!(
            provider
                .task_status("host-1", "pve", "UPID:pve:1")
                .await
                .unwrap(),
            TaskState::Succeeded
        );
        server.abort();
    }

    fn plan_request(resource: ResourceRef, action: &str, input: Value) -> PlanRequest {
        PlanRequest {
            action: action.into(),
            resource,
            input,
        }
    }

    fn step_request(
        resource: ResourceRef,
        action: &str,
        external_operation_id: Option<String>,
    ) -> StepRequest {
        StepRequest {
            job_id: "job-1".into(),
            action: action.into(),
            resource,
            input: json!({}),
            step: PlannedStepV1 {
                kind: "execute".into(),
                name: action.into(),
                retry_class: "provider_safe".into(),
                recovery_class: "reconcile".into(),
            },
            attempt: 1,
            external_operation_id,
        }
    }

    #[tokio::test]
    async fn plan_is_deterministic_and_changes_when_provider_state_changes() {
        let pool = pool().await;
        let resource = guest(&pool).await;
        let provider = Arc::new(MockProvider::new(
            Ok(Execution::Complete(json!({}))),
            TaskState::Succeeded,
        ));
        let adapter =
            ProxmoxAdapter::with_provider(pool, provider.clone(), PathBuf::from("/unused"));
        let request = plan_request(resource, "proxmox.guest.start", json!({}));
        let first = adapter.plan(request.clone()).await.unwrap();
        let second = adapter.plan(request.clone()).await.unwrap();
        assert_eq!(first.external_fingerprint, second.external_fingerprint);
        provider.set_snapshot(json!({"status": "stopped"}));
        assert_ne!(
            first.external_fingerprint,
            adapter.external_fingerprint(&request).await.unwrap()
        );
    }

    #[tokio::test]
    async fn submitted_task_is_persisted_for_reconciliation() {
        let pool = pool().await;
        let resource = guest(&pool).await;
        let provider = Arc::new(MockProvider::new(
            Ok(Execution::Submitted {
                upid: "UPID:pve:1".into(),
                result: json!({"status": "submitted"}),
            }),
            TaskState::Running,
        ));
        let adapter = ProxmoxAdapter::with_provider(pool, provider, PathBuf::from("/unused"));
        let outcome = adapter
            .execute_step(step_request(resource.clone(), "proxmox.guest.start", None))
            .await
            .unwrap();
        assert!(
            matches!(outcome, StepOutcome::Uncertain { external_operation_id: Some(ref id), .. } if id == "UPID:pve:1")
        );
        let reconciled = adapter
            .reconcile(step_request(
                resource,
                "proxmox.guest.start",
                Some("UPID:pve:1".into()),
            ))
            .await
            .unwrap();
        assert!(matches!(
            reconciled,
            ReconcileOutcome::StillUncertain { .. }
        ));
    }

    #[tokio::test]
    async fn reconciliation_maps_terminal_success_and_failure() {
        for (task, succeeded) in [
            (TaskState::Succeeded, true),
            (TaskState::Failed("permission denied".into()), false),
        ] {
            let pool = pool().await;
            let resource = guest(&pool).await;
            let provider = Arc::new(MockProvider::new(Ok(Execution::Complete(json!({}))), task));
            let adapter = ProxmoxAdapter::with_provider(pool, provider, PathBuf::from("/unused"));
            let outcome = adapter
                .reconcile(step_request(
                    resource,
                    "proxmox.guest.stop",
                    Some("UPID:pve:2".into()),
                ))
                .await
                .unwrap();
            assert_eq!(
                matches!(outcome, ReconcileOutcome::Succeeded { .. }),
                succeeded
            );
            assert_eq!(
                matches!(outcome, ReconcileOutcome::Failed { .. }),
                !succeeded
            );
        }
    }

    #[tokio::test]
    async fn inconclusive_provider_error_never_replays_or_claims_success() {
        let pool = pool().await;
        let resource = guest(&pool).await;
        let provider = Arc::new(MockProvider::new(
            Err("timeout token=supersecret".into()),
            TaskState::Running,
        ));
        let adapter = ProxmoxAdapter::with_provider(pool, provider, PathBuf::from("/unused"));
        let outcome = adapter
            .execute_step(step_request(resource, "proxmox.guest.reboot", None))
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            StepOutcome::Uncertain {
                external_operation_id: None,
                ..
            }
        ));
        assert!(!format!("{outcome:?}").contains("supersecret"));
    }

    #[tokio::test]
    async fn host_creation_requires_a_secret_reference_not_token_material() {
        let pool = pool().await;
        let provider = Arc::new(MockProvider::new(
            Ok(Execution::Complete(json!({}))),
            TaskState::Succeeded,
        ));
        let adapter = ProxmoxAdapter::with_provider(pool, provider, PathBuf::from("/unused"));
        let request = plan_request(
            ResourceRef {
                id: "system".into(),
                kind: "system".into(),
                display_name: "This VoidTower".into(),
                revision: 0,
            },
            "proxmox.host.create",
            json!({"host_id": "host-1", "name": "PVE", "url": "https://pve.invalid:8006", "token_id": "root@pam!api", "token_secret": "literal"}),
        );
        let message = adapter.plan(request).await.unwrap_err().to_string();
        assert!(message.contains("unsupported Proxmox input fields"));
        assert!(!message.contains("literal"));
    }

    #[tokio::test]
    async fn referenced_token_rotation_stales_a_host_creation_plan() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at, version) \
             VALUES ('token-ref', 'candidate-token', 'opaque', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let adapter =
            ProxmoxAdapter::new(pool.clone(), Arc::new([0u8; 32]), PathBuf::from("/unused"));
        let request = plan_request(
            ResourceRef {
                id: "system".into(),
                kind: "system".into(),
                display_name: "This VoidTower".into(),
                revision: 0,
            },
            "proxmox.host.create",
            json!({
                "host_id": "host-1", "name": "PVE", "url": "https://pve.invalid:8006",
                "token_secret_id": "token-ref"
            }),
        );
        let first = adapter.external_fingerprint(&request).await.unwrap();
        sqlx::query("UPDATE secrets SET version = version + 1 WHERE id = 'token-ref'")
            .execute(&pool)
            .await
            .unwrap();
        assert_ne!(first, adapter.external_fingerprint(&request).await.unwrap());
    }

    #[tokio::test]
    async fn staged_upload_fingerprint_tracks_file_content() {
        let directory = std::env::temp_dir().join(format!(
            "voidtower-proxmox-upload-test-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir(&directory).await.unwrap();
        let path = directory.join("image.iso");
        tokio::fs::write(&path, b"first").await.unwrap();
        let first = staged_file_snapshot(&path).await.unwrap();
        tokio::fs::write(&path, b"second").await.unwrap();
        let second = staged_file_snapshot(&path).await.unwrap();
        assert_ne!(first["sha256"], second["sha256"]);
        assert_ne!(first["length"], second["length"]);
        tokio::fs::remove_file(&path).await.unwrap();
        tokio::fs::remove_dir(&directory).await.unwrap();
    }

    #[tokio::test]
    async fn lxc_deploy_rejects_inline_compose_that_could_contain_secrets() {
        let pool = pool().await;
        let provider = Arc::new(MockProvider::new(
            Ok(Execution::Complete(json!({}))),
            TaskState::Succeeded,
        ));
        let adapter = ProxmoxAdapter::with_provider(pool, provider, PathBuf::from("/unused"));
        let request = plan_request(
            ResourceRef {
                id: "host-resource".into(),
                kind: "proxmox_host".into(),
                display_name: "PVE".into(),
                revision: 0,
            },
            "proxmox.lxc.deploy",
            json!({
                "node": "pve", "hostname": "app", "ostemplate": "local:vztmpl/debian.tar.zst",
                "compose_yaml": "services:\n  app:\n    environment:\n      PASSWORD: literal"
            }),
        );
        let message = adapter.plan(request).await.unwrap_err().to_string();
        assert!(message.contains("unsupported Proxmox input fields"));
        assert!(!message.contains("PASSWORD"));
    }

    #[test]
    fn irreversible_actions_remain_always_approved() {
        for action in [
            "proxmox.snapshot.delete",
            "proxmox.disk.wipe",
            "proxmox.disk.initialize",
        ] {
            let metadata = action_registry::action(action).unwrap();
            assert_eq!(metadata.risk, RiskClass::Irreversible);
            assert_eq!(metadata.approval, action_registry::ApprovalPolicy::Always);
        }
    }

    #[test]
    fn adapter_covers_every_declared_proxmox_job() {
        let mut declared: Vec<&str> = action_registry::ACTIONS
            .iter()
            .filter(|action| action.adapter_key == Some("proxmox"))
            .map(|action| action.name)
            .collect();
        let mut implemented = ACTIONS.to_vec();
        declared.sort_unstable();
        implemented.sort_unstable();
        assert_eq!(implemented, declared);
    }
}
