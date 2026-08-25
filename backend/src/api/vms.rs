use crate::{
    auth,
    error::{AppError, Result},
    operations::proxmox_adoption::{self, ProxmoxSelector, LEGACY_HOST_ID},
    AppState,
};
use axum::{extract::State, http::HeaderMap, response::Response, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use super::operation_adoption::{self, CompatibilityResult};

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
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("Id") {
            past_header = true;
            continue;
        }
        if trimmed.starts_with('-') {
            continue;
        }
        if !past_header {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let id = if parts[0] == "-" {
            None
        } else {
            parts[0].parse().ok()
        };
        let name = parts[1].to_string();
        let state = parts[2..].join(" ");
        vms.push(LocalVm { name, id, state });
    }
    vms
}

pub async fn list_local(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<LocalVmsResponse>> {
    require_admin(&state, &jar).await?;
    if !crate::vms::is_libvirt_available() {
        return Ok(Json(LocalVmsResponse {
            vms: vec![],
            libvirt_available: false,
        }));
    }
    let out = std::process::Command::new("virsh")
        .args(["list", "--all"])
        .output()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(Json(LocalVmsResponse {
        vms: parse_virsh_list(&stdout),
        libvirt_available: true,
    }))
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
        "start" => "start",
        "shutdown" => "shutdown",
        "reboot" => "reboot",
        "suspend" => "suspend",
        "resume" => "resume",
        "destroy" => "destroy",
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
        Ok(Json(
            serde_json::json!({ "ok": false, "message": if stderr.is_empty() { stdout } else { stderr } }),
        ))
    }
}

// ── Proxmox ───────────────────────────────────────────────────────────────────

const PX_HOST_KEY: &str = "proxmox_host";
const PX_PORT_KEY: &str = "proxmox_port";
const PX_TOKEN_KEY: &str = "proxmox_token";
const PX_NODE_KEY: &str = "proxmox_node";
const PX_VERIFY_KEY: &str = "proxmox_verify_ssl";
const PX_TOKEN_SECRET_NAME: &str = "proxmox_legacy_token";

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
        .bind(PX_HOST_KEY)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let host = host?;
    let port: u16 = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(PX_PORT_KEY)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8006);
    let token =
        match sqlx::query_scalar::<_, String>("SELECT value_enc FROM secrets WHERE name = ?")
            .bind(PX_TOKEN_SECRET_NAME)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
        {
            Some(value_enc) => crate::api::secrets::decrypt(&state.secrets_key, &value_enc).ok(),
            None => sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
                .bind(PX_TOKEN_KEY)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten(),
        };
    let node: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(PX_NODE_KEY)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let verify_ssl: bool =
        sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
            .bind(PX_VERIFY_KEY)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
    Some(ProxmoxConfig {
        host,
        port,
        token: token.unwrap_or_default(),
        node: node.unwrap_or_else(|| "pve".into()),
        verify_ssl,
    })
}

pub async fn get_proxmox_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Option<ProxmoxConfig>>> {
    require_admin(&state, &jar).await?;
    let mut config = load_proxmox_config(&state).await;
    if let Some(value) = &mut config {
        value.token.clear();
    }
    Ok(Json(config))
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
    headers: HeaderMap,
    Json(req): Json<SaveProxmoxConfig>,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(&state, &jar, None).await?;
    let adopted = proxmox_adoption::resolve_target(
        &state.db,
        state.secrets_key.clone(),
        &credential,
        "proxmox.host.configure",
        ProxmoxSelector::System,
    )
    .await?;
    let token_secret_id = if req.token.is_empty() {
        None
    } else {
        Some(super::proxmox::stage_compatibility_secret(&state, &req.token).await?)
    };
    let input = serde_json::json!({
        "host_id": LEGACY_HOST_ID,
        "name": "Legacy Proxmox",
        "url": format!("https://{}:{}", req.host, req.port.unwrap_or(8006)),
        "node": req.node,
        "token_secret_id": token_secret_id,
        "verify_ssl": req.verify_ssl,
    });
    let result = operation_adoption::submit(
        &state,
        &credential,
        &adopted.resource.id,
        "proxmox.host.configure",
        input,
        &headers,
    )
    .await;
    if result.is_err() {
        if let Some(secret_id) = &token_secret_id {
            super::proxmox::discard_compatibility_secret(&state, secret_id).await;
        }
    }
    result
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

pub async fn list_proxmox(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<ProxmoxVmsResponse>> {
    require_admin(&state, &jar).await?;
    let cfg = load_proxmox_config(&state)
        .await
        .ok_or_else(|| AppError::BadRequest("Proxmox not configured".into()))?;

    let client =
        proxmox_client(cfg.verify_ssl).map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let base = proxmox_base(&cfg);
    let auth_header = format!("PVEAPIToken={}", cfg.token);

    // Get nodes list
    let nodes_res: serde_json::Value = client
        .get(format!("{base}/nodes"))
        .header("Authorization", &auth_header)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let nodes: Vec<String> = nodes_res["data"]
        .as_array()
        .unwrap_or(&vec![])
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
            .send()
            .await
        {
            if let Ok(body) = res.json::<serde_json::Value>().await {
                for vm in body["data"].as_array().unwrap_or(&vec![]) {
                    all_vms.push(ProxmoxVm {
                        vmid: vm["vmid"].as_u64().unwrap_or(0),
                        name: vm["name"].as_str().unwrap_or("").to_string(),
                        kind: "qemu".into(),
                        node: node.clone(),
                        status: vm["status"].as_str().unwrap_or("unknown").to_string(),
                        mem: vm["mem"].as_u64().unwrap_or(0),
                        maxmem: vm["maxmem"].as_u64().unwrap_or(0),
                        cpu: vm["cpu"].as_f64().unwrap_or(0.0),
                        uptime: vm["uptime"].as_u64().unwrap_or(0),
                        cpus: vm["cpus"].as_u64().unwrap_or(1),
                    });
                }
            }
        }
        // LXC containers
        if let Ok(res) = client
            .get(format!("{base}/nodes/{node}/lxc"))
            .header("Authorization", &auth_header)
            .send()
            .await
        {
            if let Ok(body) = res.json::<serde_json::Value>().await {
                for vm in body["data"].as_array().unwrap_or(&vec![]) {
                    all_vms.push(ProxmoxVm {
                        vmid: vm["vmid"].as_u64().unwrap_or(0),
                        name: vm["name"].as_str().unwrap_or("").to_string(),
                        kind: "lxc".into(),
                        node: node.clone(),
                        status: vm["status"].as_str().unwrap_or("unknown").to_string(),
                        mem: vm["mem"].as_u64().unwrap_or(0),
                        maxmem: vm["maxmem"].as_u64().unwrap_or(0),
                        cpu: vm["cpu"].as_f64().unwrap_or(0.0),
                        uptime: vm["uptime"].as_u64().unwrap_or(0),
                        cpus: vm["cpus"].as_u64().unwrap_or(1),
                    });
                }
            }
        }
    }

    all_vms.sort_by_key(|v| v.vmid);
    Ok(Json(ProxmoxVmsResponse {
        vms: all_vms,
        nodes,
    }))
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
    headers: HeaderMap,
    Json(req): Json<ProxmoxActionRequest>,
) -> CompatibilityResult<Response> {
    let action = match req.action.as_str() {
        "start" => "start",
        "stop" => "stop",
        "shutdown" => "shutdown",
        "reboot" => "reboot",
        "suspend" => "suspend",
        "resume" => "resume",
        _ => return Err(AppError::BadRequest("unknown action".into()).into()),
    };
    let credential = super::actions::credential(&state, &jar, None).await?;
    let adopted = proxmox_adoption::resolve_target(
        &state.db,
        state.secrets_key.clone(),
        &credential,
        &format!("proxmox.guest.{action}"),
        ProxmoxSelector::Guest {
            host_id: LEGACY_HOST_ID.into(),
            node: Some(req.node),
            kind: Some(req.kind),
            vmid: req.vmid,
        },
    )
    .await?;
    operation_adoption::submit(
        &state,
        &credential,
        &adopted.resource.id,
        &format!("proxmox.guest.{action}"),
        serde_json::json!({}),
        &headers,
    )
    .await
}

pub async fn test_proxmox(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(&state, &jar, None).await?;
    let adopted = proxmox_adoption::resolve_target(
        &state.db,
        state.secrets_key.clone(),
        &credential,
        "proxmox.host.test",
        ProxmoxSelector::Host {
            host_id: LEGACY_HOST_ID.into(),
        },
    )
    .await?;
    operation_adoption::submit(
        &state,
        &credential,
        &adopted.resource.id,
        "proxmox.host.test",
        serde_json::json!({}),
        &headers,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::Cookie;

    #[tokio::test]
    async fn compatibility_config_stages_token_without_persisting_plaintext() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let state = crate::api::mcp::test_support::build(pool.clone());
        let jar = CookieJar::new().add(Cookie::new("vt_session", session));

        let _ = set_proxmox_config(
            State(state.clone()),
            jar.clone(),
            HeaderMap::new(),
            Json(SaveProxmoxConfig {
                host: "pve.internal".into(),
                port: Some(8006),
                token: "root@pam!voidtower=supersecret".into(),
                node: "pve".into(),
                verify_ssl: true,
            }),
        )
        .await
        .unwrap();

        let plaintext: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'proxmox_token'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(plaintext.is_none());
        let encrypted: String =
            sqlx::query_scalar("SELECT value_enc FROM secrets WHERE name LIKE 'proxmox_staged_%'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            crate::api::secrets::decrypt(&state.secrets_key, &encrypted).unwrap(),
            "root@pam!voidtower=supersecret"
        );
        let input: String = sqlx::query_scalar(
            "SELECT input_json FROM jobs WHERE action = 'proxmox.host.configure'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!input.contains("supersecret"));
        assert!(input.contains("token_secret_id"));
    }

    #[test]
    fn legacy_proxmox_mutations_only_submit_canonical_jobs() {
        let source = include_str!("vms.rs");
        for name in ["set_proxmox_config", "proxmox_action", "test_proxmox"] {
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
            let body = &tail[..end];
            assert!(
                body.contains("operation_adoption::submit"),
                "legacy handler {name} must submit through the durable boundary"
            );
            for needle in [
                ".send(",
                ".post(",
                ".delete(",
                "INSERT INTO settings",
                "DELETE FROM settings",
                "audit::log",
            ] {
                assert!(
                    !body.contains(needle),
                    "legacy handler {name} contains forbidden direct execution marker {needle}"
                );
            }
        }
    }
}
