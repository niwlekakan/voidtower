use crate::{
    auth,
    containers,
    error::AppError,
    services,
    AppState,
};
use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

fn ok_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn err_response(id: Option<Value>, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError { code, message: message.into() }),
    }
}

// ---------------------------------------------------------------------------
// Auth helpers
// ---------------------------------------------------------------------------

async fn get_setting(state: &AppState, key: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

async fn check_mcp_auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    // Check mcp_enabled setting
    if get_setting(state, "odysseus.mcp_enabled").await != "true" {
        return Err(StatusCode::FORBIDDEN);
    }

    // Require a valid Bearer token
    let raw_token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    auth::validate_api_token_any(&state.db, raw_token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// SSE endpoint — GET /api/mcp
// ---------------------------------------------------------------------------

pub async fn sse_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if let Err(status) = check_mcp_auth(&state, &headers).await {
        return (status, "").into_response();
    }

    // Send the endpoint event then a keepalive comment and close.
    // Simple implementation: no persistent connection needed for tool use.
    let body = "event: endpoint\ndata: /api/mcp/message\n\n:\n\n";

    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/event-stream")
        .header("Cache-Control", "no-cache")
        .header("X-Accel-Buffering", "no")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

// ---------------------------------------------------------------------------
// Message endpoint — POST /api/mcp/message
// ---------------------------------------------------------------------------

pub async fn message_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    if let Err(status) = check_mcp_auth(&state, &headers).await {
        let code = if status == StatusCode::UNAUTHORIZED { -32001 } else { -32003 };
        let msg = if status == StatusCode::UNAUTHORIZED { "Unauthorized" } else { "MCP is not enabled" };
        return (status, Json(err_response(req.id, code, msg)));
    }

    let resp = dispatch(&state, req).await;
    (StatusCode::OK, Json(resp))
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

async fn dispatch(state: &AppState, req: JsonRpcRequest) -> JsonRpcResponse {
    let id = req.id.clone();
    match req.method.as_str() {
        "initialize" => handle_initialize(id),
        "tools/list" => handle_tools_list(id),
        "tools/call" => handle_tools_call(state, id, req.params).await,
        _ => err_response(id, -32601, "Method not found"),
    }
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    ok_response(id, serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "voidtower", "version": "0.1.0" }
    }))
}

// ---------------------------------------------------------------------------
// tools/list
// ---------------------------------------------------------------------------

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    ok_response(id, serde_json::json!({
        "tools": [
            {
                "name": "list_nodes",
                "description": "List all VoidTower nodes with health status",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_node_metrics",
                "description": "Get current CPU/RAM/disk metrics for the local node",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_containers",
                "description": "List all Docker containers with status",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_services",
                "description": "List systemd services with active state",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "list_alerts",
                "description": "List active alerts",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "get_container_logs",
                "description": "Get recent logs for a container",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "container_id": { "type": "string" }
                    },
                    "required": ["container_id"]
                }
            }
        ]
    }))
}

// ---------------------------------------------------------------------------
// tools/call
// ---------------------------------------------------------------------------

async fn handle_tools_call(state: &AppState, id: Option<Value>, params: Value) -> JsonRpcResponse {
    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => return err_response(id, -32602, "Missing tool name in params"),
    };
    let args = params.get("arguments").cloned().unwrap_or(serde_json::json!({}));

    let result = match tool_name.as_str() {
        "list_nodes"        => tool_list_nodes(state).await,
        "get_node_metrics"  => tool_get_node_metrics(state).await,
        "list_containers"   => tool_list_containers().await,
        "list_services"     => tool_list_services().await,
        "list_alerts"       => tool_list_alerts(state).await,
        "get_container_logs" => tool_get_container_logs(args).await,
        _ => Err(format!("Unknown tool: {tool_name}")),
    };

    match result {
        Ok(text) => ok_response(id, serde_json::json!({
            "content": [{ "type": "text", "text": text }]
        })),
        Err(e) => ok_response(id, serde_json::json!({
            "content": [{ "type": "text", "text": format!("Error: {e}") }],
            "isError": true
        })),
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn tool_list_nodes(state: &AppState) -> Result<String, String> {
    // Return the local node; cluster peers can be added when the cluster module exposes them.
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| {
            std::fs::read_to_string("/etc/hostname")
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|_| "local".to_string());

    let metrics_opt = state.latest_metrics.read().await.clone();
    let status = if metrics_opt.is_some() { "healthy" } else { "unknown" };

    let node = serde_json::json!([{
        "id": "local",
        "hostname": hostname,
        "role": "primary",
        "status": status
    }]);

    serde_json::to_string(&node).map_err(|e| e.to_string())
}

async fn tool_get_node_metrics(state: &AppState) -> Result<String, String> {
    let snap = state.latest_metrics.read().await.clone()
        .ok_or_else(|| "Metrics not yet collected".to_string())?;

    serde_json::to_string(&snap).map_err(|e| e.to_string())
}

async fn tool_list_containers() -> Result<String, String> {
    if !containers::is_docker_available() {
        return serde_json::to_string(&serde_json::json!({ "docker_available": false, "containers": [] }))
            .map_err(|e| e.to_string());
    }

    let cs = containers::list_containers().await
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&serde_json::json!({ "docker_available": true, "containers": cs }))
        .map_err(|e| e.to_string())
}

async fn tool_list_services() -> Result<String, String> {
    let available = services::is_systemd_available();
    let svcs = if available {
        services::list_services().unwrap_or_default()
    } else {
        vec![]
    };

    serde_json::to_string(&serde_json::json!({ "systemd_available": available, "services": svcs }))
        .map_err(|e| e.to_string())
}

async fn tool_list_alerts(state: &AppState) -> Result<String, String> {
    #[derive(sqlx::FromRow, serde::Serialize)]
    struct AlertRow {
        id: String,
        title: String,
        message: String,
        severity: String,
        state: String,
        created_at: i64,
    }

    let rows = sqlx::query_as::<_, AlertRow>(
        "SELECT id, title, message, severity, state, created_at FROM alerts WHERE state = 'active' ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| e.to_string())?;

    serde_json::to_string(&serde_json::json!({ "alerts": rows }))
        .map_err(|e| e.to_string())
}

async fn tool_get_container_logs(args: Value) -> Result<String, String> {
    let container_id = args
        .get("container_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: container_id".to_string())?;

    if !containers::is_docker_available() {
        return Err("Docker is not available".to_string());
    }

    let lines = containers::get_container_logs(container_id, 100).await
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&serde_json::json!({ "lines": lines }))
        .map_err(|e| e.to_string())
}

// Silence unused import warning — AppError is imported for consistent error handling style
// but direct status returns are used instead.
#[allow(dead_code)]
fn _use_app_error(_: AppError) {}
