use crate::{
    api::{mcp::action_registry, secrets},
    operations::{
        contracts::{CapabilityAvailability, ResourceRef},
        invocation::{self, InvocationContext, InvocationError},
        resources::{self, ObserveResource},
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;

pub const LEGACY_HOST_ID: &str = "legacy-proxmox";

#[derive(Debug, thiserror::Error)]
pub enum ProxmoxAdoptionError {
    #[error(transparent)]
    Invocation(#[from] InvocationError),
    #[error("Proxmox target is unavailable: {0}")]
    Unavailable(String),
    #[error("Proxmox target adoption failed")]
    Internal(#[source] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxmoxSelector {
    System,
    Host {
        host_id: String,
    },
    Guest {
        host_id: String,
        node: Option<String>,
        kind: Option<String>,
        vmid: u64,
    },
    Storage {
        host_id: String,
        node: String,
        storage: String,
    },
    Disk {
        host_id: String,
        node: String,
        disk: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProxmoxEvidence {
    Host {
        host_id: String,
        display_name: String,
    },
    Guest {
        host_id: String,
        node: String,
        kind: String,
        vmid: u64,
        display_name: String,
    },
    Storage {
        host_id: String,
        node: String,
        storage: String,
        display_name: String,
    },
    Disk {
        host_id: String,
        node: String,
        disk: String,
        display_name: String,
    },
}

#[derive(Debug, Clone)]
pub struct AdoptedProxmoxTarget {
    pub resource: ResourceRef,
    pub host_id: Option<String>,
    pub node: Option<String>,
    pub guest_kind: Option<String>,
}

#[async_trait]
trait ProxmoxEvidenceProvider: Send + Sync {
    async fn evidence(&self, selector: &ProxmoxSelector) -> Result<ProxmoxEvidence>;
}

struct HostProxmoxEvidenceProvider {
    pool: SqlitePool,
    secrets_key: Arc<[u8; 32]>,
}

#[derive(Debug)]
struct HostAccess {
    id: String,
    name: String,
    url: String,
    node: String,
    token: String,
    verify_ssl: bool,
}

impl HostProxmoxEvidenceProvider {
    async fn host(&self, host_id: &str) -> Result<HostAccess> {
        if host_id == LEGACY_HOST_ID {
            return self.legacy_host().await;
        }
        let (name, url, node): (String, String, String) =
            sqlx::query_as("SELECT name, url, node FROM proxmox_hosts WHERE id = ?")
                .bind(host_id)
                .fetch_optional(&self.pool)
                .await?
                .context("Proxmox host is not configured")?;
        let value_enc: String = sqlx::query_scalar("SELECT value_enc FROM secrets WHERE name = ?")
            .bind(format!("proxmox_token_{host_id}"))
            .fetch_optional(&self.pool)
            .await?
            .context("Proxmox host token is not configured")?;
        let token = secrets::decrypt(&self.secrets_key, &value_enc)
            .context("Proxmox host token decryption failed")?;
        Ok(HostAccess {
            id: host_id.into(),
            name,
            url,
            node,
            token,
            verify_ssl: false,
        })
    }

    async fn legacy_host(&self) -> Result<HostAccess> {
        let host: String = setting(&self.pool, "proxmox_host")
            .await?
            .context("legacy Proxmox host is not configured")?;
        let port = setting(&self.pool, "proxmox_port")
            .await?
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(8006);
        let node = setting(&self.pool, "proxmox_node")
            .await?
            .filter(|value| !value.is_empty() && value != "all")
            .unwrap_or_else(|| "pve".into());
        let verify_ssl = setting(&self.pool, "proxmox_verify_ssl")
            .await?
            .is_some_and(|value| value == "true");
        let token = match sqlx::query_scalar::<_, String>(
            "SELECT value_enc FROM secrets WHERE name = 'proxmox_legacy_token'",
        )
        .fetch_optional(&self.pool)
        .await?
        {
            Some(value) => secrets::decrypt(&self.secrets_key, &value)
                .context("legacy Proxmox token decryption failed")?,
            None => setting(&self.pool, "proxmox_token")
                .await?
                .context("legacy Proxmox token is not configured")?,
        };
        Ok(HostAccess {
            id: LEGACY_HOST_ID.into(),
            name: "Legacy Proxmox".into(),
            url: format!("https://{host}:{port}"),
            node,
            token,
            verify_ssl,
        })
    }

    fn client(host: &HostAccess) -> Result<reqwest::Client> {
        Ok(reqwest::Client::builder()
            .danger_accept_invalid_certs(!host.verify_ssl)
            .timeout(std::time::Duration::from_secs(30))
            .build()?)
    }

    async fn get(&self, host: &HostAccess, path: &str) -> Result<Option<Value>> {
        let response = Self::client(host)?
            .get(format!(
                "{}/api2/json{path}",
                host.url.trim_end_matches('/')
            ))
            .header("Authorization", format!("PVEAPIToken={}", host.token))
            .send()
            .await
            .context("Proxmox evidence request failed")?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let body: Value = response
            .json()
            .await
            .context("Proxmox evidence response was invalid")?;
        Ok(Some(body.get("data").cloned().unwrap_or(Value::Null)))
    }
}

#[async_trait]
impl ProxmoxEvidenceProvider for HostProxmoxEvidenceProvider {
    async fn evidence(&self, selector: &ProxmoxSelector) -> Result<ProxmoxEvidence> {
        match selector {
            ProxmoxSelector::System => bail!("system selectors do not require provider evidence"),
            ProxmoxSelector::Host { host_id } => {
                let host = self.host(host_id).await?;
                Ok(ProxmoxEvidence::Host {
                    host_id: host.id,
                    display_name: host.name,
                })
            }
            ProxmoxSelector::Guest {
                host_id,
                node,
                kind,
                vmid,
            } => {
                let host = self.host(host_id).await?;
                let node = node.as_deref().unwrap_or(&host.node);
                let kinds: &[&str] = match kind.as_deref() {
                    Some("qemu" | "vm") => &["qemu"],
                    Some("lxc") => &["lxc"],
                    Some(_) => bail!("invalid Proxmox guest kind"),
                    None => &["qemu", "lxc"],
                };
                for kind in kinds {
                    if let Some(snapshot) = self
                        .get(
                            &host,
                            &format!("/nodes/{node}/{kind}/{vmid}/status/current"),
                        )
                        .await?
                    {
                        let display_name = snapshot
                            .get("name")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("{kind} {vmid}"));
                        return Ok(ProxmoxEvidence::Guest {
                            host_id: host.id,
                            node: node.into(),
                            kind: (*kind).into(),
                            vmid: *vmid,
                            display_name,
                        });
                    }
                }
                bail!("Proxmox guest was not found")
            }
            ProxmoxSelector::Storage {
                host_id,
                node,
                storage,
            } => {
                let host = self.host(host_id).await?;
                ensure!(
                    self.get(&host, &format!("/nodes/{node}/storage/{storage}/status"))
                        .await?
                        .is_some(),
                    "Proxmox storage was not found"
                );
                Ok(ProxmoxEvidence::Storage {
                    host_id: host.id,
                    node: node.clone(),
                    storage: storage.clone(),
                    display_name: storage.clone(),
                })
            }
            ProxmoxSelector::Disk {
                host_id,
                node,
                disk,
            } => {
                let host = self.host(host_id).await?;
                let disks = self
                    .get(&host, &format!("/nodes/{node}/disks/list"))
                    .await?
                    .context("Proxmox disk inventory was unavailable")?;
                let found = disks
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|value| {
                        value.get("devpath").and_then(Value::as_str) == Some(disk.as_str())
                            || value.get("device").and_then(Value::as_str) == Some(disk.as_str())
                    })
                    .context("Proxmox disk was not found")?;
                let display_name = found
                    .get("model")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .unwrap_or(disk)
                    .to_owned();
                Ok(ProxmoxEvidence::Disk {
                    host_id: host.id,
                    node: node.clone(),
                    disk: disk.clone(),
                    display_name,
                })
            }
        }
    }
}

async fn setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?,
    )
}

pub fn authorize(
    context: &InvocationContext,
    action_name: &str,
) -> Result<(), ProxmoxAdoptionError> {
    let action = action_registry::action(action_name).ok_or(InvocationError::UnknownAction)?;
    invocation::authorize_action(action, context)?;
    Ok(())
}

pub async fn resolve_target(
    pool: &SqlitePool,
    secrets_key: Arc<[u8; 32]>,
    context: &InvocationContext,
    action_name: &str,
    selector: ProxmoxSelector,
) -> Result<AdoptedProxmoxTarget, ProxmoxAdoptionError> {
    let provider = HostProxmoxEvidenceProvider {
        pool: pool.clone(),
        secrets_key,
    };
    resolve_target_with_provider(pool, context, action_name, selector, &provider).await
}

async fn resolve_target_with_provider(
    pool: &SqlitePool,
    context: &InvocationContext,
    action_name: &str,
    selector: ProxmoxSelector,
    provider: &dyn ProxmoxEvidenceProvider,
) -> Result<AdoptedProxmoxTarget, ProxmoxAdoptionError> {
    authorize(context, action_name)?;
    validate_selector(action_name, &selector)?;
    if matches!(selector, ProxmoxSelector::System) {
        let resource =
            match resources::resolve_alias(pool, "voidtower.singleton", "local", "system")
                .await
                .map_err(ProxmoxAdoptionError::Internal)?
            {
                Some(resource) => resource,
                None => resources::observe(
                    pool,
                    ObserveResource {
                        kind: "system",
                        display_name: "VoidTower System",
                        node_id: None,
                        provider: Some("local"),
                        namespace: "voidtower.singleton",
                        scope_key: "local",
                        alias: "system",
                    },
                    Some(context.actor()),
                    &uuid::Uuid::new_v4().to_string(),
                )
                .await
                .map_err(ProxmoxAdoptionError::Internal)?,
            };
        if resource.kind != "system" {
            return Err(InvocationError::ResourceKindMismatch.into());
        }
        publish_capability(pool, &resource.id, action_name, "system").await?;
        return Ok(AdoptedProxmoxTarget {
            resource,
            host_id: None,
            node: None,
            guest_kind: None,
        });
    }
    let evidence = provider
        .evidence(&selector)
        .await
        .map_err(|error| ProxmoxAdoptionError::Unavailable(safe_text(&error.to_string())))?;
    adopt_evidence(pool, context, action_name, evidence).await
}

fn validate_selector(
    action_name: &str,
    selector: &ProxmoxSelector,
) -> Result<(), ProxmoxAdoptionError> {
    let valid = match selector {
        ProxmoxSelector::System => {
            matches!(
                action_name,
                "proxmox.host.create" | "proxmox.host.configure"
            )
        }
        ProxmoxSelector::Host { .. } => matches!(
            action_name,
            "proxmox.host.delete" | "proxmox.host.test" | "proxmox.lxc.deploy"
        ),
        ProxmoxSelector::Guest { .. } => {
            action_name.starts_with("proxmox.guest.")
                || action_name.starts_with("proxmox.snapshot.")
                || action_name == "proxmox.disk.attach"
        }
        ProxmoxSelector::Storage { .. } => action_name.starts_with("proxmox.storage."),
        ProxmoxSelector::Disk { .. } => {
            matches!(action_name, "proxmox.disk.wipe" | "proxmox.disk.initialize")
        }
    };
    if !valid {
        return Err(InvocationError::ResourceKindMismatch.into());
    }
    Ok(())
}

async fn adopt_evidence(
    pool: &SqlitePool,
    context: &InvocationContext,
    action_name: &str,
    evidence: ProxmoxEvidence,
) -> Result<AdoptedProxmoxTarget, ProxmoxAdoptionError> {
    let correlation_id = uuid::Uuid::new_v4().to_string();
    let (kind, display_name, namespace, scope_key, alias, host_id, node, guest_kind) =
        match evidence {
            ProxmoxEvidence::Host {
                host_id,
                display_name,
            } => (
                "proxmox_host",
                display_name,
                "voidtower.proxmox_host",
                "local".into(),
                host_id.clone(),
                Some(host_id),
                None,
                None,
            ),
            ProxmoxEvidence::Guest {
                host_id,
                node,
                kind,
                vmid,
                display_name,
            } => {
                if matches!(action_name, "proxmox.guest.reset" | "proxmox.disk.attach")
                    && kind != "qemu"
                {
                    return Err(ProxmoxAdoptionError::Unavailable(
                        "this action is only available for QEMU guests".into(),
                    ));
                }
                (
                    "proxmox_guest",
                    display_name,
                    "proxmox.guest",
                    format!("{host_id}/{node}"),
                    format!("{kind}:{vmid}"),
                    Some(host_id),
                    Some(node),
                    Some(kind),
                )
            }
            ProxmoxEvidence::Storage {
                host_id,
                node,
                storage,
                display_name,
            } => (
                "proxmox_storage",
                display_name,
                "proxmox.storage",
                format!("{host_id}/{node}"),
                storage,
                Some(host_id),
                Some(node),
                None,
            ),
            ProxmoxEvidence::Disk {
                host_id,
                node,
                disk,
                display_name,
            } => (
                "proxmox_disk",
                display_name,
                "proxmox.disk",
                format!("{host_id}/{node}"),
                disk,
                Some(host_id),
                Some(node),
                None,
            ),
        };
    let resource = resources::observe(
        pool,
        ObserveResource {
            kind,
            display_name: &display_name,
            node_id: None,
            provider: Some("proxmox"),
            namespace,
            scope_key: &scope_key,
            alias: &alias,
        },
        Some(context.actor()),
        &correlation_id,
    )
    .await
    .map_err(ProxmoxAdoptionError::Internal)?;
    publish_capability(pool, &resource.id, action_name, &correlation_id).await?;
    Ok(AdoptedProxmoxTarget {
        resource,
        host_id,
        node,
        guest_kind,
    })
}

async fn publish_capability(
    pool: &SqlitePool,
    resource_id: &str,
    action_name: &str,
    correlation_id: &str,
) -> Result<(), ProxmoxAdoptionError> {
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
    .map_err(ProxmoxAdoptionError::Internal)
}

fn safe_text(value: &str) -> String {
    crate::api::mcp::redact::redact_patterns(value.trim())
        .chars()
        .take(512)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeProvider {
        evidence: ProxmoxEvidence,
        calls: Mutex<Vec<ProxmoxSelector>>,
    }

    #[async_trait]
    impl ProxmoxEvidenceProvider for FakeProvider {
        async fn evidence(&self, selector: &ProxmoxSelector) -> Result<ProxmoxEvidence> {
            self.calls.lock().unwrap().push(selector.clone());
            Ok(self.evidence.clone())
        }
    }

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

    fn admin() -> InvocationContext {
        InvocationContext::Session {
            user_id: "admin-1".into(),
            role: "admin".into(),
        }
    }

    #[tokio::test]
    async fn system_target_resolves_without_provider_evidence() {
        let pool = pool().await;
        let provider = FakeProvider {
            evidence: ProxmoxEvidence::Host {
                host_id: "unused".into(),
                display_name: "Unused".into(),
            },
            calls: Mutex::new(Vec::new()),
        };
        let adopted = resolve_target_with_provider(
            &pool,
            &admin(),
            "proxmox.host.create",
            ProxmoxSelector::System,
            &provider,
        )
        .await
        .unwrap();
        assert_eq!(adopted.resource.kind, "system");
        assert!(provider.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn guest_evidence_is_observed_under_normalized_scoped_alias() {
        let pool = pool().await;
        let provider = FakeProvider {
            evidence: ProxmoxEvidence::Guest {
                host_id: "host-1".into(),
                node: "pve-a".into(),
                kind: "qemu".into(),
                vmid: 101,
                display_name: "media".into(),
            },
            calls: Mutex::new(Vec::new()),
        };
        let adopted = resolve_target_with_provider(
            &pool,
            &admin(),
            "proxmox.guest.reboot",
            ProxmoxSelector::Guest {
                host_id: "host-1".into(),
                node: None,
                kind: None,
                vmid: 101,
            },
            &provider,
        )
        .await
        .unwrap();
        let resolved = resources::resolve_alias(&pool, "proxmox.guest", "host-1/pve-a", "qemu:101")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(resolved.id, adopted.resource.id);
        assert_eq!(adopted.guest_kind.as_deref(), Some("qemu"));
    }

    #[tokio::test]
    async fn storage_and_disk_evidence_use_exact_scoped_identity() {
        for (action, selector, evidence, namespace, alias) in [
            (
                "proxmox.storage.delete",
                ProxmoxSelector::Storage {
                    host_id: "host-1".into(),
                    node: "pve".into(),
                    storage: "local".into(),
                },
                ProxmoxEvidence::Storage {
                    host_id: "host-1".into(),
                    node: "pve".into(),
                    storage: "local".into(),
                    display_name: "local".into(),
                },
                "proxmox.storage",
                "local",
            ),
            (
                "proxmox.disk.wipe",
                ProxmoxSelector::Disk {
                    host_id: "host-1".into(),
                    node: "pve".into(),
                    disk: "/dev/sdb".into(),
                },
                ProxmoxEvidence::Disk {
                    host_id: "host-1".into(),
                    node: "pve".into(),
                    disk: "/dev/sdb".into(),
                    display_name: "Disk".into(),
                },
                "proxmox.disk",
                "/dev/sdb",
            ),
        ] {
            let pool = pool().await;
            let provider = FakeProvider {
                evidence,
                calls: Mutex::new(Vec::new()),
            };
            let adopted =
                resolve_target_with_provider(&pool, &admin(), action, selector, &provider)
                    .await
                    .unwrap();
            let resolved = resources::resolve_alias(&pool, namespace, "host-1/pve", alias)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(resolved.id, adopted.resource.id);
        }
    }

    #[tokio::test]
    async fn authorization_denial_precedes_provider_and_resource_observation() {
        let pool = pool().await;
        let provider = FakeProvider {
            evidence: ProxmoxEvidence::Guest {
                host_id: "host-1".into(),
                node: "pve".into(),
                kind: "qemu".into(),
                vmid: 101,
                display_name: "media".into(),
            },
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
            "proxmox.guest.start",
            ProxmoxSelector::Guest {
                host_id: "host-1".into(),
                node: None,
                kind: None,
                vmid: 101,
            },
            &provider,
        )
        .await
        .unwrap_err();
        assert!(matches!(
            error,
            ProxmoxAdoptionError::Invocation(InvocationError::Forbidden)
        ));
        assert!(provider.calls.lock().unwrap().is_empty());
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resources")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn qemu_only_actions_do_not_publish_capability_for_lxc() {
        let pool = pool().await;
        let provider = FakeProvider {
            evidence: ProxmoxEvidence::Guest {
                host_id: "host-1".into(),
                node: "pve".into(),
                kind: "lxc".into(),
                vmid: 101,
                display_name: "media".into(),
            },
            calls: Mutex::new(Vec::new()),
        };
        let error = resolve_target_with_provider(
            &pool,
            &admin(),
            "proxmox.disk.attach",
            ProxmoxSelector::Guest {
                host_id: "host-1".into(),
                node: None,
                kind: None,
                vmid: 101,
            },
            &provider,
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ProxmoxAdoptionError::Unavailable(_)));
        let resource = resources::resolve_alias(&pool, "proxmox.guest", "host-1/pve", "lxc:101")
            .await
            .unwrap();
        assert!(resource.is_none());
    }
}
