use crate::{
    audit,
    auth,
    containers,
    error::{AppError, Result},
    AppState,
};
use super::members;
use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashMap, net::SocketAddr};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiIntegration {
    pub level: String,       // "native" | "aware"
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

fn default_post_deploy_wait() -> u64 { 120 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployedApp {
    pub id: String,
    pub app_id: String,
    pub app_name: String,
    pub project_name: String,
    pub status: String,
    pub deployed_at: i64,
    pub compose_path: String,
    pub primary_port: Option<i64>,
    #[serde(default = "default_origin")]
    pub origin: String,
    /// Self-hosting hub tenancy (all nullable — NULL means "admin-deployed on
    /// the primary host", i.e. every deploy before this feature existed).
    #[serde(default)]
    pub owner_user_id: Option<String>,
    #[serde(default)]
    pub storage_root: Option<String>,
    #[serde(default)]
    pub target_node_id: Option<String>,
}

fn default_origin() -> String { "voidtower".into() }

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
                    if p > 0 { return Some(p); }
                }
            }
            // Short syntax integer: 3000
            if let Some(n) = entry.as_u64() {
                if n > 0 && n <= 65535 { return Some(n as u16); }
            }
            // Long syntax: { published: 3000, target: 80 }
            if let Some(p) = entry.get("published").and_then(|v| v.as_u64()) {
                if p > 0 { return Some(p as u16); }
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
    app.web_port.or_else(|| first_port_from_compose(&app.compose))
}

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
        if !dir.exists() { continue; }
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yml") { continue; }
            let Ok(content) = std::fs::read_to_string(&path) else { continue };
            let Ok(app) = serde_yaml::from_str::<AppDef>(&content) else { continue };
            apps.push(app);
        }
        if !apps.is_empty() { break; }
    }

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps
}

// ─── LLM auto-detection ──────────────────────────────────────────────────────

/// Known local LLM services in priority order.
/// Each entry: (port, path to probe, human label, OpenAI-compat /v1 base URL)
const LLM_PROBES: &[(u16, &str, &str, &str)] = &[
    (8090,  "/health",      "llama.cpp",               "http://host.docker.internal:8090/v1"),
    (8080,  "/health",      "llama.cpp",               "http://host.docker.internal:8080/v1"),
    (11434, "/api/version", "Ollama",                  "http://host.docker.internal:11434/v1"),
    (1234,  "/v1/models",   "LM Studio",               "http://host.docker.internal:1234/v1"),
    (5001,  "/v1/models",   "Text Generation Web UI",  "http://host.docker.internal:5001/v1"),
    (8000,  "/v1/models",   "vLLM",                    "http://host.docker.internal:8000/v1"),
];

pub struct DetectedLlm {
    pub label: String,
    pub port:  u16,
    pub url:   String,
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
        if client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false) {
            return Some(DetectedLlm { label: label.into(), port, url: v1_url.into() });
        }
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
    let local = tokio::process::Command::new("nvidia-smi")
        .arg("-L")
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if local {
        return true;
    }

    tokio::process::Command::new("docker")
        .args(["info", "--format", "{{json .Runtimes}}"])
        .output()
        .await
        .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("nvidia"))
        .unwrap_or(false)
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
        .args(["network", "inspect", "vt-proxy", "--format", "{{.EnableIPv6}}"])
        .output()
        .await;

    match inspect {
        // Network exists and is already IPv4-only — nothing to do.
        Ok(out) if out.status.success()
            && String::from_utf8_lossy(&out.stdout).trim() != "true" => (),
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
        .args(["network", "inspect", "vt-proxy", "--format", "{{range .Containers}}{{.Name}} {{end}}"])
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
    let Some(root) = compose.as_object_mut() else { return };
    let nets = root
        .entry("networks")
        .or_insert_with(|| serde_json::json!({}));
    let Some(nets_obj) = nets.as_object_mut() else { return };

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
        let Some(obj) = cfg.as_object_mut() else { continue };
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
    let data_dir = config.data_dir.to_string_lossy().trim_end_matches('/').to_string();

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
    let data_dir = config.data_dir.to_string_lossy().trim_end_matches('/').to_string();
    let host_data_dir = config.host_data_dir.to_string_lossy().trim_end_matches('/').to_string();

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
fn rewrite_named_volumes_to_storage_root(compose: &mut Value, storage_root: &str, project_name: &str) {
    let rewritable_names: Vec<String> = compose
        .get("volumes")
        .and_then(|v| v.as_object())
        .map(|m| {
            m.iter()
                .filter(|(_, def)| !def.get("external").and_then(|e| e.as_bool()).unwrap_or(false))
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
            let Some(vols) = svc.get_mut("volumes").and_then(|v| v.as_array_mut()) else { continue };
            for vol in vols.iter_mut() {
                let Some(s) = vol.as_str() else { continue };
                let Some((name, rest)) = s.split_once(':') else { continue };
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
fn validate_and_sanitize_custom_deploy(svc: &mut Value, storage_root: &str) -> std::result::Result<(), String> {
    let Some(obj) = svc.as_object_mut() else { return Ok(()) };

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
        let canon_root = std::fs::canonicalize(storage_root).unwrap_or_else(|_| std::path::PathBuf::from(storage_root));
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
            let canon_host = std::fs::canonicalize(host_path).unwrap_or_else(|_| host_path.to_path_buf());
            if !canon_host.starts_with(&canon_root) {
                return Err(format!("Bind mount '{host_part}' is outside your own storage"));
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
        .await.ok()?;
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
        obj.entry("ipc".to_string()).or_insert(Value::String("host".into()));
        obj.entry("privileged".to_string()).or_insert(Value::Bool(true));
        if matches!(host_cuda_major, Some(v) if v > 12) {
            if let Some(env) = obj.get_mut("environment").and_then(|e| e.as_array_mut()) {
                if !env.iter().any(|e| matches!(e.as_str(), Some(s) if s.starts_with("NVIDIA_DISABLE_REQUIRE="))) {
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
        let Some(img_val) = svc.get_mut("image") else { continue };
        if let Some(new_img) = img_val.as_str()
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
        let Some(deploy) = svc.get_mut("deploy") else { continue };
        let Some(resources) = deploy.get_mut("resources") else { continue };
        let Some(reservations) = resources.get_mut("reservations") else { continue };
        if let Some(obj) = reservations.as_object_mut() {
            obj.remove("devices");
        }
        // Prune empty intermediate keys
        if reservations.as_object().map(|o| o.is_empty()).unwrap_or(false) {
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
        let Some(obj) = svc.as_object_mut() else { continue };
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
    if in_docker() { return; }
    let Some(services) = compose.get("services").and_then(|s| s.as_object()) else {
        return;
    };
    let home = std::env::var("HOME").unwrap_or_default();
    for svc in services.values() {
        let Some(volumes) = svc.get("volumes").and_then(|v| v.as_array()) else { continue };
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
    if overrides.contains_key("LLM_API_BASE") { return None; }

    // Check that at least one service in this compose references LLM_API_BASE
    let has_llm_var = compose
        .get("services")
        .and_then(|s| s.as_object())
        .map(|svcs| svcs.values().any(|svc| {
            svc.get("environment")
                .and_then(|e| e.as_array())
                .map(|arr| arr.iter().any(|v| {
                    v.as_str().map(|s| s.starts_with("LLM_API_BASE")).unwrap_or(false)
                }))
                .unwrap_or(false)
        }))
        .unwrap_or(false);

    if !has_llm_var { return None; }

    let detected = detect_llm_endpoint().await?;
    let label = detected.label.clone();
    let url   = detected.url.clone();

    // Inject into all services that have LLM_API_BASE
    if let Some(svcs) = compose.get_mut("services").and_then(|s| s.as_object_mut()) {
        for svc in svcs.values_mut() {
            if let Some(arr) = svc.get_mut("environment").and_then(|e| e.as_array_mut()) {
                let has_var = arr.iter().any(|v| {
                    v.as_str().map(|s| s.starts_with("LLM_API_BASE")).unwrap_or(false)
                });
                if has_var {
                    arr.retain(|v| !v.as_str().map(|s| s.starts_with("LLM_API_BASE=")).unwrap_or(false));
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
    require_user(&state, &jar).await?;
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
    let mut apps = load_catalog(&state.config.catalog_dir);

    // Members only ever see catalog apps an admin explicitly granted them —
    // every other role sees the full catalog, unchanged.
    if user.role == "member" {
        let allowed: std::collections::HashSet<String> = sqlx::query_scalar(
            "SELECT app_id FROM member_app_access WHERE user_id = ?",
        )
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

    let docker_available = containers::is_docker_available();

    // Members only ever see their own deployed apps (owner_user_id = self);
    // every other role keeps seeing everything, exactly as before.
    let rows = if user.role == "member" {
        sqlx::query_as::<_, DeployedAppRow>(
            &format!("{SELECT_DEPLOYED} WHERE owner_user_id = ? ORDER BY deployed_at DESC"),
        )
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?
    } else {
        sqlx::query_as::<_, DeployedAppRow>(
            &format!("{SELECT_DEPLOYED} ORDER BY deployed_at DESC"),
        )
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?
    };
    let apps = rows.into_iter().map(row_to_app).collect();

    Ok(Json(DeployedResponse { apps, docker_available }))
}

fn row_to_app(r: DeployedAppRow) -> DeployedApp {
    DeployedApp {
        id: r.id, app_id: r.app_id, app_name: r.app_name,
        project_name: r.project_name, status: r.status,
        deployed_at: r.deployed_at, compose_path: r.compose_path,
        primary_port: r.primary_port, origin: r.origin,
        owner_user_id: r.owner_user_id, storage_root: r.storage_root, target_node_id: r.target_node_id,
    }
}

const SELECT_DEPLOYED: &str =
    "SELECT id, app_id, app_name, project_name, status, deployed_at, compose_path, \
     COALESCE(primary_port, NULL) AS primary_port, \
     COALESCE(origin, 'voidtower') AS origin, \
     owner_user_id, storage_root, target_node_id FROM deployed_apps";

/// Non-admin `member` callers may only ever see/manage apps they own
/// (`owner_user_id` matches their own id); every other role's visibility is
/// unaffected (this only gates when `role == "member"`).
fn require_app_owner_or_admin(user: &auth::User, row: &DeployedAppRow) -> Result<()> {
    if user.role == "member" && row.owner_user_id.as_deref() != Some(user.id.as_str()) {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Runs `hook.command` inside the target container, retrying every 3s until it
/// succeeds or `max_wait_secs` elapses. Spawned in the background so it doesn't
/// hold up the deploy response — the container needs time to finish its own
/// startup/migrations before the command can succeed.
fn spawn_post_deploy_hook(project_name: String, hook: PostDeployHook, dotenv_map: HashMap<String, String>) {
    let container = format!("{}-{}-1", project_name, hook.container_suffix);
    let command: Vec<String> = hook.command.iter().map(|part| {
        let mut s = part.clone();
        for (k, v) in &dotenv_map {
            s = s.replace(&format!("${{{k}}}"), v);
        }
        s
    }).collect();

    tokio::spawn(async move {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(hook.max_wait_secs);
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
                    tracing::warn!("post_deploy hook for {container} did not succeed within {}s", hook.max_wait_secs);
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }

    let catalog = load_catalog(&state.config.catalog_dir);
    let app = catalog
        .into_iter()
        .find(|a| a.id == req.app_id)
        .ok_or(AppError::NotFound)?;

    // Refuse to deploy if a matching system service is installed on the host.
    // The installer writes a marker file when it sets up Odysseus or Ollama as
    // a systemd service; deploying the container would cause a port conflict.
    if let Some(ref conflict_key) = app.system_conflict_check {
        let marker = std::path::Path::new("/var/lib/voidtower")
            .join(format!(".{}-system-installed", conflict_key));
        if marker.exists() {
            let port_hint = match conflict_key.as_str() {
                "odysseus" => " (port 7000 conflict)",
                "ollama"   => " (port 11434 conflict)",
                _          => "",
            };
            return Err(AppError::BadRequest(format!(
                "{} is already installed as a system service on this host{}. \
                 Use the system service or remove it first with: \
                 systemctl stop {conflict_key} && systemctl disable {conflict_key}",
                app.name, port_hint,
            )));
        }
    }

    // Self-hosting hub: resolve the acting member's ownership/storage/target-node
    // context. Untouched (all None) for every other role, which preserves the
    // exact pre-existing admin/global deploy behavior.
    let mut owner_user_id: Option<String> = None;
    let mut storage_root: Option<String> = None;
    let mut target_node_id: Option<String> = None;
    if user.role == "member" {
        members::check_member_app_access(&state, &user.id, &app.id).await?;
        members::check_member_quota(&state, &user.id).await?;
        let resolved = members::resolve_member_storage_root(&state, &user.id, req.storage_drive_id.as_deref()).await?;
        storage_root = Some(resolved.path);
        target_node_id = members::resolve_member_target_node(&state, &user.id, req.target_node_id.as_deref()).await?;
        owner_user_id = Some(user.id.clone());
    }

    let project_name = req
        .project_name
        .unwrap_or_else(|| format!("vt-{}", app.id));

    // Write the compose file
    let app_dir = state.config.apps_dir().join(&project_name);
    std::fs::create_dir_all(&app_dir).map_err(|e| AppError::Internal(e.into()))?;

    let mut compose_val = app.compose.clone();

    // Apply env overrides if any
    if let Some(overrides) = &req.env_overrides {
        if let Some(services) = compose_val.get_mut("services") {
            if let Some(obj) = services.as_object_mut() {
                for svc in obj.values_mut() {
                    if let Some(env) = svc.get_mut("environment") {
                        if let Some(arr) = env.as_array_mut() {
                            for (k, v) in overrides {
                                arr.retain(|e| {
                                    !e.as_str()
                                        .map(|s| s.starts_with(&format!("{}=", k)))
                                        .unwrap_or(false)
                                });
                                arr.push(Value::String(format!("{}={}", k, v)));
                            }
                        }
                    }
                }
            }
        }
    }

    // Resolve values for required_env entries not supplied by the user, via
    // `generate` strategy or `default` fallback. These (plus all user-supplied
    // overrides) are written to a .env file below — compose templates reference
    // them as `${VAR}` interpolation, which docker compose only resolves from a
    // .env file or process environment, never from another `environment:` entry.
    let mut resolved_required_env: HashMap<String, String> = HashMap::new();
    for req_var in &app.required_env {
        let already_set = req.env_overrides.as_ref()
            .map(|o| o.contains_key(&req_var.key))
            .unwrap_or(false);
        if already_set {
            continue;
        }
        let value = req_var.generate.as_ref()
            .filter(|g| !g.is_empty())
            .map(|g| generate_required_env_value(g))
            .or_else(|| req_var.default.clone());
        if let Some(value) = value {
            resolved_required_env.insert(req_var.key.clone(), value);
        }
    }

    // Replace null volume entries with empty maps so docker compose accepts them
    if let Some(vols) = compose_val.get_mut("volumes") {
        if let Some(obj) = vols.as_object_mut() {
            for v in obj.values_mut() {
                if v.is_null() { *v = serde_json::json!({}); }
            }
        }
    }

    // Strip GPU requirements on non-NVIDIA hosts; on NVIDIA hosts apply CUDA
    // compatibility fixes (ipc: host, NVIDIA_DISABLE_REQUIRE) for the driver.
    let has_gpu = detect_gpu().await;
    if !has_gpu {
        strip_gpu_requirements(&mut compose_val);
        adjust_cuda_image_for_no_gpu(&mut compose_val);
    } else {
        apply_nvidia_compat(&mut compose_val, detect_cuda_major_version().await);
    }

    // Member deploys: redirect the app's own named-volume data onto the
    // member's resolved storage (drive or quota dir) instead of an opaque
    // Docker-managed volume.
    if let Some(ref root) = storage_root {
        rewrite_named_volumes_to_storage_root(&mut compose_val, root, &project_name);
    }

    // Create any host-side bind-mount directories referenced in volumes.
    ensure_volume_dirs(&compose_val);

    // Auto-detect running LLM service and inject LLM_API_BASE if not manually set
    let detected_llm = auto_inject_llm(
        &mut compose_val,
        req.env_overrides.as_ref().unwrap_or(&HashMap::new()),
    ).await;

    // Inject top-level external network declaration when services reference vt-proxy
    inject_external_networks(&mut compose_val);
    // Pin all compose-managed networks to IPv4 (TrueNAS IPv6 pool workaround)
    force_ipv4_networks(&mut compose_val);
    // Rewrite VoidTower-data bind-mount sources for containerized installs (TrueNAS)
    rewrite_voidtower_home_paths(&mut compose_val, &state.config);
    rewrite_host_bind_mounts(&mut compose_val, &state.config);
    strip_unavailable_devices(&mut compose_val);

    let compose_str = serde_yaml::to_string(&compose_val).map_err(|e| AppError::Internal(e.into()))?;
    let compose_path = app_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_str).map_err(|e| AppError::Internal(e.into()))?;

    // Write a .env file next to the compose file so docker compose can resolve
    // ${VAR} interpolation (project directory defaults to the -f file's dir).
    let mut dotenv_map = req.env_overrides.clone().unwrap_or_default();
    dotenv_map.extend(resolved_required_env.clone());
    if !dotenv_map.is_empty() {
        let dotenv_content = dotenv_map.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(app_dir.join(".env"), dotenv_content + "\n")
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    // Ensure shared Docker network exists before composing
    ensure_vt_proxy_network().await;

    // Run docker compose up — cancellable via POST /api/apps/deploy/cancel/{project_name}
    containers::deploy_compose_cancellable(&project_name, &compose_path, &state.deploy_registry)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    if let Some(hook) = app.post_deploy.clone() {
        spawn_post_deploy_hook(project_name.clone(), hook, dotenv_map.clone());
    }

    // Record in DB
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let primary_port = extract_primary_port(&app).map(|p| p as i64);

    sqlx::query(
        "INSERT OR REPLACE INTO deployed_apps \
         (id, app_id, app_name, project_name, status, deployed_at, compose_path, primary_port, origin, \
          owner_user_id, storage_root, target_node_id) \
         VALUES (?, ?, ?, ?, 'running', ?, ?, ?, 'voidtower', ?, ?, ?)"
    )
    .bind(&id)
    .bind(&app.id)
    .bind(&app.name)
    .bind(&project_name)
    .bind(now)
    .bind(compose_path.to_string_lossy().as_ref())
    .bind(primary_port)
    .bind(&owner_user_id)
    .bind(&storage_root)
    .bind(&target_node_id)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    audit::log(
        &state.db,
        Some(&user.id),
        &user.username,
        "app.deploy",
        Some("app"),
        Some(&app.id),
        "success",
        Some(&ip),
        Some(&format!(
            "project={},owner={}",
            project_name,
            owner_user_id.as_deref().unwrap_or("admin"),
        )),
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "project_name": project_name,
        "detected_llm": detected_llm,
        "generated_env": resolved_required_env,
    })))
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
    let ip = addr.ip().to_string();

    let cancelled = containers::cancel_deploy(&state.deploy_registry, &project_name).await;

    audit::log(&state.db, Some(&user.id), &user.username, "app.deploy.cancel",
        Some("app"), Some(&project_name), if cancelled { "success" } else { "not_found" }, Some(&ip), None).await;

    Ok(Json(serde_json::json!({ "ok": true, "cancelled": cancelled })))
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
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<CustomDeployRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }

    let name = req.name.trim().to_string();
    if name.is_empty() || req.image.trim().is_empty() {
        return Err(AppError::BadRequest("name and image are required".into()));
    }

    // Custom-tier deploys are a materially bigger trust boundary than the
    // curated catalog (plan §5) — a member needs the opt-in flag, and their
    // submission gets validated/sanitized below. Admin/operator callers keep
    // the pre-existing free-form behavior untouched.
    let is_member = user.role == "member";
    let mut owner_user_id: Option<String> = None;
    let mut storage_root: Option<String> = None;
    let mut target_node_id: Option<String> = None;
    if is_member {
        if !members::member_can_deploy_custom(&state, &user.id).await {
            return Err(AppError::Forbidden);
        }
        members::check_member_quota(&state, &user.id).await?;
        let resolved = members::resolve_member_storage_root(&state, &user.id, req.storage_drive_id.as_deref()).await?;
        storage_root = Some(resolved.path);
        target_node_id = members::resolve_member_target_node(&state, &user.id, req.target_node_id.as_deref()).await?;
        owner_user_id = Some(user.id.clone());
    }

    // Sanitise project name — alphanumeric + hyphens only
    let project_name = format!("vt-custom-{}",
        name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>()
    );

    // Member port bindings: manual host ports must fall in the auto-allocated
    // range; a bare container port (no host side given) gets one allocated.
    let mut ports = req.ports.clone();
    if is_member {
        for p in ports.iter_mut() {
            if let Some((host, _container)) = p.split_once(':') {
                let h: u16 = host.parse().map_err(|_| AppError::BadRequest(format!("Invalid host port '{host}'")))?;
                if !members::MEMBER_CUSTOM_PORT_RANGE.contains(&h) {
                    return Err(AppError::BadRequest(format!(
                        "Port {h} is outside the allowed range {}-{}",
                        members::MEMBER_CUSTOM_PORT_RANGE.start(), members::MEMBER_CUSTOM_PORT_RANGE.end(),
                    )));
                }
            } else {
                let host_port = members::allocate_member_port(&state).await?;
                *p = format!("{host_port}:{p}");
            }
        }
    }

    // Member volumes: a relative host path (or none) is anchored under their
    // own storage_root automatically — they never need to know the real path.
    let mut volumes = req.volumes.clone();
    if is_member {
        let root = storage_root.clone().unwrap_or_default();
        for v in volumes.iter_mut() {
            if let Some((host, rest)) = v.split_once(':') {
                if !host.starts_with('/') {
                    let full = std::path::Path::new(&root).join(host.trim_start_matches("./"));
                    let _ = std::fs::create_dir_all(&full);
                    *v = format!("{}:{}", full.display(), rest);
                }
            }
        }
    }

    // Build compose YAML manually
    let mut svc = serde_json::json!({
        "image": req.image.trim(),
        "restart": "unless-stopped",
    });
    if !ports.is_empty() {
        svc["ports"] = Value::Array(ports.iter().map(|p| Value::String(p.clone())).collect());
    }
    if !volumes.is_empty() {
        svc["volumes"] = Value::Array(volumes.iter().map(|v| Value::String(v.clone())).collect());
    }
    if !req.env.is_empty() {
        svc["environment"] = Value::Array(req.env.iter().map(|e| Value::String(e.clone())).collect());
    }

    if is_member {
        let root = storage_root.clone().unwrap_or_default();
        validate_and_sanitize_custom_deploy(&mut svc, &root).map_err(AppError::BadRequest)?;
        // Mandatory resource limits — arbitrary member-supplied images on
        // shared infrastructure get a hard ceiling, unlike the vetted catalog.
        if let Some(obj) = svc.as_object_mut() {
            obj.entry("mem_limit".to_string()).or_insert(Value::String("1g".into()));
            obj.entry("cpus".to_string()).or_insert(Value::String("1.0".into()));
        }
    }

    let mut compose_val = serde_json::json!({
        "services": { &name: svc }
    });

    // Extract primary port before writing (custom deploys have no AppDef)
    let primary_port = first_port_from_compose(&compose_val).map(|p| p as i64);

    // Ensure any bind-mount host paths exist
    ensure_volume_dirs(&compose_val);

    // Write compose file
    let app_dir = state.config.apps_dir().join(&project_name);
    std::fs::create_dir_all(&app_dir).map_err(|e| AppError::Internal(e.into()))?;

    // Replace null volumes
    if let Some(vols) = compose_val.get_mut("volumes") {
        if let Some(obj) = vols.as_object_mut() {
            for v in obj.values_mut() {
                if v.is_null() { *v = serde_json::json!({}); }
            }
        }
    }

    // Pin all compose-managed networks to IPv4 (TrueNAS IPv6 pool workaround)
    force_ipv4_networks(&mut compose_val);
    // Rewrite VoidTower-data bind-mount sources for containerized installs (TrueNAS)
    rewrite_voidtower_home_paths(&mut compose_val, &state.config);
    rewrite_host_bind_mounts(&mut compose_val, &state.config);
    strip_unavailable_devices(&mut compose_val);

    let compose_str = serde_yaml::to_string(&compose_val).map_err(|e| AppError::Internal(e.into()))?;
    let compose_path = app_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_str).map_err(|e| AppError::Internal(e.into()))?;

    containers::deploy_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

    sqlx::query(
        "INSERT OR REPLACE INTO deployed_apps \
         (id, app_id, app_name, project_name, status, deployed_at, compose_path, primary_port, origin, \
          owner_user_id, storage_root, target_node_id) \
         VALUES (?, 'custom', ?, ?, 'running', ?, ?, ?, 'voidtower', ?, ?, ?)"
    )
    .bind(&id).bind(&name).bind(&project_name)
    .bind(now).bind(compose_path.to_string_lossy().as_ref()).bind(primary_port)
    .bind(&owner_user_id).bind(&storage_root).bind(&target_node_id)
    .execute(&state.db).await.map_err(AppError::Database)?;

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "app.deploy_custom", Some("app"), Some(&project_name),
        "success", Some(&ip),
        Some(&format!("image={},owner={}", req.image.trim(), owner_user_id.as_deref().unwrap_or("admin"))),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "project_name": project_name })))
}

pub async fn start_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    if !compose_path.exists() {
        return Err(AppError::BadRequest(
            format!("Compose file not found at {}. Re-deploy the app to restore it.", compose_path.display())
        ));
    }
    containers::deploy_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    sqlx::query("UPDATE deployed_apps SET status = 'running' WHERE project_name = ?")
        .bind(&project_name)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    audit::log(&state.db, Some(&user.id), &user.username, "app.start",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn redeploy_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }

    // Look up the existing deployment to get app_id
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    // Re-read the latest catalog definition so updated env vars are picked up
    let catalog = load_catalog(&state.config.catalog_dir);
    let app = catalog.into_iter().find(|a| a.id == row.app_id);

    let compose_path = std::path::PathBuf::from(&row.compose_path);

    if let Some(app) = app {
        // Rewrite compose file from latest catalog YAML (picks up new env/GPU config)
        let app_dir = compose_path.parent().unwrap_or(&compose_path);
        std::fs::create_dir_all(app_dir).map_err(|e| AppError::Internal(e.into()))?;

        let mut compose_val = app.compose.clone();
        if let Some(vols) = compose_val.get_mut("volumes") {
            if let Some(obj) = vols.as_object_mut() {
                for v in obj.values_mut() {
                    if v.is_null() { *v = serde_json::json!({}); }
                }
            }
        }
        let has_gpu = detect_gpu().await;
        if !has_gpu {
            strip_gpu_requirements(&mut compose_val);
            adjust_cuda_image_for_no_gpu(&mut compose_val);
        } else {
            apply_nvidia_compat(&mut compose_val, detect_cuda_major_version().await);
        }
        ensure_volume_dirs(&compose_val);
        auto_inject_llm(&mut compose_val, &HashMap::new()).await;
        inject_external_networks(&mut compose_val);
        force_ipv4_networks(&mut compose_val);
        rewrite_voidtower_home_paths(&mut compose_val, &state.config);
        rewrite_host_bind_mounts(&mut compose_val, &state.config);
        strip_unavailable_devices(&mut compose_val);
        let compose_str = serde_yaml::to_string(&compose_val)
            .map_err(|e| AppError::Internal(e.into()))?;
        std::fs::write(&compose_path, &compose_str)
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    ensure_vt_proxy_network().await;
    containers::deploy_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    sqlx::query("UPDATE deployed_apps SET status = 'running', deployed_at = ? WHERE project_name = ?")
        .bind(now)
        .bind(&project_name)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    audit::log(&state.db, Some(&user.id), &user.username, "app.redeploy",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn restart_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    containers::restart_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    audit::log(&state.db, Some(&user.id), &user.username, "app.restart",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn remove_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    let _ = containers::remove_compose(&project_name, &compose_path).await;

    // Remove compose file and project dir
    let _ = std::fs::remove_file(&compose_path);
    if let Some(dir) = compose_path.parent() {
        let _ = std::fs::remove_dir(dir);
    }

    sqlx::query("DELETE FROM deployed_apps WHERE project_name = ?")
        .bind(&project_name)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    audit::log(&state.db, Some(&user.id), &user.username, "app.remove",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn app_logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    let logs = containers::logs_compose(&project_name, &compose_path, 300)
        .await
        .unwrap_or_else(|e| format!("Error fetching logs: {e}"));

    let lines: Vec<&str> = logs.lines().collect();
    Ok(Json(serde_json::json!({ "lines": lines })))
}

pub async fn app_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    let containers = containers::status_compose(&project_name, &compose_path)
        .await
        .unwrap_or_default();

    Ok(Json(serde_json::json!({ "containers": containers })))
}

pub async fn stop_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let compose_path = std::path::PathBuf::from(&row.compose_path);
    containers::stop_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    sqlx::query("UPDATE deployed_apps SET status = 'stopped' WHERE project_name = ?")
        .bind(&project_name)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    audit::log(
        &state.db,
        Some(&user.id),
        &user.username,
        "app.stop",
        Some("app"),
        Some(&project_name),
        "success",
        Some(&ip),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn get_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    let content = std::fs::read_to_string(&row.compose_path)
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "compose_path": row.compose_path, "content": content })))
}

#[derive(Deserialize)]
pub struct UpdateComposeRequest {
    pub content: String,
}

// ─── open-ui: auto-create embed proxy ────────────────────────────────────────

#[derive(Deserialize)]
pub struct OpenUiRequest {
    pub project_name: String,
    pub primary_port: u16,
}

pub async fn open_ui(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<OpenUiRequest>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;

    // Use the Host header so the returned URL works from any machine on the LAN,
    // not just localhost. Strip the port portion if present.
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|h| h.rsplit_once(':').map(|(h, _)| h).unwrap_or(h).to_string())
        .unwrap_or_else(|| "localhost".to_string());

    let direct_url = format!("http://{}:{}", host, req.primary_port);

    // Use a slug-based domain key for DB lookup (keep existing record format).
    let domain = format!("{}.embed", req.project_name);
    let upstream = format!("http://localhost:{}", req.primary_port);

    // Look up any existing embed record. The upstream + nginx conf are always
    // refreshed below to match the current `upstream` even when a port was
    // already allocated — otherwise an app whose catalog `web_port` changed
    // (or that was first opened before `web_port`/`web_path` were added) keeps
    // routing to the stale port forever, since the conf was only ever written
    // once at first-open time.
    let existing: Option<(bool, Option<i64>, String)> = sqlx::query_as(
        "SELECT allow_embed, embed_port, upstream FROM proxy_configs WHERE domain = ?",
    )
    .bind(&domain)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let embed_url: Option<String>;
    let proxy_created: bool;

    let nginx_ok = tokio::task::spawn_blocking(|| {
        crate::api::proxy::nginx_active_pub()
    }).await.unwrap_or(false);

    if nginx_ok {
        let embed_port = match &existing {
            Some((_, Some(port), _)) => *port as u16,
            _ => {
                // Allocate next free embed port in 8800–8899 range.
                let next_port: i64 = sqlx::query_scalar(
                    "SELECT COALESCE(MAX(embed_port), 8799) + 1 FROM proxy_configs WHERE embed_port IS NOT NULL",
                )
                .fetch_one(&state.db)
                .await
                .unwrap_or(8800);
                (next_port as u16).clamp(8800, 8899)
            }
        };

        proxy_created = existing.is_none();
        let upstream_changed = existing.as_ref().map(|(_, _, u)| u != &upstream).unwrap_or(true);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let saved = if proxy_created {
            sqlx::query(
                "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, embed_port, created_at) VALUES (?,?,?,0,1,1,?,?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&domain)
            .bind(&upstream)
            .bind(embed_port as i64)
            .bind(now)
            .execute(&state.db)
            .await
            .map(|_| true)
            .unwrap_or(false)
        } else if upstream_changed {
            sqlx::query("UPDATE proxy_configs SET upstream = ?, embed_port = ? WHERE domain = ?")
                .bind(&upstream)
                .bind(embed_port as i64)
                .bind(&domain)
                .execute(&state.db)
                .await
                .map(|_| true)
                .unwrap_or(false)
        } else {
            true
        };

        if saved {
            // Always rewrite the conf — self-heals if the upstream port drifted
            // since it was last written, without requiring the user to delete
            // and recreate the proxy entry.
            let _ = crate::api::proxy::write_nginx_port_conf(&req.project_name, &upstream, embed_port);
            let _ = crate::api::proxy::reload_nginx_pub();
            if proxy_created {
                // Open the port in the local firewall non-blocking.
                let port_str = embed_port.to_string();
                tokio::task::spawn_blocking(move || {
                    crate::api::proxy::open_firewall_port(&port_str);
                });
            }
            embed_url = Some(format!("http://{}:{}", host, embed_port));
        } else {
            embed_url = None;
        }
    } else {
        embed_url = None;
        proxy_created = false;
    }

    Ok(Json(serde_json::json!({
        "url": direct_url,
        "embed_url": embed_url,
        "proxy_created": proxy_created,
    })))
}

pub async fn update_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<UpdateComposeRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    let ip = addr.ip().to_string();

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;
    require_app_owner_or_admin(&user, &row)?;

    // Validate that the content is valid YAML
    serde_yaml::from_str::<serde_yaml::Value>(&req.content)
        .map_err(|e| AppError::BadRequest(format!("Invalid YAML: {e}")))?;

    std::fs::write(&row.compose_path, &req.content)
        .map_err(|e| AppError::Internal(e.into()))?;

    // Restart the app with new compose
    let compose_path = std::path::PathBuf::from(&row.compose_path);
    containers::deploy_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "app.compose.update", Some("app"), Some(&project_name),
        "success", Some(&ip), None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Embed proxy — strips X-Frame-Options so App Vault iframes load ────────────

pub async fn embed_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((project_name, path)): Path<(String, String)>,
) -> impl IntoResponse {
    let session_id = match jar.get("vt_session").map(|c| c.value().to_string()) {
        Some(s) => s,
        None => return (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
    };
    match auth::validate_session(&state.db, &session_id).await {
        Ok(Some(_)) => {}
        _ => return (StatusCode::UNAUTHORIZED, "unauthorized").into_response(),
    }

    let row = match sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?"),
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND, "app not found").into_response(),
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response(),
    };

    let port = match row.primary_port {
        Some(p) if p > 0 => p as u16,
        _ => return (StatusCode::BAD_GATEWAY, "no port configured").into_response(),
    };

    let upstream_url = format!("http://localhost:{}/{}", port, path);

    let client = reqwest::Client::new();
    let upstream_resp = match client
        .get(&upstream_url)
        .header("X-Forwarded-For", "127.0.0.1")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return (StatusCode::BAD_GATEWAY, "upstream error").into_response(),
    };

    let status = upstream_resp.status();
    let mut resp_headers = axum::http::HeaderMap::new();

    for (name, value) in upstream_resp.headers() {
        let n = name.as_str().to_lowercase();
        if n == "x-frame-options" || n == "content-security-policy" { continue; }
        resp_headers.insert(name.clone(), value.clone());
    }
    resp_headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors *"),
    );

    let body = Body::from_stream(upstream_resp.bytes_stream());
    (status, resp_headers, body).into_response()
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
    pub compose_path: Option<String>,
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
    let mut out  = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if let Some(arrow) = part.find("->") {
            let host = part[..arrow].rsplit(':').next().unwrap_or("").trim();
            let cont = &part[arrow + 2..];
            if !host.is_empty() {
                let entry = format!("{host}:{cont}");
                if seen.insert(entry.clone()) { out.push(entry); }
            }
        }
    }
    out
}

pub async fn detect_external(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ExternalStack>>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Ok(Json(vec![]));
    }

    // Fetch all containers including stopped ones
    let out = tokio::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .output()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Already-managed project names
    let managed: std::collections::HashSet<String> = sqlx::query_scalar(
        "SELECT project_name FROM deployed_apps"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default()
    .into_iter()
    .collect();

    // Group containers by compose project
    let mut groups: std::collections::HashMap<String, (Option<String>, Vec<ExternalContainer>)> = std::collections::HashMap::new();

    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) else { continue };

        let id    = obj["ID"].as_str().unwrap_or("").to_string();
        let name  = obj["Names"].as_str().unwrap_or("").to_string();
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
        if project.starts_with("vt-") || managed.contains(&project) { continue; }

        let compose_path = labels
            .get("com.docker.compose.project.working_dir")
            .and_then(|dir| {
                for f in &["docker-compose.yml","docker-compose.yaml","compose.yml","compose.yaml"] {
                    let p = std::path::Path::new(dir).join(f);
                    if p.exists() { return Some(p.to_string_lossy().into_owned()); }
                }
                None
            });

        let entry = groups.entry(project).or_insert((compose_path, vec![]));
        entry.1.push(ExternalContainer {
            id, name, image, state: state_str,
            ports: parse_docker_ports(ports_str),
        });
    }

    let mut stacks: Vec<ExternalStack> = groups
        .into_iter()
        .map(|(project_name, (compose_path, containers))| {
            let primary_port = containers.iter()
                .flat_map(|c| c.ports.iter())
                .filter_map(|p| p.split(':').next()?.parse::<u16>().ok())
                .min();
            ExternalStack { project_name, compose_path, containers, primary_port }
        })
        .collect();
    stacks.sort_by(|a, b| a.project_name.cmp(&b.project_name));

    Ok(Json(stacks))
}

// ── Adopt external app ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AdoptRequest {
    pub project_name: String,
    pub app_name: String,
    pub compose_path: Option<String>,
    pub primary_port: Option<i64>,
}

pub async fn adopt_app(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<AdoptRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM deployed_apps WHERE project_name = ?"
    ).bind(&req.project_name).fetch_optional(&state.db).await?;
    if existing.is_some() {
        return Err(AppError::BadRequest("Already managed by VoidTower".into()));
    }

    let id  = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    let compose_path = req.compose_path.as_deref().unwrap_or("");

    sqlx::query(
        "INSERT INTO deployed_apps \
         (id, app_id, app_name, project_name, status, deployed_at, compose_path, primary_port, origin) \
         VALUES (?, 'external', ?, ?, 'running', ?, ?, ?, 'adopted')"
    )
    .bind(&id).bind(&req.app_name).bind(&req.project_name)
    .bind(now).bind(compose_path).bind(req.primary_port)
    .execute(&state.db).await?;

    // Connect all containers in the project to vt-proxy so the reverse proxy can reach them
    ensure_vt_proxy_network().await;
    let ps = tokio::process::Command::new("docker")
        .args(["ps", "-q", "--filter", &format!("label=com.docker.compose.project={}", req.project_name)])
        .output().await;
    if let Ok(ps_out) = ps {
        for cid in String::from_utf8_lossy(&ps_out.stdout).split_whitespace() {
            let _ = tokio::process::Command::new("docker")
                .args(["network", "connect", "vt-proxy", cid])
                .output().await;
        }
    }

    audit::log(&state.db, Some(&user.id), &user.username, "app.adopt",
        Some("app"), Some(&req.project_name), "success",
        Some(&addr.ip().to_string()), None).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Convert adopted app to VoidTower management ───────────────────────────────

pub async fn convert_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    ).bind(&project_name).fetch_optional(&state.db).await?
     .ok_or(AppError::NotFound)?;

    if row.compose_path.is_empty() {
        return Err(AppError::BadRequest(
            "No compose file detected — cannot convert a standalone container automatically".into()
        ));
    }

    let src = std::path::PathBuf::from(&row.compose_path);
    if !src.exists() {
        return Err(AppError::BadRequest(format!(
            "Compose file not found at {}", src.display()
        )));
    }

    let dest_dir = state.config.apps_dir().join(&project_name);
    std::fs::create_dir_all(&dest_dir).map_err(|e| AppError::Internal(e.into()))?;
    let dest = dest_dir.join("docker-compose.yml");

    // Copy compose file only if destination differs from source
    if src != dest {
        std::fs::copy(&src, &dest).map_err(|e| AppError::Internal(e.into()))?;
    }

    sqlx::query(
        "UPDATE deployed_apps SET compose_path = ?, origin = 'voidtower' WHERE project_name = ?"
    ).bind(dest.to_string_lossy().as_ref()).bind(&project_name)
     .execute(&state.db).await?;

    ensure_vt_proxy_network().await;
    containers::deploy_compose(&project_name, &dest)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    audit::log(&state.db, Some(&user.id), &user.username, "app.convert",
        Some("app"), Some(&project_name), "success",
        Some(&addr.ip().to_string()), None).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Toolpack-backed handlers ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PatchEnvRequest {
    pub env: HashMap<String, String>,
    pub service: Option<String>,
}

#[derive(Deserialize)]
pub struct ExposeAppRequest {
    pub domain: String,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default = "default_true")]
    pub allow_embed: bool,
}
fn default_true() -> bool { true }

pub async fn pull_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();
    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
        .bind(&project_name).fetch_optional(&state.db).await
        .map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let compose_path = std::path::PathBuf::from(&row.compose_path);
    if !compose_path.exists() {
        return Err(AppError::BadRequest(format!("Compose file not found at {}", compose_path.display())));
    }
    containers::pull_compose(&project_name, &compose_path).await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    containers::deploy_compose(&project_name, &compose_path).await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    sqlx::query("UPDATE deployed_apps SET status = 'running' WHERE project_name = ?")
        .bind(&project_name).execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), &user.username, "app.pull",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn patch_app_env(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<PatchEnvRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    let ip = addr.ip().to_string();
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
        .bind(&project_name).fetch_optional(&state.db).await
        .map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let content = std::fs::read_to_string(&row.compose_path)
        .map_err(|e| AppError::Internal(e.into()))?;
    let mut compose: serde_json::Value = serde_yaml::from_str(&content)
        .map_err(|e| AppError::BadRequest(format!("Invalid compose YAML: {e}")))?;
    if let Some(services) = compose.get_mut("services").and_then(|s| s.as_object_mut()) {
        for (svc_name, svc) in services.iter_mut() {
            if let Some(ref target) = req.service {
                if svc_name != target { continue; }
            }
            let Some(svc_obj) = svc.as_object_mut() else { continue };
            match svc_obj.get("environment") {
                Some(serde_json::Value::Object(_)) => {
                    if let Some(serde_json::Value::Object(map)) = svc_obj.get_mut("environment") {
                        for (k, v) in &req.env {
                            map.insert(k.clone(), serde_json::Value::String(v.clone()));
                        }
                    }
                }
                Some(serde_json::Value::Array(arr)) => {
                    let mut map = serde_json::Map::new();
                    for item in arr.clone() {
                        if let Some(s) = item.as_str() {
                            if let Some((k, v)) = s.split_once('=') {
                                map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                            }
                        }
                    }
                    for (k, v) in &req.env { map.insert(k.clone(), serde_json::Value::String(v.clone())); }
                    svc_obj.insert("environment".to_string(), serde_json::Value::Object(map));
                }
                _ => {
                    let map: serde_json::Map<_, _> = req.env.iter()
                        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                        .collect();
                    svc_obj.insert("environment".to_string(), serde_json::Value::Object(map));
                }
            }
        }
    }
    let new_content = serde_yaml::to_string(&compose)
        .map_err(|e| AppError::Internal(e.into()))?;
    std::fs::write(&row.compose_path, &new_content)
        .map_err(|e| AppError::Internal(e.into()))?;
    let compose_path = std::path::PathBuf::from(&row.compose_path);
    containers::deploy_compose(&project_name, &compose_path).await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    audit::log(&state.db, Some(&user.id), &user.username, "app.env.patch",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn expose_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<ExposeAppRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role != "admin" { return Err(AppError::Forbidden); }
    let ip = addr.ip().to_string();
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
        .bind(&project_name).fetch_optional(&state.db).await
        .map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let port = row.primary_port.filter(|&p| p > 0)
        .ok_or_else(|| AppError::BadRequest("No port configured for this app".into()))? as u16;
    let upstream = format!("http://localhost:{port}");
    let proxy_id = crate::api::proxy::create_proxy_record(
        &state.db, &req.domain, &upstream, req.ssl, req.allow_embed,
    ).await?;
    audit::log(&state.db, Some(&user.id), &user.username, "app.expose",
        Some("app"), Some(&project_name), "success", Some(&ip),
        Some(&format!("domain={},proxy={proxy_id}", req.domain))).await;
    Ok(Json(serde_json::json!({ "ok": true, "proxy_id": proxy_id, "upstream": upstream })))
}

pub async fn delete_app_volumes(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }
    let ip = addr.ip().to_string();
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
        .bind(&project_name).fetch_optional(&state.db).await
        .map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let compose_path = std::path::PathBuf::from(&row.compose_path);
    containers::remove_compose(&project_name, &compose_path).await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;
    sqlx::query("UPDATE deployed_apps SET status = 'stopped' WHERE project_name = ?")
        .bind(&project_name).execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), &user.username, "app.delete_volumes",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn purge_app(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(project_name): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role != "admin" { return Err(AppError::Forbidden); }
    let ip = addr.ip().to_string();
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?"))
        .bind(&project_name).fetch_optional(&state.db).await
        .map_err(AppError::Database)?.ok_or(AppError::NotFound)?;
    let compose_path = std::path::PathBuf::from(&row.compose_path);
    let _ = containers::remove_compose(&project_name, &compose_path).await;
    if let Some(dir) = compose_path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
    sqlx::query("DELETE FROM deployed_apps WHERE project_name = ?")
        .bind(&project_name).execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), &user.username, "app.purge",
        Some("app"), Some(&project_name), "success", Some(&ip), None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}
