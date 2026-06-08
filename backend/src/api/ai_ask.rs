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

    // Build user message with optional panel context
    let user_msg = if let Some(ctx) = &req.context {
        if ctx.is_empty() { req.query.clone() }
        else { format!("[Focused panel: {}]\n{}", ctx, req.query) }
    } else {
        req.query.clone()
    };

    // Compact VoidTower system prompt so Odysseus understands the codebase
    let system_prompt =
        "You are Odysseus, the AI assistant embedded in VoidTower — a self-hosted \
infrastructure control plane.\n\n\
VoidTower stack: Rust (axum 0.7, sqlx/SQLite) backend at port 8743; \
React 18 + TypeScript + Vite frontend; Zustand state; xterm.js terminals.\n\n\
Key API patterns:\n\
• All handlers: `pub async fn h(State(s): State<AppState>, jar: CookieJar, Json(r): Json<Req>) -> Result<Json<Value>>`\n\
• Auth guard: `auth::validate_session(&s.db, &sid).await?.ok_or(Unauthorized)?`\n\
• DB query: `sqlx::query_as::<_, T>(\"SELECT …\").fetch_all(&s.db).await.map_err(AppError::Database)?`\n\
• Errors: `AppError::NotFound | BadRequest(msg) | Forbidden | Internal(anyhow)`\n\
• Alerts: `api::alerts::create_alert(&db, title, msg, severity, category, res_type, res_id).await`\n\n\
Key files:\n\
• Routes: backend/src/api/mod.rs\n\
• State: backend/src/main.rs (AppState)\n\
• Sidebar nav: frontend/src/components/layout/Sidebar.tsx\n\
• AIOS panels registry: frontend/src/aios/AiosLayout.tsx\n\
• API client: frontend/src/api/client.ts\n\n\
MCP tools available: list_nodes, get_node_metrics, list_containers, list_services, \
list_alerts, get_container_logs, list_routes, read_file, search_code, get_template.\n\
Use read_file / search_code to look up exact current implementations before generating code.";

    // Build upstream request body (OpenAI-compatible chat completions)
    let body = serde_json::json!({
        "model": "default",
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user",   "content": user_msg }
        ],
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
