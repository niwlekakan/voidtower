use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use axum::{
    extract::{FromRequest, Multipart, Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    audit, auth,
    error::{AppError, Result},
    operations::{
        invocation::{CredentialContext, PreparedInvocation},
        proxmox_adoption::{self, ProxmoxSelector},
    },
    AppState,
};

use super::operation_adoption::{self, CompatibilityResult};

// ── auth helper ───────────────────────────────────────────────────────────────

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

async fn prepare_or_submit(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
    action: &str,
    selector: ProxmoxSelector,
    input: serde_json::Value,
    dry_run: bool,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(state, jar, None).await?;
    prepare_or_submit_with_credential(
        state,
        &credential,
        headers,
        action,
        selector,
        input,
        dry_run,
    )
    .await
}

async fn prepare_or_submit_with_credential(
    state: &AppState,
    credential: &CredentialContext,
    headers: &HeaderMap,
    action: &str,
    selector: ProxmoxSelector,
    input: serde_json::Value,
    dry_run: bool,
) -> CompatibilityResult<Response> {
    let adopted = proxmox_adoption::resolve_target(
        &state.db,
        state.secrets_key.clone(),
        credential,
        action,
        selector,
    )
    .await?;
    if dry_run {
        let prepared =
            operation_adoption::prepare(state, credential, &adopted.resource.id, action, input)
                .await?;
        return legacy_plan_response(prepared);
    }
    operation_adoption::submit(
        state,
        credential,
        &adopted.resource.id,
        action,
        input,
        headers,
    )
    .await
}

fn legacy_plan_response(prepared: PreparedInvocation) -> CompatibilityResult<Response> {
    let view = prepared.view();
    let mut plan =
        serde_json::to_value(view.operation).map_err(|error| AppError::Internal(error.into()))?;
    plan["risk"] = serde_json::Value::String(
        match plan["risk"].as_str() {
            Some("read") => "low",
            Some("mutate") => "medium",
            _ => "high",
        }
        .into(),
    );
    Ok(Json(serde_json::json!({
        "dry_run": true,
        "plan": plan,
        "policy": view.policy,
        "resource": view.resource,
    }))
    .into_response())
}

fn guest_selector(host_id: String, vmid: u64) -> ProxmoxSelector {
    ProxmoxSelector::Guest {
        host_id,
        node: None,
        kind: None,
        vmid,
    }
}

fn encrypt_secret(key: &[u8; 32], value: &str) -> anyhow::Result<String> {
    use aes_gcm::aead::{rand_core::RngCore, OsRng};
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, value.as_bytes())
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ciphertext);
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &blob,
    ))
}

pub(crate) async fn stage_compatibility_secret(state: &AppState, value: &str) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let name = format!("proxmox_staged_{id}");
    let encrypted = encrypt_secret(&state.secrets_key, value).map_err(AppError::Internal)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query(
        "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) \
         VALUES (?, ?, 'Temporary Proxmox compatibility credential', ?, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(encrypted)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(id)
}

pub(crate) async fn discard_compatibility_secret(state: &AppState, id: &str) {
    let _ = sqlx::query("DELETE FROM secrets WHERE id = ? AND name LIKE 'proxmox_staged_%'")
        .bind(id)
        .execute(&state.db)
        .await;
}

// ── host + token loader ───────────────────────────────────────────────────────

struct HostInfo {
    url: String,
    node: String,
    token: String,
}

async fn get_host_and_token(state: &AppState, host_id: &str) -> Result<HostInfo> {
    let row = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT url, node, fingerprint FROM proxmox_hosts WHERE id = ?",
    )
    .bind(host_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

    let (url, node, _fingerprint) = row;

    let secret_name = format!("proxmox_token_{host_id}");
    let secret_id: String = sqlx::query_scalar("SELECT id FROM secrets WHERE name = ?")
        .bind(&secret_name)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::BadRequest(format!("No token configured for host {host_id}")))?;

    let token =
        crate::api::secrets::resolve(&state.db, &state.secrets_key, &secret_id, "proxmox_api")
            .await
            .map_err(|error| AppError::BadRequest(format!("Proxmox token unavailable: {error}")))?;

    Ok(HostInfo { url, node, token })
}

fn proxmox_base(url: &str) -> String {
    format!("{}/api2/json", url.trim_end_matches('/'))
}

fn build_client() -> std::result::Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

// ── VM type detection ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VmKind {
    Qemu,
    Lxc,
}

impl VmKind {
    fn path_segment(self) -> &'static str {
        match self {
            VmKind::Qemu => "qemu",
            VmKind::Lxc => "lxc",
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            VmKind::Qemu => "qemu",
            VmKind::Lxc => "lxc",
        }
    }
}

async fn detect_vm_kind(
    client: &reqwest::Client,
    base: &str,
    node: &str,
    vmid: u64,
    auth_header: &str,
) -> Result<VmKind> {
    for kind in &[VmKind::Qemu, VmKind::Lxc] {
        let url = format!(
            "{}/nodes/{}/{}/{}/status/current",
            base,
            node,
            kind.path_segment(),
            vmid
        );
        if let Ok(res) = client
            .get(&url)
            .header("Authorization", auth_header)
            .send()
            .await
        {
            if res.status().is_success() {
                return Ok(*kind);
            }
        }
    }
    Err(AppError::NotFound)
}

// ── host CRUD routes ──────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ProxmoxHost {
    pub id: String,
    pub name: String,
    pub url: String,
    pub node: String,
    pub fingerprint: Option<String>,
}

// ── background VM state monitor ───────────────────────────────────────────────

pub async fn run_vm_state_monitor(state: crate::AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(90));
    let mut known: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut initialised = false;

    loop {
        interval.tick().await;

        let hosts: Vec<(String,)> = match sqlx::query_as("SELECT id FROM proxmox_hosts")
            .fetch_all(&state.db)
            .await
        {
            Ok(h) => h,
            Err(_) => continue,
        };

        for (host_id,) in &hosts {
            let host = match get_host_and_token(&state, host_id).await {
                Ok(h) => h,
                Err(_) => continue,
            };
            let client = match build_client() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let base = proxmox_base(&host.url);
            let auth = format!("PVEAPIToken={}", host.token);

            let nodes = match pve_get(&client, &format!("{}/nodes", base), &auth).await {
                Ok(v) => v,
                Err(_) => continue,
            };
            let node_names: Vec<String> = nodes
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|n| n["node"].as_str().map(String::from))
                .collect();

            for node in &node_names {
                for kind in &["qemu", "lxc"] {
                    let Ok(data) =
                        pve_get(&client, &format!("{}/nodes/{}/{}", base, node, kind), &auth).await
                    else {
                        continue;
                    };
                    for vm in data.as_array().unwrap_or(&vec![]) {
                        let vmid = vm["vmid"].as_u64().unwrap_or(0);
                        if vmid == 0 {
                            continue;
                        }
                        let status = vm["status"].as_str().unwrap_or("unknown").to_string();
                        let name = vm["name"].as_str().unwrap_or("unknown").to_string();
                        let key = format!("{}/{}", host_id, vmid);

                        if initialised {
                            if let Some(prev) = known.get(&key) {
                                if *prev != status {
                                    let (title, sev) = match (prev.as_str(), status.as_str()) {
                                        ("running", s) if s != "running" => {
                                            (format!("VM stopped: {name}"), "warning")
                                        }
                                        (_, "running") => (format!("VM started: {name}"), "info"),
                                        _ => (format!("VM state changed: {name}"), "info"),
                                    };
                                    super::alerts::create_alert(
                                        &state.db,
                                        &title,
                                        &format!("{name} on {node} ({host_id}): {prev} → {status}"),
                                        sev,
                                        "containers",
                                        Some("proxmox_vm"),
                                        Some(&vmid.to_string()),
                                    )
                                    .await;
                                }
                            }
                        }
                        known.insert(key, status);
                    }
                }
            }
        }
        initialised = true;
    }
}

pub async fn list_hosts(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let hosts = sqlx::query_as::<_, ProxmoxHost>(
        "SELECT id, name, url, node, fingerprint FROM proxmox_hosts ORDER BY name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(serde_json::json!(hosts)))
}

#[derive(Deserialize)]
pub struct CreateHostRequest {
    pub name: String,
    pub url: String,
    pub node: Option<String>,
    pub fingerprint: Option<String>,
    pub token_id: String,
    pub token_secret: String,
}

pub async fn create_host(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<CreateHostRequest>,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(&state, &jar, None).await?;
    proxmox_adoption::resolve_target(
        &state.db,
        state.secrets_key.clone(),
        &credential,
        "proxmox.host.create",
        ProxmoxSelector::System,
    )
    .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let token = format!("{}={}", req.token_id, req.token_secret);
    let secret_id = stage_compatibility_secret(&state, &token).await?;
    let input = serde_json::json!({
        "host_id": id,
        "name": req.name,
        "url": req.url,
        "node": req.node.unwrap_or_else(|| "pve".into()),
        "fingerprint": req.fingerprint,
        "token_secret_id": secret_id,
    });
    let result = prepare_or_submit_with_credential(
        &state,
        &credential,
        &headers,
        "proxmox.host.create",
        ProxmoxSelector::System,
        input,
        false,
    )
    .await;
    if result.is_err() {
        discard_compatibility_secret(&state, &secret_id).await;
    }
    result
}

pub async fn delete_host(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(host_id): Path<String>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.host.delete",
        ProxmoxSelector::Host { host_id },
        serde_json::json!({}),
        false,
    )
    .await
}
// ── proxmox passthrough routes ────────────────────────────────────────────────

/// GET a Proxmox API endpoint, unwrap `data`, propagate HTTP errors as 502.
async fn pve_get(client: &reqwest::Client, url: &str, auth: &str) -> Result<serde_json::Value> {
    let res = client
        .get(url)
        .header("Authorization", auth)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Proxmox unreachable: {}", e)))?;
    let status = res.status();
    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| AppError::BadRequest(format!("Proxmox response parse error: {}", e)))?;
    if !status.is_success() {
        let msg = body["errors"].to_string();
        return Err(AppError::BadRequest(format!(
            "Proxmox {} — {}",
            status, msg
        )));
    }
    Ok(body["data"].clone())
}

pub async fn list_nodes(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let auth = format!("PVEAPIToken={}", host.token);
    let base = proxmox_base(&host.url);

    // Step 1: list node names
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["node"].as_str().map(String::from))
        .collect();

    // Step 2: fetch full status per node (includes cpu/mem/disk metrics + kversion)
    let mut result = Vec::new();
    for name in &names {
        match pve_get(&client, &format!("{}/nodes/{}/status", base, name), &auth).await {
            Ok(status) => {
                let mut entry = status.clone();
                entry["node"] = serde_json::json!(name);
                entry["status"] = serde_json::json!("online");

                // Subscription is a separate per-node endpoint, not part of /status
                if let Ok(sub) = pve_get(
                    &client,
                    &format!("{}/nodes/{}/subscription", base, name),
                    &auth,
                )
                .await
                {
                    entry["subscription_status"] = sub["status"].clone();
                }

                result.push(entry);
            }
            // Surface the real Proxmox error instead of silently falling back to the
            // metrics-less basic listing — the frontend used to guess "needs Sys.Audit"
            // regardless of the actual cause, which is wrong as often as it's right.
            Err(e) => {
                if let Some(basic) = node_list
                    .as_array()
                    .and_then(|a| a.iter().find(|n| n["node"].as_str() == Some(name.as_str())))
                {
                    let mut entry = basic.clone();
                    entry["status_error"] = serde_json::json!(e.to_string());
                    result.push(entry);
                }
            }
        }
    }
    Ok(Json(serde_json::json!(result)))
}

pub async fn list_vms(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    // Discover node names, then query qemu+lxc per node for complete data
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["node"].as_str().map(String::from))
        .collect();

    let mut all: Vec<serde_json::Value> = Vec::new();
    for node in &names {
        for kind in &["qemu", "lxc"] {
            if let Ok(data) =
                pve_get(&client, &format!("{}/nodes/{}/{}", base, node, kind), &auth).await
            {
                if let Some(arr) = data.as_array() {
                    for vm in arr {
                        let mut v = vm.clone();
                        v["type"] = serde_json::json!(kind);
                        v["node"] = serde_json::json!(node);
                        all.push(v);
                    }
                }
            }
        }
    }
    all.sort_by_key(|v| v["vmid"].as_u64().unwrap_or(0));
    let actor = Some(crate::operations::contracts::ActorRef {
        actor_type: crate::operations::contracts::ActorType::Human,
        id: Some(user.id),
        source: Some("proxmox_inventory".into()),
    });
    let correlation_id = uuid::Uuid::new_v4().to_string();
    for vm in &all {
        let Some(vmid) = vm["vmid"].as_u64() else {
            continue;
        };
        let node = vm["node"].as_str().unwrap_or(&host.node);
        let kind = vm["type"].as_str().unwrap_or("qemu");
        let name = vm["name"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{kind} {vmid}"));
        let scope = format!("{host_id}/{node}");
        let alias = format!("{kind}:{vmid}");
        crate::operations::resources::observe(
            &state.db,
            crate::operations::resources::ObserveResource {
                kind: "proxmox_guest",
                display_name: &name,
                node_id: None,
                provider: Some("proxmox"),
                namespace: "proxmox.guest",
                scope_key: &scope,
                alias: &alias,
            },
            actor.clone(),
            &correlation_id,
        )
        .await
        .map_err(AppError::Internal)?;
    }
    Ok(Json(serde_json::json!(all)))
}

pub async fn list_storage(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    // Discover nodes, collect storage from each
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["node"].as_str().map(String::from))
        .collect();

    let mut all: Vec<serde_json::Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in &names {
        if let Ok(data) = pve_get(&client, &format!("{}/nodes/{}/storage", base, node), &auth).await
        {
            if let Some(arr) = data.as_array() {
                for s in arr {
                    let key = s["storage"].as_str().unwrap_or("").to_string();
                    if !key.is_empty() {
                        let scope = format!("{host_id}/{node}");
                        crate::operations::resources::observe(
                            &state.db,
                            crate::operations::resources::ObserveResource {
                                kind: "proxmox_storage",
                                display_name: &key,
                                node_id: None,
                                provider: Some("proxmox"),
                                namespace: "proxmox.storage",
                                scope_key: &scope,
                                alias: &key,
                            },
                            Some(crate::operations::contracts::ActorRef {
                                actor_type: crate::operations::contracts::ActorType::Human,
                                id: Some(user.id.clone()),
                                source: Some("proxmox_inventory".into()),
                            }),
                            &uuid::Uuid::new_v4().to_string(),
                        )
                        .await
                        .map_err(AppError::Internal)?;
                    }
                    if seen.insert(key) {
                        let mut entry = s.clone();
                        entry["node"] = serde_json::json!(node);
                        all.push(entry);
                    }
                }
            }
        }
    }
    Ok(Json(serde_json::json!(all)))
}

pub async fn list_tasks(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["node"].as_str().map(String::from))
        .collect();

    let mut all_tasks: Vec<serde_json::Value> = Vec::new();
    for node in &names {
        if let Ok(tasks) = pve_get(
            &client,
            &format!("{}/nodes/{}/tasks?limit=50", base, node),
            &auth,
        )
        .await
        {
            if let Some(arr) = tasks.as_array() {
                all_tasks.extend(arr.iter().cloned());
            }
        }
    }
    all_tasks.sort_by(|a, b| {
        b["starttime"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["starttime"].as_u64().unwrap_or(0))
    });
    all_tasks.truncate(50);
    Ok(Json(serde_json::json!(all_tasks)))
}

// ── PBS backup jobs ───────────────────────────────────────────────────────────

pub async fn list_backup_jobs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    // Cluster-level scheduled backup jobs
    let jobs = pve_get(&client, &format!("{}/cluster/backup", base), &auth)
        .await
        .unwrap_or(serde_json::json!([]));

    // Backup archives: query each node's storages and collect backup content
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth)
        .await
        .unwrap_or(serde_json::json!([]));
    let nodes: Vec<String> = node_list
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["node"].as_str().map(String::from))
        .collect();

    let mut archives: Vec<serde_json::Value> = Vec::new();
    for node in &nodes {
        if let Ok(storages) =
            pve_get(&client, &format!("{}/nodes/{}/storage", base, node), &auth).await
        {
            let storage_names: Vec<String> = storages
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter(|s| {
                    s["content"]
                        .as_str()
                        .map(|c| c.contains("backup"))
                        .unwrap_or(false)
                        && s["active"].as_u64().unwrap_or(0) == 1
                })
                .filter_map(|s| s["storage"].as_str().map(String::from))
                .collect();

            for storage in &storage_names {
                let url = format!(
                    "{}/nodes/{}/storage/{}/content?content=backup",
                    base, node, storage
                );
                if let Ok(content) = pve_get(&client, &url, &auth).await {
                    if let Some(arr) = content.as_array() {
                        for item in arr {
                            let mut entry = item.clone();
                            entry["node"] = serde_json::Value::String(node.clone());
                            entry["storage"] = serde_json::Value::String(storage.clone());
                            archives.push(entry);
                        }
                    }
                }
            }
        }
    }

    archives.sort_by(|a, b| {
        b["ctime"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&a["ctime"].as_u64().unwrap_or(0))
    });

    Ok(Json(serde_json::json!({
        "jobs": jobs,
        "archives": archives,
    })))
}

// ── lifecycle action routes ───────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct DryRunBody {
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn vm_start(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "start", body).await
}

pub async fn vm_stop(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "stop", body).await
}

pub async fn vm_shutdown(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "shutdown", body).await
}

pub async fn vm_reboot(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "reboot", body).await
}

pub async fn vm_reset(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "reset", body).await
}

pub async fn vm_suspend(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "suspend", body).await
}

pub async fn vm_resume(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
) -> CompatibilityResult<Response> {
    guest_action(&state, &jar, &headers, host_id, vmid, "resume", None).await
}

async fn guest_action(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
    host_id: String,
    vmid: u64,
    action: &str,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        state,
        jar,
        headers,
        &format!("proxmox.guest.{action}"),
        guest_selector(host_id, vmid),
        serde_json::json!({}),
        body.is_some_and(|body| body.dry_run),
    )
    .await
}
#[derive(Deserialize)]
pub struct SnapshotBody {
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn vm_snapshot(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    Json(req): Json<SnapshotBody>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.snapshot.create",
        guest_selector(host_id, vmid),
        serde_json::json!({"name": req.name, "description": req.description}),
        req.dry_run,
    )
    .await
}

pub async fn vm_rollback(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid, snapname)): Path<(String, u64, String)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.snapshot.rollback",
        guest_selector(host_id, vmid),
        serde_json::json!({"name": snapname}),
        body.is_some_and(|body| body.dry_run),
    )
    .await
}

pub async fn vm_delete_snapshot(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid, snapname)): Path<(String, u64, String)>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.snapshot.delete",
        guest_selector(host_id, vmid),
        serde_json::json!({"name": snapname}),
        body.is_some_and(|body| body.dry_run),
    )
    .await
}
pub async fn vm_vncproxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid)): Path<(String, u64)>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;
    let url = format!(
        "{}/nodes/{}/{}/{}/vncproxy",
        base,
        host.node,
        kind.path_segment(),
        vmid
    );

    let res = client
        .post(&url)
        .header("Authorization", &auth_header)
        .form(&[("websocket", "1")])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("vncproxy request: {}", e)))?;

    if !res.status().is_success() {
        let msg = res.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "vncproxy error: {}",
            msg
        )));
    }

    let body: serde_json::Value = res
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("vncproxy parse: {}", e)))?;

    let data = &body["data"];
    let ticket = data["ticket"].as_str().unwrap_or("").to_string();
    let port = data["port"].as_u64().unwrap_or(5900);

    audit::log(
        &state.db,
        Some(&user.id),
        &user.username,
        "proxmox.vnc.ticket.issue",
        Some("proxmox_guest"),
        Some(&vmid.to_string()),
        "success",
        None,
        Some(&format!(
            "host={} node={} kind={}",
            host_id,
            host.node,
            kind.as_str()
        )),
    )
    .await;

    // Strip scheme so the frontend can build wss:// from it
    let proxmox_host = host
        .url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string();

    Ok(Json(serde_json::json!({
        "ticket":       ticket,
        "port":         port,
        "proxmox_host": proxmox_host,
        "node":         host.node,
        "kind":         kind.as_str(),
        "vmid":         vmid,
    })))
}

#[derive(Deserialize)]
pub struct DeployToLxcRequest {
    pub node: String,
    pub hostname: String,
    pub ostemplate: String,
    pub compose_yaml: String,
    #[serde(default = "lxc_default_cores")]
    pub cores: u32,
    #[serde(default = "lxc_default_memory")]
    pub memory: u32,
    #[serde(default = "lxc_default_storage")]
    pub storage: String,
    #[serde(default = "lxc_default_disk")]
    pub disk_gb: u32,
}

fn lxc_default_cores() -> u32 {
    2
}
fn lxc_default_memory() -> u32 {
    1024
}
fn lxc_default_storage() -> String {
    "local-lvm".into()
}
fn lxc_default_disk() -> u32 {
    20
}

pub async fn deploy_app_to_lxc(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path(host_id): Path<String>,
    Json(req): Json<DeployToLxcRequest>,
) -> CompatibilityResult<Response> {
    let _compose_yaml = req.compose_yaml;
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.lxc.deploy",
        ProxmoxSelector::Host { host_id },
        serde_json::json!({
            "node": req.node,
            "hostname": req.hostname,
            "ostemplate": req.ostemplate,
            "cores": req.cores,
            "memory": req.memory,
            "storage": req.storage,
            "disk_gb": req.disk_gb,
        }),
        false,
    )
    .await
}
pub async fn list_snapshots(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid)): Path<(String, u64)>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let auth = format!("PVEAPIToken={}", host.token);
    let kind = params.get("kind").map(|s| s.as_str()).unwrap_or("qemu");
    let url = format!(
        "{}/nodes/{}/{}/{}/snapshot",
        proxmox_base(&host.url),
        host.node,
        kind,
        vmid
    );
    let data = pve_get(&client, &url, &auth).await?;
    Ok(Json(data))
}

// ── storage content browser ───────────────────────────────────────────────────

pub async fn list_storage_content(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, node, storage)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let url = format!("{}/nodes/{}/storage/{}/content", base, node, storage);
    let data = pve_get(&client, &url, &auth).await?;
    Ok(Json(data))
}

pub async fn upload_storage_content(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, node, storage)): Path<(String, String, String)>,
    request: axum::extract::Request,
) -> CompatibilityResult<Response> {
    use tokio::io::AsyncWriteExt;

    let credential = super::actions::credential(&state, &jar, None).await?;
    let selector = ProxmoxSelector::Storage {
        host_id,
        node,
        storage,
    };
    proxmox_adoption::resolve_target(
        &state.db,
        state.secrets_key.clone(),
        &credential,
        "proxmox.storage.upload",
        selector.clone(),
    )
    .await?;
    let mut multipart = Multipart::from_request(request, &state)
        .await
        .map_err(|error| AppError::BadRequest(format!("Invalid upload: {error}")))?;

    let root = state.config.data_dir.join("proxmox-uploads");
    tokio::fs::create_dir_all(&root)
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let mut content = None;
    let mut staged_name: Option<String> = None;
    loop {
        let next = match multipart.next_field().await {
            Ok(next) => next,
            Err(error) => {
                if let Some(name) = &staged_name {
                    let _ = tokio::fs::remove_file(root.join(name)).await;
                }
                return Err(AppError::BadRequest(format!("Invalid upload: {error}")).into());
            }
        };
        let Some(mut field) = next else { break };
        match field.name() {
            Some("content") => {
                content = Some(match field.text().await {
                    Ok(content) => content,
                    Err(error) => {
                        if let Some(name) = &staged_name {
                            let _ = tokio::fs::remove_file(root.join(name)).await;
                        }
                        return Err(AppError::BadRequest(format!("Invalid upload: {error}")).into());
                    }
                });
            }
            Some("filename") => {
                if let Some(name) = &staged_name {
                    let _ = tokio::fs::remove_file(root.join(name)).await;
                    return Err(
                        AppError::BadRequest("Only one upload file is allowed".into()).into(),
                    );
                }
                let safe_name: String = field
                    .file_name()
                    .unwrap_or("upload.bin")
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                            c
                        } else {
                            '_'
                        }
                    })
                    .take(180)
                    .collect();
                let name = format!("{}--{}", uuid::Uuid::new_v4(), safe_name);
                let path = root.join(&name);
                let mut file = tokio::fs::File::create(&path)
                    .await
                    .map_err(|error| AppError::Internal(error.into()))?;
                let mut length = 0u64;
                loop {
                    let chunk = match field.chunk().await {
                        Ok(chunk) => chunk,
                        Err(error) => {
                            let _ = tokio::fs::remove_file(&path).await;
                            return Err(
                                AppError::BadRequest(format!("Invalid upload: {error}")).into()
                            );
                        }
                    };
                    let Some(chunk) = chunk else { break };
                    length += chunk.len() as u64;
                    if length > 16 * 1024 * 1024 * 1024u64 {
                        let _ = tokio::fs::remove_file(&path).await;
                        return Err(AppError::BadRequest("Upload exceeds 16 GiB".into()).into());
                    }
                    if let Err(error) = file.write_all(&chunk).await {
                        let _ = tokio::fs::remove_file(&path).await;
                        return Err(AppError::Internal(error.into()).into());
                    }
                }
                if let Err(error) = file.flush().await {
                    let _ = tokio::fs::remove_file(&path).await;
                    return Err(AppError::Internal(error.into()).into());
                }
                staged_name = Some(name);
            }
            _ => {}
        }
    }
    let staged_name =
        staged_name.ok_or_else(|| AppError::BadRequest("Missing filename field".into()))?;
    let content = match content {
        Some(content) => content,
        None => {
            let _ = tokio::fs::remove_file(root.join(&staged_name)).await;
            return Err(AppError::BadRequest("Missing content field".into()).into());
        }
    };
    let input = serde_json::json!({"content": content, "staged_file": staged_name});
    let result = prepare_or_submit_with_credential(
        &state,
        &credential,
        &headers,
        "proxmox.storage.upload",
        selector,
        input,
        false,
    )
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(root.join(&staged_name)).await;
    }
    result
}
#[derive(Deserialize)]
pub struct VolidQuery {
    pub volid: String,
}

pub async fn delete_storage_content(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, node, storage)): Path<(String, String, String)>,
    Query(q): Query<VolidQuery>,
    body: Option<Json<DryRunBody>>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.storage.delete",
        ProxmoxSelector::Storage {
            host_id,
            node,
            storage,
        },
        serde_json::json!({"volid": q.volid}),
        body.is_some_and(|body| body.dry_run),
    )
    .await
}
// ── physical disk management ──────────────────────────────────────────────────

pub async fn list_node_disks(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, node)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let url = format!("{}/nodes/{}/disks/list", base, node);
    let data = pve_get(&client, &url, &auth).await?;
    if let Some(disks) = data.as_array() {
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let scope = format!("{host_id}/{node}");
        for disk in disks {
            let path = disk["devpath"]
                .as_str()
                .or_else(|| disk["device"].as_str())
                .unwrap_or("");
            if path.is_empty() {
                continue;
            }
            let display_name = disk["model"]
                .as_str()
                .filter(|value| !value.is_empty())
                .unwrap_or(path);
            crate::operations::resources::observe(
                &state.db,
                crate::operations::resources::ObserveResource {
                    kind: "proxmox_disk",
                    display_name,
                    node_id: None,
                    provider: Some("proxmox"),
                    namespace: "proxmox.disk",
                    scope_key: &scope,
                    alias: path,
                },
                Some(crate::operations::contracts::ActorRef {
                    actor_type: crate::operations::contracts::ActorType::Human,
                    id: Some(user.id.clone()),
                    source: Some("proxmox_inventory".into()),
                }),
                &correlation_id,
            )
            .await
            .map_err(AppError::Internal)?;
        }
    }
    Ok(Json(data))
}

#[derive(Deserialize)]
pub struct DiskQuery {
    pub disk: String,
}

pub async fn disk_smart(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, node)): Path<(String, String)>,
    Query(q): Query<DiskQuery>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let base_url = format!("{}/nodes/{}/disks/smart", base, node);
    let url = reqwest::Url::parse_with_params(&base_url, &[("disk", q.disk.as_str())])
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let data = pve_get(&client, url.as_str(), &auth).await?;
    Ok(Json(data))
}

#[derive(Deserialize)]
pub struct WipeDiskBody {
    pub disk: String,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn wipe_disk(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, node)): Path<(String, String)>,
    Json(req): Json<WipeDiskBody>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.disk.wipe",
        ProxmoxSelector::Disk {
            host_id,
            node,
            disk: req.disk,
        },
        serde_json::json!({}),
        req.dry_run,
    )
    .await
}
#[derive(Deserialize)]
pub struct InitDiskBody {
    pub disk: String,
    pub fstype: String,
    pub name: String,
    #[serde(default)]
    pub raidlevel: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn init_disk_storage(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, node)): Path<(String, String)>,
    Json(req): Json<InitDiskBody>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.disk.initialize",
        ProxmoxSelector::Disk {
            host_id,
            node,
            disk: req.disk,
        },
        serde_json::json!({
            "fstype": req.fstype,
            "name": req.name,
            "raidlevel": req.raidlevel,
        }),
        req.dry_run,
    )
    .await
}
// ── disk passthrough to VM ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DiskPassthroughBody {
    pub disk_path: String,
    #[serde(default = "default_passthrough_bus")]
    pub bus: String,
    #[serde(default)]
    pub dry_run: bool,
}

fn default_passthrough_bus() -> String {
    "scsi1".to_string()
}

pub async fn vm_disk_passthrough(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Path((host_id, vmid)): Path<(String, u64)>,
    Json(req): Json<DiskPassthroughBody>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        &headers,
        "proxmox.disk.attach",
        guest_selector(host_id, vmid),
        serde_json::json!({"disk_path": req.disk_path, "bus": req.bus}),
        req.dry_run,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::Cookie;

    #[tokio::test]
    async fn host_token_loading_uses_secret_resolver_and_fails_closed_when_disabled() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(pool.clone());
        let host_id = "host-loader-test";
        let secret_id = uuid::Uuid::new_v4().to_string();
        let encrypted = crate::api::secrets::encrypt(&state.secrets_key, "fixture-token").unwrap();
        sqlx::query(
            "INSERT INTO proxmox_hosts (id, name, url, node, fingerprint) VALUES (?, 'PVE', 'https://pve.internal:8006', 'pve', NULL)",
        )
        .bind(host_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES (?, ?, ?, 1, 1)",
        )
        .bind(&secret_id)
        .bind(format!("proxmox_token_{host_id}"))
        .bind(encrypted)
        .execute(&pool)
        .await
        .unwrap();

        let loaded = get_host_and_token(&state, host_id).await.unwrap();
        assert_eq!(loaded.token, "fixture-token");
        let last_used_at: Option<i64> =
            sqlx::query_scalar("SELECT last_used_at FROM secrets WHERE id = ?")
                .bind(&secret_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(last_used_at.is_some());

        sqlx::query("UPDATE secrets SET disabled = 1 WHERE id = ?")
            .bind(&secret_id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(get_host_and_token(&state, host_id).await.is_err());
    }

    #[tokio::test]
    async fn host_creation_stages_the_token_and_only_submits_a_durable_job() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let state = crate::api::mcp::test_support::build(pool.clone());
        let jar = CookieJar::new().add(Cookie::new("vt_session", session));

        create_host(
            State(state.clone()),
            jar,
            HeaderMap::new(),
            Json(CreateHostRequest {
                name: "PVE".into(),
                url: "https://pve.internal:8006".into(),
                node: Some("pve".into()),
                fingerprint: None,
                token_id: "root@pam!voidtower".into(),
                token_secret: "supersecret".into(),
            }),
        )
        .await
        .unwrap();

        let hosts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM proxmox_hosts")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(hosts, 0);
        let input: String =
            sqlx::query_scalar("SELECT input_json FROM jobs WHERE action = 'proxmox.host.create'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!input.contains("supersecret"));
        assert!(!input.contains("root@pam!voidtower="));
        assert!(input.contains("token_secret_id"));
        let encrypted: String =
            sqlx::query_scalar("SELECT value_enc FROM secrets WHERE name LIKE 'proxmox_staged_%'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            crate::api::secrets::decrypt(&state.secrets_key, &encrypted).unwrap(),
            "root@pam!voidtower=supersecret"
        );
    }

    #[test]
    fn compatibility_mutations_only_delegate_to_the_canonical_boundary() {
        let source = include_str!("proxmox.rs");
        let function = |name: &str| {
            let declaration = format!("pub async fn {name}");
            let start = source
                .find(&declaration)
                .unwrap_or_else(|| panic!("missing handler {name}"));
            let tail = &source[start..];
            let body_start = tail.find('{').expect("handler must have a body");
            let mut depth = 0usize;
            let mut end = tail.len();
            for (offset, character) in tail[body_start..].char_indices() {
                match character {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = body_start + offset + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            &tail[..end]
        };

        let handlers = [
            "create_host",
            "delete_host",
            "vm_start",
            "vm_stop",
            "vm_shutdown",
            "vm_reboot",
            "vm_reset",
            "vm_suspend",
            "vm_resume",
            "vm_snapshot",
            "vm_rollback",
            "vm_delete_snapshot",
            "deploy_app_to_lxc",
            "upload_storage_content",
            "delete_storage_content",
            "wipe_disk",
            "init_disk_storage",
            "vm_disk_passthrough",
        ];
        let forbidden = [
            ".send(",
            ".post(",
            ".delete(",
            "pve_post",
            "INSERT INTO proxmox_hosts",
            "DELETE FROM proxmox_hosts",
            "INSERT INTO settings",
            "DELETE FROM settings",
            "audit::log",
        ];
        for name in handlers {
            let body = function(name);
            assert!(
                body.contains("prepare_or_submit")
                    || body.contains("guest_action")
                    || body.contains("operation_adoption::submit"),
                "adopted handler {name} must delegate to the durable boundary"
            );
            for needle in forbidden {
                assert!(
                    !body.contains(needle),
                    "adopted handler {name} contains forbidden direct execution marker {needle}"
                );
            }
        }

        for action in [
            "proxmox.host.create",
            "proxmox.host.delete",
            "proxmox.guest.{action}",
            "proxmox.snapshot.create",
            "proxmox.snapshot.rollback",
            "proxmox.snapshot.delete",
            "proxmox.disk.attach",
            "proxmox.lxc.deploy",
            "proxmox.storage.upload",
            "proxmox.storage.delete",
            "proxmox.disk.wipe",
            "proxmox.disk.initialize",
        ] {
            assert!(
                source.contains(action),
                "missing compatibility mapping {action}"
            );
        }
    }
}
