use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::collections::HashMap;
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{audit, auth, error::{AppError, Result}, AppState};

// ── crypto helper (same logic as api/secrets.rs) ─────────────────────────────

fn decrypt_secret(key: &[u8; 32], encoded: &str) -> anyhow::Result<String> {
    let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| anyhow::anyhow!("base64 decode failed"))?;
    anyhow::ensure!(blob.len() > 12, "ciphertext too short");
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    String::from_utf8(plaintext).map_err(Into::into)
}

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

// ── host + token loader ───────────────────────────────────────────────────────

struct HostInfo {
    url:  String,
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

    // token is stored encrypted in the secrets table under key proxmox_token_{host_id}
    let secret_name = format!("proxmox_token_{}", host_id);
    let enc_row = sqlx::query_as::<_, (String,)>(
        "SELECT value_enc FROM secrets WHERE name = ?",
    )
    .bind(&secret_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::BadRequest(format!("No token configured for host {}", host_id)))?;

    let token = decrypt_secret(&state.secrets_key, &enc_row.0)
        .map_err(|e| AppError::BadRequest(format!("Token decryption failed: {}", e)))?;

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
enum VmKind { Qemu, Lxc }

impl VmKind {
    fn path_segment(self) -> &'static str {
        match self { VmKind::Qemu => "qemu", VmKind::Lxc => "lxc" }
    }
    fn as_str(self) -> &'static str {
        match self { VmKind::Qemu => "qemu", VmKind::Lxc => "lxc" }
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
            base, node, kind.path_segment(), vmid
        );
        if let Ok(res) = client.get(&url).header("Authorization", auth_header).send().await {
            if res.status().is_success() {
                return Ok(*kind);
            }
        }
    }
    Err(AppError::NotFound)
}

// ── shared response helpers ───────────────────────────────────────────────────

fn task_response(body: serde_json::Value) -> serde_json::Value {
    let upid = body["data"].as_str().unwrap_or("").to_string();
    serde_json::json!({ "ok": true, "task": upid })
}

// ── host CRUD routes ──────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct ProxmoxHost {
    pub id:          String,
    pub name:        String,
    pub url:         String,
    pub node:        String,
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
            .fetch_all(&state.db).await { Ok(h) => h, Err(_) => continue };

        for (host_id,) in &hosts {
            let host = match get_host_and_token(&state, host_id).await {
                Ok(h) => h, Err(_) => continue,
            };
            let client = match build_client() { Ok(c) => c, Err(_) => continue };
            let base = proxmox_base(&host.url);
            let auth = format!("PVEAPIToken={}", host.token);

            let nodes = match pve_get(&client, &format!("{}/nodes", base), &auth).await {
                Ok(v) => v, Err(_) => continue,
            };
            let node_names: Vec<String> = nodes.as_array().unwrap_or(&vec![])
                .iter().filter_map(|n| n["node"].as_str().map(String::from)).collect();

            for node in &node_names {
                for kind in &["qemu", "lxc"] {
                    let Ok(data) = pve_get(&client, &format!("{}/nodes/{}/{}", base, node, kind), &auth).await
                        else { continue };
                    for vm in data.as_array().unwrap_or(&vec![]) {
                        let vmid = vm["vmid"].as_u64().unwrap_or(0);
                        if vmid == 0 { continue; }
                        let status = vm["status"].as_str().unwrap_or("unknown").to_string();
                        let name   = vm["name"].as_str().unwrap_or("unknown").to_string();
                        let key    = format!("{}/{}", host_id, vmid);

                        if initialised {
                            if let Some(prev) = known.get(&key) {
                                if *prev != status {
                                    let (title, sev) = match (prev.as_str(), status.as_str()) {
                                        ("running", s) if s != "running" =>
                                            (format!("VM stopped: {name}"), "warning"),
                                        (_, "running") =>
                                            (format!("VM started: {name}"), "info"),
                                        _ =>
                                            (format!("VM state changed: {name}"), "info"),
                                    };
                                    super::alerts::create_alert(
                                        &state.db, &title,
                                        &format!("{name} on {node} ({host_id}): {prev} → {status}"),
                                        sev, "containers",
                                        Some("proxmox_vm"), Some(&vmid.to_string()),
                                    ).await;
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
    pub name:        String,
    pub url:         String,
    pub node:        Option<String>,
    pub fingerprint: Option<String>,
    pub token_id:    String,
    pub token_secret: String,
}

pub async fn create_host(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateHostRequest>,
) -> Result<Json<serde_json::Value>> {
    use aes_gcm::aead::{OsRng, rand_core::RngCore};

    require_admin(&state, &jar).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let node = req.node.as_deref().unwrap_or("pve").to_string();

    sqlx::query(
        "INSERT INTO proxmox_hosts (id, name, url, node, fingerprint) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.url)
    .bind(&node)
    .bind(&req.fingerprint)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    // PVE API token format: "user@realm!tokenname=uuid"
    let token = format!("{}={}", req.token_id, req.token_secret);

    // encrypt and store token
    let cipher = Aes256Gcm::new(state.secrets_key.as_ref().into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, token.as_bytes())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("encryption failed")))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ciphertext);
    let enc = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blob);

    let secret_name = format!("proxmox_token_{}", id);
    let secret_id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query(
        "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&secret_id)
    .bind(&secret_name)
    .bind(format!("Proxmox API token for host {}", req.name))
    .bind(&enc)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

pub async fn delete_host(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    sqlx::query("DELETE FROM proxmox_hosts WHERE id = ?")
        .bind(&host_id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;
    let secret_name = format!("proxmox_token_{}", host_id);
    let _ = sqlx::query("DELETE FROM secrets WHERE name = ?")
        .bind(&secret_name)
        .execute(&state.db)
        .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── proxmox passthrough routes ────────────────────────────────────────────────

/// GET a Proxmox API endpoint, unwrap `data`, propagate HTTP errors as 502.
async fn pve_get(client: &reqwest::Client, url: &str, auth: &str) -> Result<serde_json::Value> {
    let res = client.get(url).header("Authorization", auth)
        .send().await.map_err(|e| AppError::BadRequest(format!("Proxmox unreachable: {}", e)))?;
    let status = res.status();
    let body: serde_json::Value = res.json().await
        .map_err(|e| AppError::BadRequest(format!("Proxmox response parse error: {}", e)))?;
    if !status.is_success() {
        let msg = body["errors"].to_string();
        return Err(AppError::BadRequest(format!("Proxmox {} — {}", status, msg)));
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
    let auth  = format!("PVEAPIToken={}", host.token);
    let base  = proxmox_base(&host.url);

    // Step 1: list node names
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list.as_array().unwrap_or(&vec![])
        .iter().filter_map(|n| n["node"].as_str().map(String::from)).collect();

    // Step 2: fetch full status per node (includes cpu/mem/disk metrics)
    let mut result = Vec::new();
    for name in &names {
        if let Ok(status) = pve_get(&client, &format!("{}/nodes/{}/status", base, name), &auth).await {
            let mut entry = status.clone();
            entry["node"]   = serde_json::json!(name);
            entry["status"] = serde_json::json!("online");
            result.push(entry);
        } else if let Some(basic) = node_list.as_array().and_then(|a| a.iter().find(|n| n["node"].as_str() == Some(name.as_str()))) {
            result.push(basic.clone());
        }
    }
    Ok(Json(serde_json::json!(result)))
}

pub async fn list_vms(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    // Discover node names, then query qemu+lxc per node for complete data
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list.as_array().unwrap_or(&vec![])
        .iter().filter_map(|n| n["node"].as_str().map(String::from)).collect();

    let mut all: Vec<serde_json::Value> = Vec::new();
    for node in &names {
        for kind in &["qemu", "lxc"] {
            if let Ok(data) = pve_get(&client, &format!("{}/nodes/{}/{}", base, node, kind), &auth).await {
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
    Ok(Json(serde_json::json!(all)))
}

pub async fn list_storage(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    // Discover nodes, collect storage from each
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await?;
    let names: Vec<String> = node_list.as_array().unwrap_or(&vec![])
        .iter().filter_map(|n| n["node"].as_str().map(String::from)).collect();

    let mut all: Vec<serde_json::Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for node in &names {
        if let Ok(data) = pve_get(&client, &format!("{}/nodes/{}/storage", base, node), &auth).await {
            if let Some(arr) = data.as_array() {
                for s in arr {
                    let key = s["storage"].as_str().unwrap_or("").to_string();
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
    let names: Vec<String> = node_list.as_array().unwrap_or(&vec![])
        .iter().filter_map(|n| n["node"].as_str().map(String::from)).collect();

    let mut all_tasks: Vec<serde_json::Value> = Vec::new();
    for node in &names {
        if let Ok(tasks) = pve_get(&client, &format!("{}/nodes/{}/tasks?limit=50", base, node), &auth).await {
            if let Some(arr) = tasks.as_array() {
                all_tasks.extend(arr.iter().cloned());
            }
        }
    }
    all_tasks.sort_by(|a, b| b["starttime"].as_u64().unwrap_or(0).cmp(&a["starttime"].as_u64().unwrap_or(0)));
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
    let jobs = pve_get(&client, &format!("{}/cluster/backup", base), &auth).await
        .unwrap_or(serde_json::json!([]));

    // Backup archives: query each node's storages and collect backup content
    let node_list = pve_get(&client, &format!("{}/nodes", base), &auth).await
        .unwrap_or(serde_json::json!([]));
    let nodes: Vec<String> = node_list.as_array().unwrap_or(&vec![])
        .iter().filter_map(|n| n["node"].as_str().map(String::from)).collect();

    let mut archives: Vec<serde_json::Value> = Vec::new();
    for node in &nodes {
        if let Ok(storages) = pve_get(&client, &format!("{}/nodes/{}/storage", base, node), &auth).await {
            let storage_names: Vec<String> = storages.as_array().unwrap_or(&vec![])
                .iter()
                .filter(|s| s["content"].as_str().map(|c| c.contains("backup")).unwrap_or(false)
                    && s["active"].as_u64().unwrap_or(0) == 1)
                .filter_map(|s| s["storage"].as_str().map(String::from))
                .collect();

            for storage in &storage_names {
                let url = format!("{}/nodes/{}/storage/{}/content?content=backup", base, node, storage);
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

    archives.sort_by(|a, b| b["ctime"].as_u64().unwrap_or(0).cmp(&a["ctime"].as_u64().unwrap_or(0)));

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
    Path((host_id, vmid)): Path<(String, u64)>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;
    let url = format!("{}/nodes/{}/{}/{}/status/start", base, host.node, kind.path_segment(), vmid);

    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

pub async fn vm_stop(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid)): Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let dry_run = body.map(|b| b.dry_run).unwrap_or(false);
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;

    if dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "action": "stop",
            "vmid": vmid,
            "type": kind.as_str(),
            "node": host.node,
        })));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.vm.stop", Some("vm"), Some(&vmid.to_string()),
        "success", None, Some(&format!("host={} node={} type={}", host_id, host.node, kind.as_str())),
    ).await;

    // graceful ACPI shutdown
    let url = format!("{}/nodes/{}/{}/{}/status/shutdown", base, host.node, kind.path_segment(), vmid);
    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

// shutdown is an alias for stop (same graceful ACPI behaviour)
pub async fn vm_shutdown(
    state: State<AppState>,
    jar: CookieJar,
    path: Path<(String, u64)>,
    body: Option<Json<DryRunBody>>,
) -> Result<Json<serde_json::Value>> {
    vm_stop(state, jar, path, body).await
}

pub async fn vm_reboot(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid)): Path<(String, u64)>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;
    let url = format!("{}/nodes/{}/{}/{}/status/reboot", base, host.node, kind.path_segment(), vmid);

    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
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
    Path((host_id, vmid)): Path<(String, u64)>,
    Json(req): Json<SnapshotBody>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;

    if req.dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "action": "snapshot",
            "vmid": vmid,
            "type": kind.as_str(),
            "node": host.node,
            "snap_name": req.name,
        })));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.vm.snapshot", Some("vm"), Some(&vmid.to_string()),
        "success", None,
        Some(&format!("host={} node={} type={} snap={}", host_id, host.node, kind.as_str(), req.name)),
    ).await;

    let url = format!("{}/nodes/{}/{}/{}/snapshot", base, host.node, kind.path_segment(), vmid);
    let mut params = std::collections::HashMap::new();
    params.insert("snapname", req.name.clone());
    if let Some(desc) = &req.description {
        params.insert("description", desc.clone());
    }

    let res = client.post(&url).header("Authorization", &auth_header)
        .form(&params)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

pub async fn vm_rollback(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid, snapname)): Path<(String, u64, String)>,
    body: Option<Json<DryRunBody>>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let dry_run = body.map(|b| b.dry_run).unwrap_or(false);
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;

    if dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "action": "rollback",
            "vmid": vmid,
            "type": kind.as_str(),
            "node": host.node,
            "snap_name": snapname,
        })));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.vm.rollback", Some("vm"), Some(&vmid.to_string()),
        "success", None,
        Some(&format!("host={} node={} type={} snap={}", host_id, host.node, kind.as_str(), snapname)),
    ).await;

    let url = format!(
        "{}/nodes/{}/{}/{}/snapshot/{}/rollback",
        base, host.node, kind.path_segment(), vmid, snapname
    );
    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

pub async fn vm_delete_snapshot(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid, snapname)): Path<(String, u64, String)>,
    body: Option<Json<DryRunBody>>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let dry_run = body.map(|b| b.dry_run).unwrap_or(false);
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;

    if dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "action": "delete_snapshot",
            "vmid": vmid,
            "type": kind.as_str(),
            "node": host.node,
            "snap_name": snapname,
        })));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.vm.delete_snapshot", Some("vm"), Some(&vmid.to_string()),
        "success", None,
        Some(&format!("host={} node={} type={} snap={}", host_id, host.node, kind.as_str(), snapname)),
    ).await;

    let url = format!(
        "{}/nodes/{}/{}/{}/snapshot/{}",
        base, host.node, kind.path_segment(), vmid, snapname
    );
    let res = client.delete(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

pub async fn vm_vncproxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, vmid)): Path<(String, u64)>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;
    let url = format!("{}/nodes/{}/{}/{}/vncproxy", base, host.node, kind.path_segment(), vmid);

    let res = client
        .post(&url)
        .header("Authorization", &auth_header)
        .form(&[("websocket", "1")])
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("vncproxy request: {}", e)))?;

    if !res.status().is_success() {
        let msg = res.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("vncproxy error: {}", msg)));
    }

    let body: serde_json::Value = res.json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("vncproxy parse: {}", e)))?;

    let data = &body["data"];
    let ticket = data["ticket"].as_str().unwrap_or("").to_string();
    let port   = data["port"].as_u64().unwrap_or(5900);

    // Strip scheme so the frontend can build wss:// from it
    let proxmox_host = host.url
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
    let url = format!("{}/nodes/{}/{}/{}/snapshot", proxmox_base(&host.url), host.node, kind, vmid);
    let data = pve_get(&client, &url, &auth).await?;
    Ok(Json(data))
}
