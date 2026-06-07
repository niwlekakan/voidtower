use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct AskRequest {
    pub query: String,
    pub context: Option<String>,
}

pub async fn ask(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AskRequest>,
) -> Result<Response> {
    // Auth: validate session cookie
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    // Read the Odysseus URL from settings
    let odysseus_url = sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = ?",
    )
    .bind("odysseus.allowed_url")
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .unwrap_or_default();

    if odysseus_url.is_empty() {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Odysseus not configured" })),
        )
            .into_response());
    }

    // Build prompt with optional panel context
    let prompt = if let Some(ctx) = &req.context {
        if ctx.is_empty() {
            req.query.clone()
        } else {
            format!("[Context: {}]\n{}", ctx, req.query)
        }
    } else {
        req.query.clone()
    };

    // Build upstream request body (OpenAI-compatible chat completions)
    let body = serde_json::json!({
        "model": "default",
        "messages": [{ "role": "user", "content": prompt }],
        "stream": true,
    });

    let upstream_url = format!("{}/api/chat/completions", odysseus_url.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let upstream_res = client
        .post(&upstream_url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let status = upstream_res.status();
    let content_type = upstream_res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/event-stream")
        .to_string();

    // Stream the upstream body back to the client
    let stream = upstream_res.bytes_stream();
    let axum_body = Body::from_stream(stream);

    let response = Response::builder()
        .status(status.as_u16())
        .header("content-type", content_type)
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(axum_body)
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(response)
}
