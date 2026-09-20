use crate::{
    audit, auth, containers,
    error::{AppError, Result},
    AppState,
};

use axum::{
    body::{Body, Bytes},
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, io::Read, net::SocketAddr, path::Path as StdPath, sync::OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiIntegration {
    pub level: String, // "native" | "aware"
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredEnvVar {
    pub key: String,
    pub description: String,
    /// If set, auto-generate a value at deploy time when not supplied by the user.
    /// Supported: "random_hex_N" (N hex chars), "uuid"
    #[serde(default)]
    pub generate: Option<String>,
    /// Fallback value used when not supplied by the user and `generate` is unset/empty.
    #[serde(default)]
    pub default: Option<String>,
}

/// Generate a value for a required_env `generate` strategy string.
/// "uuid" -> a v4 UUID; "random_hex_N" -> N random hex characters (default 32).
fn generate_required_env_value(strategy: &str) -> String {
    if strategy == "uuid" {
        return uuid::Uuid::new_v4().to_string();
    }
    let hex_len = strategy
        .strip_prefix("random_hex_")
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(32);
    let mut bytes = vec![0u8; hex_len.div_ceil(2)];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let hex = hex::encode(bytes);
    hex[..hex_len.min(hex.len())].to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub version_hint: String,
    pub compose: Value,
    #[serde(default)]
    pub links: HashMap<String, String>,
    #[serde(default)]
    pub ai_integration: Option<AiIntegration>,
    #[serde(default)]
    pub no_web_ui: bool,
    /// If set, VoidTower checks for a marker file at
    /// `/var/lib/voidtower/.<value>-system-installed` before deploying.
    /// If the marker exists the deploy is rejected with a port-conflict error.
    #[serde(default)]
    pub system_conflict_check: Option<String>,
    /// Explicit host port for the web UI. When set, overrides the first port
    /// extracted from the compose file so that port badges and the embed proxy
    /// point at the correct UI endpoint rather than an internal/API port.
    #[serde(default)]
    pub web_port: Option<u16>,
    /// URL path prefix for the web UI (e.g. "/admin" for Pi-hole).
    /// Appended to the embed URL so the iframe lands on the right page.
    #[serde(default)]
    pub web_path: Option<String>,
    /// Env vars this app needs at deploy time. Entries with `generate` are
    /// auto-populated if the user doesn't provide them; others are shown as
    /// required fields in the pre-deploy modal.
    #[serde(default)]
    pub required_env: Vec<RequiredEnvVar>,
    /// A one-shot `docker exec` run in the background after a successful deploy,
    /// retried until it succeeds or `max_wait_secs` elapses. Used for apps whose
    /// own bootstrap env vars (e.g. Authentik's AUTHENTIK_BOOTSTRAP_PASSWORD) don't
    /// reliably apply on every image/version, so the admin account is set directly
    /// instead. `${VAR}` placeholders in `command` are substituted from the same
    /// resolved required_env/override values written to the deploy's `.env` file.
    #[serde(default)]
    pub post_deploy: Option<PostDeployHook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostDeployHook {
    /// Container to exec into is `{project_name}-{container_suffix}-1`.
    pub container_suffix: String,
    pub command: Vec<String>,
    #[serde(default = "default_post_deploy_wait")]
    pub max_wait_secs: u64,
}

fn default_post_deploy_wait() -> u64 {
    120
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedApp {
    pub id: String,
    pub app_id: String,
    pub app_name: String,
    pub project_name: String,
    pub status: String,
    pub deployed_at: i64,
    pub primary_port: Option<i64>,
    #[serde(default = "default_origin")]
    pub origin: String,
    #[serde(default)]
    pub target_node_id: Option<String>,
}

fn default_origin() -> String {
    "voidtower".into()
}

/// Extract the first published host port from a docker-compose services block.
fn first_port_from_compose(compose: &Value) -> Option<u16> {
    let services = compose.get("services")?.as_object()?;
    for svc in services.values() {
        let ports = svc.get("ports")?.as_array()?;
        for entry in ports {
            // Short syntax string: "3000:80" or "3000"
            if let Some(s) = entry.as_str() {
                let host_part = s.split(':').next().unwrap_or("").trim();
                if let Ok(p) = host_part.parse::<u16>() {
                    if p > 0 {
                        return Some(p);
                    }
                }
            }
            // Short syntax integer: 3000
            if let Some(n) = entry.as_u64() {
                if n > 0 && n <= 65535 {
                    return Some(n as u16);
                }
            }
            // Long syntax: { published: 3000, target: 80 }
            if let Some(p) = entry.get("published").and_then(|v| v.as_u64()) {
                if p > 0 && p <= u64::from(u16::MAX) {
                    return Some(p as u16);
                }
            }
        }
    }
    None
}

/// Return the host port that serves the web UI for a catalog app.
///
/// If the YAML declares `web_port`, that value wins. Otherwise the first
/// published host port from the compose file is used.
fn extract_primary_port(app: &AppDef) -> Option<u16> {
    app.web_port
        .or_else(|| first_port_from_compose(&app.compose))
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct DeployRequest {
    pub app_id: String,
    pub project_name: Option<String>,
    pub env_overrides: Option<HashMap<String, String>>,
    /// Member-only: manual override of which assigned drive (by id) to use as
    /// the volume root. Ignored for non-member callers. Absent/empty = auto
    /// (least-full assigned drive, else the member's quota directory).
    #[serde(default)]
    pub storage_drive_id: Option<String>,
    /// Member-only: manual override of which of the member's own
    /// `agent_capable` nodes to target. Ignored for non-member callers.
    /// Absent/empty = primary host.
    #[serde(default)]
    pub target_node_id: Option<String>,
}

#[derive(Serialize)]
pub struct CatalogResponse {
    pub apps: Vec<AppDef>,
}

#[derive(Serialize)]
pub struct DeployedResponse {
    pub apps: Vec<DeployedApp>,
    pub docker_available: bool,
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn load_catalog(catalog_dir: &std::path::Path) -> Vec<AppDef> {
    let mut apps = Vec::new();

    // Also check dev fallback
    let search_dirs = [
        catalog_dir.to_path_buf(),
        std::path::PathBuf::from("../app-vault/apps"),
        std::path::PathBuf::from("../../app-vault/apps"),
    ];

    for dir in &search_dirs {
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yml") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(app) = serde_yaml::from_str::<AppDef>(&content) else {
                continue;
            };
            apps.push(app);
        }
        if !apps.is_empty() {
            break;
        }
    }

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps
}

// ─── LLM auto-detection ──────────────────────────────────────────────────────

/// Known local LLM services in priority order.
/// Each entry: (port, path to probe, human label, OpenAI-compat /v1 base URL)
const LLM_PROBES: &[(u16, &str, &str, &str)] = &[
    (
        8090,
        "/health",
        "llama.cpp",
        "http://host.docker.internal:8090/v1",
    ),
    (
        8080,
        "/health",
        "llama.cpp",
        "http://host.docker.internal:8080/v1",
    ),
    (
        11434,
        "/api/version",
        "Ollama",
        "http://host.docker.internal:11434/v1",
    ),
    (
        1234,
        "/v1/models",
        "LM Studio",
        "http://host.docker.internal:1234/v1",
    ),
    (
        5001,
        "/v1/models",
        "Text Generation Web UI",
        "http://host.docker.internal:5001/v1",
    ),
    (
        8000,
        "/v1/models",
        "vLLM",
        "http://host.docker.internal:8000/v1",
    ),
];

pub struct DetectedLlm {
    pub label: String,
    pub port: u16,
    pub url: String,
}

/// Try each known LLM service port with a short timeout.
/// Returns the first one that responds.
pub async fn detect_llm_endpoint() -> Option<DetectedLlm> {
    use std::time::Duration;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(400))
        .build()
        .ok()?;

    for &(port, path, label, v1_url) in LLM_PROBES {
        let url = format!("http://127.0.0.1:{port}{path}");
        let Ok(mut response) = client.get(&url).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let mut body = Vec::new();
        loop {
            let Ok(chunk) = response.chunk().await else {
                body.clear();
                break;
            };
            let Some(chunk) = chunk else {
                break;
            };
            if body.len().saturating_add(chunk.len()) > 64 * 1024 {
                body.clear();
                break;
            }
            body.extend_from_slice(&chunk);
        }
        if body.is_empty() {
            continue;
        }
        let Ok(body) = std::str::from_utf8(&body) else {
            continue;
        };
        if serde_json::from_str::<Value>(body).is_err() {
            continue;
        }
        return Some(DetectedLlm {
            label: label.into(),
            port,
            url: v1_url.into(),
        });
    }
    None
}

/// Returns true if NVIDIA GPU support is available for *deployed* containers.
///
/// Checks two things, because VoidTower itself may be running containerized
/// (e.g. the TrueNAS SCALE AIO) without GPU passthrough into its own
/// container — `nvidia-smi` would be absent there even on a GPU host:
///
///   1. `nvidia-smi -L` exits 0 — true on bare-metal/native installs where
///      VoidTower runs directly on the host with the NVIDIA driver.
///   2. The host Docker daemon (reached via the bind-mounted
///      `/var/run/docker.sock`) has the `nvidia` runtime registered — true
///      whenever nvidia-container-toolkit is configured on the host,
///      regardless of whether VoidTower's own container has GPU access.
///      This is the path that matters for TrueNAS SCALE, which configures
///      the `nvidia` runtime on its Docker daemon when a GPU is assigned to
///      apps in System Settings → Advanced.
pub async fn detect_gpu() -> bool {
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    async fn bounded_probe_output(
        mut reader: impl tokio::io::AsyncRead + Unpin,
    ) -> Option<Vec<u8>> {
        const MAX_BYTES: usize = 64 * 1024;
        let mut output = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = reader.read(&mut buffer).await.ok()?;
            if read == 0 {
                break;
            }
            if output.len() < MAX_BYTES {
                let retained = (MAX_BYTES - output.len()).min(read);
                output.extend_from_slice(&buffer[..retained]);
                if retained < read {
                    return None;
                }
            } else {
                return None;
            }
        }
        Some(output)
    }

    async fn run_probe(program: &str, args: &[&str], capture_stdout: bool) -> Option<Vec<u8>> {
        let mut command = tokio::process::Command::new(program);
        command.args(args).kill_on_drop(true).stderr(Stdio::null());
        if capture_stdout {
            command.stdout(Stdio::piped());
        } else {
            command.stdout(Stdio::null());
        }
        let mut child = command.spawn().ok()?;
        if !capture_stdout {
            return tokio::time::timeout(Duration::from_secs(2), child.wait())
                .await
                .ok()?
                .ok()
                .filter(|status| status.success())
                .map(|_| Vec::new());
        }
        let stdout = child.stdout.take()?;
        let mut output_task = tokio::spawn(bounded_probe_output(stdout));
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut wait = Box::pin(child.wait());
            tokio::select! {
                output = &mut output_task => {
                    let output = match output {
                        Ok(Some(output)) => output,
                        _ => {
                            drop(wait);
                            let _ = child.kill().await;
                            let _ = child.wait().await;
                            let _ = output_task.await;
                            return None;
                        }
                    };
                    let status = wait.await.ok()?;
                    status.success().then_some(output)
                }
                status = &mut wait => {
                    let status = status.ok()?;
                    let output = output_task.await.ok()??;
                    status.success().then_some(output)
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    if run_probe("nvidia-smi", &["-L"], false).await.is_some() {
        return true;
    }
    run_probe("docker", &["info", "--format", "{{json .Runtimes}}"], true)
        .await
        .and_then(|output| serde_json::from_slice::<Value>(&output).ok())
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|runtimes| runtimes.contains_key("nvidia"))
}

/// If any service references vt-proxy in its networks section, inject the
/// top-level `networks: { vt-proxy: { external: true } }` declaration so
/// Docker Compose uses the shared external network instead of creating a
/// project-scoped one.
fn inject_external_networks(compose: &mut Value) {
    let references_vt_proxy = compose
        .get("services")
        .and_then(|s| s.as_object())
        .map(|svcs| {
            svcs.values().any(|svc| {
                svc.get("networks")
                    .and_then(|n| n.as_object())
                    .map(|nets| nets.contains_key("vt-proxy"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if !references_vt_proxy {
        return;
    }

    let nets = compose
        .as_object_mut()
        .map(|m| m.entry("networks").or_insert_with(|| serde_json::json!({})));

    if let Some(nets_val) = nets {
        if let Some(obj) = nets_val.as_object_mut() {
            obj.entry("vt-proxy")
                .or_insert_with(|| serde_json::json!({ "external": true }));
        }
    }
}

/// Ensure the vt-proxy Docker network exists (creates it if missing), and is
/// IPv4-only.
///
/// New networks are created with `--ipv6=false` so they never inherit the
/// Docker daemon's IPv6 default address-pool. On hosts whose daemon enables
/// an IPv6 ULA pool (e.g. TrueNAS SCALE, pool `fdd0::/48`), an auto-assigned
/// IPv6 gateway gets stored with its CIDR suffix (`fdd0:0:0:2::1/64`) which
/// later makes `docker compose up` fail with
/// `ParseAddr("fdd0:0:0:2::1/64"): unexpected character, want colon`.
///
/// If `vt-proxy` already exists with IPv6 enabled (created by a VoidTower
/// build before this fix), it's recreated IPv4-only — any containers
/// currently attached are reconnected afterwards — so existing installs
/// self-heal without a manual `docker network rm vt-proxy`.
async fn ensure_vt_proxy_network() {
    let inspect = tokio::process::Command::new("docker")
        .args([
            "network",
            "inspect",
            "vt-proxy",
            "--format",
            "{{.EnableIPv6}}",
        ])
        .output()
        .await;

    match inspect {
        // Network exists and is already IPv4-only — nothing to do.
        Ok(out)
            if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() != "true" => {}
        // Network exists but has IPv6 enabled — recreate it IPv4-only.
        Ok(out) if out.status.success() => recreate_vt_proxy_network_ipv4().await,
        // Network doesn't exist (or docker errored) — create it fresh.
        _ => {
            let _ = tokio::process::Command::new("docker")
                .args(["network", "create", "--ipv6=false", "vt-proxy"])
                .output()
                .await;
        }
    }
}

/// Recreate the `vt-proxy` network IPv4-only, reconnecting any containers
/// that were attached to the old (IPv6-enabled) network.
async fn recreate_vt_proxy_network_ipv4() {
    let containers = tokio::process::Command::new("docker")
        .args([
            "network",
            "inspect",
            "vt-proxy",
            "--format",
            "{{range .Containers}}{{.Name}} {{end}}",
        ])
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    for name in &containers {
        let _ = tokio::process::Command::new("docker")
            .args(["network", "disconnect", "-f", "vt-proxy", name])
            .output()
            .await;
    }

    let _ = tokio::process::Command::new("docker")
        .args(["network", "rm", "vt-proxy"])
        .output()
        .await;

    let _ = tokio::process::Command::new("docker")
        .args(["network", "create", "--ipv6=false", "vt-proxy"])
        .output()
        .await;

    for name in &containers {
        let _ = tokio::process::Command::new("docker")
            .args(["network", "connect", "vt-proxy", name])
            .output()
            .await;
    }
}

/// Force IPv4-only on every compose-managed network so deployments don't fail
/// on hosts whose Docker daemon has an IPv6 default address-pool (see
/// `ensure_vt_proxy_network` for the full failure mode). Ensures the implicit
/// project `default` network is declared and pins `enable_ipv6: false` on all
/// non-external networks. External networks are skipped — their config is fixed
/// at creation time.
fn force_ipv4_networks(compose: &mut Value) {
    let Some(root) = compose.as_object_mut() else {
        return;
    };
    let nets = root
        .entry("networks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(nets_obj) = nets.as_object_mut() else {
        return;
    };

    // Pin the implicit project `default` network to IPv4 even when no app-level
    // network is declared.
    nets_obj
        .entry("default")
        .or_insert_with(|| serde_json::json!({}));

    for cfg in nets_obj.values_mut() {
        // A bare `network: ` entry deserialises as null — normalise to an object.
        if cfg.is_null() {
            *cfg = serde_json::json!({});
        }
        let Some(obj) = cfg.as_object_mut() else {
            continue;
        };
        // Cannot set options on external networks.
        if obj.get("external").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }
        obj.insert("enable_ipv6".into(), Value::Bool(false));
    }
}

/// Rewrite VoidTower-managed bind-mount sources under `${HOME}/.local/share/voidtower/`
/// (currently just Ollama's shared model directory) to live under VoidTower's own
/// `data_dir` instead.
///
/// `${HOME}` in a compose file is expanded by the `docker compose` CLI using
/// *VoidTower's own* process environment. That's correct on bare-metal (VoidTower's
/// `$HOME` is the host user's home), but meaningless when VoidTower itself runs
/// containerized (TrueNAS SCALE AIO): `$HOME` there is the container's home (e.g.
/// `/root`), and the host Docker daemon then resolves that path against its own
/// root filesystem, not VoidTower's data. Rewriting to `data_dir` keeps the path on
/// the one VoidTower already knows how to translate for containerized installs —
/// `rewrite_host_bind_mounts` (below) then maps it to `host_data_dir` on TrueNAS, or
/// leaves it as-is on bare-metal.
fn rewrite_voidtower_home_paths(compose: &mut Value, config: &crate::config::Config) {
    const PREFIX: &str = "${HOME}/.local/share/voidtower/";
    let data_dir = config
        .data_dir
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();

    let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) else {
        return;
    };
    for svc in services.values_mut() {
        let Some(vols) = svc.get_mut("volumes").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for vol in vols.iter_mut() {
            let Some(s) = vol.as_str() else { continue };
            if let Some(rest) = s.strip_prefix(PREFIX) {
                *vol = Value::String(format!("{data_dir}/{rest}"));
            }
        }
    }
}

/// Rewrite bind-mount sources under `config.data_dir` to `config.host_data_dir`.
///
/// VoidTower writes files it wants to share with deployed containers (e.g.
/// nginx-proxy's `conf.d`) under its own data directory and bind-mounts that
/// path into the other container. On bare-metal installs this is a no-op —
/// `data_dir` and `host_data_dir` are the same path.
///
/// When VoidTower itself runs containerized (TrueNAS SCALE Custom App), the
/// `docker compose` CLI inside VoidTower's container talks to the *host's*
/// Docker daemon over the bind-mounted socket. Bind-mount sources in compose
/// files are resolved by that daemon against the host filesystem, not
/// VoidTower's container filesystem — so a source of `/var/lib/voidtower/...`
/// (valid inside VoidTower's container) would resolve to a nonexistent or
/// unrelated path on the TrueNAS host instead of the actual dataset at
/// `/mnt/<pool>/voidtower/data/...`. This rewrites such sources to
/// `host_data_dir` so the host daemon finds the right files.
fn rewrite_host_bind_mounts(compose: &mut Value, config: &crate::config::Config) {
    if config.data_dir == config.host_data_dir {
        return;
    }
    let data_dir = config
        .data_dir
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let host_data_dir = config
        .host_data_dir
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();

    let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) else {
        return;
    };
    for svc in services.values_mut() {
        let Some(vols) = svc.get_mut("volumes").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for vol in vols.iter_mut() {
            let Some(s) = vol.as_str() else { continue };
            // Bind mounts are `<source>:<target>[:mode]`; named volumes have no
            // leading `/` on the source and must be left untouched.
            if let Some(rest) = s.strip_prefix(&format!("{data_dir}/")) {
                *vol = Value::String(format!("{host_data_dir}/{rest}"));
            } else if s == data_dir || s.starts_with(&format!("{data_dir}:")) {
                let rest = s.strip_prefix(&data_dir).unwrap_or("");
                *vol = Value::String(format!("{host_data_dir}{rest}"));
            }
        }
    }
}

/// For a member-owned deploy, rewrite the compose's top-level named-volume
/// declarations (Docker-managed volumes living under `/var/lib/docker/volumes`,
/// invisible to the member and outside their quota/drive) into bind mounts
/// under the member's resolved `storage_root` — this is what makes "their own
/// isolated storage" a real, member-visible host directory instead of an
/// opaque Docker volume. Externally-declared volumes (`external: true`) are
/// left untouched since they reference something outside VoidTower's control.
fn rewrite_named_volumes_to_storage_root(
    compose: &mut Value,
    storage_root: &str,
    project_name: &str,
) {
    let rewritable_names: Vec<String> = compose
        .get("volumes")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter(|(_, def)| {
                    !def.get("external")
                        .and_then(|e| e.as_bool())
                        .unwrap_or(false)
                })
                .map(|(k, _)| k.clone())
                .collect()
        })
        .unwrap_or_default();
    if rewritable_names.is_empty() {
        return;
    }

    let base = std::path::Path::new(storage_root).join(project_name);

    if let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) {
        for svc in services.values_mut() {
            let Some(vols) = svc.get_mut("volumes").and_then(|v| v.as_array_mut()) else {
                continue;
            };
            for vol in vols.iter_mut() {
                let Some(s) = vol.as_str() else { continue };
                let Some((name, rest)) = s.split_once(':') else {
                    continue;
                };
                if !rewritable_names.iter().any(|n| n == name) {
                    continue;
                }
                let host_path = base.join(name);
                let _ = std::fs::create_dir_all(&host_path);
                *vol = Value::String(format!("{}:{}", host_path.display(), rest));
            }
        }
    }

    // Drop the now-unused top-level declarations for the ones we rewrote —
    // they're bind mounts on specific services now, not compose-managed volumes.
    if let Some(vols) = compose.get_mut("volumes").and_then(|v| v.as_object_mut()) {
        for name in &rewritable_names {
            vols.remove(name);
        }
    }
}

/// Custom-tier compose security boundary (plan §5) — applied only to
/// member-submitted custom deploys, never to catalog deploys (those are
/// already backend-generated/vetted, not user-supplied). Silently downgrades
/// privilege escalation (safe to strip); hard-rejects anything that would
/// escape the member's own storage or the project-scoped bridge network,
/// since those can't be "fixed" without changing what was actually asked for.
#[allow(dead_code)]
fn validate_and_sanitize_custom_deploy(
    svc: &mut Value,
    storage_root: &str,
) -> std::result::Result<(), String> {
    let Some(obj) = svc.as_object_mut() else {
        return Ok(());
    };

    // Strip privilege/capability escalation — safe to silently downgrade.
    obj.remove("privileged");
    obj.remove("cap_add");

    // Reject host networking outright — custom deploys get the same
    // project-scoped bridge network every catalog deploy already gets.
    if obj.get("network_mode").and_then(|v| v.as_str()) == Some("host") {
        return Err("network_mode: host is not allowed for member-deployed apps".to_string());
    }

    // Every bind-mount host path must resolve under the member's own
    // storage_root; the Docker socket is never allowed regardless of path.
    if let Some(vols) = obj.get("volumes").and_then(|v| v.as_array()) {
        let canon_root = std::fs::canonicalize(storage_root)
            .unwrap_or_else(|_| std::path::PathBuf::from(storage_root));
        for vol in vols {
            let Some(s) = vol.as_str() else { continue };
            let host_part = s.split(':').next().unwrap_or("");
            if host_part.is_empty() || !host_part.starts_with('/') {
                continue; // named volume, not a host bind mount
            }
            if host_part.contains("docker.sock") {
                return Err("Mounting the Docker socket is not allowed".to_string());
            }
            let host_path = std::path::Path::new(host_part);
            let canon_host =
                std::fs::canonicalize(host_path).unwrap_or_else(|_| host_path.to_path_buf());
            if !canon_host.starts_with(&canon_root) {
                return Err(format!(
                    "Bind mount '{host_part}' is outside your own storage"
                ));
            }
        }
    }

    Ok(())
}

/// Detect the CUDA major version supported by the host driver.
/// Parses "CUDA Version: X.Y" from `nvidia-smi -q` output.
async fn detect_cuda_major_version() -> Option<u32> {
    let out = tokio::process::Command::new("nvidia-smi")
        .arg("-q")
        .output()
        .await
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|l| l.trim_start().starts_with("CUDA Version"))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().split('.').next())
        .and_then(|s| s.parse::<u32>().ok())
}

/// For services using `runtime: nvidia`, apply GPU compatibility fixes:
/// - `ipc: host`              — required for CUDA IPC on open-kernel-module hosts
/// - `NVIDIA_DISABLE_REQUIRE` — suppresses CUDA version checks when the host
///   driver is ahead of the container's bundled CUDA toolkit (e.g. host CUDA 13
///   running a container built against CUDA 12)
fn apply_nvidia_compat(compose: &mut Value, host_cuda_major: Option<u32>) {
    let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) else {
        return;
    };
    for svc in services.values_mut() {
        if !matches!(svc.get("runtime").and_then(|v| v.as_str()), Some("nvidia")) {
            continue;
        }
        let obj = svc.as_object_mut().expect("service is object");
        obj.entry("ipc".to_string())
            .or_insert(Value::String("host".into()));
        obj.entry("privileged".to_string())
            .or_insert(Value::Bool(true));
        if matches!(host_cuda_major, Some(v) if v > 12) {
            if let Some(env) = obj.get_mut("environment").and_then(|e| e.as_array_mut()) {
                if !env.iter().any(
                    |e| matches!(e.as_str(), Some(s) if s.starts_with("NVIDIA_DISABLE_REQUIRE=")),
                ) {
                    env.push(Value::String("NVIDIA_DISABLE_REQUIRE=1".into()));
                }
            }
        }
    }
}

/// When no GPU is available, switch the llama.cpp image from the CUDA variant
/// to the CPU-only variant so deployment doesn't fail pulling an unusable image.
fn adjust_cuda_image_for_no_gpu(compose: &mut Value) {
    let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) else {
        return;
    };
    for svc in services.values_mut() {
        let Some(img_val) = svc.get_mut("image") else {
            continue;
        };
        if let Some(new_img) = img_val
            .as_str()
            .filter(|s| s.contains("llama.cpp:server-cuda"))
            .map(|s| s.replace("server-cuda", "server"))
        {
            *img_val = Value::String(new_img);
        }
    }
}

/// Remove GPU requirements from all services when NVIDIA hardware is not detected.
/// Strips both `runtime: nvidia` (requires NVIDIA Container Toolkit) and
/// `deploy.resources.reservations.devices` so deployment doesn't fail with
/// "unknown runtime specified nvidia" on machines without NVIDIA drivers.
fn strip_gpu_requirements(compose: &mut Value) {
    let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) else {
        return;
    };
    for svc in services.values_mut() {
        // Remove `runtime: nvidia` — requires NVIDIA Container Toolkit
        if let Some(obj) = svc.as_object_mut() {
            if obj.get("runtime").and_then(|v| v.as_str()) == Some("nvidia") {
                obj.remove("runtime");
            }
        }
        let Some(deploy) = svc.get_mut("deploy") else {
            continue;
        };
        let Some(resources) = deploy.get_mut("resources") else {
            continue;
        };
        let Some(reservations) = resources.get_mut("reservations") else {
            continue;
        };
        if let Some(obj) = reservations.as_object_mut() {
            obj.remove("devices");
        }
        // Prune empty intermediate keys
        if reservations
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(false)
        {
            resources.as_object_mut().map(|o| o.remove("reservations"));
        }
        if resources.as_object().map(|o| o.is_empty()).unwrap_or(false) {
            deploy.as_object_mut().map(|o| o.remove("resources"));
        }
        if deploy.as_object().map(|o| o.is_empty()).unwrap_or(false) {
            svc.as_object_mut().map(|o| o.remove("deploy"));
        }
    }
}

/// Remove `devices:` entries for host device nodes that don't exist, so deploys
/// don't fail outright over optional hardware passthrough — e.g. Ollama's
/// `/dev/dri:/dev/dri` (Intel/AMD VAAPI render node), which NVIDIA-only,
/// virtualised, and most containerized-VoidTower (TrueNAS AIO) hosts don't have.
/// NVIDIA GPU access goes through `runtime: nvidia` /
/// `deploy.resources.reservations.devices`, handled separately by
/// `strip_gpu_requirements` — this only covers raw `/dev/*` device-node mounts.
fn strip_unavailable_devices(compose: &mut Value) {
    let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) else {
        return;
    };
    for svc in services.values_mut() {
        let Some(obj) = svc.as_object_mut() else {
            continue;
        };
        let became_empty = match obj.get_mut("devices").and_then(|d| d.as_array_mut()) {
            Some(devices) => {
                devices.retain(|d| {
                    let Some(s) = d.as_str() else { return true };
                    let host_path = s.split(':').next().unwrap_or(s);
                    std::path::Path::new(host_path).exists()
                });
                devices.is_empty()
            }
            None => false,
        };
        if became_empty {
            obj.remove("devices");
        }
    }
}

/// Ensure host directories referenced by volume mounts actually exist so Docker
/// doesn't create them as root-owned. Only expands `~` in simple `~/…` paths.
fn in_docker() -> bool {
    std::path::Path::new("/.dockerenv").exists()
}

fn ensure_volume_dirs(compose: &Value) {
    // In Docker the compose runs against the host daemon via socket — the daemon
    // creates bind-mount dirs on the host automatically. Creating them here would
    // land inside the container filesystem, not on the host.
    if in_docker() {
        return;
    }
    let Some(services) = compose.get("services").and_then(|s| s.as_object()) else {
        return;
    };
    let home = std::env::var("HOME").unwrap_or_default();
    for svc in services.values() {
        let Some(volumes) = svc.get("volumes").and_then(|v| v.as_array()) else {
            continue;
        };
        for vol in volumes {
            let Some(s) = vol.as_str() else { continue };
            // Only handle bind-mounts (host:container[:opts])
            let host_part = s.split(':').next().unwrap_or("");
            if host_part.is_empty() || !host_part.starts_with('~') && !host_part.starts_with('/') {
                continue;
            }
            let expanded = if host_part.starts_with("~/") {
                format!("{}{}", home, &host_part[1..])
            } else if host_part == "~" {
                home.clone()
            } else {
                host_part.to_string()
            };
            let _ = std::fs::create_dir_all(&expanded);
        }
    }
}

/// For compose services that have LLM_API_BASE in their environment and no
/// manual override, inject the auto-detected endpoint.
async fn auto_inject_llm(
    compose: &mut serde_json::Value,
    overrides: &HashMap<String, String>,
) -> Option<String> {
    // Only inject if caller hasn't already set it
    if overrides.contains_key("LLM_API_BASE") {
        return None;
    }

    // Check that at least one service in this compose references LLM_API_BASE
    let has_llm_var = compose
        .get("services")
        .and_then(|s| s.as_object())
        .map(|svcs| {
            svcs.values().any(|svc| {
                svc.get("environment")
                    .and_then(|e| e.as_array())
                    .map(|arr| {
                        arr.iter().any(|v| {
                            v.as_str()
                                .map(|s| s.starts_with("LLM_API_BASE"))
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if !has_llm_var {
        return None;
    }

    let detected = detect_llm_endpoint().await?;
    let label = detected.label.clone();
    let url = detected.url.clone();

    // Inject into all services that have LLM_API_BASE
    if let Some(svcs) = compose.get_mut("services").and_then(|s| s.as_object_mut()) {
        for svc in svcs.values_mut() {
            if let Some(arr) = svc.get_mut("environment").and_then(|e| e.as_array_mut()) {
                let has_var = arr.iter().any(|v| {
                    v.as_str()
                        .map(|s| s.starts_with("LLM_API_BASE"))
                        .unwrap_or(false)
                });
                if has_var {
                    arr.retain(|v| {
                        !v.as_str()
                            .map(|s| s.starts_with("LLM_API_BASE="))
                            .unwrap_or(false)
                    });
                    arr.push(serde_json::Value::String(format!("LLM_API_BASE={url}")));
                }
            }
        }
    }

    Some(label)
}

pub async fn detect_env(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;
    let llm = detect_llm_endpoint().await;
    let gpu = detect_gpu().await;
    Ok(Json(serde_json::json!({
        "gpu": gpu,
        "llm": llm.map(|d| serde_json::json!({ "label": d.label, "port": d.port, "url": d.url })),
    })))
}

pub async fn catalog(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<CatalogResponse>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;
    let mut apps = load_catalog(&state.config.catalog_dir);

    // Members only ever see catalog apps an admin explicitly granted them —
    // every other role sees the full catalog, unchanged.
    if user.role == "member" {
        let allowed: std::collections::HashSet<String> =
            sqlx::query_scalar("SELECT app_id FROM member_app_access WHERE user_id = ?")
                .bind(&user.id)
                .fetch_all(&state.db)
                .await
                .map_err(AppError::Database)?
                .into_iter()
                .collect();
        apps.retain(|a| allowed.contains(&a.id));
    }

    Ok(Json(CatalogResponse { apps }))
}

pub async fn deployed(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<DeployedResponse>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;

    // Members only ever see their own deployed apps (owner_user_id = self);
    // every other role keeps seeing everything, exactly as before.
    let rows = if user.role == "member" {
        sqlx::query_as::<_, DeployedAppRow>(&format!(
            "{SELECT_DEPLOYED} WHERE owner_user_id = ? ORDER BY deployed_at DESC"
        ))
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?
    } else {
        sqlx::query_as::<_, DeployedAppRow>(&format!("{SELECT_DEPLOYED} ORDER BY deployed_at DESC"))
            .fetch_all(&state.db)
            .await
            .map_err(AppError::Database)?
    };
    let docker_available = containers::is_docker_available();
    let apps = rows.into_iter().map(row_to_app).collect();

    Ok(Json(DeployedResponse {
        apps,
        docker_available,
    }))
}

fn row_to_app(r: DeployedAppRow) -> DeployedApp {
    DeployedApp {
        id: r.id,
        app_id: r.app_id,
        app_name: r.app_name,
        project_name: r.project_name,
        status: r.status,
        deployed_at: r.deployed_at,
        primary_port: r.primary_port,
        origin: r.origin,
        target_node_id: r.target_node_id,
    }
}

const SELECT_DEPLOYED: &str =
    "SELECT id, app_id, app_name, project_name, status, deployed_at, compose_path, \
     COALESCE(primary_port, NULL) AS primary_port, \
     COALESCE(origin, 'voidtower') AS origin, \
     owner_user_id, storage_root, target_node_id FROM deployed_apps";

fn require_app_read_role(user: &auth::User) -> Result<()> {
    if !matches!(
        user.role.as_str(),
        "owner" | "admin" | "operator" | "viewer" | "guest" | "demo" | "member"
    ) {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn require_app_deploy_role(user: &auth::User) -> Result<()> {
    if !matches!(
        user.role.as_str(),
        "owner" | "admin" | "operator" | "member"
    ) {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn require_app_operator(user: &auth::User) -> Result<()> {
    if !matches!(user.role.as_str(), "owner" | "admin" | "operator") {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn require_app_owner_or_admin(user: &auth::User, row: &DeployedAppRow) -> Result<()> {
    require_app_read_role(user)?;
    if user.role == "member" && row.owner_user_id.as_deref() != Some(user.id.as_str()) {
        return Err(AppError::NotFound);
    }
    Ok(())
}

fn parse_json_body<T: DeserializeOwned>(body: Bytes) -> Result<T> {
    serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("invalid JSON request body".into()))
}

/// Runs `hook.command` inside the target container, retrying every 3s until it
/// succeeds or `max_wait_secs` elapses. Spawned in the background so it doesn't
/// hold up the deploy response — the container needs time to finish its own
/// startup/migrations before the command can succeed.
fn spawn_post_deploy_hook(
    project_name: String,
    hook: PostDeployHook,
    dotenv_map: HashMap<String, String>,
) {
    let container = format!("{}-{}-1", project_name, hook.container_suffix);
    let command: Vec<String> = hook
        .command
        .iter()
        .map(|part| {
            let mut s = part.clone();
            for (k, v) in &dotenv_map {
                s = s.replace(&format!("${{{k}}}"), v);
            }
            s
        })
        .collect();

    tokio::spawn(async move {
        let deadline =
            tokio::time::Instant::now() + std::time::Duration::from_secs(hook.max_wait_secs);
        loop {
            let result = tokio::process::Command::new("docker")
                .arg("exec")
                .arg(&container)
                .args(&command)
                .process_group(0)
                .output()
                .await;
            match result {
                Ok(out) if out.status.success() => {
                    tracing::info!("post_deploy hook succeeded for {container}");
                    return;
                }
                _ if tokio::time::Instant::now() >= deadline => {
                    tracing::warn!(
                        "post_deploy hook for {container} did not succeed within {}s",
                        hook.max_wait_secs
                    );
                    return;
                }
                _ => tokio::time::sleep(std::time::Duration::from_secs(3)).await,
            }
        }
    });
}

#[derive(sqlx::FromRow)]
struct DeployedAppRow {
    id: String,
    app_id: String,
    app_name: String,
    project_name: String,
    status: String,
    deployed_at: i64,
    compose_path: String,
    primary_port: Option<i64>,
    origin: String,
    owner_user_id: Option<String>,
    storage_root: Option<String>,
    target_node_id: Option<String>,
}

pub async fn deploy(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_deploy_role(&user)?;
    let req: DeployRequest = parse_json_body(body)?;
    if user.role == "member" {
        let allowed = sqlx::query_scalar::<_, String>(
            "SELECT app_id FROM member_app_access WHERE user_id = ? AND app_id = ?",
        )
        .bind(&user.id)
        .bind(&req.app_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .is_some();
        if !allowed {
            return Err(AppError::Forbidden);
        }
    }
    Err(AppError::FeatureUnavailable(
        "App deployment requires a canonical operation adapter".into(),
    ))
}

/// Gracefully cancel an in-flight deploy started via `deploy()` (SIGTERM, escalating to
/// SIGKILL after a grace period). Safe to call even if the deploy has already finished —
/// it's a no-op when the project isn't found in the registry.
pub async fn cancel_deploy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_operator(&user)?;
    let ip = addr.ip().to_string();

    let cancelled = containers::cancel_deploy(&state.deploy_registry, &project_name).await;

    audit::log(
        &state.db,
        Some(&user.id),
        &user.username,
        "app.deploy.cancel",
        Some("app"),
        Some(&project_name),
        if cancelled { "success" } else { "not_found" },
        Some(&ip),
        None,
    )
    .await;

    Ok(Json(
        serde_json::json!({ "ok": true, "cancelled": cancelled }),
    ))
}

#[derive(Deserialize)]
pub struct CustomDeployRequest {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    /// Member-only overrides — see `DeployRequest` for the same fields.
    #[serde(default)]
    pub storage_drive_id: Option<String>,
    #[serde(default)]
    pub target_node_id: Option<String>,
}

pub async fn deploy_custom(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_deploy_role(&user)?;
    let req: CustomDeployRequest = parse_json_body(body)?;
    if user.role == "member" {
        let allowed = sqlx::query_scalar::<_, bool>(
            "SELECT can_deploy_custom FROM member_settings WHERE user_id = ?",
        )
        .bind(&user.id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .unwrap_or(false);
        if !allowed {
            return Err(AppError::Forbidden);
        }
        super::members::resolve_member_storage_root(
            &state,
            &user.id,
            req.storage_drive_id.as_deref(),
        )
        .await?;
        super::members::resolve_member_target_node(&state, &user.id, req.target_node_id.as_deref())
            .await?;
    }
    let _ = (
        &req.name,
        &req.image,
        &req.ports,
        &req.volumes,
        &req.env,
        &req.storage_drive_id,
        &req.target_node_id,
    );
    Err(AppError::FeatureUnavailable(
        "Custom app deployment requires a canonical operation adapter".into(),
    ))
}

pub async fn start_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_deploy_role(&user)?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App start requires a canonical operation adapter".into(),
    ))
}

pub async fn redeploy_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App redeploy requires a canonical operation adapter".into(),
    ))
}

pub async fn restart_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App restart requires a canonical operation adapter".into(),
    ))
}

pub async fn remove_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_admin(&user)?;
    Err(AppError::FeatureUnavailable(
        "App removal requires a canonical operation adapter".into(),
    ))
}

pub async fn app_logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;

    let row =
        sqlx::query_as::<_, DeployedAppRow>(&format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
            .bind(&project_name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?
            .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    let logs = containers::logs_compose(&project_name, &compose_path, 300)
        .await
        .map_err(|_| AppError::FeatureUnavailable("App logs are unavailable".into()))?;

    let bounded = bound_log_output(&logs);
    let lines: Vec<&str> = bounded.lines().take(300).collect();
    Ok(Json(serde_json::json!({ "lines": lines })))
}

const MAX_LOG_BYTES: usize = 64 * 1024;

fn bound_log_output(logs: &str) -> String {
    bound_utf8_bytes(logs, MAX_LOG_BYTES)
}

fn bound_utf8_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

pub async fn app_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;

    let row =
        sqlx::query_as::<_, DeployedAppRow>(&format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
            .bind(&project_name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?
            .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    let containers = containers::status_compose(&project_name, &compose_path)
        .await
        .map_err(|_| AppError::FeatureUnavailable("App status is unavailable".into()))?;

    Ok(Json(serde_json::json!({ "containers": containers })))
}

pub async fn stop_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App stop requires a canonical operation adapter".into(),
    ))
}

pub async fn get_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;
    let row =
        sqlx::query_as::<_, DeployedAppRow>(&format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
            .bind(&project_name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?
            .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let content = read_bounded_compose(StdPath::new(&row.compose_path))?;
    Ok(Json(serde_json::json!({ "content": content })))
}

const MAX_COMPOSE_BYTES: usize = 256 * 1024;
const SENSITIVE_COMPOSE_KEYS: &[&str] = &[
    "api_key",
    "api-key",
    "apikey",
    "access_key",
    "access-key",
    "aws_access_key_id",
    "aws_secret_access_key",
    "private_key",
    "private-key",
    "secret_key",
    "secret-key",
    "encryption_key",
    "encryption-key",
    "signing_key",
    "signing-key",
    "password",
    "passwd",
    "secret",
    "token",
    "credential",
    "database_url",
    "database-url",
    "connection_string",
    "connection-string",
    "client_secret",
    "client-secret",
    "authorization",
    "bearer",
];

fn is_sensitive_compose_key(key: &str) -> bool {
    let key = key
        .trim()
        .trim_start_matches('-')
        .trim()
        .to_ascii_lowercase();
    SENSITIVE_COMPOSE_KEYS
        .iter()
        .any(|candidate| key.contains(candidate))
}

fn redact_inline_sensitive_assignments(value: &str) -> String {
    let mut output = value.to_string();
    let mut search_from = 0;
    while let Some(relative) = output[search_from..].find('=') {
        let delimiter = search_from + relative;
        let key_start = output[..delimiter]
            .char_indices()
            .rev()
            .find(|(_, character)| {
                character.is_whitespace() || matches!(character, '"' | '\'' | '[' | ',')
            })
            .map(|(offset, character)| offset + character.len_utf8())
            .unwrap_or(0);
        let key = &output[key_start..delimiter];
        if !is_sensitive_compose_key(key) {
            search_from = delimiter + 1;
            continue;
        }
        let mut value_start = delimiter + 1;
        let quoted = output
            .as_bytes()
            .get(value_start)
            .is_some_and(|byte| *byte == b'"' || *byte == b'\'');
        if quoted {
            let quote = output.as_bytes()[value_start];
            value_start += 1;
            let value_end = output[value_start..]
                .find(quote as char)
                .map(|offset| value_start + offset)
                .unwrap_or(output.len());
            output.replace_range(value_start..value_end, "[redacted]");
            search_from = value_start + "[redacted]".len();
        } else {
            let value_end = output[value_start..]
                .find(|character: char| character.is_whitespace() || matches!(character, ',' | ']'))
                .map(|offset| value_start + offset)
                .unwrap_or(output.len());
            output.replace_range(value_start..value_end, "[redacted]");
            search_from = value_start + "[redacted]".len();
        }
    }
    output
}

fn redact_command_scalar(value: &str) -> String {
    const FLAGS: &[&str] = &[
        "--header",
        "--password",
        "--token",
        "--api-key",
        "--secret",
        "--private-key",
        "--env",
        "--environment",
        "-e",
        "-H",
    ];
    let mut output = value.to_string();
    for flag in FLAGS {
        let mut search_from = 0;
        loop {
            let lower = output.to_ascii_lowercase();
            let flag_lower = flag.to_ascii_lowercase();
            let Some(relative) = lower[search_from..].find(&flag_lower) else {
                break;
            };
            let flag_start = search_from + relative;
            let mut value_start = flag_start + flag.len();
            while output
                .as_bytes()
                .get(value_start)
                .is_some_and(|byte| *byte == b' ' || *byte == b'\t' || *byte == b'=')
            {
                value_start += 1;
            }
            if value_start >= output.len() {
                break;
            }
            let is_header_flag =
                flag.eq_ignore_ascii_case("--header") || flag.eq_ignore_ascii_case("-H");
            let (value_end, replacement) = if output.as_bytes()[value_start] == b'"'
                || output.as_bytes()[value_start] == b'\''
            {
                let quote = output.as_bytes()[value_start];
                let end = output[value_start + 1..]
                    .find(quote as char)
                    .map(|offset| value_start + 2 + offset)
                    .unwrap_or(output.len());
                (end, format!("{}[redacted]{}", quote as char, quote as char))
            } else {
                let end = if is_header_flag {
                    output.len()
                } else {
                    output[value_start..]
                        .find(char::is_whitespace)
                        .map(|offset| value_start + offset)
                        .unwrap_or(output.len())
                };
                (end, "[redacted]".to_string())
            };
            output.replace_range(value_start..value_end, &replacement);
            search_from = value_start + replacement.len();
        }
    }
    for flag in ["-e=", "--env=", "--environment="] {
        let lower = output.to_ascii_lowercase();
        if let Some(start) = lower.find(flag) {
            let value_start = start + flag.len();
            if value_start < output.len() {
                output.replace_range(value_start.., "[redacted]");
            }
        }
    }
    output = redact_inline_sensitive_assignments(&output);
    let trailing_newline = output.ends_with('\n');
    let mut redacted = output
        .lines()
        .map(redact_compose_line)
        .collect::<Vec<_>>()
        .join("\n");
    if trailing_newline {
        redacted.push('\n');
    }
    redacted
}

fn is_sensitive_command_flag(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    [
        "--password",
        "--token",
        "--api-key",
        "--secret",
        "--private-key",
        "--env",
        "--environment",
        "-e",
        "-h",
        "--header",
    ]
    .iter()
    .any(|flag| value == *flag || value.starts_with(&format!("{flag}=")))
}

fn redact_command_value(value: &mut serde_yaml::Value) {
    match value {
        serde_yaml::Value::Sequence(sequence) => {
            let mut redact_next = false;
            for child in sequence {
                if let Some(entry) = child.as_str() {
                    if redact_next {
                        *child = serde_yaml::Value::String("[redacted]".into());
                        redact_next = false;
                    } else {
                        redact_next = is_sensitive_command_flag(entry);
                        *child = serde_yaml::Value::String(redact_command_scalar(entry));
                    }
                } else {
                    redact_next = false;
                }
            }
        }
        serde_yaml::Value::String(text) => *text = redact_command_scalar(text),
        _ => {}
    }
}

fn redact_compose_line(line: &str) -> String {
    let lower_line = line.to_ascii_lowercase();
    let Some((delimiter_offset, delimiter)) = [':', '=']
        .iter()
        .filter_map(|delimiter| line.find(*delimiter).map(|offset| (offset, *delimiter)))
        .min_by_key(|(offset, _)| *offset)
    else {
        return line.to_string();
    };

    let sensitive = is_sensitive_compose_key(&lower_line[..delimiter_offset]);
    if !sensitive {
        return line.to_string();
    }

    format!(
        "{}{}{}",
        &line[..delimiter_offset],
        delimiter,
        if delimiter == ':' {
            " [redacted]"
        } else {
            "[redacted]"
        },
    )
}

fn redact_flow_compose_content(content: &str) -> Option<String> {
    if !content
        .lines()
        .any(|line| line.contains('[') || line.contains('{'))
    {
        return None;
    }

    let mut value = serde_yaml::from_str::<serde_yaml::Value>(content).ok()?;
    fn redact(value: &mut serde_yaml::Value) {
        match value {
            serde_yaml::Value::Mapping(mapping) => {
                for (key, child) in mapping.iter_mut() {
                    if key.as_str().is_some_and(is_sensitive_compose_key) {
                        *child = serde_yaml::Value::String("[redacted]".into());
                    } else if key.as_str().is_some_and(|key| {
                        matches!(key.to_ascii_lowercase().as_str(), "command" | "entrypoint")
                    }) {
                        redact_command_value(child);
                    } else {
                        redact(child);
                    }
                }
            }
            serde_yaml::Value::Sequence(sequence) => {
                for child in sequence {
                    if let Some(entry) = child.as_str() {
                        *child = serde_yaml::Value::String(redact_compose_line(entry));
                    } else {
                        redact(child);
                    }
                }
            }
            serde_yaml::Value::String(text) => {
                *text = redact_command_scalar(&redact_compose_line(text));
            }
            _ => {}
        }
    }

    redact(&mut value);
    serde_yaml::to_string(&value).ok()
}

fn redact_compose_content(content: &str) -> String {
    if let Some(flow_redacted) = redact_flow_compose_content(content) {
        return flow_redacted;
    }

    let mut block_indent: Option<usize> = None;
    let mut redacted = Vec::new();
    for line in content.lines() {
        let indent = line.len() - line.trim_start().len();
        if let Some(parent_indent) = block_indent {
            if !line.trim().is_empty() && indent <= parent_indent {
                block_indent = None;
            } else if !line.trim().is_empty() {
                redacted.push(format!("{}[redacted]", &line[..indent]));
                continue;
            }
        }

        let safe_line = redact_command_scalar(&redact_compose_line(line));
        let value = line
            .split_once(':')
            .map(|(_, value)| value.trim())
            .unwrap_or_default();
        let indicator = value.split('#').next().unwrap_or_default().trim();
        let is_block_value = indicator.starts_with('|') || indicator.starts_with('>');
        if safe_line != line && is_block_value {
            block_indent = Some(indent);
        }
        redacted.push(safe_line);
    }

    redacted.join("\n") + if content.ends_with('\n') { "\n" } else { "" }
}

fn read_bounded_compose(path: &StdPath) -> Result<String> {
    let file = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
                .open(path)
        }
        #[cfg(not(unix))]
        {
            std::fs::File::open(path)
        }
    }
    .map_err(|error| AppError::Internal(error.into()))?;
    if !file
        .metadata()
        .map_err(|error| AppError::Internal(error.into()))?
        .is_file()
    {
        return Err(AppError::BadRequest(
            "compose path is not a regular file".into(),
        ));
    }
    if file
        .metadata()
        .map_err(|error| AppError::Internal(error.into()))?
        .len()
        > MAX_COMPOSE_BYTES as u64
    {
        return Err(AppError::PayloadTooLarge);
    }

    let mut bytes = Vec::with_capacity(MAX_COMPOSE_BYTES.min(8192));
    file.take((MAX_COMPOSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| AppError::Internal(error.into()))?;
    if bytes.len() > MAX_COMPOSE_BYTES {
        return Err(AppError::PayloadTooLarge);
    }

    let content = String::from_utf8(bytes)
        .map_err(|_| AppError::BadRequest("compose file is not valid UTF-8".into()))?;
    Ok(bound_utf8_bytes(
        &redact_compose_content(&content),
        MAX_COMPOSE_BYTES,
    ))
}

#[derive(Deserialize)]
pub struct UpdateComposeRequest {
    pub content: String,
}

// ─── open-ui: resolve an existing embed proxy without mutating state ─────────

#[derive(Deserialize)]
pub struct OpenUiRequest {
    pub project_name: String,
    pub primary_port: u16,
}

fn validate_ui_host(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty()
        || raw.chars().any(|c| {
            c.is_control() || c.is_whitespace() || matches!(c, '/' | '?' | '#' | '@' | '\\')
        })
    {
        return None;
    }

    if raw.starts_with('[') {
        let end = raw.find(']')?;
        if raw[1..end].parse::<std::net::Ipv6Addr>().is_err() {
            return None;
        }
        let suffix = &raw[end + 1..];
        if !suffix.is_empty()
            && (!suffix.starts_with(':')
                || suffix[1..]
                    .parse::<u16>()
                    .ok()
                    .filter(|port| *port > 0)
                    .is_none())
        {
            return None;
        }
        return Some(raw[..=end].to_string());
    }
    if raw.matches(':').count() > 1 {
        return None;
    }

    let host = if let Some((host, port)) = raw.rsplit_once(':') {
        port.parse::<u16>().ok().filter(|value| *value > 0)?;
        host
    } else {
        raw
    };
    (!host.is_empty()).then(|| host.to_string())
}

pub async fn open_ui(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_app_read_role(&user)?;
    let req: OpenUiRequest = parse_json_body(body)?;
    if req.primary_port == 0
        || req.project_name.is_empty()
        || req.project_name.trim() != req.project_name
    {
        return Err(AppError::BadRequest(
            "invalid app identifier or port".into(),
        ));
    }
    let row =
        sqlx::query_as::<_, DeployedAppRow>(&format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
            .bind(&req.project_name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?
            .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;
    if row.primary_port != Some(i64::from(req.primary_port)) {
        return Err(AppError::BadRequest(
            "port does not match deployed app".into(),
        ));
    }

    // Use the Host header so the returned URL works from any machine on the LAN,
    // not just localhost. Strip the port portion if present.
    let host = match headers.get("host") {
        Some(value) => validate_ui_host(
            value
                .to_str()
                .map_err(|_| AppError::BadRequest("invalid host header".into()))?,
        )
        .ok_or_else(|| AppError::BadRequest("invalid host header".into()))?,
        None => "localhost".to_string(),
    };

    let direct_url = format!("http://{}:{}", host, req.primary_port);

    // Use a slug-based domain key for DB lookup (keep existing record format).
    let domain = format!("{}.embed", req.project_name);
    let upstream = format!("http://localhost:{}", req.primary_port);

    // Opening an app is read-only. Reuse an existing enabled embed rule only when
    // it still targets this app's current port and nginx is actually available.
    let existing: Option<(bool, bool, Option<i64>, String)> = sqlx::query_as(
        "SELECT enabled, allow_embed, embed_port, upstream FROM proxy_configs WHERE domain = ?",
    )
    .bind(&domain)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let nginx_ok = tokio::task::spawn_blocking(crate::api::proxy::nginx_active_pub)
        .await
        .unwrap_or(false);
    let embed_url = existing
        .filter(|(enabled, allow_embed, _, configured_upstream)| {
            nginx_ok && *enabled && *allow_embed && configured_upstream == &upstream
        })
        .and_then(|(_, _, port, _)| port)
        .and_then(|port| u16::try_from(port).ok())
        .map(|port| format!("http://{}:{}", host, port));
    let proxy_available = embed_url.is_some();

    Ok(Json(serde_json::json!({
        "url": direct_url,
        "embed_url": embed_url,
        "proxy_created": false,
        "proxy_available": proxy_available,
    })))
}

pub async fn update_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    let _req: UpdateComposeRequest = parse_json_body(body)?;
    Err(AppError::FeatureUnavailable(
        "App compose update requires a canonical operation adapter".into(),
    ))
}

// ── Embed proxy — strips X-Frame-Options so App Vault iframes load ────────────

fn build_embed_target_url(port: u16, path: &str, query: Option<&str>) -> String {
    let query_suffix = query
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.starts_with('?') {
                value.to_string()
            } else {
                format!("?{value}")
            }
        })
        .unwrap_or_default();
    format!("http://localhost:{port}/{path}{query_suffix}")
}

fn embed_error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (
        status,
        Json(serde_json::json!({
            "error": { "code": code, "message": message }
        })),
    )
        .into_response()
}

const MAX_EMBED_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_EMBED_PATH_BYTES: usize = 4096;
const MAX_EMBED_QUERY_BYTES: usize = 8192;
const MAX_EMBED_HEADER_COUNT: usize = 64;
const MAX_EMBED_HEADER_BYTES: usize = 32 * 1024;
static EMBED_CONCURRENCY: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

pub async fn embed_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((project_name, path)): Path<(String, String)>,
    uri: Uri,
) -> Response {
    let session_id = match jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_string())
    {
        Some(session_id) => session_id,
        None => return embed_error(StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized"),
    };
    let user = match auth::validate_session(&state.db, &session_id).await {
        Ok(Some(user)) => user,
        _ => return embed_error(StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized"),
    };
    if require_app_read_role(&user).is_err() {
        return embed_error(StatusCode::FORBIDDEN, "forbidden", "Forbidden");
    }

    let row = match sqlx::query_as::<_, DeployedAppRow>(&format!(
        "{SELECT_DEPLOYED} WHERE project_name = ?"
    ))
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => return embed_error(StatusCode::NOT_FOUND, "not_found", "Not found"),
        Err(_) => {
            return embed_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "A database error occurred",
            )
        }
    };

    if require_app_owner_or_admin(&user, &row).is_err() {
        return embed_error(StatusCode::NOT_FOUND, "not_found", "Not found");
    }

    let port = match row.primary_port {
        Some(port) if port > 0 && port <= i64::from(u16::MAX) => port as u16,
        _ => {
            return embed_error(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "Embedded app is unavailable",
            )
        }
    };

    let _permit = match EMBED_CONCURRENCY
        .get_or_init(|| tokio::sync::Semaphore::new(16))
        .try_acquire()
    {
        Ok(permit) => permit,
        Err(_) => {
            return embed_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "upstream_busy",
                "Embedded app is busy",
            )
        }
    };
    if path.contains('?') || path.contains('#') || path.contains('\0') {
        return embed_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "Invalid embedded path",
        );
    }
    if path.len() > MAX_EMBED_PATH_BYTES
        || uri
            .query()
            .is_some_and(|query| query.len() > MAX_EMBED_QUERY_BYTES)
    {
        return embed_error(
            StatusCode::BAD_REQUEST,
            "invalid_path",
            "Embedded path or query is too large",
        );
    }
    let upstream_url = build_embed_target_url(port, &path, uri.query());
    let client = match reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return embed_error(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "Embedded app is unavailable",
            )
        }
    };
    let upstream_resp = match client
        .get(&upstream_url)
        .header("X-Forwarded-For", "127.0.0.1")
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return embed_error(
                StatusCode::BAD_GATEWAY,
                "upstream_unavailable",
                "Embedded app is unavailable",
            )
        }
    };

    if !upstream_resp.status().is_success() {
        return embed_error(
            StatusCode::BAD_GATEWAY,
            "upstream_error",
            "Embedded app returned an unsuccessful response",
        );
    }
    if upstream_resp
        .content_length()
        .is_some_and(|length| length > MAX_EMBED_RESPONSE_BYTES as u64)
    {
        return embed_error(
            StatusCode::BAD_GATEWAY,
            "upstream_response_too_large",
            "Embedded app response exceeds the allowed size",
        );
    }

    let upstream_status =
        StatusCode::from_u16(upstream_resp.status().as_u16()).unwrap_or(StatusCode::OK);
    let upstream_headers = upstream_resp.headers().clone();
    use futures_util::StreamExt;
    let mut stream = upstream_resp.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(_) => {
                return embed_error(
                    StatusCode::BAD_GATEWAY,
                    "upstream_error",
                    "Embedded app is unavailable",
                )
            }
        };
        if body.len().saturating_add(chunk.len()) > MAX_EMBED_RESPONSE_BYTES {
            return embed_error(
                StatusCode::BAD_GATEWAY,
                "upstream_response_too_large",
                "Embedded app response exceeds the allowed size",
            );
        }
        body.extend_from_slice(&chunk);
    }

    let mut connection_tokens = std::collections::HashSet::new();
    for value in upstream_headers.get_all("connection").iter() {
        if let Ok(value) = value.to_str() {
            connection_tokens.extend(
                value
                    .split(',')
                    .map(|token| token.trim().to_ascii_lowercase())
                    .filter(|token| !token.is_empty()),
            );
        }
    }

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'self'"),
    );
    let mut response_header_bytes =
        axum::http::header::CONTENT_SECURITY_POLICY.as_str().len() + "frame-ancestors 'self'".len();
    for (name, value) in &upstream_headers {
        let name_lower = name.as_str().to_ascii_lowercase();
        if matches!(
            name_lower.as_str(),
            "x-frame-options"
                | "content-security-policy"
                | "location"
                | "set-cookie"
                | "transfer-encoding"
                | "connection"
                | "keep-alive"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "proxy-connection"
                | "te"
                | "trailer"
                | "upgrade"
                | "content-length"
        ) || connection_tokens.contains(&name_lower)
        {
            continue;
        }
        if response_headers.len() >= MAX_EMBED_HEADER_COUNT
            || response_header_bytes
                .saturating_add(name.as_str().len())
                .saturating_add(value.len())
                > MAX_EMBED_HEADER_BYTES
        {
            return embed_error(
                StatusCode::BAD_GATEWAY,
                "upstream_headers_too_large",
                "Embedded app response headers exceed the allowed size",
            );
        }
        response_header_bytes = response_header_bytes
            .saturating_add(name.as_str().len())
            .saturating_add(value.len());
        response_headers.insert(name.clone(), value.clone());
    }
    (upstream_status, response_headers, Body::from(body)).into_response()
}

// ── External app detection ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ExternalContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub ports: Vec<String>,
}

#[derive(Serialize)]
pub struct ExternalStack {
    pub project_name: String,
    pub compose_available: bool,
    pub containers: Vec<ExternalContainer>,
    pub primary_port: Option<u16>,
}

/// Parse Docker's label string "k=v,k=v,…" into a HashMap.
fn parse_docker_labels(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in s.split(',') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// Extract host ports from a Docker ports string like "0.0.0.0:8080->80/tcp".
fn parse_docker_ports(s: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some(arrow) = part.find("->") {
            let host = part[..arrow].rsplit(':').next().unwrap_or("").trim();
            let cont = &part[arrow + 2..];
            if !host.is_empty() {
                let entry = format!("{host}:{cont}");
                if seen.insert(entry.clone()) {
                    out.push(entry);
                }
            }
        }
    }
    out
}

fn parse_external_container_output(output: &[u8]) -> anyhow::Result<Vec<Value>> {
    let text = String::from_utf8(output.to_vec())?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let value = serde_json::from_str::<Value>(line.trim())?;
            let object = value
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("docker container record is not an object"))?;
            for key in ["ID", "Names", "Image", "State", "Ports", "Labels"] {
                if !object.get(key).is_some_and(Value::is_string) {
                    anyhow::bail!("docker container record is missing a required field");
                }
            }
            Ok(value)
        })
        .collect()
}

pub async fn detect_external(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ExternalStack>>> {
    let user = require_user(&state, &jar).await?;
    require_app_operator(&user)?;

    if !containers::is_docker_available() {
        return Ok(Json(vec![]));
    }

    // Fetch all containers including stopped ones through the bounded Docker seam.
    let output = containers::list_external_containers()
        .await
        .map_err(AppError::Internal)?;

    // Already-managed project names
    let managed: std::collections::HashSet<String> =
        sqlx::query_scalar("SELECT project_name FROM deployed_apps")
            .fetch_all(&state.db)
            .await
            .map_err(AppError::Database)?
            .into_iter()
            .collect();

    // Group containers by compose project
    let mut groups: std::collections::HashMap<String, (bool, Vec<ExternalContainer>)> =
        std::collections::HashMap::new();

    let records = parse_external_container_output(&output).map_err(AppError::Internal)?;
    for obj in records {
        let id = obj["ID"].as_str().unwrap_or("").to_string();
        let name = obj["Names"].as_str().unwrap_or("").to_string();
        let image = obj["Image"].as_str().unwrap_or("").to_string();
        let state_str = obj["State"].as_str().unwrap_or("").to_string();
        let ports_str = obj["Ports"].as_str().unwrap_or("");
        let labels_str = obj["Labels"].as_str().unwrap_or("");

        let labels = parse_docker_labels(labels_str);
        let project = labels
            .get("com.docker.compose.project")
            .cloned()
            .unwrap_or_else(|| format!("standalone-{}", name.trim_start_matches('/')));

        // Skip anything already managed by VoidTower
        if project.starts_with("vt-") || managed.contains(&project) {
            continue;
        }

        let compose_available = labels
            .get("com.docker.compose.project.config_files")
            .or_else(|| labels.get("com.docker.compose.project.working_dir"))
            .is_some_and(|value| !value.trim().is_empty());
        let entry = groups.entry(project).or_insert((false, Vec::new()));
        entry.0 |= compose_available;
        entry.1.push(ExternalContainer {
            id,
            name,
            image,
            state: state_str,
            ports: parse_docker_ports(ports_str),
        });
    }

    let mut stacks: Vec<ExternalStack> = groups
        .into_iter()
        .map(|(project_name, (compose_available, containers))| {
            let primary_port = containers
                .iter()
                .flat_map(|c| c.ports.iter())
                .filter_map(|p| p.split(':').next()?.parse::<u16>().ok())
                .min();
            ExternalStack {
                project_name,
                compose_available,
                containers,
                primary_port,
            }
        })
        .collect();
    stacks.sort_by(|a, b| a.project_name.cmp(&b.project_name));

    Ok(Json(stacks))
}

// ── Adopt external app ────────────────────────────────────────────────────────

#[allow(dead_code)] // retained as the request contract for the future canonical adapter
#[derive(Deserialize)]
pub struct AdoptRequest {
    pub project_name: String,
    pub app_name: String,
    pub primary_port: Option<i64>,
}

pub async fn adopt_app(
    State(state): State<AppState>,
    jar: CookieJar,
    body: Bytes,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    let _req: AdoptRequest = parse_json_body(body)?;
    Err(AppError::FeatureUnavailable(
        "App adoption requires a canonical operation adapter".into(),
    ))
}

// ── Convert adopted app to VoidTower management ───────────────────────────────

pub async fn convert_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App conversion requires a canonical operation adapter".into(),
    ))
}

// ── Toolpack-backed handlers ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ExposeAppRequest {
    pub domain: String,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default = "default_true")]
    pub allow_embed: bool,
}
fn default_true() -> bool {
    true
}

pub async fn pull_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App pull requires a canonical operation adapter".into(),
    ))
}

pub async fn patch_app_env(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    let _req: serde_json::Value = parse_json_body(body)?;
    Err(AppError::FeatureUnavailable(
        "App environment patch requires a canonical operation adapter".into(),
    ))
}

pub async fn expose_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> super::operation_adoption::CompatibilityResult<Response> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_admin(&user)?;
    let req: ExposeAppRequest =
        parse_json_body(body).map_err(super::operation_adoption::CompatibilityError::from)?;
    let row =
        sqlx::query_as::<_, DeployedAppRow>(&format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
            .bind(&project_name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?
            .ok_or(AppError::NotFound)?;
    let port = row
        .primary_port
        .and_then(|port| u16::try_from(port).ok())
        .filter(|port| *port > 0)
        .ok_or_else(|| AppError::BadRequest("No valid port configured for this app".into()))?;
    let upstream = format!("http://localhost:{port}");
    let credential = super::actions::credential(&state, &jar, None).await?;
    crate::api::proxy::create_with_credential(
        &state,
        &credential,
        &headers,
        crate::api::proxy::CreateRequest {
            domain: req.domain,
            upstream,
            ssl: req.ssl,
            allow_embed: req.allow_embed,
            sso_protect: false,
            dry_run: false,
            custom_headers: Vec::new(),
            rate_limit_rpm: None,
            basic_auth_user: None,
            basic_auth_password: None,
            basic_auth_secret_id: None,
            websocket_extended: false,
            cache_static: false,
        },
    )
    .await
}

pub async fn delete_app_volumes(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;
    Err(AppError::FeatureUnavailable(
        "App volume deletion requires a canonical operation adapter".into(),
    ))
}

pub async fn purge_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_project_name): Path<String>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_admin(&user)?;
    Err(AppError::FeatureUnavailable(
        "App purge requires a canonical operation adapter".into(),
    ))
}

#[cfg(test)]
mod tests {
    use axum::{
        body::{to_bytes, Body},
        extract::ConnectInfo,
        http::{header, Request, StatusCode},
    };
    use serde_json::json;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn update_compose_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/example/compose")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(json!({ "content": "services: {}" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn update_compose_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/example/compose")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(json!({ "content": "services: {}" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App compose update requires a canonical operation adapter"
        );
    }

    #[test]
    fn update_compose_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn update_compose(")
            .nth(1)
            .and_then(|rest| rest.split("// ── Embed proxy").next())
            .expect("update-compose handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as(",
            "std::fs::",
            "std::fs::write(",
            "containers::",
            "audit::log(",
        ] {
            assert!(
                !handler.contains(marker),
                "update-compose handler marker: {marker}"
            );
        }
    }

    #[tokio::test]
    async fn deploy_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/deploy")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(json!({ "app_id": "example" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn deploy_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/deploy")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(json!({ "app_id": "example" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App deployment requires a canonical operation adapter"
        );
    }

    #[test]
    fn deploy_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn deploy(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn cancel_deploy(").next())
            .expect("deploy handler");

        for marker in ["sqlx::query(", "std::fs::", "containers::", "audit::log("] {
            assert!(!handler.contains(marker), "deploy handler marker: {marker}");
        }
    }

    #[tokio::test]
    async fn deploy_custom_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/deploy-custom")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(
                        json!({ "name": "custom-app", "image": "example/image:latest" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn deploy_custom_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn deploy_custom(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn start_app(").next())
            .expect("deploy-custom handler");

        for marker in ["sqlx::query(", "std::fs::", "containers::", "audit::log("] {
            assert!(
                !handler.contains(marker),
                "deploy-custom handler marker: {marker}"
            );
        }
    }

    #[tokio::test]
    async fn deploy_custom_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/deploy-custom")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(
                        json!({ "name": "custom-app", "image": "example/image:latest" })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "Custom app deployment requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn adopt_app_fails_closed_before_persisting_or_mutating_docker() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/adopt")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(
                        json!({
                            "project_name": "external-stack",
                            "app_name": "External Stack",
                            "compose_path": "/srv/external/compose.yml",
                            "primary_port": 8080
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = response.status();
        let payload = json_body(response).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App adoption requires a canonical operation adapter"
        );

        let stored: Option<String> = sqlx::query_scalar(
            "SELECT project_name FROM deployed_apps WHERE project_name = 'external-stack'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(stored, None);
    }

    #[tokio::test]
    async fn adopt_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/adopt")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(
                        json!({
                            "project_name": "external-stack",
                            "app_name": "External Stack"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn pull_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/pull")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App pull requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn pull_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/pull")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn pull_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn pull_app(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn patch_app_env(").next())
            .expect("pull handler");

        for marker in ["sqlx::query(", "std::fs::", "containers::", "audit::log("] {
            assert!(!handler.contains(marker), "pull handler marker: {marker}");
        }
    }

    #[tokio::test]
    async fn patch_app_env_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/env")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"env": {"EXAMPLE": "bounded"}}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App environment patch requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn patch_app_env_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/env")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({"env": {"EXAMPLE": "bounded"}}).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn patch_app_env_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn patch_app_env(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn expose_app(").next())
            .expect("patch env handler");

        for marker in ["sqlx::query(", "std::fs::", "containers::", "audit::log("] {
            assert!(
                !handler.contains(marker),
                "patch env handler marker: {marker}"
            );
        }
    }

    #[tokio::test]
    async fn delete_app_volumes_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/delete-volumes")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App volume deletion requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn delete_app_volumes_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/delete-volumes")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn delete_app_volumes_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn delete_app_volumes(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn purge_app(").next())
            .expect("delete-volumes handler");

        for marker in ["sqlx::query(", "std::fs::", "containers::", "audit::log("] {
            assert!(
                !handler.contains(marker),
                "delete-volumes handler marker: {marker}"
            );
        }
    }

    #[tokio::test]
    async fn purge_app_fails_closed_after_admin_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/purge")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App purge requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn purge_app_rejects_operator_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session =
            crate::api::mcp::test_support::user_with_role_session(&pool, "operator").await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/purge")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(json_body(response).await["error"]["code"], "forbidden");
    }

    #[tokio::test]
    async fn purge_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/purge")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn purge_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn purge_app(")
            .nth(1)
            .and_then(|rest| rest.split("#[cfg(test)]").next())
            .expect("purge handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "purge handler marker: {marker}");
        }
    }

    #[tokio::test]
    async fn stop_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/stop")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App stop requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn stop_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/stop")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn stop_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn stop_app(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn get_compose(").next())
            .expect("stop handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "stop handler marker: {marker}");
        }
    }

    #[tokio::test]
    async fn start_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/start")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App start requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn start_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/start")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn start_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn start_app(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn redeploy_app(").next())
            .expect("start handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "start handler marker: {marker}");
        }
    }

    #[tokio::test]
    async fn redeploy_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/redeploy")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App redeploy requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn redeploy_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/redeploy")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn redeploy_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn redeploy_app(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn restart_app(").next())
            .expect("redeploy handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(
                !handler.contains(marker),
                "redeploy handler marker: {marker}"
            );
        }
    }

    #[tokio::test]
    async fn remove_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/apps/external-stack")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "App removal requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn remove_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/apps/external-stack")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn remove_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn remove_app(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn app_logs(").next())
            .expect("remove handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "remove handler marker: {marker}");
        }
    }

    #[tokio::test]
    async fn convert_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/convert")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            json_body(response).await["error"]["code"],
            "feature_unavailable"
        );
    }

    #[tokio::test]
    async fn convert_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/convert")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn restart_app_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/restart")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            json_body(response).await["error"]["code"],
            "feature_unavailable"
        );
    }

    #[tokio::test]
    async fn restart_app_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/apps/external-stack/restart")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[test]
    fn restart_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn restart_app(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn remove_app(").next())
            .expect("restart handler");

        for marker in [
            "sqlx::query(",
            "sqlx::query_as",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(
                !handler.contains(marker),
                "restart handler marker: {marker}"
            );
        }
    }

    #[test]
    fn convert_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn convert_app(")
            .nth(1)
            .and_then(|rest| rest.split("// ── Toolpack-backed handlers").next())
            .expect("convert handler");

        for marker in ["sqlx::query(", "std::fs::", "containers::", "audit::log("] {
            assert!(
                !handler.contains(marker),
                "convert handler marker: {marker}"
            );
        }
    }

    #[test]
    fn adopt_app_handler_has_no_direct_mutation_path() {
        let source = include_str!("apps.rs");
        let handler = source
            .split("pub async fn adopt_app(")
            .nth(1)
            .and_then(|rest| rest.split("// ── Convert adopted app").next())
            .expect("adopt handler");

        for marker in [
            "sqlx::query(",
            "ensure_vt_proxy_network(",
            "tokio::process::Command",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "adopt handler marker: {marker}");
        }
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
        routing::any,
        Router,
    };
    use tower::ServiceExt;

    async fn json_body(response: axum::response::Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[test]
    fn member_embed_access_requires_ownership() {
        let member = auth::User {
            id: "member-1".into(),
            username: "member".into(),
            role: "member".into(),
            password_hash: String::new(),
            force_password_change: false,
            totp_enabled: false,
            totp_secret: None,
            created_at: 0,
            updated_at: 0,
            expires_at: None,
        };
        let owned = DeployedAppRow {
            id: "a".into(),
            app_id: "a".into(),
            app_name: "a".into(),
            project_name: "p".into(),
            status: "running".into(),
            deployed_at: 0,
            compose_path: "/tmp/a".into(),
            primary_port: Some(1),
            origin: "voidtower".into(),
            owner_user_id: Some("member-1".into()),
            storage_root: Some("/srv/private".into()),
            target_node_id: None,
        };
        let other = DeployedAppRow {
            id: "a".into(),
            app_id: "a".into(),
            app_name: "a".into(),
            project_name: "p".into(),
            status: "running".into(),
            deployed_at: 0,
            compose_path: "/tmp/a".into(),
            primary_port: Some(1),
            origin: "voidtower".into(),
            owner_user_id: Some("member-2".into()),
            storage_root: None,
            target_node_id: None,
        };
        assert!(require_app_owner_or_admin(&member, &owned).is_ok());
        assert!(matches!(
            require_app_owner_or_admin(&member, &other),
            Err(AppError::NotFound)
        ));
        let public = serde_json::to_value(row_to_app(owned)).unwrap();
        assert!(public.get("compose_path").is_none());
        assert!(public.get("storage_root").is_none());
    }

    #[test]
    fn unknown_app_roles_fail_closed() {
        let unknown = auth::User {
            id: "unknown-1".into(),
            username: "unknown".into(),
            role: "future-role".into(),
            password_hash: String::new(),
            force_password_change: false,
            totp_enabled: false,
            totp_secret: None,
            created_at: 0,
            updated_at: 0,
            expires_at: None,
        };
        assert!(matches!(
            require_app_read_role(&unknown),
            Err(AppError::Forbidden)
        ));
    }

    #[test]
    fn external_container_parser_rejects_invalid_utf8_and_json() {
        assert!(parse_external_container_output(&[0xff]).is_err());
        assert!(parse_external_container_output(b"{\"ID\":\"ok\"}\n{not-json}").is_err());
    }

    #[test]
    fn external_stack_response_does_not_disclose_host_compose_path() {
        let value = serde_json::to_value(ExternalStack {
            project_name: "demo".into(),
            compose_available: false,
            containers: Vec::new(),
            primary_port: None,
        })
        .unwrap();

        assert!(value.get("compose_path").is_none());
    }

    #[test]
    fn compose_content_is_bounded_and_redacts_sensitive_environment_values() {
        let content = "services:\n  app:\n    environment:\n      - API_TOKEN=do-not-return\n      PASSWORD: do-not-return\n      API-KEY: do-not-return\n      ordinary: keep-me\n    PRIVATE_KEY: |\n      BEGIN PRIVATE KEY do-not-return\n";

        let safe = redact_compose_content(content);

        assert!(!safe.contains("do-not-return"));
        assert!(safe.contains("API_TOKEN=[redacted]"));
        assert!(safe.contains("PASSWORD: [redacted]"));
        assert!(safe.contains("API-KEY: [redacted]"));
        assert!(!safe.contains("BEGIN PRIVATE KEY"));
        assert!(safe.contains("ordinary: keep-me"));
        assert!(safe.len() <= MAX_COMPOSE_BYTES);
    }
    #[test]
    fn flow_style_compose_environment_values_are_redacted() {
        let content = "services:\n  app:\n    environment: [\"API_TOKEN=do-not-return\", \"AWS_ACCESS_KEY_ID=do-not-return\", \"DATABASE_URL=postgres://user:secret@db/app\", \"ordinary=keep-me\"]\n    labels: {PASSWORD: do-not-return, safe: keep-me}\n";
        let safe = redact_compose_content(content);

        assert!(!safe.contains("do-not-return"));
        assert!(!safe.contains("postgres://user:secret"));
        assert!(safe.contains("ordinary=keep-me"));
        assert!(safe.contains("safe: keep-me"));
    }

    #[test]
    fn flow_redaction_does_not_recurse_on_compose_interpolation_scalars() {
        let content = "services:\n  app:\n    image: \"registry.example/app:${APP_TAG}\"\n    labels: {safe: keep-me}\n";
        let safe = redact_compose_content(content);

        assert!(safe.contains("${APP_TAG}"));
        assert!(safe.contains("safe: keep-me"));
    }

    #[test]
    fn command_and_entrypoint_arguments_are_redacted_in_scalar_and_flow_forms() {
        let content = "services:\n  app:\n    command: \"API_TOKEN=do-not-return curl --header 'Authorization: Bearer do-not-return'\"\n    entrypoint: [\"-e\", \"API_TOKEN=do-not-return\", \"-H\", \"Authorization: Bearer do-not-return\", \"--safe\"]\n";
        let safe = redact_compose_content(content);
        assert!(!safe.contains("do-not-return"));
        assert!(safe.contains("--safe"));
    }

    #[test]
    fn compose_block_scalar_modifiers_and_comments_are_redacted() {
        let content = "PRIVATE_KEY: |- # preserve the YAML shape\n  secret-body-do-not-return\nordinary: keep-me\n";
        let safe = redact_compose_content(content);

        assert!(!safe.contains("secret-body-do-not-return"));
        assert!(safe.contains("ordinary: keep-me"));
    }

    #[test]
    fn flow_yaml_does_not_bypass_generic_block_scalar_redaction() {
        let content = "services:\n  app:\n    labels: {safe: keep-me}\n    command: API_TOKEN=inline-secret-do-not-return\n    entrypoint: --HEADER Authorization: Bearer header-secret-do-not-return\n    command: |\n      PASSWORD=block-secret-do-not-return\n";
        let safe = redact_compose_content(content);

        assert!(!safe.contains("inline-secret-do-not-return"));
        assert!(!safe.contains("header-secret-do-not-return"));
        assert!(!safe.contains("block-secret-do-not-return"));
        assert!(safe.contains("safe: keep-me"));
    }

    #[test]
    fn oversized_compose_files_fail_before_their_contents_are_returned() {
        let path =
            std::env::temp_dir().join(format!("voidtower-compose-{}.yml", uuid::Uuid::new_v4()));
        std::fs::write(&path, "x".repeat(MAX_COMPOSE_BYTES + 1)).unwrap();

        let result = read_bounded_compose(&path);
        let _ = std::fs::remove_file(&path);

        assert!(matches!(result, Err(AppError::PayloadTooLarge)));
    }

    #[cfg(unix)]
    #[test]
    fn compose_fifo_paths_fail_without_blocking() {
        let path =
            std::env::temp_dir().join(format!("voidtower-compose-fifo-{}", uuid::Uuid::new_v4()));
        let path_string = path.to_string_lossy().into_owned();
        let path_c = std::ffi::CString::new(path_string).unwrap();
        assert_eq!(unsafe { nix::libc::mkfifo(path_c.as_ptr(), 0o600) }, 0);

        let result = read_bounded_compose(&path);
        let _ = std::fs::remove_file(&path);

        assert!(result.is_err());
    }

    #[test]
    fn embed_target_url_preserves_the_incoming_query_string() {
        assert_eq!(
            build_embed_target_url(8080, "login", Some("?next=%2Fadmin")),
            "http://localhost:8080/login?next=%2Fadmin"
        );
    }

    #[tokio::test]
    async fn embed_proxy_uses_the_public_error_envelope() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/apps/embed/example/login?next=%2Fadmin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "unauthorized");
        assert_eq!(payload["error"]["message"], "Unauthorized");
    }

    #[tokio::test]
    async fn embed_proxy_forwards_query_and_filters_upstream_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let upstream = Router::new().fallback(any(|uri: Uri| async move {
            let mut headers = HeaderMap::new();
            headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
            headers.insert(
                "set-cookie",
                HeaderValue::from_static("secret=do-not-forward"),
            );
            headers.insert("connection", HeaderValue::from_static("x-upstream-secret"));
            headers.insert(
                "x-upstream-secret",
                HeaderValue::from_static("do-not-forward"),
            );
            (
                headers,
                format!("{}?{}", uri.path(), uri.query().unwrap_or_default()),
            )
        }));
        let server = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });

        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO deployed_apps (id, app_id, app_name, project_name, status, deployed_at, compose_path, primary_port, origin, owner_user_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("app-1")
        .bind("catalog-app")
        .bind("Catalog App")
        .bind("catalog-app")
        .bind("running")
        .bind(0_i64)
        .bind("/tmp/compose.yml")
        .bind(i64::from(port))
        .bind("voidtower")
        .bind("u1")
        .execute(&pool)
        .await
        .unwrap();
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/apps/embed/catalog-app/login?next=%2Fadmin")
                    .header("cookie", format!("vt_session={session}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), MAX_EMBED_RESPONSE_BYTES)
            .await
            .unwrap();

        server.abort();
        assert_eq!(body, "/login?next=%2Fadmin");
        assert_eq!(
            headers.get("content-security-policy").unwrap(),
            "frame-ancestors 'self'"
        );
        assert!(headers.get("x-frame-options").is_none());
        assert!(headers.get("set-cookie").is_none());
        assert!(headers.get("connection").is_none());
        assert!(headers.get("x-upstream-secret").is_none());
    }

    #[test]
    fn log_output_bound_is_a_utf8_safe_byte_limit() {
        let bounded = bound_log_output(&"é".repeat(MAX_LOG_BYTES));

        assert!(bounded.len() <= MAX_LOG_BYTES);
        assert!(bounded.is_char_boundary(bounded.len()));
    }

    #[test]
    fn redacted_compose_output_remains_within_the_public_byte_bound() {
        let content = format!("SECRET: {}\n", "x".repeat(MAX_COMPOSE_BYTES - 8));
        let safe = bound_utf8_bytes(&redact_compose_content(&content), MAX_COMPOSE_BYTES);

        assert!(safe.len() <= MAX_COMPOSE_BYTES);
        assert!(safe.is_char_boundary(safe.len()));
        assert!(!safe.contains(&"x".repeat(32)));
    }

    #[test]
    fn ui_host_validation_preserves_bracketed_ipv6_and_rejects_authority_injection() {
        assert_eq!(validate_ui_host("[::1]:8743").as_deref(), Some("[::1]"));
        assert_eq!(validate_ui_host("[::1]").as_deref(), Some("[::1]"));
        assert!(validate_ui_host("[]").is_none());
        assert!(validate_ui_host("[not-an-ip]").is_none());
        assert!(validate_ui_host("attacker.example/@voidtower").is_none());
        assert!(validate_ui_host("2001:db8::1").is_none());
    }
}
