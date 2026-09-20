use crate::{
    auth, containers,
    error::AppError,
    operations::invocation::{self, CredentialContext},
    services,
    voidwatch::{self, ActionKind, Actor, ActorKind, Resource},
    AppState,
};
use axum::{
    body::Body,
    extract::{FromRequest, Request, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Kept under this leaf module to prevent rustfmt from traversing the whole crate when the shared
// registry changes. The file remains at `backend/src/action_registry.rs`; this is build-tooling
// containment, not MCP ownership.
#[path = "../action_registry.rs"]
pub(crate) mod action_registry;
use action_registry::ActionKind as RegistryActionKind;

// Declared here rather than in `api/mod.rs`: `api/mod.rs` lists every module in
// this directory via `pub mod`, so rustfmt invoked on it (as gates.sh's G0 format
// step does whenever it's touched) walks that whole graph and reformats every
// sibling file, including forbidden-zone ones. Nesting the declaration under this
// leaf module instead confines rustfmt's module-graph walk to this file and
// `redact.rs`/`test_support.rs` alone. `redact` is shared by `studio.rs` and
// `ai_context.rs` too (via `super::mcp::redact`), not mcp-exclusive; the nesting
// is a build-tooling workaround, not a statement about ownership.
#[path = "redact.rs"]
pub mod redact;
#[cfg(test)]
#[path = "test_support.rs"]
pub(crate) mod test_support;

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    #[serde(deserialize_with = "deserialize_json_rpc_id")]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default, deserialize_with = "deserialize_json_rpc_params")]
    pub params: Option<Value>,
}

fn deserialize_json_rpc_id<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let id = Option::<Value>::deserialize(deserializer)?;
    match id {
        None | Some(Value::Null | Value::String(_) | Value::Number(_)) => Ok(id),
        Some(_) => Err(serde::de::Error::custom("invalid JSON-RPC id")),
    }
}

fn deserialize_json_rpc_params<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
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
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
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

async fn check_mcp_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<CredentialContext, StatusCode> {
    if get_setting(state, "odysseus.mcp_enabled").await != "true" {
        return Err(StatusCode::FORBIDDEN);
    }

    let raw_token = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity = auth::validate_api_token_identity(&state.db, raw_token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let scopes = auth::token_scopes(&state.db, raw_token)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let user = auth::find_user_by_id(&state.db, &identity.user_id)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if user
        .expires_at
        .is_some_and(|expires_at| expires_at <= crate::unix_now())
    {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(CredentialContext::Mcp {
        token_id: identity.token_id,
        user_id: identity.user_id,
        role: user.role,
        scopes,
    })
}

// ---------------------------------------------------------------------------
// SSE endpoint — GET /api/mcp
// ---------------------------------------------------------------------------

pub async fn sse_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
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
    request: Request,
) -> (StatusCode, Json<JsonRpcResponse>) {
    let credential = match check_mcp_auth(&state, &headers).await {
        Ok(credential) => credential,
        Err(status) => {
            let code = if status == StatusCode::UNAUTHORIZED {
                -32001
            } else {
                -32003
            };
            let msg = if status == StatusCode::UNAUTHORIZED {
                "Unauthorized"
            } else {
                "MCP is not enabled"
            };
            return (status, Json(err_response(None, code, msg)));
        }
    };

    let Json(req) = match Json::<JsonRpcRequest>::from_request(request, &state).await {
        Ok(request) => request,
        Err(rejection) => {
            return (
                rejection.status(),
                Json(err_response(None, -32600, "Invalid Request")),
            )
        }
    };

    let resp = dispatch_with_context(&state, req, credential).await;
    (StatusCode::OK, Json(resp))
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

#[cfg(test)]
async fn dispatch(state: &AppState, req: JsonRpcRequest) -> JsonRpcResponse {
    dispatch_with_context(
        state,
        req,
        CredentialContext::Mcp {
            token_id: "[REDACTED]".into(),
            user_id: "test-user".into(),
            role: "owner".into(),
            scopes: vec![
                "metrics:read".into(),
                "containers:read".into(),
                "containers:logs".into(),
                "services:read".into(),
                "alerts:read".into(),
                "files:read".into(),
            ],
        },
    )
    .await
}

async fn dispatch_with_context(
    state: &AppState,
    req: JsonRpcRequest,
    credential: CredentialContext,
) -> JsonRpcResponse {
    let id = req.id.clone();
    if req.jsonrpc != "2.0" {
        return err_response(id, -32600, "Invalid Request");
    }
    match req.method.as_str() {
        "initialize" => {
            if !structured_params(&req.params) {
                return err_response(id, -32602, "Invalid params");
            }
            handle_initialize(id)
        }
        "tools/list" => {
            if !structured_params(&req.params) {
                return err_response(id, -32602, "Invalid params");
            }
            handle_tools_list(id)
        }
        "tools/call" => handle_tools_call(state, id, req.params, credential).await,
        _ => err_response(id, -32601, "Method not found"),
    }
}

fn structured_params(params: &Option<Value>) -> bool {
    params.as_ref().is_none_or(Value::is_object)
}

// ---------------------------------------------------------------------------
// initialize
// ---------------------------------------------------------------------------

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    ok_response(
        id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "voidtower", "version": "0.1.0" }
        }),
    )
}

// ---------------------------------------------------------------------------
// tools/list
// ---------------------------------------------------------------------------

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    ok_response(
        id,
        serde_json::json!({
            "tools": [
                {
                    "name": "list_nodes",
                    "description": "List all VoidTower nodes with health status",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                },
                {
                    "name": "get_node_metrics",
                    "description": "Get current CPU/RAM/disk metrics for the local node",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                },
                {
                    "name": "list_containers",
                    "description": "List all Docker containers with status",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                },
                {
                    "name": "list_services",
                    "description": "List systemd services with active state",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                },
                {
                    "name": "list_alerts",
                    "description": "List active alerts",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                },
                {
                    "name": "get_container_logs",
                    "description": "Get recent logs for a container",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "container_id": { "type": "string" }
                        },
                        "required": ["container_id"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "container.start",
                    "description": "Start an existing canonical container resource through the durable operation boundary",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "resource_id": { "type": "string" },
                            "request_id": { "type": "string", "description": "Stable idempotency key for this intent" }
                        },
                        "required": ["resource_id", "request_id"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "list_routes",
                    "description": "List all registered VoidTower API routes",
                    "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
                },
                {
                    "name": "read_file",
                    "description": "Read a file from the VoidTower project (path relative to repo root, e.g. backend/src/api/apps.rs)",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Relative path within project root" }
                        },
                        "required": ["path"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "search_code",
                    "description": "Search for a string/symbol across backend/src and frontend/src",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Search string (grep)" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "get_template",
                    "description": "Get a VoidTower extension template. Names: new_api_endpoint, new_tower_page, new_native_panel, new_background, new_catalog_entry, new_mcp_tool",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" }
                        },
                        "required": ["name"],
                        "additionalProperties": false
                    }
                }
            ]
        }),
    )
}

// ---------------------------------------------------------------------------
// tools/call
// ---------------------------------------------------------------------------

async fn handle_tools_call(
    state: &AppState,
    id: Option<Value>,
    params: Option<Value>,
    credential: CredentialContext,
) -> JsonRpcResponse {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ToolCallParams {
        name: String,
        #[serde(default, deserialize_with = "deserialize_json_rpc_params")]
        arguments: Option<Value>,
    }

    let Some(params) = params else {
        return err_response(id, -32602, "Invalid params");
    };
    if !params.is_object() {
        return err_response(id, -32602, "Invalid params");
    }

    let ToolCallParams {
        name: tool_name,
        arguments,
    } = match serde_json::from_value(params) {
        Ok(params) => params,
        Err(_) => return err_response(id, -32602, "Invalid params"),
    };
    let args = match arguments {
        None => serde_json::json!({}),
        Some(arguments) if arguments.is_object() => arguments,
        Some(_) => return err_response(id, -32602, "Tool arguments must be an object"),
    };

    let result = invoke_tool(state, credential, &tool_name, args).await;

    match result {
        Ok(text) => ok_response(
            id,
            serde_json::json!({
                "content": [{ "type": "text", "text": text }]
            }),
        ),
        Err(e) => ok_response(
            id,
            serde_json::json!({
                "content": [{ "type": "text", "text": serialized_tool_error(&e) }],
                "isError": true
            }),
        ),
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn tool_list_nodes(state: &AppState) -> Result<String, String> {
    // Return the local node; cluster peers can be added when the cluster module exposes them.
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
        .unwrap_or_else(|_| "local".to_string());

    let metrics_opt = state.latest_metrics.read().await.clone();
    let status = if metrics_opt.is_some() {
        "healthy"
    } else {
        "unknown"
    };

    let node = serde_json::json!([{
        "id": "local",
        "hostname": hostname,
        "role": "primary",
        "status": status
    }]);

    serde_json::to_string(&node).map_err(|e| e.to_string())
}

async fn tool_get_node_metrics(state: &AppState) -> Result<String, String> {
    let snap = state
        .latest_metrics
        .read()
        .await
        .clone()
        .ok_or_else(|| "Metrics not yet collected".to_string())?;

    serde_json::to_string(&snap).map_err(|e| e.to_string())
}

async fn tool_list_containers() -> Result<String, String> {
    if !containers::is_docker_available() {
        return serde_json::to_string(
            &serde_json::json!({ "docker_available": false, "containers": [] }),
        )
        .map_err(|e| e.to_string());
    }

    let cs = containers::list_containers()
        .await
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

    serde_json::to_string(&serde_json::json!({ "alerts": rows })).map_err(|e| e.to_string())
}

async fn tool_get_container_logs(args: Value) -> Result<String, String> {
    let container_id = args
        .get("container_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: container_id".to_string())?;

    if !containers::is_docker_available() {
        return Err("Docker is not available".to_string());
    }

    let lines = containers::get_container_logs(container_id, 100)
        .await
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&serde_json::json!({ "lines": lines })).map_err(|e| e.to_string())
}

fn tool_list_routes(state: &AppState) -> std::result::Result<String, String> {
    let root = super::ai_context::safe_project_root_from_frontend_dir(&state.config.frontend_dir);
    let src = std::fs::read_to_string(root.join("backend/src/api/mod.rs"))
        .map_err(|e| format!("Could not read mod.rs: {e}"))?;
    let lines: Vec<&str> = src
        .lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with(".route("))
        .collect();
    Ok(lines.join("\n"))
}

fn tool_read_file(state: &AppState, args: Value) -> std::result::Result<String, String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'path' argument".to_string())?;
    let root = super::ai_context::safe_project_root_from_frontend_dir(&state.config.frontend_dir);
    let content = super::ai_context::read_project_file(&root, path)?;
    if content.len() > 8000 {
        Ok(format!("{}\n…(truncated)", &content[..8000]))
    } else {
        Ok(content)
    }
}

fn tool_get_template(args: Value) -> std::result::Result<String, String> {
    let name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'name' argument".to_string())?;
    super::ai_context::get_template(name)
}

fn tool_search_code(state: &AppState, args: Value) -> std::result::Result<String, String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'query' argument".to_string())?;
    let root = super::ai_context::safe_project_root_from_frontend_dir(&state.config.frontend_dir);
    super::ai_context::search_project_code(&root, query)
}

// Silence unused import warning
#[allow(dead_code)]
fn _use_app_error(_: AppError) {}

// ---------------------------------------------------------------------------
// Public helpers for the Studio MCP panel (session-auth invocation)
// ---------------------------------------------------------------------------

pub fn tools_json() -> Value {
    handle_tools_list(None)
        .result
        .unwrap_or(serde_json::json!({"tools":[]}))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerStartArgs {
    resource_id: String,
    request_id: String,
}

async fn tool_container_start(
    state: &AppState,
    credential: &CredentialContext,
    args: Value,
) -> Result<String, String> {
    let args: ContainerStartArgs = serde_json::from_value(args)
        .map_err(|error| format!("Invalid container.start arguments: {error}"))?;
    let job = invocation::submit(
        &state.db,
        &state.operation_adapters,
        credential,
        &args.resource_id,
        "container.start",
        serde_json::json!({}),
        &args.request_id,
    )
    .await
    .map_err(|error| error.to_string())?;
    serde_json::to_string(&job).map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyToolArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ContainerLogsArgs {
    #[serde(rename = "container_id")]
    _container_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathToolArgs {
    #[serde(rename = "path")]
    _path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryToolArgs {
    #[serde(rename = "query")]
    _query: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateToolArgs {
    #[serde(rename = "name")]
    _name: String,
}

fn validate_tool_args<T: serde::de::DeserializeOwned>(
    name: &str,
    args: &Value,
) -> Result<(), String> {
    serde_json::from_value::<T>(args.clone())
        .map(|_| ())
        .map_err(|_| format!("Invalid {name} arguments"))
}

async fn redact_tool_error(state: &AppState, error: &str) -> String {
    redact::redact_for_ai(state, error)
        .await
        .chars()
        .take(4096)
        .collect()
}

fn serialized_tool_error(error: &str) -> String {
    const MAX_CHARS: usize = 4096;
    const PREFIX: &str = "Error: ";
    let available = MAX_CHARS.saturating_sub(PREFIX.chars().count());
    format!(
        "{PREFIX}{}",
        error.chars().take(available).collect::<String>()
    )
}

fn tool_action_kind(name: &str) -> ActionKind {
    match action_registry::action(name).map(|metadata| metadata.kind) {
        Some(RegistryActionKind::Read) => ActionKind::Read,
        Some(RegistryActionKind::Mutating) | None => ActionKind::Mutating,
    }
}

/// The single entry point for running an MCP tool, gated by `voidwatch::evaluate`.
/// Used by both the bearer-token JSON-RPC dispatch (`handle_tools_call`) and the
/// session-authenticated Studio panel (`api/studio.rs`'s `mcp_invoke`).
pub async fn invoke_tool(
    state: &AppState,
    credential: CredentialContext,
    name: &str,
    args: Value,
) -> std::result::Result<String, String> {
    match invoke_tool_unredacted(state, credential, name, args).await {
        Ok(text) => Ok(redact::redact_for_ai(state, &text).await),
        Err(error) => Err(redact_tool_error(state, &error).await),
    }
}

async fn invoke_tool_unredacted(
    state: &AppState,
    credential: CredentialContext,
    name: &str,
    args: Value,
) -> std::result::Result<String, String> {
    let metadata = action_registry::action(name).ok_or_else(|| format!("Unknown tool: {name}"))?;
    invocation::authorize_action(metadata, &credential).map_err(|error| error.to_string())?;

    if metadata.execution != action_registry::ActionExecution::DurableJob {
        let actor = match &credential {
            CredentialContext::Mcp { .. } => Actor {
                kind: ActorKind::ApiToken,
            },
            CredentialContext::Studio { .. } => Actor {
                kind: ActorKind::User,
            },
            _ => return Err("The tool is not available from this ingress".to_string()),
        };
        let verdict = voidwatch::evaluate(
            &state.db,
            actor,
            tool_action_kind(name),
            name,
            Resource {
                resource_type: "mcp_tool",
                resource_id: name,
            },
        )
        .await;

        match verdict {
            // Direct MCP tools are still gated by the shared AI-context policy. Durable actions
            // skip this synthetic mcp_tool evaluation because invocation::submit evaluates policy
            // against the canonical resource and persists its approval outcome with the job.
            voidwatch::Verdict::Allow | voidwatch::Verdict::AllowRequireSnapshot(_) => {}
            voidwatch::Verdict::RequireApproval(reason) => {
                return Err(format!("Requires approval: {reason}"))
            }
            voidwatch::Verdict::Deny(reason) => return Err(format!("Denied by policy: {reason}")),
        }
    }

    let result = match name {
        "container.start" => tool_container_start(state, &credential, args).await,
        "list_nodes" => {
            validate_tool_args::<EmptyToolArgs>(name, &args)?;
            tool_list_nodes(state).await
        }
        "get_node_metrics" => {
            validate_tool_args::<EmptyToolArgs>(name, &args)?;
            tool_get_node_metrics(state).await
        }
        "list_containers" => {
            validate_tool_args::<EmptyToolArgs>(name, &args)?;
            tool_list_containers().await
        }
        "list_services" => {
            validate_tool_args::<EmptyToolArgs>(name, &args)?;
            tool_list_services().await
        }
        "list_alerts" => {
            validate_tool_args::<EmptyToolArgs>(name, &args)?;
            tool_list_alerts(state).await
        }
        "get_container_logs" => {
            validate_tool_args::<ContainerLogsArgs>(name, &args)?;
            tool_get_container_logs(args).await
        }
        "list_routes" => {
            validate_tool_args::<EmptyToolArgs>(name, &args)?;
            tool_list_routes(state)
        }
        "read_file" => {
            validate_tool_args::<PathToolArgs>(name, &args)?;
            tool_read_file(state, args)
        }
        "search_code" => {
            validate_tool_args::<QueryToolArgs>(name, &args)?;
            tool_search_code(state, args)
        }
        "get_template" => {
            validate_tool_args::<TemplateToolArgs>(name, &args)?;
            tool_get_template(args)
        }
        other => Err(format!("Unknown tool: {other}")),
    };

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use async_trait::async_trait;
    use sha2::Digest;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct PlanningAdapter {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl crate::operations::adapters::OperationAdapter for PlanningAdapter {
        fn key(&self) -> &'static str {
            "containers"
        }

        fn actions(&self) -> &[&'static str] {
            &["container.start"]
        }

        async fn plan(
            &self,
            request: crate::operations::adapters::PlanRequest,
        ) -> Result<crate::operations::contracts::OperationPlanV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(crate::operations::contracts::OperationPlanV1 {
                schema_version: 1,
                title: "Start container".into(),
                risk: "mutate".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: "provider-state-1".into(),
                steps: vec![crate::operations::contracts::PlannedStepV1 {
                    kind: "execute".into(),
                    name: format!("{} {}", request.action, request.resource.display_name),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                }],
            })
        }

        async fn external_fingerprint(
            &self,
            _request: &crate::operations::adapters::PlanRequest,
        ) -> Result<String> {
            Ok("provider-state-1".into())
        }

        async fn execute_step(
            &self,
            _request: crate::operations::adapters::StepRequest,
        ) -> Result<crate::operations::adapters::StepOutcome> {
            unreachable!()
        }

        async fn reconcile(
            &self,
            _request: crate::operations::adapters::StepRequest,
        ) -> Result<crate::operations::adapters::ReconcileOutcome> {
            unreachable!()
        }
    }

    #[test]
    fn container_start_is_an_explicit_typed_tool() {
        let tools = tools_json();
        let tool_names = tools["tools"]
            .as_array()
            .expect("MCP tools must be an array")
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect::<Vec<_>>();
        assert!(tool_names.contains(&"container.start"));

        let action = action_registry::action("container.start").expect("registered action");
        assert!(action
            .ingresses
            .contains(&action_registry::ActionIngress::Mcp));
        assert!(action
            .ingresses
            .contains(&action_registry::ActionIngress::Studio));
        assert_eq!(action.mcp_scope(), Some("containers:restart"));
    }

    #[tokio::test]
    async fn container_start_rejects_insufficient_scope_before_resource_lookup() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(pool);
        let credential = CredentialContext::Mcp {
            token_id: "token-1".into(),
            user_id: "test-user".into(),
            role: "owner".into(),
            scopes: vec!["containers:read".into()],
        };

        let error = invoke_tool(
            &state,
            credential,
            "container.start",
            serde_json::json!({
                "resource_id": "missing-resource",
                "request_id": "mcp-start-denied"
            }),
        )
        .await
        .expect_err("wrong MCP scope must fail before lookup");
        assert_eq!(error, "the API token scope does not permit this action");
    }

    #[tokio::test]
    async fn container_start_rejects_unknown_arguments() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(pool);
        let credential = CredentialContext::Studio {
            user_id: "studio-user".into(),
            role: "owner".into(),
        };

        let error = invoke_tool(
            &state,
            credential,
            "container.start",
            serde_json::json!({
                "resource_id": "missing-resource",
                "request_id": "studio-start-invalid",
                "provider_token": "must-not-be-accepted"
            }),
        )
        .await
        .expect_err("typed tool must reject unknown fields");
        assert!(error.starts_with("Invalid container.start arguments:"));
    }

    #[tokio::test]
    async fn read_tools_reject_unknown_arguments() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(pool);
        let credential = CredentialContext::Studio {
            user_id: "studio-user".into(),
            role: "owner".into(),
        };

        let error = invoke_tool(
            &state,
            credential,
            "list_nodes",
            serde_json::json!({ "unexpected": true }),
        )
        .await
        .expect_err("read tools must reject unknown fields");
        assert_eq!(error, "Invalid list_nodes arguments");

        let long_unknown_tool = "x".repeat(10_000);
        let error = invoke_tool(
            &state,
            CredentialContext::Studio {
                user_id: "studio-user".into(),
                role: "owner".into(),
            },
            &long_unknown_tool,
            serde_json::json!({}),
        )
        .await
        .expect_err("unknown tool errors must be bounded");
        assert!(error.len() <= 4096);
        assert!(!error.contains(&long_unknown_tool));
    }

    #[test]
    fn serialized_tool_errors_remain_bounded_after_the_mcp_prefix() {
        let rendered = serialized_tool_error(&"x".repeat(10_000));
        assert_eq!(rendered.chars().count(), 4096);
        assert!(rendered.starts_with("Error: "));
    }

    #[test]
    fn direct_tool_schemas_accept_the_published_argument_names() {
        assert!(validate_tool_args::<PathToolArgs>(
            "read_file",
            &serde_json::json!({ "path": "backend/src/api/mcp.rs" }),
        )
        .is_ok());
        assert!(validate_tool_args::<QueryToolArgs>(
            "search_code",
            &serde_json::json!({ "query": "CredentialContext" }),
        )
        .is_ok());
        assert!(validate_tool_args::<TemplateToolArgs>(
            "get_template",
            &serde_json::json!({ "name": "new_mcp_tool" }),
        )
        .is_ok());
    }

    #[tokio::test]
    async fn container_start_submits_a_durable_job_and_replays_idempotently() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let resource = crate::operations::resources::observe(
            &pool,
            crate::operations::resources::ObserveResource {
                kind: "container",
                display_name: "web",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "web",
            },
            None,
            "mcp-test",
        )
        .await
        .unwrap();
        crate::operations::resources::set_capability(
            &pool,
            &resource.id,
            "container.start",
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            "mcp-test-capability",
        )
        .await
        .unwrap();

        let calls = Arc::new(AtomicUsize::new(0));
        let mut adapters = crate::operations::adapters::AdapterRegistry::new();
        adapters
            .register(Arc::new(PlanningAdapter {
                calls: calls.clone(),
            }))
            .unwrap();
        let mut state = crate::api::mcp::test_support::build(pool.clone());
        state.operation_adapters = Arc::new(adapters);
        let credential = CredentialContext::Mcp {
            token_id: "token-1".into(),
            user_id: "test-user".into(),
            role: "owner".into(),
            scopes: vec!["containers:restart".into()],
        };
        let args = serde_json::json!({
            "resource_id": resource.id,
            "request_id": "mcp-start-replay-1"
        });

        let first: Value = serde_json::from_str(
            &invoke_tool(&state, credential.clone(), "container.start", args.clone())
                .await
                .expect("canonical submit should create a job"),
        )
        .unwrap();
        let second: Value = serde_json::from_str(
            &invoke_tool(&state, credential, "container.start", args)
                .await
                .expect("same intent should replay the existing job"),
        )
        .unwrap();

        assert_eq!(first["id"], second["id"]);
        assert_eq!(first["action"], "container.start");
        assert_eq!(first["ingress"], "mcp");
        let persisted_key: String =
            sqlx::query_scalar("SELECT idempotency_key FROM jobs WHERE id = ?")
                .bind(first["id"].as_str().expect("job id"))
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(persisted_key, "mcp-start-replay-1");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn container_start_uses_canonical_invocation_errors() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(pool);
        let credential = CredentialContext::Mcp {
            token_id: "token-1".into(),
            user_id: "test-user".into(),
            role: "owner".into(),
            scopes: vec!["containers:restart".into()],
        };

        let error = invoke_tool(
            &state,
            credential,
            "container.start",
            serde_json::json!({
                "resource_id": "missing-resource",
                "request_id": "mcp-start-1"
            }),
        )
        .await
        .expect_err("missing canonical resource must fail closed");
        assert_eq!(error, "resource not found");
    }

    #[tokio::test]
    async fn container_start_uses_canonical_invocation_for_studio() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(pool);
        let credential = CredentialContext::Studio {
            user_id: "studio-user".into(),
            role: "owner".into(),
        };

        let error = invoke_tool(
            &state,
            credential,
            "container.start",
            serde_json::json!({
                "resource_id": "missing-resource",
                "request_id": "studio-start-1"
            }),
        )
        .await
        .expect_err("missing canonical resource must fail closed");
        assert_eq!(error, "resource not found");
    }

    #[tokio::test]
    async fn mcp_rejects_an_expired_token_owner_before_dispatch() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let now = crate::unix_now();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, expires_at, created_at, updated_at) \
             VALUES ('expired-user', 'expired', 'x', 'owner', ?, 0, 0)",
        )
        .bind(now - 1)
        .execute(&pool)
        .await
        .unwrap();
        let raw_token = "[REDACTED]";
        let token_hash = hex::encode(sha2::Sha256::digest(raw_token.as_bytes()));
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, expires_at, created_at) \
             VALUES ('expired-token', 'expired-user', 'mcp', ?, '[\\\"alerts:read\\\"]', ?, 0)",
        )
        .bind(token_hash)
        .bind(now + 3600)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('odysseus.mcp_enabled', 'true', ?)",
        )
        .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let state = crate::api::mcp::test_support::build(pool);
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", "Bearer [REDACTED]".parse().unwrap());
        assert_eq!(
            check_mcp_auth(&state, &headers).await,
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    /// regardless of whether they're registered VoidTower secrets.
    fn secret_corpus() -> Vec<&'static str> {
        vec![
            "fakevendor_51H8x9K2eZvKYlo2CxpqrstuvWXYZ", // API-key-shaped
            "hunter2ReallyLongPasswordValue",           // password-shaped
        ]
    }

    async fn seed_alert_with_corpus(pool: &sqlx::SqlitePool) {
        let message = format!(
            "Startup banner: api_key={} password: \"{}\" -----BEGIN TEST PRIVATE KEY-----\nMIIBOGONOTAREALKEYBYTES\n-----END TEST PRIVATE KEY-----",
            secret_corpus()[0],
            secret_corpus()[1],
        );
        sqlx::query(
            "INSERT INTO alerts (id, title, message, severity, category, state, created_at, updated_at) \
             VALUES ('a1', 'leaky app', ?, 'warning', 'general', 'active', 0, 0)",
        )
        .bind(&message)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn redaction_corpus_never_appears_in_mcp_tool_call_output() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        seed_alert_with_corpus(&pool).await;
        let state = crate::api::mcp::test_support::build(pool);

        let resp = dispatch(
            &state,
            JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(serde_json::json!(1)),
                method: "tools/call".into(),
                params: Some(serde_json::json!({ "name": "list_alerts", "arguments": {} })),
            },
        )
        .await;

        let result = resp.result.expect("tools/call should succeed");
        let text = result["content"][0]["text"].as_str().unwrap().to_string();

        for secret in secret_corpus() {
            assert!(
                !text.contains(secret),
                "corpus secret leaked into mcp tool output: {secret}"
            );
        }
        assert!(
            !text.contains("MIIBOGONOTAREALKEYBYTES"),
            "PEM key body leaked into mcp tool output"
        );
        // Non-secret content must survive.
        assert!(text.contains("leaky app"));
    }

    #[tokio::test]
    async fn redaction_does_not_break_non_secret_content() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO alerts (id, title, message, severity, category, state, created_at, updated_at) \
             VALUES ('a2', 'disk check', 'commit 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08 deployed ok', 'info', 'general', 'active', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = crate::api::mcp::test_support::build(pool);

        let resp = dispatch(
            &state,
            JsonRpcRequest {
                jsonrpc: "2.0".into(),
                id: Some(serde_json::json!(1)),
                method: "tools/call".into(),
                params: Some(serde_json::json!({ "name": "list_alerts", "arguments": {} })),
            },
        )
        .await;

        let result = resp.result.expect("tools/call should succeed");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"));
        assert!(text.contains("disk check"));
    }

    /// Docker is unavailable in this sandbox/CI (`containers::is_docker_available`
    /// checks for `/var/run/docker.sock`, which doesn't exist here), so
    /// `tool_get_container_logs` can't be driven through a real container. This
    /// test instead seeds a real secret through the same path `secrets.rs` uses
    /// (encrypt + insert), builds a fixture payload byte-for-byte identical to
    /// what `tool_get_container_logs` serializes (`{"lines": [...]}"`), and runs
    /// it through the exact `redact::redact_for_ai` call that `invoke_tool`
    /// applies to that tool's real output — proving the registered secret value
    /// is stripped end-to-end through the production redaction path.
    #[tokio::test]
    async fn tool_get_container_logs_redacts_known_secret_value_end_to_end() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let key: [u8; 32] = [3u8; 32];
        let secret_value = "prod-db-conn-str-p@ssw0rd-xyz123";
        let enc = crate::api::secrets::encrypt(&key, secret_value).unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) VALUES ('s1', 'db-conn', NULL, ?, 0, 0)",
        )
        .bind(&enc)
        .execute(&pool)
        .await
        .unwrap();

        let mut state = crate::api::mcp::test_support::build(pool);
        state.secrets_key = std::sync::Arc::new(key);

        let fixture_lines = vec![
            "Booting app v1.2.3".to_string(),
            format!("Connecting with DATABASE_URL=postgres://app:{secret_value}@db:5432/app"),
            "Ready to accept connections".to_string(),
        ];
        let raw = serde_json::to_string(&serde_json::json!({ "lines": fixture_lines })).unwrap();

        let redacted = crate::api::mcp::redact::redact_for_ai(&state, &raw).await;

        assert!(
            !redacted.contains(secret_value),
            "known secret value leaked into container logs tool output"
        );
        assert!(redacted.contains("Booting app v1.2.3"));
        assert!(redacted.contains("Ready to accept connections"));
    }
}
