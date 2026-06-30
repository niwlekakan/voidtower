use crate::{
    ai::{AiMessage, AiOrchestrator, AiRequest},
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
    /// Optional: pin to a specific provider by id.
    pub provider_id: Option<String>,
}

pub async fn ask(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AskRequest>,
) -> Result<Response> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    let user_msg = match &req.context {
        Some(ctx) if !ctx.is_empty() => format!("[Focused panel: {}]\n{}", ctx, req.query),
        _ => req.query.clone(),
    };

    let mut ai_req = AiRequest::new(vec![AiMessage {
        role: "user".into(),
        content: user_msg,
    }]);
    ai_req.system_prompt = Some(voidtower_system_prompt());

    if let Some(pid) = &req.provider_id {
        ai_req.context = Some(serde_json::json!({ "provider_id": pid }));
    }

    let orchestrator = AiOrchestrator::new(state.db.clone());

    // Try the multi-provider orchestrator first.
    // If no providers are configured, fall back to the legacy Odysseus path.
    match orchestrator.stream(&ai_req).await {
        Ok((_provider_id, upstream_res)) => {
            let status = upstream_res.status();
            let content_type = upstream_res
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("text/event-stream")
                .to_string();

            let body = Body::from_stream(upstream_res.bytes_stream());
            let response = Response::builder()
                .status(status.as_u16())
                .header("content-type", content_type)
                .header("cache-control", "no-cache")
                .header("x-accel-buffering", "no")
                .body(body)
                .map_err(|e| AppError::Internal(e.into()))?;
            Ok(response)
        }
        Err(AppError::BadRequest(_)) => {
            // No providers configured — try legacy Odysseus settings key
            legacy_odysseus_fallback(&state, &ai_req).await
        }
        Err(e) => Err(e),
    }
}

/// Legacy path: read `odysseus.allowed_url` from settings and proxy directly.
/// Kept for backwards compatibility with existing Odysseus integrations that
/// predate the provider abstraction.
async fn legacy_odysseus_fallback(state: &AppState, ai_req: &AiRequest) -> Result<Response> {
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
            Json(serde_json::json!({
                "error": "No AI providers configured. Add one at Settings → AI Providers, or configure the Odysseus integration."
            })),
        )
            .into_response());
    }

    let messages: Vec<serde_json::Value> = {
        let mut msgs = Vec::new();
        if let Some(sys) = &ai_req.system_prompt {
            msgs.push(serde_json::json!({ "role": "system", "content": sys }));
        }
        for m in &ai_req.messages {
            msgs.push(serde_json::json!({ "role": m.role, "content": m.content }));
        }
        msgs
    };

    let body = serde_json::json!({
        "model": "default",
        "messages": messages,
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

    let response = Response::builder()
        .status(status.as_u16())
        .header("content-type", content_type)
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(upstream_res.bytes_stream()))
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(response)
}

fn voidtower_system_prompt() -> String {
    "You are an AI assistant embedded in VoidTower — a self-hosted \
infrastructure control plane.\n\n\
VoidTower stack: Rust (axum 0.7, sqlx/SQLite) backend at port 8743; \
React 18 + TypeScript + Vite frontend; Zustand state; xterm.js terminals.\n\n\
Key API patterns:\n\
• All handlers: `pub async fn h(State(s): State<AppState>, jar: CookieJar, Json(r): Json<Req>) -> Result<Json<Value>>`\n\
• Auth guard: `auth::validate_session(&s.db, &sid).await?.ok_or(Unauthorized)?`\n\
• DB query: `sqlx::query_as::<_, T>(\"SELECT …\").fetch_all(&s.db).await.map_err(AppError::Database)?`\n\
• Errors: `AppError::NotFound | BadRequest(msg) | Forbidden | Internal(anyhow)`\n\n\
Key files:\n\
• Routes: backend/src/api/mod.rs\n\
• State: backend/src/main.rs (AppState)\n\
• Sidebar nav: frontend/src/components/layout/Sidebar.tsx\n\
• AIOS panels registry: frontend/src/aios/AiosLayout.tsx\n\
• API client: frontend/src/api/client.ts\n\
• AI providers: backend/src/ai/ (AiProvider trait + provider adapters)\n\n\
MCP tools (odysseus-mcp-servers/voidtower_server.py — usable from Odysseus, Claude Desktop, Open WebUI, Cursor):\n\
Read: vt_get_metrics, vt_list_services, vt_get_service_logs, vt_list_containers, vt_get_container_logs, \
vt_list_alerts, vt_get_status_summary, vt_list_status_checks, vt_list_deployed_apps, vt_list_app_catalog, \
vt_get_app_status, vt_get_app_logs, vt_list_backups, vt_list_automations, vt_get_timeline, \
vt_get_audit_log, vt_list_proxies, vt_list_firewall_rules, vt_list_wireguard_peers, \
vt_get_storage, vt_get_network_neighbors, vt_list_vms, vt_list_secrets, vt_list_tags, \
vt_list_users, vt_run_diagnostics, vt_get_capabilities.\n\
Write: vt_control_service, vt_control_container, vt_control_app, vt_deploy_app, \
vt_toggle_proxy, vt_create_proxy, vt_run_backup, vt_run_automation_job, \
vt_control_vm, vt_acknowledge_alert, vt_resolve_alert."
        .to_string()
}
