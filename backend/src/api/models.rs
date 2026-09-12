use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

// ─── Download state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DownloadState {
    pub id: String,
    pub filename: String,
    pub total_bytes: Option<u64>,
    pub downloaded_bytes: u64,
    pub status: String, // "downloading" | "done" | "error"
    pub error: Option<String>,
}

static DOWNLOADS: OnceLock<Mutex<HashMap<String, DownloadState>>> = OnceLock::new();

fn downloads() -> &'static Mutex<HashMap<String, DownloadState>> {
    DOWNLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ─── Ollama pull state ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct OllamaPullState {
    pub id: String,
    pub model: String,
    pub status: String, // "pulling" | "done" | "error"
    pub current_layer: Option<String>,
    pub total_bytes: Option<u64>,
    pub pulled_bytes: Option<u64>,
    pub error: Option<String>,
}

static OLLAMA_PULLS: OnceLock<Mutex<HashMap<String, OllamaPullState>>> = OnceLock::new();

fn ollama_pulls() -> &'static Mutex<HashMap<String, OllamaPullState>> {
    OLLAMA_PULLS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ModelFile {
    pub filename: String,
    pub size_bytes: u64,
    pub modified: i64,
    pub active: bool,
    pub source: String, // "voidtower" | "ollama"
}

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

async fn models_dir(state: &AppState) -> std::path::PathBuf {
    let setting =
        sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'models_dir'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .map(|(v,)| v);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let default = std::path::PathBuf::from(home).join(".local/share/voidtower/models");
    let dir = setting.map(std::path::PathBuf::from).unwrap_or(default);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Query the running llama.cpp server for its loaded model.
/// Tries the Docker host port (8090) first, then the native port (8080).
async fn get_active_model_from_server() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .build()
        .ok()?;
    for port in [8090u16, 8080] {
        let url = format!("http://127.0.0.1:{port}/v1/models");
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    let model_id = body.get("data")?.as_array()?.first()?.get("id")?.as_str()?;
                    // Strip any leading path prefix, keep just the filename
                    let name = std::path::Path::new(model_id)
                        .file_name()?
                        .to_string_lossy()
                        .into_owned();
                    return Some(name);
                }
            }
        }
    }
    None
}

// ─── Ollama integration ───────────────────────────────────────────────────────

async fn fetch_ollama_models() -> std::result::Result<Vec<ModelFile>, ()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .map_err(|_| ())?;

    let resp = client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map_err(|_| ())?;

    if !resp.status().is_success() {
        return Err(());
    }

    let body: serde_json::Value = resp.json().await.map_err(|_| ())?;
    let list = body.get("models").and_then(|v| v.as_array()).ok_or(())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(list
        .iter()
        .filter_map(|m| {
            let name = m.get("name")?.as_str()?.to_string();
            let size_bytes = m.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            // modified_at is RFC-3339; parse the leading Unix seconds portion
            let modified = m
                .get("modified_at")
                .and_then(|v| v.as_str())
                .and_then(|s| s.split('T').next())
                .and_then(|date| {
                    // "YYYY-MM-DD" → rough epoch seconds (good enough for sorting)
                    let parts: Vec<u32> = date.split('-').filter_map(|p| p.parse().ok()).collect();
                    if parts.len() == 3 {
                        // days since epoch: very rough but sufficient for sort order
                        let days = (parts[0] as i64 - 1970) * 365
                            + (parts[1] as i64) * 30
                            + parts[2] as i64;
                        Some(days * 86400)
                    } else {
                        None
                    }
                })
                .unwrap_or(now);
            Some(ModelFile {
                filename: name,
                size_bytes,
                modified,
                active: false,
                source: "ollama".into(),
            })
        })
        .collect())
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub async fn list_models(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ModelFile>>> {
    require_admin(&state, &jar).await?;
    let dir = models_dir(&state).await;

    // Find active model by querying the live llama.cpp server (ports 8090/8080)
    let active = get_active_model_from_server().await;

    let mut models = Vec::new();

    // VoidTower-managed flat .gguf files
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("gguf") {
                continue;
            }
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let meta = std::fs::metadata(&path).ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let is_active = active.as_deref() == Some(&filename);
            models.push(ModelFile {
                filename,
                size_bytes,
                modified,
                active: is_active,
                source: "voidtower".into(),
            });
        }
    }

    // Ollama models — best-effort, silently skipped if Ollama isn't running
    if let Ok(ollama_models) = fetch_ollama_models().await {
        for m in ollama_models {
            models.push(m);
        }
    }

    models.sort_by_key(|m| std::cmp::Reverse(m.modified));
    Ok(Json(models))
}

#[derive(Deserialize)]
pub struct DownloadReq {
    pub url: String,
    pub filename: Option<String>,
}

pub async fn start_download(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<DownloadReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    if !req.url.starts_with("http") {
        return Err(AppError::BadRequest("URL must start with http".into()));
    }

    let filename = req.filename.filter(|f| !f.is_empty()).unwrap_or_else(|| {
        req.url
            .split('/')
            .next_back()
            .unwrap_or("model.gguf")
            .split('?')
            .next()
            .unwrap_or("model.gguf")
            .to_string()
    });

    if filename.contains('/') || filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let dir = models_dir(&state).await;

    {
        let mut map = downloads().lock().unwrap();
        map.insert(
            id.clone(),
            DownloadState {
                id: id.clone(),
                filename: filename.clone(),
                total_bytes: None,
                downloaded_bytes: 0,
                status: "downloading".into(),
                error: None,
            },
        );
    }

    let id2 = id.clone();
    let url = req.url.clone();
    tokio::spawn(async move {
        let result = download_file(&id2, &url, &dir, &filename).await;
        let mut map = downloads().lock().unwrap();
        if let Some(entry) = map.get_mut(&id2) {
            match result {
                Ok(_) => {
                    entry.status = "done".into();
                }
                Err(e) => {
                    entry.status = "error".into();
                    entry.error = Some(e);
                }
            }
        }
    });

    Ok(Json(serde_json::json!({ "id": id })))
}

async fn download_file(
    id: &str,
    url: &str,
    dir: &std::path::Path,
    filename: &str,
) -> std::result::Result<(), String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Mozilla/5.0 (compatible; VoidTower/1.0; +https://github.com/voidtower)")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(url)
        .header("Accept", "*/*")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    // Reject HTML responses — user likely pasted a model page URL instead of a direct file link
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if content_type.starts_with("text/html") {
        return Err("Got an HTML page instead of a file. \
             Use a direct download URL ending in .gguf — e.g. \
             https://huggingface.co/{user}/{repo}/resolve/main/{file}.gguf"
            .into());
    }

    let total = resp.content_length();
    {
        let mut map = downloads().lock().unwrap();
        if let Some(s) = map.get_mut(id) {
            s.total_bytes = total;
        }
    }

    let tmp_path = dir.join(format!("{filename}.tmp"));
    let final_path = dir.join(filename);

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&tmp_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let mut map = downloads().lock().unwrap();
        if let Some(s) = map.get_mut(id) {
            s.downloaded_bytes = downloaded;
        }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);

    // Verify GGUF magic bytes (0x47 0x47 0x55 0x46 = "GGUF") before keeping the file
    {
        use tokio::io::AsyncReadExt;
        let mut f = tokio::fs::File::open(&tmp_path)
            .await
            .map_err(|e| e.to_string())?;
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)
            .await
            .map_err(|_| "Downloaded file is too small to be a valid GGUF model".to_string())?;
        if &magic != b"GGUF" {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(
                "Downloaded file is not a valid GGUF model (wrong magic bytes). \
                 Make sure the URL points directly to a .gguf file, not a model page."
                    .into(),
            );
        }
    }

    tokio::fs::rename(&tmp_path, &final_path)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn download_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<DownloadState>> {
    require_admin(&state, &jar).await?;
    let map = downloads().lock().unwrap();
    map.get(&id).cloned().map(Json).ok_or(AppError::NotFound)
}

pub async fn delete_model(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if filename.contains('/') || filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".into()));
    }
    let dir = models_dir(&state).await;
    let path = dir.join(&filename);
    if !path.exists() {
        return Err(AppError::NotFound);
    }
    tokio::fs::remove_file(&path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn get_active(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let active = get_active_model_from_server().await;
    Ok(Json(serde_json::json!({ "filename": active })))
}

fn llama_entrypoint_with_exec(server_args: &str) -> String {
    [
        "if [ -n \"$$MODEL_PATH\" ]; then",
        "  MODEL=\"$$MODEL_PATH\"",
        "else",
        "  MODEL=\"$$(find /models -name '*.gguf' 2>/dev/null | head -1)\"",
        "fi",
        "while [ -z \"$$MODEL\" ]; do",
        "  echo \"[llama.cpp] No .gguf found -- retrying in 15s...\"",
        "  sleep 15",
        "  MODEL=\"$$(find /models -name '*.gguf' 2>/dev/null | head -1)\"",
        "done",
        "echo \"[llama.cpp] Loading: $$MODEL\"",
        &format!("exec /app/llama-server --model \"$$MODEL\" {}", server_args),
    ]
    .join("\n")
}

fn llama_entrypoint_script() -> String {
    llama_entrypoint_with_exec(
        "--host 0.0.0.0 --port 8080 --n-gpu-layers 999 --ctx-size 8192 --batch-size 512 --threads 4 --cont-batching --parallel 1",
    )
}

async fn switch_llama_model(state: &AppState, filename: &str) -> Result<()> {
    let (project_name, compose_path_str) = sqlx::query_as::<_, (String, String)>(
        "SELECT project_name, compose_path FROM deployed_apps WHERE app_id = 'llama-cpp' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::BadRequest("llama.cpp is not deployed".into()))?;

    let compose_path = std::path::PathBuf::from(&compose_path_str);
    let content =
        std::fs::read_to_string(&compose_path).map_err(|e| AppError::Internal(e.into()))?;
    let mut val: serde_json::Value =
        serde_yaml::from_str(&content).map_err(|e| AppError::Internal(e.into()))?;

    if let Some(services) = val.get_mut("services").and_then(|s| s.as_object_mut()) {
        for svc in services.values_mut() {
            if let Some(env) = svc.get_mut("environment").and_then(|e| e.as_array_mut()) {
                env.retain(|e| !matches!(e.as_str(), Some(s) if s.starts_with("MODEL_PATH=")));
                env.push(serde_json::Value::String(format!(
                    "MODEL_PATH=/models/{}",
                    filename
                )));
            }
            if let Some(ep) = svc.get_mut("entrypoint").and_then(|e| e.as_array_mut()) {
                if ep.len() >= 3 {
                    ep[2] = serde_json::Value::String(llama_entrypoint_script());
                }
            }
        }
    }

    let new_content = serde_yaml::to_string(&val).map_err(|e| AppError::Internal(e.into()))?;
    std::fs::write(&compose_path, new_content).map_err(|e| AppError::Internal(e.into()))?;

    crate::containers::deploy_compose(&project_name, &compose_path)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    Ok(())
}

pub async fn load_model(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(_req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "Model loading requires a canonical operation adapter".into(),
    ))
}

// ─── Ollama pull ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct OllamaPullReq {
    pub model: String,
}

pub async fn start_ollama_pull(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<OllamaPullReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    // Allow letters, digits, colons (tags), hyphens, dots, underscores, slashes (registry)
    if req.model.is_empty()
        || !req
            .model
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '.' | '_' | '/'))
    {
        return Err(AppError::BadRequest("Invalid model name".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut map = ollama_pulls().lock().unwrap();
        map.insert(
            id.clone(),
            OllamaPullState {
                id: id.clone(),
                model: req.model.clone(),
                status: "pulling".into(),
                current_layer: Some("Connecting to Ollama…".into()),
                total_bytes: None,
                pulled_bytes: None,
                error: None,
            },
        );
    }

    let id2 = id.clone();
    let model = req.model.clone();
    tokio::spawn(async move {
        let result = do_ollama_pull(&id2, &model).await;
        let mut map = ollama_pulls().lock().unwrap();
        if let Some(entry) = map.get_mut(&id2) {
            match result {
                Ok(_) => {
                    entry.status = "done".into();
                    entry.current_layer = Some("Complete".into());
                }
                Err(e) => {
                    entry.status = "error".into();
                    entry.error = Some(e);
                }
            }
        }
    });

    Ok(Json(serde_json::json!({ "id": id })))
}

async fn do_ollama_pull(id: &str, model: &str) -> std::result::Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post("http://localhost:11434/api/pull")
        .json(&serde_json::json!({ "name": model, "stream": true }))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Ollama returned HTTP {}", resp.status()));
    }

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf = buf[nl + 1..].to_string();
            if line.is_empty() {
                continue;
            }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                let status_msg = v
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let total = v.get("total").and_then(|n| n.as_u64());
                let completed = v.get("completed").and_then(|n| n.as_u64());

                {
                    let mut map = ollama_pulls().lock().unwrap();
                    if let Some(entry) = map.get_mut(id) {
                        entry.current_layer = Some(status_msg.clone());
                        if total.is_some() {
                            entry.total_bytes = total;
                            entry.pulled_bytes = completed;
                        }
                    }
                }

                if status_msg == "success" {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

pub async fn get_ollama_pull_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<OllamaPullState>> {
    require_admin(&state, &jar).await?;
    let map = ollama_pulls().lock().unwrap();
    map.get(&id).cloned().map(Json).ok_or(AppError::NotFound)
}

// ─── Ollama create (load GGUF into Ollama) ────────────────────────────────────

static OLLAMA_CREATES: OnceLock<Mutex<HashMap<String, OllamaPullState>>> = OnceLock::new();

fn ollama_creates() -> &'static Mutex<HashMap<String, OllamaPullState>> {
    OLLAMA_CREATES.get_or_init(|| Mutex::new(HashMap::new()))
}

#[allow(dead_code)]
fn gguf_to_ollama_name(filename: &str) -> String {
    filename
        .trim_end_matches(".gguf")
        .to_lowercase()
        .replace(' ', "-")
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct OllamaCreateReq {
    pub filename: String,
}

pub async fn start_ollama_create(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "Ollama model creation requires a canonical operation adapter".into(),
    ))
}

#[allow(dead_code)]
async fn do_ollama_create(
    id: &str,
    filename: &str,
    model_name: &str,
) -> std::result::Result<(), String> {
    let modelfile = format!("FROM /vt-models/{filename}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post("http://localhost:11434/api/create")
        .json(&serde_json::json!({ "name": model_name, "modelfile": modelfile, "stream": true }))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Ollama: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Ollama error: {body}"));
    }

    use futures_util::StreamExt;
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim().to_string();
            buf = buf[nl + 1..].to_string();
            if line.is_empty() {
                continue;
            }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                    return Err(err.to_string());
                }
                let status_msg = v
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                let total = v.get("total").and_then(|n| n.as_u64());
                let completed = v.get("completed").and_then(|n| n.as_u64());

                {
                    let mut map = ollama_creates().lock().unwrap();
                    if let Some(entry) = map.get_mut(id) {
                        entry.current_layer = Some(status_msg.clone());
                        if total.is_some() {
                            entry.total_bytes = total;
                            entry.pulled_bytes = completed;
                        }
                    }
                }

                if status_msg == "success" {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

pub async fn get_ollama_create_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<OllamaPullState>> {
    require_admin(&state, &jar).await?;
    let map = ollama_creates().lock().unwrap();
    map.get(&id).cloned().map(Json).ok_or(AppError::NotFound)
}

// ─── GET /api/models/ollama — installed Ollama models proxy ──────────────────

#[derive(Serialize)]
pub struct OllamaTagsResponse {
    pub available: bool,
    pub models: Vec<OllamaModelInfo>,
}

#[derive(Serialize)]
pub struct OllamaModelInfo {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

pub async fn get_ollama_tags(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<OllamaTagsResponse>> {
    require_admin(&state, &jar).await?;

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(2000))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            return Ok(Json(OllamaTagsResponse {
                available: false,
                models: vec![],
            }))
        }
    };

    let resp = match client.get("http://127.0.0.1:11434/api/tags").send().await {
        Ok(r) => r,
        Err(_) => {
            return Ok(Json(OllamaTagsResponse {
                available: false,
                models: vec![],
            }))
        }
    };

    if !resp.status().is_success() {
        return Ok(Json(OllamaTagsResponse {
            available: false,
            models: vec![],
        }));
    }

    let body: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => {
            return Ok(Json(OllamaTagsResponse {
                available: false,
                models: vec![],
            }))
        }
    };

    let models = body
        .get("models")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let name = m.get("name")?.as_str()?.to_string();
                    let size = m.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    let modified_at = m
                        .get("modified_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(OllamaModelInfo {
                        name,
                        size,
                        modified_at,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(Json(OllamaTagsResponse {
        available: true,
        models,
    }))
}

// ─── Ollama config ────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OllamaConfig {
    pub deployed: bool,
    pub keep_alive_secs: i32,
}

pub async fn get_ollama_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<OllamaConfig>> {
    require_admin(&state, &jar).await?;

    let row = sqlx::query_as::<_, (String,)>(
        "SELECT compose_path FROM deployed_apps WHERE app_id = 'ollama' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?;

    let Some((compose_path_str,)) = row else {
        return Ok(Json(OllamaConfig {
            deployed: false,
            keep_alive_secs: 300,
        }));
    };

    let content =
        std::fs::read_to_string(&compose_path_str).map_err(|e| AppError::Internal(e.into()))?;
    let val: serde_json::Value =
        serde_yaml::from_str(&content).map_err(|e| AppError::Internal(e.into()))?;

    let keep_alive_secs = val["services"]
        .as_object()
        .and_then(|s| s.values().next())
        .and_then(|svc| svc["environment"].as_array())
        .and_then(|env| {
            env.iter().find_map(|e| {
                e.as_str()
                    .and_then(|s| s.strip_prefix("OLLAMA_KEEP_ALIVE="))
                    .and_then(|v| v.parse::<i32>().ok())
            })
        })
        .unwrap_or(300);

    Ok(Json(OllamaConfig {
        deployed: true,
        keep_alive_secs,
    }))
}

pub async fn save_ollama_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "Saving Ollama configuration requires a canonical operation adapter".into(),
    ))
}

// ─── llama.cpp config ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LlamaConfig {
    pub deployed: bool,
    pub threads: u32,
    pub ctx_size: u32,
    pub batch_size: u32,
    pub parallel: u32,
    pub n_gpu_layers: u32,
    pub flash_attn: bool,
    pub cont_batching: bool,
    pub cache_type_k: String,
    pub cache_type_v: String,
}

impl Default for LlamaConfig {
    fn default() -> Self {
        Self {
            deployed: false,
            threads: 4,
            ctx_size: 8192,
            batch_size: 512,
            parallel: 1,
            n_gpu_layers: 999,
            flash_attn: false,
            cont_batching: true,
            cache_type_k: "f16".into(),
            cache_type_v: "f16".into(),
        }
    }
}

fn parse_llama_exec_args(line: &str) -> LlamaConfig {
    let mut cfg = LlamaConfig {
        deployed: true,
        cont_batching: false,
        ..Default::default()
    };
    let parts: Vec<&str> = line.split_whitespace().collect();
    let mut i = 0usize;
    while i < parts.len() {
        let next = |i: usize| parts.get(i + 1).and_then(|s| s.parse().ok());
        match parts[i] {
            "--threads" => {
                if let Some(v) = next(i) {
                    cfg.threads = v;
                }
                i += 1;
            }
            "--ctx-size" => {
                if let Some(v) = next(i) {
                    cfg.ctx_size = v;
                }
                i += 1;
            }
            "--batch-size" => {
                if let Some(v) = next(i) {
                    cfg.batch_size = v;
                }
                i += 1;
            }
            "--parallel" => {
                if let Some(v) = next(i) {
                    cfg.parallel = v;
                }
                i += 1;
            }
            "--n-gpu-layers" => {
                if let Some(v) = next(i) {
                    cfg.n_gpu_layers = v;
                }
                i += 1;
            }
            "--flash-attn" => {
                cfg.flash_attn = true;
            }
            "--cont-batching" => {
                cfg.cont_batching = true;
            }
            "--cache-type-k" => {
                if let Some(v) = parts.get(i + 1) {
                    cfg.cache_type_k = v.to_string();
                }
                i += 1;
            }
            "--cache-type-v" => {
                if let Some(v) = parts.get(i + 1) {
                    cfg.cache_type_v = v.to_string();
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    cfg
}

pub async fn get_llama_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<LlamaConfig>> {
    require_admin(&state, &jar).await?;

    let row = sqlx::query_as::<_, (String,)>(
        "SELECT compose_path FROM deployed_apps WHERE app_id = 'llama-cpp' LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?;

    let Some((compose_path_str,)) = row else {
        return Ok(Json(LlamaConfig::default()));
    };

    let content =
        std::fs::read_to_string(&compose_path_str).map_err(|e| AppError::Internal(e.into()))?;
    let val: serde_json::Value =
        serde_yaml::from_str(&content).map_err(|e| AppError::Internal(e.into()))?;

    let exec_line = val["services"]
        .as_object()
        .and_then(|s| s.values().next())
        .and_then(|svc| svc["entrypoint"].as_array())
        .and_then(|arr| arr.get(2))
        .and_then(|s| s.as_str())
        .and_then(|script| {
            script
                .lines()
                .find(|l| l.trim_start().starts_with("exec /app/llama-server"))
        })
        .unwrap_or("");

    Ok(Json(parse_llama_exec_args(exec_line)))
}

pub async fn save_llama_config(
    State(state): State<AppState>,
    jar: CookieJar,
    _body: Body,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "Saving llama.cpp configuration requires a canonical operation adapter".into(),
    ))
}

// ─── OpenAI-compatible proxy ──────────────────────────────────────────────────

/// GET /v1/models — lists every .gguf file in the models directory.
pub async fn openai_list_models(State(state): State<AppState>) -> Result<Json<serde_json::Value>> {
    let dir = models_dir(&state).await;
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().map(|x| x == "gguf").unwrap_or(false) {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    names.sort();
    let data: Vec<serde_json::Value> = names
        .into_iter()
        .map(|id| {
            serde_json::json!({
                "id": id, "object": "model", "created": 0, "owned_by": "local"
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "object": "list", "data": data })))
}

/// POST /v1/chat/completions — auto-switches the loaded model if needed, then streams through to llama.cpp.
pub async fn openai_chat_completions(
    State(state): State<AppState>,
    Json(req_body): Json<serde_json::Value>,
) -> Result<axum::response::Response> {
    if let Some(requested) = req_body
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        if get_active_model_from_server().await.as_deref() != Some(requested) {
            let dir = models_dir(&state).await;
            let gguf = format!("{}.gguf", requested);
            if dir.join(&gguf).exists() {
                switch_llama_model(&state, &gguf).await?;
                let poll = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(2))
                    .build()
                    .map_err(|e| AppError::Internal(e.into()))?;
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    if poll
                        .get("http://127.0.0.1:8090/v1/models")
                        .send()
                        .await
                        .map(|r| r.status().is_success())
                        .unwrap_or(false)
                    {
                        break;
                    }
                    if std::time::Instant::now() > deadline {
                        return Err(AppError::BadRequest("Model switch timed out".into()));
                    }
                }
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;
    let upstream = client
        .post("http://127.0.0.1:8090/v1/chat/completions")
        .json(&req_body)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let status = axum::http::StatusCode::from_u16(upstream.status().as_u16())
        .unwrap_or(axum::http::StatusCode::OK);
    let content_type = upstream.headers().get("content-type").cloned();

    let resp_body = axum::body::Body::from_stream(upstream.bytes_stream());
    let mut resp = axum::response::Response::new(resp_body);
    *resp.status_mut() = status;
    if let Some(ct) = content_type {
        resp.headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, ct);
    }
    Ok(resp)
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
    async fn start_ollama_create_rejects_unauthenticated_malformed_input_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/ollama/create")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn start_ollama_create_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/ollama/create")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "Ollama model creation requires a canonical operation adapter"
        );
    }

    #[test]
    fn start_ollama_create_handler_has_no_direct_mutation_path() {
        let source = include_str!("models.rs");
        let handler = source
            .split("pub async fn start_ollama_create(")
            .nth(1)
            .and_then(|rest| rest.split("async fn do_ollama_create").next())
            .expect("start-ollama-create handler");

        for marker in [
            "reqwest::",
            "sqlx::query",
            "std::fs::",
            "tokio::spawn",
            "do_ollama_create(",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "start-ollama-create marker: {marker}");
        }
    }

    #[tokio::test]
    async fn load_model_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/load")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(json!({ "filename": "model.gguf" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn load_model_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/load")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from(json!({ "filename": "model.gguf" }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "Model loading requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn save_llama_config_rejects_unauthenticated_call_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/llama-config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn save_llama_config_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/llama-config")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "Saving llama.cpp configuration requires a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn save_ollama_config_rejects_unauthenticated_malformed_input_before_feature_boundary() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/ollama-config")
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(json_body(response).await["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn save_ollama_config_fails_closed_after_authentication() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/models/ollama-config")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))))
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let payload = json_body(response).await;
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "Saving Ollama configuration requires a canonical operation adapter"
        );
    }

    #[test]
    fn save_ollama_config_handler_has_no_direct_mutation_path() {
        let source = include_str!("models.rs");
        let handler = source
            .split("pub async fn save_ollama_config(")
            .nth(1)
            .and_then(|rest| rest.split("// ─── llama.cpp config").next())
            .expect("save-ollama-config handler");

        for marker in [
            "sqlx::query",
            "std::fs::",
            "containers::",
            "deploy_compose(",
            "audit::log(",
        ] {
            assert!(
                !handler.contains(marker),
                "save-ollama-config handler marker: {marker}"
            );
        }
    }

    #[test]
    fn load_model_handler_has_no_direct_mutation_path() {
        let source = include_str!("models.rs");
        let handler = source
            .split("pub async fn load_model(")
            .nth(1)
            .and_then(|rest| rest.split("// ─── Ollama pull").next())
            .expect("load-model handler");

        for marker in [
            "switch_llama_model(",
            "sqlx::query(",
            "std::fs::",
            "containers::",
            "audit::log(",
        ] {
            assert!(!handler.contains(marker), "load-model handler marker: {marker}");
        }
    }

    #[test]
    fn save_llama_config_handler_has_no_direct_mutation_path() {
        let source = include_str!("models.rs");
        let handler = source
            .split("pub async fn save_llama_config(")
            .nth(1)
            .and_then(|rest| rest.split("// ─── OpenAI-compatible proxy").next())
            .expect("save-llama-config handler");

        for marker in [
            "sqlx::query",
            "std::fs::",
            "containers::",
            "deploy_compose(",
            "audit::log(",
        ] {
            assert!(
                !handler.contains(marker),
                "save-llama-config handler marker: {marker}"
            );
        }
    }
}
