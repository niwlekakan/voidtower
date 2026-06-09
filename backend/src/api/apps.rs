use crate::{
    audit,
    auth,
    containers,
    error::{AppError, Result},
    AppState,
};
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
}

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

/// Returns true if NVIDIA GPU is available (nvidia-smi exits 0).
pub fn detect_gpu() -> bool {
    std::process::Command::new("nvidia-smi")
        .arg("-L")
        .output()
        .map(|o| o.status.success())
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

/// Ensure the vt-proxy Docker network exists (creates it if missing).
async fn ensure_vt_proxy_network() {
    let _ = tokio::process::Command::new("docker")
        .args(["network", "create", "vt-proxy"])
        .output()
        .await;
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
    let gpu = detect_gpu();
    Ok(Json(serde_json::json!({
        "gpu": gpu,
        "llm": llm.map(|d| serde_json::json!({ "label": d.label, "port": d.port, "url": d.url })),
    })))
}

pub async fn catalog(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<CatalogResponse>> {
    require_user(&state, &jar).await?;
    let apps = load_catalog(&state.config.catalog_dir);
    Ok(Json(CatalogResponse { apps }))
}

pub async fn deployed(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<DeployedResponse>> {
    require_user(&state, &jar).await?;

    let docker_available = containers::is_docker_available();
    let apps = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} ORDER BY deployed_at DESC"),
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?
    .into_iter()
    .map(row_to_app)
    .collect();

    Ok(Json(DeployedResponse { apps, docker_available }))
}

fn row_to_app(r: DeployedAppRow) -> DeployedApp {
    DeployedApp {
        id: r.id, app_id: r.app_id, app_name: r.app_name,
        project_name: r.project_name, status: r.status,
        deployed_at: r.deployed_at, compose_path: r.compose_path,
        primary_port: r.primary_port, origin: r.origin,
    }
}

const SELECT_DEPLOYED: &str =
    "SELECT id, app_id, app_name, project_name, status, deployed_at, compose_path, \
     COALESCE(primary_port, NULL) AS primary_port, \
     COALESCE(origin, 'voidtower') AS origin FROM deployed_apps";

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

    // Replace null volume entries with empty maps so docker compose accepts them
    if let Some(vols) = compose_val.get_mut("volumes") {
        if let Some(obj) = vols.as_object_mut() {
            for v in obj.values_mut() {
                if v.is_null() { *v = serde_json::json!({}); }
            }
        }
    }

    // Strip NVIDIA GPU requirements if no GPU is detected so deployment works
    // on machines that don't have NVIDIA Container Toolkit configured.
    if !detect_gpu() {
        strip_gpu_requirements(&mut compose_val);
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

    let compose_str = serde_yaml::to_string(&compose_val).map_err(|e| AppError::Internal(e.into()))?;
    let compose_path = app_dir.join("docker-compose.yml");
    std::fs::write(&compose_path, &compose_str).map_err(|e| AppError::Internal(e.into()))?;

    // Ensure shared Docker network exists before composing
    ensure_vt_proxy_network().await;

    // Run docker compose up
    containers::deploy_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    // Record in DB
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let primary_port = extract_primary_port(&app).map(|p| p as i64);

    sqlx::query(
        "INSERT OR REPLACE INTO deployed_apps \
         (id, app_id, app_name, project_name, status, deployed_at, compose_path, primary_port, origin) \
         VALUES (?, ?, ?, ?, 'running', ?, ?, ?, 'voidtower')"
    )
    .bind(&id)
    .bind(&app.id)
    .bind(&app.name)
    .bind(&project_name)
    .bind(now)
    .bind(compose_path.to_string_lossy().as_ref())
    .bind(primary_port)
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
        Some(&format!("project={}", project_name)),
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "project_name": project_name,
        "detected_llm": detected_llm,
    })))
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
    // Sanitise project name — alphanumeric + hyphens only
    let project_name = format!("vt-custom-{}",
        name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect::<String>()
    );

    // Build compose YAML manually
    let mut svc = serde_json::json!({
        "image": req.image.trim(),
        "restart": "unless-stopped",
    });
    if !req.ports.is_empty() {
        svc["ports"] = Value::Array(req.ports.iter().map(|p| Value::String(p.clone())).collect());
    }
    if !req.volumes.is_empty() {
        svc["volumes"] = Value::Array(req.volumes.iter().map(|v| Value::String(v.clone())).collect());
    }
    if !req.env.is_empty() {
        svc["environment"] = Value::Array(req.env.iter().map(|e| Value::String(e.clone())).collect());
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
         (id, app_id, app_name, project_name, status, deployed_at, compose_path, primary_port, origin) \
         VALUES (?, 'custom', ?, ?, 'running', ?, ?, ?, 'voidtower')"
    )
    .bind(&id).bind(&name).bind(&project_name)
    .bind(now).bind(compose_path.to_string_lossy().as_ref()).bind(primary_port)
    .execute(&state.db).await.map_err(AppError::Database)?;

    audit::log(
        &state.db, Some(&user.id), &user.username,
        "app.deploy_custom", Some("app"), Some(&project_name),
        "success", Some(&ip), Some(&format!("image={}", req.image.trim())),
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
        if !detect_gpu() {
            strip_gpu_requirements(&mut compose_val);
        }
        ensure_volume_dirs(&compose_val);
        auto_inject_llm(&mut compose_val, &HashMap::new()).await;
        inject_external_networks(&mut compose_val);
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
    require_user(&state, &jar).await?;

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

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
    require_user(&state, &jar).await?;

    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

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
    require_user(&state, &jar).await?;
    let row = sqlx::query_as::<_, DeployedAppRow>(
        &format!("{SELECT_DEPLOYED} WHERE project_name = ?")
    )
    .bind(&project_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

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

    // Look up existing embed record to get any already-allocated port.
    let existing: Option<(bool, Option<i64>)> = sqlx::query_as(
        "SELECT allow_embed, embed_port FROM proxy_configs WHERE domain = ?",
    )
    .bind(&domain)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let embed_url: Option<String>;
    let proxy_created: bool;

    if let Some((_, Some(port))) = existing {
        // Already set up — return the existing LAN URL.
        embed_url = Some(format!("http://{}:{}", host, port));
        proxy_created = false;
    } else {
        let nginx_ok = tokio::task::spawn_blocking(|| {
            crate::api::proxy::nginx_active_pub()
        }).await.unwrap_or(false);

        if nginx_ok {
            // Allocate next free embed port in 8800–8899 range.
            let next_port: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(embed_port), 8799) + 1 FROM proxy_configs WHERE embed_port IS NOT NULL",
            )
            .fetch_one(&state.db)
            .await
            .unwrap_or(8800);
            let embed_port = (next_port as u16).clamp(8800, 8899);

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let inserted = if existing.is_none() {
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
            } else {
                // Record exists but has no port yet — patch it.
                sqlx::query("UPDATE proxy_configs SET embed_port = ? WHERE domain = ?")
                    .bind(embed_port as i64)
                    .bind(&domain)
                    .execute(&state.db)
                    .await
                    .map(|_| true)
                    .unwrap_or(false)
            };

            if inserted {
                let _ = crate::api::proxy::write_nginx_port_conf(&req.project_name, &upstream, embed_port);
                let _ = crate::api::proxy::reload_nginx_pub();
                // Open the port in the local firewall non-blocking.
                let port_str = embed_port.to_string();
                tokio::task::spawn_blocking(move || {
                    open_firewall_port(&port_str);
                });
                embed_url = Some(format!("http://{}:{}", host, embed_port));
                proxy_created = true;
            } else {
                embed_url = None;
                proxy_created = false;
            }
        } else {
            embed_url = None;
            proxy_created = false;
        }
    }

    Ok(Json(serde_json::json!({
        "url": direct_url,
        "embed_url": embed_url,
        "proxy_created": proxy_created,
    })))
}

fn open_firewall_port(port: &str) {
    let tcp = format!("{port}/tcp");
    // ufw
    if std::process::Command::new("ufw")
        .args(["status"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Status: active"))
        .unwrap_or(false)
    {
        let _ = std::process::Command::new("ufw")
            .args(["allow", &tcp, "comment", "VoidTower embed"])
            .output();
        return;
    }
    // firewalld
    if std::process::Command::new("firewall-cmd")
        .args(["--state"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let _ = std::process::Command::new("firewall-cmd")
            .args(["--permanent", "--add-port", &tcp, "--quiet"])
            .output();
        let _ = std::process::Command::new("firewall-cmd").args(["--reload", "--quiet"]).output();
        return;
    }
    // iptables fallback
    if std::process::Command::new("iptables")
        .args(["-C", "INPUT", "-p", "tcp", "--dport", port, "-j", "ACCEPT"])
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        let _ = std::process::Command::new("iptables")
            .args(["-I", "INPUT", "-p", "tcp", "--dport", port, "-j", "ACCEPT"])
            .output();
    }
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
