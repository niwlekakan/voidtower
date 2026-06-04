use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::{Path, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::{Mutex, OnceLock}};

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
    let session_id = jar.get("vt_session").map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id).await
        .map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

async fn models_dir(state: &AppState) -> std::path::PathBuf {
    let setting = sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = 'models_dir'")
        .fetch_optional(&state.db).await.ok().flatten().map(|(v,)| v);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let default = std::path::PathBuf::from(home).join(".local/share/voidtower/models");
    let dir = setting.map(std::path::PathBuf::from).unwrap_or(default);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn get_active_model_from_compose(compose_path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(compose_path).ok()?;
    let val: serde_json::Value = serde_yaml::from_str(&content).ok()?;
    let cmd = val.get("services")?.as_object()?.values().next()?
        .get("command")?.as_str()?;
    // Find "--model /models/<filename>"
    let after = cmd.split("--model ").nth(1)?;
    let filename = after.split_whitespace().next()?
        .trim_start_matches("/models/").to_string();
    if filename.is_empty() { None } else { Some(filename) }
}

// ─── Ollama integration ───────────────────────────────────────────────────────

async fn fetch_ollama_models() -> std::result::Result<Vec<ModelFile>, ()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build().map_err(|_| ())?;

    let resp = client.get("http://localhost:11434/api/tags")
        .send().await.map_err(|_| ())?;

    if !resp.status().is_success() { return Err(()); }

    let body: serde_json::Value = resp.json().await.map_err(|_| ())?;
    let list = body.get("models").and_then(|v| v.as_array()).ok_or(())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default().as_secs() as i64;

    Ok(list.iter().filter_map(|m| {
        let name = m.get("name")?.as_str()?.to_string();
        let size_bytes = m.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        // modified_at is RFC-3339; parse the leading Unix seconds portion
        let modified = m.get("modified_at").and_then(|v| v.as_str())
            .and_then(|s| s.split('T').next())
            .and_then(|date| {
                // "YYYY-MM-DD" → rough epoch seconds (good enough for sorting)
                let parts: Vec<u32> = date.split('-').filter_map(|p| p.parse().ok()).collect();
                if parts.len() == 3 {
                    // days since epoch: very rough but sufficient for sort order
                    let days = (parts[0] as i64 - 1970) * 365 + (parts[1] as i64) * 30 + parts[2] as i64;
                    Some(days * 86400)
                } else { None }
            })
            .unwrap_or(now);
        Some(ModelFile { filename: name, size_bytes, modified, active: false, source: "ollama".into() })
    }).collect())
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub async fn list_models(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<ModelFile>>> {
    require_admin(&state, &jar).await?;
    let dir = models_dir(&state).await;

    // Find active model
    let active = sqlx::query_as::<_, (String,)>(
        "SELECT compose_path FROM deployed_apps WHERE app_id = 'llama-cpp' LIMIT 1"
    ).fetch_optional(&state.db).await.ok().flatten()
     .and_then(|(p,)| get_active_model_from_compose(std::path::Path::new(&p)));

    let mut models = Vec::new();

    // VoidTower-managed flat .gguf files
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("gguf") { continue; }
            let filename = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let meta = std::fs::metadata(&path).ok();
            let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta.as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64).unwrap_or(0);
            let is_active = active.as_deref() == Some(&filename);
            models.push(ModelFile { filename, size_bytes, modified, active: is_active, source: "voidtower".into() });
        }
    }

    // Ollama models — best-effort, silently skipped if Ollama isn't running
    if let Ok(ollama_models) = fetch_ollama_models().await {
        for m in ollama_models {
            models.push(m);
        }
    }

    models.sort_by(|a, b| b.modified.cmp(&a.modified));
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

    let filename = req.filename
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| {
            req.url.split('/').last()
                .unwrap_or("model.gguf")
                .split('?').next().unwrap_or("model.gguf")
                .to_string()
        });

    if filename.contains('/') || filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let dir = models_dir(&state).await;

    {
        let mut map = downloads().lock().unwrap();
        map.insert(id.clone(), DownloadState {
            id: id.clone(), filename: filename.clone(),
            total_bytes: None, downloaded_bytes: 0,
            status: "downloading".into(), error: None,
        });
    }

    let id2 = id.clone();
    let url = req.url.clone();
    tokio::spawn(async move {
        let result = download_file(&id2, &url, &dir, &filename).await;
        let mut map = downloads().lock().unwrap();
        if let Some(entry) = map.get_mut(&id2) {
            match result {
                Ok(_) => { entry.status = "done".into(); }
                Err(e) => { entry.status = "error".into(); entry.error = Some(e); }
            }
        }
    });

    Ok(Json(serde_json::json!({ "id": id })))
}

async fn download_file(id: &str, url: &str, dir: &std::path::Path, filename: &str) -> std::result::Result<(), String> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build().map_err(|e| e.to_string())?;

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total = resp.content_length();
    {
        let mut map = downloads().lock().unwrap();
        if let Some(s) = map.get_mut(id) { s.total_bytes = total; }
    }

    let tmp_path = dir.join(format!("{filename}.tmp"));
    let final_path = dir.join(filename);

    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::File::create(&tmp_path).await.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let mut map = downloads().lock().unwrap();
        if let Some(s) = map.get_mut(id) { s.downloaded_bytes = downloaded; }
    }

    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    tokio::fs::rename(&tmp_path, &final_path).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn download_status(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<DownloadState>> {
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
    if !path.exists() { return Err(AppError::NotFound); }
    tokio::fs::remove_file(&path).await.map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn get_active(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let active = sqlx::query_as::<_, (String,)>(
        "SELECT compose_path FROM deployed_apps WHERE app_id = 'llama-cpp' LIMIT 1"
    ).fetch_optional(&state.db).await.ok().flatten()
     .and_then(|(p,)| get_active_model_from_compose(std::path::Path::new(&p)));
    Ok(Json(serde_json::json!({ "filename": active })))
}

#[derive(Deserialize)]
pub struct LoadReq { pub filename: String }

pub async fn load_model(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<LoadReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if req.filename.contains('/') || req.filename.contains("..") {
        return Err(AppError::BadRequest("Invalid filename".into()));
    }

    let (project_name, compose_path_str) = sqlx::query_as::<_, (String, String)>(
        "SELECT project_name, compose_path FROM deployed_apps WHERE app_id = 'llama-cpp' LIMIT 1"
    ).fetch_optional(&state.db).await.map_err(AppError::Database)?
     .ok_or_else(|| AppError::BadRequest("llama.cpp is not deployed".into()))?;

    let compose_path = std::path::PathBuf::from(&compose_path_str);
    let content = std::fs::read_to_string(&compose_path)
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut val: serde_json::Value = serde_yaml::from_str(&content)
        .map_err(|e| AppError::Internal(e.into()))?;

    // Update --model flag in command
    if let Some(services) = val.get_mut("services").and_then(|s| s.as_object_mut()) {
        for svc in services.values_mut() {
            if let Some(cmd) = svc.get("command").and_then(|c| c.as_str()).map(|s| s.to_string()) {
                let new_cmd = if cmd.contains("--model ") {
                    let parts: Vec<&str> = cmd.splitn(2, "--model ").collect();
                    let rest = parts[1].splitn(2, ' ').nth(1).map(|s| format!(" {s}")).unwrap_or_default();
                    format!("{}--model /models/{}{}", parts[0], req.filename, rest)
                } else {
                    format!("{} --model /models/{}", cmd.trim(), req.filename)
                };
                *svc.get_mut("command").unwrap() = serde_json::Value::String(new_cmd);
            }
        }
    }

    let new_content = serde_yaml::to_string(&val).map_err(|e| AppError::Internal(e.into()))?;
    std::fs::write(&compose_path, new_content).map_err(|e| AppError::Internal(e.into()))?;

    crate::containers::deploy_compose(&project_name, &compose_path)
        .await.map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true, "model": req.filename })))
}

// ─── Ollama pull ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct OllamaPullReq { pub model: String }

pub async fn start_ollama_pull(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<OllamaPullReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    // Allow letters, digits, colons (tags), hyphens, dots, underscores, slashes (registry)
    if req.model.is_empty() || !req.model.chars().all(|c| c.is_alphanumeric() || matches!(c, ':' | '-' | '.' | '_' | '/')) {
        return Err(AppError::BadRequest("Invalid model name".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    {
        let mut map = ollama_pulls().lock().unwrap();
        map.insert(id.clone(), OllamaPullState {
            id: id.clone(), model: req.model.clone(),
            status: "pulling".into(),
            current_layer: Some("Connecting to Ollama…".into()),
            total_bytes: None, pulled_bytes: None, error: None,
        });
    }

    let id2 = id.clone();
    let model = req.model.clone();
    tokio::spawn(async move {
        let result = do_ollama_pull(&id2, &model).await;
        let mut map = ollama_pulls().lock().unwrap();
        if let Some(entry) = map.get_mut(&id2) {
            match result {
                Ok(_)  => { entry.status = "done".into();  entry.current_layer = Some("Complete".into()); }
                Err(e) => { entry.status = "error".into(); entry.error = Some(e); }
            }
        }
    });

    Ok(Json(serde_json::json!({ "id": id })))
}

async fn do_ollama_pull(id: &str, model: &str) -> std::result::Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build().map_err(|e| e.to_string())?;

    let resp = client
        .post("http://localhost:11434/api/pull")
        .json(&serde_json::json!({ "name": model, "stream": true }))
        .send().await
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
            if line.is_empty() { continue; }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                let status_msg = v.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
                let total     = v.get("total").and_then(|n| n.as_u64());
                let completed = v.get("completed").and_then(|n| n.as_u64());

                {
                    let mut map = ollama_pulls().lock().unwrap();
                    if let Some(entry) = map.get_mut(id) {
                        entry.current_layer = Some(status_msg.clone());
                        if total.is_some() {
                            entry.total_bytes  = total;
                            entry.pulled_bytes = completed;
                        }
                    }
                }

                if status_msg == "success" { return Ok(()); }
            }
        }
    }

    Ok(())
}

pub async fn get_ollama_pull_status(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OllamaPullState>> {
    let map = ollama_pulls().lock().unwrap();
    map.get(&id).cloned().map(Json).ok_or(AppError::NotFound)
}

// ─── Ollama create (load GGUF into Ollama) ────────────────────────────────────

static OLLAMA_CREATES: OnceLock<Mutex<HashMap<String, OllamaPullState>>> = OnceLock::new();

fn ollama_creates() -> &'static Mutex<HashMap<String, OllamaPullState>> {
    OLLAMA_CREATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn gguf_to_ollama_name(filename: &str) -> String {
    filename
        .trim_end_matches(".gguf")
        .to_lowercase()
        .replace(' ', "-")
}

#[derive(Deserialize)]
pub struct OllamaCreateReq { pub filename: String }

pub async fn start_ollama_create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<OllamaCreateReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    if req.filename.contains('/') || req.filename.contains("..") || !req.filename.ends_with(".gguf") {
        return Err(AppError::BadRequest("Invalid filename".into()));
    }

    // Verify the file exists
    let dir = models_dir(&state).await;
    if !dir.join(&req.filename).exists() {
        return Err(AppError::NotFound);
    }

    let model_name = gguf_to_ollama_name(&req.filename);
    let id = uuid::Uuid::new_v4().to_string();

    {
        let mut map = ollama_creates().lock().unwrap();
        map.insert(id.clone(), OllamaPullState {
            id: id.clone(), model: model_name.clone(),
            status: "pulling".into(),
            current_layer: Some("Sending to Ollama…".into()),
            total_bytes: None, pulled_bytes: None, error: None,
        });
    }

    let id2 = id.clone();
    let filename = req.filename.clone();
    let model_name_spawn = model_name.clone();
    tokio::spawn(async move {
        let result = do_ollama_create(&id2, &filename, &model_name_spawn).await;
        let mut map = ollama_creates().lock().unwrap();
        if let Some(entry) = map.get_mut(&id2) {
            match result {
                Ok(_)  => { entry.status = "done".into();  entry.current_layer = Some("Complete".into()); }
                Err(e) => { entry.status = "error".into(); entry.error = Some(e); }
            }
        }
    });

    Ok(Json(serde_json::json!({ "id": id, "model_name": model_name })))
}

async fn do_ollama_create(id: &str, filename: &str, model_name: &str) -> std::result::Result<(), String> {
    let modelfile = format!("FROM /vt-models/{filename}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(7200))
        .build().map_err(|e| e.to_string())?;

    let resp = client
        .post("http://localhost:11434/api/create")
        .json(&serde_json::json!({ "name": model_name, "modelfile": modelfile, "stream": true }))
        .send().await
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
            if line.is_empty() { continue; }

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                    return Err(err.to_string());
                }
                let status_msg = v.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string();
                let total     = v.get("total").and_then(|n| n.as_u64());
                let completed = v.get("completed").and_then(|n| n.as_u64());

                {
                    let mut map = ollama_creates().lock().unwrap();
                    if let Some(entry) = map.get_mut(id) {
                        entry.current_layer = Some(status_msg.clone());
                        if total.is_some() {
                            entry.total_bytes  = total;
                            entry.pulled_bytes = completed;
                        }
                    }
                }

                if status_msg == "success" { return Ok(()); }
            }
        }
    }

    Ok(())
}

pub async fn get_ollama_create_status(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<OllamaPullState>> {
    let map = ollama_creates().lock().unwrap();
    map.get(&id).cloned().map(Json).ok_or(AppError::NotFound)
}
