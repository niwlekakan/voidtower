use crate::{auth, error::{AppError, Result}, AppState};
use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::{SystemTime, UNIX_EPOCH}};
use tokio::fs;

fn now_millis() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()
}

// ── auth ──────────────────────────────────────────────────────────────────────

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

// ── dirs ──────────────────────────────────────────────────────────────────────

fn images_dir(state: &AppState) -> PathBuf { state.config.data_dir.join("studio").join("images") }
fn audio_dir(state:  &AppState) -> PathBuf { state.config.data_dir.join("studio").join("audio")  }

// ── service probing ───────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct StudioService {
    pub name:    String,
    pub kind:    String,   // "image" | "tts" | "stt" | "chat"
    pub url:     String,
    pub status:  String,   // "online" | "offline"
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct GpuSummary {
    pub name:           String,
    pub vram_used_mb:   u64,
    pub vram_total_mb:  u64,
    pub utilization_pct: u64,
}

#[derive(Serialize)]
pub struct StudioStatus {
    pub services: Vec<StudioService>,
    pub gpu:      Option<GpuSummary>,
}

async fn probe(url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .danger_accept_invalid_certs(true)
        .build()
        .ok()?;
    let r = client.get(url).send().await.ok()?;
    if r.status().is_success() || r.status().as_u16() == 422 {
        Some(r.text().await.unwrap_or_default())
    } else {
        None
    }
}

fn gpu_summary() -> Option<GpuSummary> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.used,memory.total,utilization.gpu", "--format=csv,noheader,nounits"])
        .output().ok()?;
    if !out.status.success() { return None; }
    let line = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = line.trim().splitn(4, ',').map(|s| s.trim()).collect();
    if parts.len() < 4 { return None; }
    Some(GpuSummary {
        name:             parts[0].to_string(),
        vram_used_mb:     parts[1].parse().unwrap_or(0),
        vram_total_mb:    parts[2].parse().unwrap_or(0),
        utilization_pct:  parts[3].parse().unwrap_or(0),
    })
}

pub async fn status(State(state): State<AppState>, jar: CookieJar) -> Result<Json<StudioStatus>> {
    require_user(&state, &jar).await?;

    let checks: &[(&str, &str, &str, &str)] = &[
        ("Stable Diffusion WebUI", "image",   "http://localhost:7860", "http://localhost:7860/sdapi/v1/options"),
        ("ComfyUI",                "image",   "http://localhost:8188", "http://localhost:8188/system_stats"),
        ("Kokoro TTS",             "tts",     "http://localhost:8880", "http://localhost:8880/health"),
        ("Whisper",                "stt",     "http://localhost:9000", "http://localhost:9000/health"),
        ("Ollama",                 "chat",    "http://localhost:11434","http://localhost:11434/api/version"),
    ];

    let mut services = Vec::with_capacity(checks.len());
    for (name, kind, base_url, probe_url) in checks {
        let body   = probe(probe_url).await;
        let online = body.is_some();
        let version = body.and_then(|b| {
            let v: serde_json::Value = serde_json::from_str(&b).ok()?;
            v.get("version").and_then(|v| v.as_str()).map(|s| s.to_string())
        });
        services.push(StudioService {
            name:    name.to_string(),
            kind:    kind.to_string(),
            url:     base_url.to_string(),
            status:  if online { "online".into() } else { "offline".into() },
            version,
        });
    }

    Ok(Json(StudioStatus { services, gpu: gpu_summary() }))
}

// ── image generation ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ImageGenRequest {
    pub prompt:          String,
    pub negative_prompt: Option<String>,
    pub width:           Option<u32>,
    pub height:          Option<u32>,
    pub steps:           Option<u32>,
    pub cfg_scale:       Option<f64>,
    pub seed:            Option<i64>,
    pub backend:         Option<String>, // "sdwebui" | "comfyui"
}

#[derive(Serialize)]
pub struct ImageGenResponse {
    pub ok:       bool,
    pub filename: String,
    pub url:      String,
}

pub async fn image_generate(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ImageGenRequest>,
) -> Result<Json<ImageGenResponse>> {
    require_user(&state, &jar).await?;

    let backend = req.backend.as_deref().unwrap_or("auto");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;

    // ComfyUI path
    if backend == "comfyui" || (backend == "auto" && probe("http://localhost:8188/system_stats").await.is_some() && probe("http://localhost:7860/sdapi/v1/options").await.is_none()) {
        return image_generate_comfyui(&state, &client, req).await;
    }

    // SD WebUI path (default)
    image_generate_sdwebui(&state, &client, req).await
}

async fn image_generate_sdwebui(
    state: &AppState,
    client: &reqwest::Client,
    req: ImageGenRequest,
) -> Result<Json<ImageGenResponse>> {
    let body = serde_json::json!({
        "prompt":          req.prompt,
        "negative_prompt": req.negative_prompt.unwrap_or_default(),
        "width":           req.width.unwrap_or(512),
        "height":          req.height.unwrap_or(512),
        "steps":           req.steps.unwrap_or(20),
        "cfg_scale":       req.cfg_scale.unwrap_or(7.0),
        "seed":            req.seed.unwrap_or(-1),
        "sampler_name":    "DPM++ 2M Karras",
    });

    let resp = client
        .post("http://localhost:7860/sdapi/v1/txt2img")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("SD WebUI unreachable: {e}")))?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("SD WebUI error: {msg}")));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Internal(e.into()))?;

    let b64 = data["images"].as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No image in SD WebUI response")))?;

    save_image(state, b64).await
}

async fn image_generate_comfyui(
    state: &AppState,
    client: &reqwest::Client,
    req: ImageGenRequest,
) -> Result<Json<ImageGenResponse>> {
    let seed = req.seed.unwrap_or_else(|| rand::random::<i32>() as i64 & 0x7FFFFFFF);

    // Minimal txt2img workflow — works with any loaded checkpoint
    let workflow = serde_json::json!({
        "4": {"inputs": {"ckpt_name": "v1-5-pruned-emaonly.safetensors"}, "class_type": "CheckpointLoaderSimple"},
        "5": {"inputs": {"width": req.width.unwrap_or(512), "height": req.height.unwrap_or(512), "batch_size": 1}, "class_type": "EmptyLatentImage"},
        "6": {"inputs": {"text": req.prompt, "clip": ["4", 1]}, "class_type": "CLIPTextEncode"},
        "7": {"inputs": {"text": req.negative_prompt.unwrap_or_default(), "clip": ["4", 1]}, "class_type": "CLIPTextEncode"},
        "3": {
            "inputs": {
                "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0],
                "latent_image": ["5", 0], "seed": seed,
                "steps": req.steps.unwrap_or(20), "cfg": req.cfg_scale.unwrap_or(7.0),
                "sampler_name": "euler_ancestral", "scheduler": "normal", "denoise": 1.0,
            },
            "class_type": "KSampler",
        },
        "8": {"inputs": {"samples": ["3", 0], "vae": ["4", 2]}, "class_type": "VAEDecode"},
        "9": {"inputs": {"images": ["8", 0], "filename_prefix": "VoidTower"}, "class_type": "SaveImage"},
    });

    let resp = client
        .post("http://localhost:8188/prompt")
        .json(&serde_json::json!({ "prompt": workflow }))
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ComfyUI unreachable: {e}")))?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("ComfyUI error: {msg}")));
    }

    let queued: serde_json::Value = resp.json().await.map_err(|e| AppError::Internal(e.into()))?;
    let prompt_id = queued["prompt_id"].as_str()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No prompt_id from ComfyUI")))?
        .to_string();

    // Poll history until done (max 120s)
    let history_url = format!("http://localhost:8188/history/{prompt_id}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if std::time::Instant::now() > deadline {
            return Err(AppError::Internal(anyhow::anyhow!("ComfyUI timed out")));
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let h = client.get(&history_url).send().await
            .map_err(|e| AppError::Internal(e.into()))?;
        let hdata: serde_json::Value = h.json().await.map_err(|e| AppError::Internal(e.into()))?;
        if hdata.get(&prompt_id).is_none() { continue; }
        let outputs = &hdata[&prompt_id]["outputs"];
        if outputs.is_null() { continue; }

        // Find first image in outputs
        for (_node_id, node_out) in outputs.as_object().unwrap_or(&serde_json::Map::new()) {
            if let Some(imgs) = node_out["images"].as_array() {
                if let Some(img) = imgs.first() {
                    let filename  = img["filename"].as_str().unwrap_or_default();
                    let subfolder = img["subfolder"].as_str().unwrap_or_default();
                    let kind      = img["type"].as_str().unwrap_or("output");
                    // Fetch image bytes from ComfyUI
                    let img_url = format!(
                        "http://localhost:8188/view?filename={filename}&subfolder={subfolder}&type={kind}"
                    );
                    let img_resp = client.get(&img_url).send().await
                        .map_err(|e| AppError::Internal(e.into()))?;
                    let bytes = img_resp.bytes().await.map_err(|e| AppError::Internal(e.into()))?;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    return save_image(state, &b64).await;
                }
            }
        }
    }
}

async fn save_image(state: &AppState, b64: &str) -> Result<Json<ImageGenResponse>> {
    // Strip data-URL prefix if present
    let raw = if let Some(pos) = b64.find(',') { &b64[pos + 1..] } else { b64 };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("base64 decode: {e}")))?;

    let dir = images_dir(state);
    fs::create_dir_all(&dir).await.map_err(|e| AppError::Internal(e.into()))?;

    let filename = format!("{}.png", now_millis());
    let path = dir.join(&filename);
    fs::write(&path, &bytes).await.map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(ImageGenResponse {
        ok:       true,
        filename: filename.clone(),
        url:      format!("/api/studio/images/{filename}"),
    }))
}

pub async fn serve_image(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(filename): Path<String>,
) -> Result<Response> {
    require_user(&state, &jar).await?;
    serve_file(images_dir(&state).join(sanitize(&filename)), "image/png").await
}

// ── TTS ───────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TtsRequest {
    pub text:  String,
    pub voice: Option<String>,
    pub speed: Option<f64>,
}

#[derive(Serialize)]
pub struct TtsResponse {
    pub ok:       bool,
    pub filename: String,
    pub url:      String,
}

pub async fn tts_generate(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<TtsRequest>,
) -> Result<Json<TtsResponse>> {
    require_user(&state, &jar).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;

    let body = serde_json::json!({
        "model":  "kokoro",
        "input":  req.text,
        "voice":  req.voice.as_deref().unwrap_or("af_heart"),
        "speed":  req.speed.unwrap_or(1.0),
        "response_format": "wav",
    });

    let resp = client
        .post("http://localhost:8880/v1/audio/speech")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Kokoro TTS unreachable: {e}")))?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Kokoro TTS error: {msg}")));
    }

    let bytes = resp.bytes().await.map_err(|e| AppError::Internal(e.into()))?;

    let dir = audio_dir(&state);
    fs::create_dir_all(&dir).await.map_err(|e| AppError::Internal(e.into()))?;

    let filename = format!("{}.wav", now_millis());
    let path = dir.join(&filename);
    fs::write(&path, &bytes).await.map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(TtsResponse {
        ok:       true,
        filename: filename.clone(),
        url:      format!("/api/studio/audio/{filename}"),
    }))
}

pub async fn serve_audio(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(filename): Path<String>,
) -> Result<Response> {
    require_user(&state, &jar).await?;
    let fname = sanitize(&filename);
    let content_type = if fname.ends_with(".mp3") { "audio/mpeg" } else { "audio/wav" };
    serve_file(audio_dir(&state).join(fname), content_type).await
}

// ── STT ───────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SttResponse {
    pub ok:   bool,
    pub text: String,
}

pub async fn stt_transcribe(
    State(state): State<AppState>,
    jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Json<SttResponse>> {
    require_user(&state, &jar).await?;

    let mut file_bytes: Option<Vec<u8>>  = None;
    let mut filename:   String           = "audio.wav".into();
    let mut language:   Option<String>   = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::Internal(e.into()))? {
        match field.name() {
            Some("file") => {
                if let Some(n) = field.file_name() { filename = n.to_string(); }
                file_bytes = Some(field.bytes().await.map_err(|e| AppError::Internal(e.into()))?.to_vec());
            }
            Some("language") => {
                language = Some(field.text().await.map_err(|e| AppError::Internal(e.into()))?);
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or_else(|| AppError::BadRequest("Missing file field".into()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;

    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename)
        .mime_str("audio/wav")
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1");

    if let Some(lang) = language {
        if !lang.is_empty() && lang != "auto" {
            form = form.text("language", lang);
        }
    }

    let resp = client
        .post("http://localhost:9000/v1/audio/transcriptions")
        .multipart(form)
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Whisper unreachable: {e}")))?;

    if !resp.status().is_success() {
        let msg = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!("Whisper error: {msg}")));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| AppError::Internal(e.into()))?;
    let text = data["text"].as_str().unwrap_or("").to_string();

    Ok(Json(SttResponse { ok: true, text }))
}

// ── gallery ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GalleryItem {
    pub id:          String,
    pub kind:        String, // "image" | "audio"
    pub filename:    String,
    pub url:         String,
    pub created_at:  i64,
    pub size_bytes:  u64,
}

pub async fn gallery_list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<GalleryItem>>> {
    require_user(&state, &jar).await?;

    let mut items: Vec<GalleryItem> = Vec::new();

    for (kind, dir, url_prefix) in &[
        ("image", images_dir(&state), "/api/studio/images/"),
        ("audio", audio_dir(&state),  "/api/studio/audio/"),
    ] {
        if !dir.exists() { continue; }
        let mut rd = match fs::read_dir(dir).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            let fname = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            let meta = match fs::metadata(&path).await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let created_at = meta.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            items.push(GalleryItem {
                id:         fname.clone(),
                kind:       kind.to_string(),
                filename:   fname.clone(),
                url:        format!("{url_prefix}{fname}"),
                created_at,
                size_bytes: meta.len(),
            });
        }
    }

    items.sort_by_key(|i| std::cmp::Reverse(i.created_at));
    Ok(Json(items))
}

pub async fn gallery_delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((kind, filename)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;

    let dir = match kind.as_str() {
        "image" => images_dir(&state),
        "audio" => audio_dir(&state),
        _ => return Err(AppError::BadRequest("Invalid kind".into())),
    };

    let path = dir.join(sanitize(&filename));
    if path.exists() {
        fs::remove_file(&path).await.map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn sanitize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect()
}

async fn serve_file(path: PathBuf, content_type: &'static str) -> Result<Response> {
    let bytes = fs::read(&path).await.map_err(|_| AppError::NotFound)?;
    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE,              content_type),
            (header::CONTENT_DISPOSITION, Box::leak(
                format!("inline; filename=\"{fname}\"").into_boxed_str()
            )),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        Body::from(bytes),
    ).into_response())
}
