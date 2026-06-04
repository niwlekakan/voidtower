use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

// ── auth helper ──────────────────────────────────────────────────────────────

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

// ── local KVM (virsh) ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LocalVm {
    pub name: String,
    pub id: Option<i32>,
    pub state: String,
}

#[derive(Serialize)]
pub struct LocalVmsResponse {
    pub vms: Vec<LocalVm>,
    pub libvirt_available: bool,
}

fn parse_virsh_list(output: &str) -> Vec<LocalVm> {
    let mut vms = Vec::new();
    let mut past_header = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if trimmed.starts_with("Id") { past_header = true; continue; }
        if trimmed.starts_with('-') { continue; }
        if !past_header { continue; }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 { continue; }
        let id = if parts[0] == "-" { None } else { parts[0].parse().ok() };
        let name = parts[1].to_string();
        let state = parts[2..].join(" ");
        vms.push(LocalVm { name, id, state });
    }
    vms
}

pub async fn list_local(State(state): State<AppState>, jar: CookieJar) -> Result<Json<LocalVmsResponse>> {
    require_admin(&state, &jar).await?;
    if !crate::vms::is_libvirt_available() {
        return Ok(Json(LocalVmsResponse { vms: vec![], libvirt_available: false }));
    }
    let out = std::process::Command::new("virsh")
        .args(["list", "--all"])
        .output()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(Json(LocalVmsResponse { vms: parse_virsh_list(&stdout), libvirt_available: true }))
}

#[derive(Deserialize)]
pub struct LocalActionRequest {
    pub name: String,
    pub action: String,
}

pub async fn local_action(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LocalActionRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let virsh_cmd = match req.action.as_str() {
        "start"    => "start",
        "shutdown" => "shutdown",
        "reboot"   => "reboot",
        "suspend"  => "suspend",
        "resume"   => "resume",
        "destroy"  => "destroy",
        _ => return Err(AppError::BadRequest("unknown action".into())),
    };
    let out = std::process::Command::new("virsh")
        .args([virsh_cmd, &req.name])
        .output()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(Json(serde_json::json!({ "ok": true, "message": stdout })))
    } else {
        Ok(Json(serde_json::json!({ "ok": false, "message": if stderr.is_empty() { stdout } else { stderr } })))
    }
}

// ── Proxmox ───────────────────────────────────────────────────────────────────

const PX_HOST_KEY: &str = "proxmox_host";
const PX_PORT_KEY: &str = "proxmox_port";
const PX_TOKEN_KEY: &str = "proxmox_token";
const PX_NODE_KEY: &str = "proxmox_node";
const PX_VERIFY_KEY: &str = "proxmox_verify_ssl";

#[derive(Serialize, Deserialize, Clone)]
pub struct ProxmoxConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub node: String,
    pub verify_ssl: bool,
}

async fn load_proxmox_config(state: &AppState) -> Option<ProxmoxConfig> {
    let host: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(PX_HOST_KEY).fetch_optional(&state.db).await.ok().flatten();
    let host = host?;
    let port: u16 = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(PX_PORT_KEY).fetch_optional(&state.db).await.ok().flatten()
        .and_then(|v| v.parse().ok()).unwrap_or(8006);
    let token: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(PX_TOKEN_KEY).fetch_optional(&state.db).await.ok().flatten();
    let node: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(PX_NODE_KEY).fetch_optional(&state.db).await.ok().flatten();
    let verify_ssl: bool = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(PX_VERIFY_KEY).fetch_optional(&state.db).await.ok().flatten()
        .map(|v| v == "true").unwrap_or(false);
    Some(ProxmoxConfig {
        host,
        port,
        token: token.unwrap_or_default(),
        node: node.unwrap_or_else(|| "pve".into()),
        verify_ssl,
    })
}

async fn save_setting(state: &AppState, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, unixepoch())
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"
    )
    .bind(key).bind(value)
    .execute(&state.db).await?;
    Ok(())
}

pub async fn get_proxmox_config(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Option<ProxmoxConfig>>> {
    require_admin(&state, &jar).await?;
    Ok(Json(load_proxmox_config(&state).await))
}

#[derive(Deserialize)]
pub struct SaveProxmoxConfig {
    pub host: String,
    pub port: Option<u16>,
    pub token: String,
    pub node: String,
    pub verify_ssl: bool,
}

pub async fn set_proxmox_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SaveProxmoxConfig>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    save_setting(&state, PX_HOST_KEY, &req.host).await?;
    save_setting(&state, PX_PORT_KEY, &req.port.unwrap_or(8006).to_string()).await?;
    save_setting(&state, PX_TOKEN_KEY, &req.token).await?;
    save_setting(&state, PX_NODE_KEY, &req.node).await?;
    save_setting(&state, PX_VERIFY_KEY, if req.verify_ssl { "true" } else { "false" }).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn proxmox_client(verify_ssl: bool) -> std::result::Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(!verify_ssl)
        .timeout(std::time::Duration::from_secs(10))
        .build()
}

fn proxmox_base(cfg: &ProxmoxConfig) -> String {
    format!("https://{}:{}/api2/json", cfg.host, cfg.port)
}

#[derive(Serialize)]
pub struct ProxmoxVm {
    pub vmid: u64,
    pub name: String,
    pub kind: String,
    pub node: String,
    pub status: String,
    pub mem: u64,
    pub maxmem: u64,
    pub cpu: f64,
    pub uptime: u64,
    pub cpus: u64,
}

#[derive(Serialize)]
pub struct ProxmoxVmsResponse {
    pub vms: Vec<ProxmoxVm>,
    pub nodes: Vec<String>,
}

pub async fn list_proxmox(State(state): State<AppState>, jar: CookieJar) -> Result<Json<ProxmoxVmsResponse>> {
    require_admin(&state, &jar).await?;
    let cfg = load_proxmox_config(&state).await
        .ok_or_else(|| AppError::BadRequest("Proxmox not configured".into()))?;

    let client = proxmox_client(cfg.verify_ssl)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&cfg);
    let auth_header = format!("PVEAPIToken={}", cfg.token);

    // Get nodes list
    let nodes_res: serde_json::Value = client
        .get(format!("{base}/nodes"))
        .header("Authorization", &auth_header)
        .send().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
        .json().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let nodes: Vec<String> = nodes_res["data"]
        .as_array().unwrap_or(&vec![])
        .iter()
        .filter_map(|n| n["node"].as_str().map(|s| s.to_string()))
        .collect();

    let target_nodes: Vec<String> = if cfg.node == "all" || cfg.node.is_empty() {
        nodes.clone()
    } else {
        vec![cfg.node.clone()]
    };

    let mut all_vms = Vec::new();

    for node in &target_nodes {
        // QEMU VMs
        if let Ok(res) = client
            .get(format!("{base}/nodes/{node}/qemu"))
            .header("Authorization", &auth_header)
            .send().await
        {
            if let Ok(body) = res.json::<serde_json::Value>().await {
                for vm in body["data"].as_array().unwrap_or(&vec![]) {
                    all_vms.push(ProxmoxVm {
                        vmid:   vm["vmid"].as_u64().unwrap_or(0),
                        name:   vm["name"].as_str().unwrap_or("").to_string(),
                        kind:   "qemu".into(),
                        node:   node.clone(),
                        status: vm["status"].as_str().unwrap_or("unknown").to_string(),
                        mem:    vm["mem"].as_u64().unwrap_or(0),
                        maxmem: vm["maxmem"].as_u64().unwrap_or(0),
                        cpu:    vm["cpu"].as_f64().unwrap_or(0.0),
                        uptime: vm["uptime"].as_u64().unwrap_or(0),
                        cpus:   vm["cpus"].as_u64().unwrap_or(1),
                    });
                }
            }
        }
        // LXC containers
        if let Ok(res) = client
            .get(format!("{base}/nodes/{node}/lxc"))
            .header("Authorization", &auth_header)
            .send().await
        {
            if let Ok(body) = res.json::<serde_json::Value>().await {
                for vm in body["data"].as_array().unwrap_or(&vec![]) {
                    all_vms.push(ProxmoxVm {
                        vmid:   vm["vmid"].as_u64().unwrap_or(0),
                        name:   vm["name"].as_str().unwrap_or("").to_string(),
                        kind:   "lxc".into(),
                        node:   node.clone(),
                        status: vm["status"].as_str().unwrap_or("unknown").to_string(),
                        mem:    vm["mem"].as_u64().unwrap_or(0),
                        maxmem: vm["maxmem"].as_u64().unwrap_or(0),
                        cpu:    vm["cpu"].as_f64().unwrap_or(0.0),
                        uptime: vm["uptime"].as_u64().unwrap_or(0),
                        cpus:   vm["cpus"].as_u64().unwrap_or(1),
                    });
                }
            }
        }
    }

    all_vms.sort_by_key(|v| v.vmid);
    Ok(Json(ProxmoxVmsResponse { vms: all_vms, nodes }))
}

#[derive(Deserialize)]
pub struct ProxmoxActionRequest {
    pub vmid: u64,
    pub kind: String,
    pub node: String,
    pub action: String,
}

pub async fn proxmox_action(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ProxmoxActionRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let cfg = load_proxmox_config(&state).await
        .ok_or_else(|| AppError::BadRequest("Proxmox not configured".into()))?;

    let pve_action = match req.action.as_str() {
        "start"    => "start",
        "stop"     => "stop",
        "shutdown" => "shutdown",
        "reboot"   => "reboot",
        "suspend"  => "suspend",
        "resume"   => "resume",
        _ => return Err(AppError::BadRequest("unknown action".into())),
    };

    let client = proxmox_client(cfg.verify_ssl)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&cfg);
    let auth_header = format!("PVEAPIToken={}", cfg.token);
    let kind_path = if req.kind == "lxc" { "lxc" } else { "qemu" };

    let url = format!("{base}/nodes/{}/{}/{}/status/{pve_action}", req.node, kind_path, req.vmid);
    let res = client
        .post(&url)
        .header("Authorization", &auth_header)
        .send().await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if res.status().is_success() {
        Ok(Json(serde_json::json!({ "ok": true })))
    } else {
        let msg = res.text().await.unwrap_or_default();
        Ok(Json(serde_json::json!({ "ok": false, "message": msg })))
    }
}

pub async fn test_proxmox(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let cfg = load_proxmox_config(&state).await
        .ok_or_else(|| AppError::BadRequest("Proxmox not configured".into()))?;

    let client = proxmox_client(cfg.verify_ssl)
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let url = format!("{}/nodes", proxmox_base(&cfg));
    let auth_header = format!("PVEAPIToken={}", cfg.token);

    match client.get(&url).header("Authorization", &auth_header).send().await {
        Ok(res) if res.status().is_success() => {
            let body: serde_json::Value = res.json().await.unwrap_or_default();
            let nodes: Vec<String> = body["data"].as_array().unwrap_or(&vec![])
                .iter().filter_map(|n| n["node"].as_str().map(|s| s.to_string())).collect();
            Ok(Json(serde_json::json!({ "ok": true, "nodes": nodes })))
        }
        Ok(res) => {
            let status = res.status().as_u16();
            let body = res.text().await.unwrap_or_default();
            Ok(Json(serde_json::json!({ "ok": false, "message": format!("HTTP {status}: {body}") })))
        }
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "message": e.to_string() }))),
    }
}
