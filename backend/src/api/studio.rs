use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Body,
    extract::{FromRequest, Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

// ── auth ──────────────────────────────────────────────────────────────────────

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

// ── dirs ──────────────────────────────────────────────────────────────────────

fn images_dir(state: &AppState) -> PathBuf {
    state.config.data_dir.join("studio").join("images")
}
fn audio_dir(state: &AppState) -> PathBuf {
    state.config.data_dir.join("studio").join("audio")
}

// ── service probing ───────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct StudioService {
    pub name: String,
    pub kind: String, // "image" | "tts" | "stt" | "chat"
    pub url: String,
    pub status: String, // "online" | "offline"
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct GpuSummary {
    pub name: String,
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
    pub utilization_pct: u64,
}

#[derive(Serialize)]
pub struct StudioStatus {
    pub services: Vec<StudioService>,
    pub gpu: Option<GpuSummary>,
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
        .args([
            "--query-gpu=name,memory.used,memory.total,utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = line.trim().splitn(4, ',').map(|s| s.trim()).collect();
    if parts.len() < 4 {
        return None;
    }
    Some(GpuSummary {
        name: parts[0].to_string(),
        vram_used_mb: parts[1].parse().unwrap_or(0),
        vram_total_mb: parts[2].parse().unwrap_or(0),
        utilization_pct: parts[3].parse().unwrap_or(0),
    })
}

pub async fn status(State(state): State<AppState>, jar: CookieJar) -> Result<Json<StudioStatus>> {
    require_user(&state, &jar).await?;

    let checks: &[(&str, &str, &str, &str)] = &[
        (
            "Stable Diffusion WebUI",
            "image",
            "http://localhost:7860",
            "http://localhost:7860/sdapi/v1/options",
        ),
        (
            "ComfyUI",
            "image",
            "http://localhost:8188",
            "http://localhost:8188/system_stats",
        ),
        (
            "Kokoro TTS",
            "tts",
            "http://localhost:8880",
            "http://localhost:8880/health",
        ),
        (
            "Whisper",
            "stt",
            "http://localhost:9000",
            "http://localhost:9000/health",
        ),
        (
            "Ollama",
            "chat",
            "http://localhost:11434",
            "http://localhost:11434/api/version",
        ),
    ];

    let mut services = Vec::with_capacity(checks.len());
    for (name, kind, base_url, probe_url) in checks {
        let body = probe(probe_url).await;
        let online = body.is_some();
        let version = body.and_then(|b| {
            let v: serde_json::Value = serde_json::from_str(&b).ok()?;
            v.get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
        services.push(StudioService {
            name: name.to_string(),
            kind: kind.to_string(),
            url: base_url.to_string(),
            status: if online {
                "online".into()
            } else {
                "offline".into()
            },
            version,
        });
    }

    Ok(Json(StudioStatus {
        services,
        gpu: gpu_summary(),
    }))
}

// ── image generation ──────────────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct ImageGenRequest {
    pub prompt: String,
    pub negative_prompt: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub steps: Option<u32>,
    pub cfg_scale: Option<f64>,
    pub seed: Option<i64>,
    pub backend: Option<String>, // "sdwebui" | "comfyui"
}

#[derive(Serialize)]
pub struct ImageGenResponse {
    pub ok: bool,
    pub filename: String,
    pub url: String,
}

pub async fn image_generate(
    State(state): State<AppState>,
    jar: CookieJar,
    _request: Request,
) -> Result<Json<ImageGenResponse>> {
    require_user(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "AI image generation requires a canonical operation adapter".into(),
    ))
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

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct TtsRequest {
    pub text: String,
    pub voice: Option<String>,
    pub speed: Option<f64>,
}

#[derive(Serialize)]
pub struct TtsResponse {
    pub ok: bool,
    pub filename: String,
    pub url: String,
}

pub async fn tts_generate(
    State(state): State<AppState>,
    jar: CookieJar,
    _request: Request,
) -> Result<Json<TtsResponse>> {
    require_user(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "AI speech generation requires a canonical operation adapter".into(),
    ))
}

pub async fn serve_audio(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(filename): Path<String>,
) -> Result<Response> {
    require_user(&state, &jar).await?;
    let fname = sanitize(&filename);
    let content_type = if fname.ends_with(".mp3") {
        "audio/mpeg"
    } else {
        "audio/wav"
    };
    serve_file(audio_dir(&state).join(fname), content_type).await
}

// ── STT ───────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SttResponse {
    pub ok: bool,
    pub text: String,
}

pub async fn stt_transcribe(
    State(state): State<AppState>,
    jar: CookieJar,
    _request: Request,
) -> Result<Json<SttResponse>> {
    require_user(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "AI speech transcription requires a canonical operation adapter".into(),
    ))
}

// ── gallery ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct GalleryItem {
    pub id: String,
    pub kind: String, // "image" | "audio"
    pub filename: String,
    pub url: String,
    pub created_at: i64,
    pub size_bytes: u64,
}

pub async fn gallery_list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<GalleryItem>>> {
    require_user(&state, &jar).await?;

    let mut items: Vec<GalleryItem> = Vec::new();

    for (kind, dir, url_prefix) in &[
        ("image", images_dir(&state), "/api/studio/images/"),
        ("audio", audio_dir(&state), "/api/studio/audio/"),
    ] {
        if !dir.exists() {
            continue;
        }
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
            let created_at = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            items.push(GalleryItem {
                id: fname.clone(),
                kind: kind.to_string(),
                filename: fname.clone(),
                url: format!("{url_prefix}{fname}"),
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
        fs::remove_file(&path)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn sanitize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect()
}

// ── MCP tool panel ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpInvokeRequest {
    pub name: String,
    pub arguments: serde_json::Value,
}

pub async fn mcp_tools(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    Ok(Json(super::mcp::tools_json()))
}

pub async fn mcp_invoke(
    State(state): State<AppState>,
    jar: CookieJar,
    request: Request,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let Json(req) = Json::<McpInvokeRequest>::from_request(request, &state)
        .await
        .map_err(|rejection| AppError::RequestBody {
            status: rejection.status(),
        })?;
    match super::mcp::invoke_tool(
        &state,
        crate::operations::invocation::CredentialContext::Studio {
            user_id: user.id,
            role: user.role,
        },
        &req.name,
        req.arguments,
    )
    .await
    {
        Ok(text) => Ok(Json(serde_json::json!({ "ok": true, "result": text }))),
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "error": e }))),
    }
}

// ── file serving ──────────────────────────────────────────────────────────────

async fn serve_file(path: PathBuf, content_type: &'static str) -> Result<Response> {
    let bytes = fs::read(&path).await.map_err(|_| AppError::NotFound)?;
    let fname = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string();
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CONTENT_DISPOSITION,
                Box::leak(format!("inline; filename=\"{fname}\"").into_boxed_str()),
            ),
            (header::CACHE_CONTROL, "private, max-age=3600"),
        ],
        Body::from(bytes),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::Cookie;

    /// The Studio MCP panel (`mcp_invoke`) is session-authenticated, unlike the
    /// bearer-token JSON-RPC dispatch in `api/mcp.rs`, but it delegates to the
    /// exact same `mcp::invoke_tool` choke point — this proves that ingress is
    /// covered by the same redaction, not a reimplementation of it.
    #[tokio::test]
    async fn redaction_corpus_never_appears_in_studio_mcp_invoke_output() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session_id = crate::api::mcp::test_support::user_with_session(&pool).await;

        let secret = "fakevendor_51H8x9K2eZvKYlo2CxpqrstuvWXYZ";
        sqlx::query(
            "INSERT INTO alerts (id, title, message, severity, category, state, created_at, updated_at) \
             VALUES ('a1', 'leaky app', ?, 'warning', 'general', 'active', 0, 0)",
        )
        .bind(format!("Startup banner: api_key={secret}"))
        .execute(&pool)
        .await
        .unwrap();

        let state = crate::api::mcp::test_support::build(pool);
        let jar = CookieJar::new().add(Cookie::new("vt_session", session_id));

        let resp = mcp_invoke(
            State(state),
            jar,
            Request::builder()
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "name": "list_alerts",
                        "arguments": {}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

        let body = resp.0;
        assert_eq!(body["ok"], serde_json::json!(true));
        let text = body["result"].as_str().unwrap();
        assert!(
            !text.contains(secret),
            "corpus secret leaked into studio mcp_invoke output"
        );
        assert!(text.contains("leaky app"));
    }
}
