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

/// Shape expected by the frontend's `ChangePlanModal` (see `components/ui/ChangePlanModal.tsx`).
fn change_plan(title: &str, risk: &str, changes: Vec<(&str, String)>) -> serde_json::Value {
    let changes: Vec<serde_json::Value> = changes
        .into_iter()
        .map(|(label, value)| serde_json::json!({ "label": label, "value": value }))
        .collect();
    serde_json::json!({
        "dry_run": true,
        "plan": { "title": title, "risk": risk, "changes": changes, "preview": null }
    })
}

fn vm_target(kind: VmKind, vmid: u64, node: &str) -> String {
    format!("{} {} ({})", kind.as_str().to_uppercase(), vmid, node)
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

    // Step 2: fetch full status per node (includes cpu/mem/disk metrics + kversion)
    let mut result = Vec::new();
    for name in &names {
        match pve_get(&client, &format!("{}/nodes/{}/status", base, name), &auth).await {
            Ok(status) => {
                let mut entry = status.clone();
                entry["node"]   = serde_json::json!(name);
                entry["status"] = serde_json::json!("online");

                // Subscription is a separate per-node endpoint, not part of /status
                if let Ok(sub) = pve_get(&client, &format!("{}/nodes/{}/subscription", base, name), &auth).await {
                    entry["subscription_status"] = sub["status"].clone();
                }

                result.push(entry);
            }
            // Surface the real Proxmox error instead of silently falling back to the
            // metrics-less basic listing — the frontend used to guess "needs Sys.Audit"
            // regardless of the actual cause, which is wrong as often as it's right.
            Err(e) => {
                if let Some(basic) = node_list.as_array().and_then(|a| a.iter().find(|n| n["node"].as_str() == Some(name.as_str()))) {
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
        return Ok(Json(change_plan(
            "Start VM/LXC", "low",
            vec![("Target", vm_target(kind, vmid, &host.node))],
        )));
    }

    let url = format!("{}/nodes/{}/{}/{}/status/start", base, host.node, kind.path_segment(), vmid);

    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        audit::log(
            &state.db, Some(&user.id), &user.username,
            "proxmox.vm.start", Some("vm"), Some(&vmid.to_string()),
            "success", None, Some(&format!("host={} node={} type={}", host_id, host.node, kind.as_str())),
        ).await;
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
        return Ok(Json(change_plan(
            "Stop VM/LXC", "medium",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Action", "Graceful shutdown (ACPI)".to_string()),
                ("Reversible", "Yes — start it again afterward".to_string()),
            ],
        )));
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
        return Ok(Json(change_plan(
            "Reboot VM/LXC", "medium",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Action", "Graceful reboot (ACPI)".to_string()),
            ],
        )));
    }

    let url = format!("{}/nodes/{}/{}/{}/status/reboot", base, host.node, kind.path_segment(), vmid);

    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        audit::log(
            &state.db, Some(&user.id), &user.username,
            "proxmox.vm.reboot", Some("vm"), Some(&vmid.to_string()),
            "success", None, Some(&format!("host={} node={} type={}", host_id, host.node, kind.as_str())),
        ).await;
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

// Hard reset — QEMU only, no LXC equivalent (a container has no virtual power button to cycle).
pub async fn vm_reset(
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
    if kind == VmKind::Lxc {
        return Err(AppError::BadRequest("Reset is not supported for LXC containers".into()));
    }

    if dry_run {
        return Ok(Json(change_plan(
            "Reset VM", "high",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Action", "Hard reset — equivalent to pressing the physical reset button".to_string()),
                ("Reversible", "No — unsaved guest OS state is lost".to_string()),
            ],
        )));
    }

    let url = format!("{}/nodes/{}/{}/{}/status/reset", base, host.node, kind.path_segment(), vmid);
    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        audit::log(
            &state.db, Some(&user.id), &user.username,
            "proxmox.vm.reset", Some("vm"), Some(&vmid.to_string()),
            "success", None, Some(&format!("host={} node={} type={}", host_id, host.node, kind.as_str())),
        ).await;
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

pub async fn vm_suspend(
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
        return Ok(Json(change_plan(
            "Suspend VM/LXC", "medium",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Action", "Suspend to RAM — execution paused, memory state kept".to_string()),
            ],
        )));
    }

    let url = format!("{}/nodes/{}/{}/{}/status/suspend", base, host.node, kind.path_segment(), vmid);
    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        audit::log(
            &state.db, Some(&user.id), &user.username,
            "proxmox.vm.suspend", Some("vm"), Some(&vmid.to_string()),
            "success", None, Some(&format!("host={} node={} type={}", host_id, host.node, kind.as_str())),
        ).await;
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

pub async fn vm_resume(
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

    let url = format!("{}/nodes/{}/{}/{}/status/resume", base, host.node, kind.path_segment(), vmid);
    let res = client.post(&url).header("Authorization", &auth_header)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        audit::log(
            &state.db, Some(&user.id), &user.username,
            "proxmox.vm.resume", Some("vm"), Some(&vmid.to_string()),
            "success", None, Some(&format!("host={} node={} type={}", host_id, host.node, kind.as_str())),
        ).await;
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
        return Ok(Json(change_plan(
            "Create Snapshot", "low",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Name", req.name.clone()),
                ("Description", req.description.clone().unwrap_or_else(|| "—".to_string())),
            ],
        )));
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
        return Ok(Json(change_plan(
            "Rollback to Snapshot", "high",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Snapshot", snapname.clone()),
                ("Effect", "Reverts disk and config to the snapshot state".to_string()),
                ("Reversible", "No — changes made since the snapshot are lost".to_string()),
            ],
        )));
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
        return Ok(Json(change_plan(
            "Delete Snapshot", "medium",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Snapshot", snapname.clone()),
                ("Reversible", "No — this snapshot cannot be recovered".to_string()),
            ],
        )));
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

async fn pve_post(
    client: &reqwest::Client,
    url: &str,
    auth: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    let res = client
        .post(url)
        .header("Authorization", auth)
        .json(body)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Proxmox unreachable: {e}")))?;
    let status = res.status();
    let resp: serde_json::Value = res
        .json()
        .await
        .map_err(|e| AppError::BadRequest(format!("Proxmox response parse error: {e}")))?;
    if !status.is_success() {
        let msg = resp["errors"].to_string();
        return Err(AppError::BadRequest(format!("Proxmox {status} — {msg}")));
    }
    Ok(resp["data"].clone())
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

fn lxc_default_cores()   -> u32    { 2 }
fn lxc_default_memory()  -> u32    { 1024 }
fn lxc_default_storage() -> String { "local-lvm".into() }
fn lxc_default_disk()    -> u32    { 20 }

pub async fn deploy_app_to_lxc(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
    Json(req): Json<DeployToLxcRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let host   = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let auth   = format!("PVEAPIToken={}", host.token);
    let base   = proxmox_base(&host.url);

    // Next available VMID
    let vmid_val = pve_get(&client, &format!("{base}/cluster/nextid"), &auth).await?;
    let vmid = vmid_val
        .as_str()
        .ok_or_else(|| AppError::BadRequest("Could not obtain next VMID".into()))?
        .to_string();

    // Create the LXC container
    let create_body = serde_json::json!({
        "vmid":     vmid,
        "hostname": req.hostname,
        "ostemplate": req.ostemplate,
        "cores":    req.cores,
        "memory":   req.memory,
        "rootfs":   format!("{}:{}", req.storage, req.disk_gb),
        "net0":     "name=eth0,bridge=vmbr0,ip=dhcp",
        "start":    1,
        "onboot":   1,
        "features": "nesting=1",
    });

    pve_post(
        &client,
        &format!("{base}/nodes/{}/lxc", req.node),
        &auth,
        &create_body,
    )
    .await?;

    // Ensure it starts (Proxmox may queue it; ignore if already running)
    let _ = pve_post(
        &client,
        &format!("{base}/nodes/{}/lxc/{vmid}/status/start", req.node),
        &auth,
        &serde_json::json!({}),
    )
    .await;

    let bootstrap = format!(
        "#!/bin/bash\n\
         # VoidTower bootstrap — {hostname}\n\
         set -e\n\
         apt-get update -q && apt-get install -y -q curl ca-certificates\n\
         curl -fsSL https://get.docker.com | sh\n\
         systemctl enable --now docker\n\
         mkdir -p /opt/app\n\
         cat > /opt/app/docker-compose.yml << 'COMPOSE_EOF'\n\
         {compose}\n\
         COMPOSE_EOF\n\
         cd /opt/app && docker compose up -d\n\
         echo \"Done — {hostname} is running.\"",
        hostname = req.hostname,
        compose  = req.compose_yaml,
    );

    Ok(Json(serde_json::json!({
        "vmid":             vmid,
        "hostname":         req.hostname,
        "node":             req.node,
        "bootstrap_script": bootstrap,
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

// ── storage content browser ───────────────────────────────────────────────────

/// Proxmox volids look like `local:iso/foo.iso` or `local-lvm:vm-101-disk-0` — `:` and `/`
/// must be percent-encoded when the volid is embedded in a URL path segment.
fn encode_volid(volid: &str) -> String {
    volid.replace('%', "%25").replace(':', "%3A").replace('/', "%2F")
}

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
    Path((host_id, node, storage)): Path<(String, String, String)>,
    request: axum::extract::Request,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;

    // Stream the browser's multipart body verbatim to Proxmox without buffering into RAM.
    // The frontend sends fields in Proxmox's required order: `content` (type) then `filename` (file data).
    let ct = request
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("multipart/form-data")
        .to_string();

    let stream = request.into_body().into_data_stream();

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(900))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let url = format!("{}/nodes/{}/storage/{}/upload", base, node, storage);

    let res = client
        .post(&url)
        .header("Authorization", &auth)
        .header(reqwest::header::CONTENT_TYPE, &ct)
        .body(reqwest::Body::wrap_stream(stream))
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if !res.status().is_success() {
        let msg = res.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Proxmox upload error: {}", msg)));
    }
    let body: serde_json::Value = res.json().await.unwrap_or_default();

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.storage.upload", Some("proxmox_storage"), Some(&storage),
        "success", None,
        Some(&format!("host={} node={} storage={}", host_id, node, storage)),
    ).await;

    Ok(Json(task_response(body)))
}

#[derive(Deserialize)]
pub struct VolidQuery {
    pub volid: String,
}

pub async fn delete_storage_content(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, node, storage)): Path<(String, String, String)>,
    Query(q): Query<VolidQuery>,
    body: Option<Json<DryRunBody>>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let dry_run = body.map(|b| b.dry_run).unwrap_or(false);
    let host = get_host_and_token(&state, &host_id).await?;

    if dry_run {
        return Ok(Json(change_plan(
            "Delete Storage Content", "high",
            vec![
                ("Target", q.volid.clone()),
                ("Storage", format!("{} ({})", storage, node)),
                ("Reversible", "No — the file is permanently removed from storage".to_string()),
            ],
        )));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.storage.delete_content", Some("proxmox_storage"), Some(&storage),
        "success", None, Some(&format!("host={} node={} storage={} volid={}", host_id, node, storage, q.volid)),
    ).await;

    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let url = format!("{}/nodes/{}/storage/{}/content/{}", base, node, storage, encode_volid(&q.volid));

    let res = client.delete(&url).header("Authorization", &auth)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await.unwrap_or_default();
        Ok(Json(task_response(body)))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}

// ── physical disk management ──────────────────────────────────────────────────

pub async fn list_node_disks(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((host_id, node)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let url = format!("{}/nodes/{}/disks/list", base, node);
    let data = pve_get(&client, &url, &auth).await?;
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
    Path((host_id, node)): Path<(String, String)>,
    Json(req): Json<WipeDiskBody>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;

    if req.dry_run {
        return Ok(Json(change_plan(
            "Wipe Disk", "high",
            vec![
                ("Disk", req.disk.clone()),
                ("Node", node.clone()),
                ("Effect", "Erases the partition table and all data on this disk".to_string()),
                ("Reversible", "No — data cannot be recovered".to_string()),
            ],
        )));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.disk.wipe", Some("proxmox_disk"), Some(&req.disk),
        "success", None, Some(&format!("host={} node={}", host_id, node)),
    ).await;

    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);
    let url = format!("{}/nodes/{}/disks/wipedisk", base, node);
    let data = pve_post(&client, &url, &auth, &serde_json::json!({ "disk": req.disk })).await?;
    let upid = data.as_str().unwrap_or("").to_string();
    Ok(Json(serde_json::json!({ "ok": true, "task": upid })))
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
    Path((host_id, node)): Path<(String, String)>,
    Json(req): Json<InitDiskBody>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;

    if req.dry_run {
        return Ok(Json(change_plan(
            "Initialize Disk as Storage", "high",
            vec![
                ("Disk", req.disk.clone()),
                ("Filesystem", req.fstype.clone()),
                ("Storage name", req.name.clone()),
                ("Effect", "Formats the disk and registers it as a new Proxmox storage pool".to_string()),
                ("Reversible", "No — existing data on the disk is destroyed".to_string()),
            ],
        )));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.disk.init", Some("proxmox_disk"), Some(&req.disk),
        "success", None, Some(&format!("host={} node={} fstype={} name={}", host_id, node, req.fstype, req.name)),
    ).await;

    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth = format!("PVEAPIToken={}", host.token);

    let (endpoint, body) = match req.fstype.as_str() {
        "directory" => ("directory", serde_json::json!({
            "device": req.disk, "name": req.name, "filesystem": "ext4", "add_storage": 1,
        })),
        "lvm" => ("lvm", serde_json::json!({
            "device": req.disk, "name": req.name, "add_storage": 1,
        })),
        "lvmthin" => ("lvmthin", serde_json::json!({
            "device": req.disk, "name": req.name, "add_storage": 1,
        })),
        "zfs" => ("zfs", serde_json::json!({
            "devices": req.disk, "name": req.name,
            "raidlevel": req.raidlevel.clone().unwrap_or_else(|| "single".to_string()),
            "add_storage": 1,
        })),
        other => return Err(AppError::BadRequest(format!("Unknown filesystem type: {}", other))),
    };

    let url = format!("{}/nodes/{}/disks/{}", base, node, endpoint);
    let data = pve_post(&client, &url, &auth, &body).await?;
    let upid = data.as_str().unwrap_or("").to_string();
    Ok(Json(serde_json::json!({ "ok": true, "task": upid })))
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
    Path((host_id, vmid)): Path<(String, u64)>,
    Json(req): Json<DiskPassthroughBody>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let host = get_host_and_token(&state, &host_id).await?;
    let client = build_client().map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&host.url);
    let auth_header = format!("PVEAPIToken={}", host.token);

    let kind = detect_vm_kind(&client, &base, &host.node, vmid, &auth_header).await?;
    if kind == VmKind::Lxc {
        return Err(AppError::BadRequest("Disk passthrough is only supported for QEMU VMs".into()));
    }

    if req.dry_run {
        return Ok(Json(change_plan(
            "Attach Disk Passthrough", "high",
            vec![
                ("Target", vm_target(kind, vmid, &host.node)),
                ("Host disk", req.disk_path.clone()),
                ("Bus/slot", req.bus.clone()),
                ("Effect", "Maps the raw host block device directly into the VM — bypasses Proxmox's virtual disk image".to_string()),
                ("Caution", "The disk becomes unavailable to the host while attached; detach before reusing it elsewhere".to_string()),
            ],
        )));
    }

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "proxmox.vm.disk_passthrough", Some("vm"), Some(&vmid.to_string()),
        "success", None,
        Some(&format!("host={} node={} disk={} bus={}", host_id, host.node, req.disk_path, req.bus)),
    ).await;

    let url = format!("{}/nodes/{}/qemu/{}/config", base, host.node, vmid);
    let mut params = std::collections::HashMap::new();
    params.insert(req.bus.clone(), req.disk_path.clone());

    let res = client.post(&url).header("Authorization", &auth_header)
        .form(&params)
        .send().await.map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Err(AppError::Internal(anyhow::anyhow!("Proxmox error: {}", msg)))
    }
}
