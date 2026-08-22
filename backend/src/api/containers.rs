use crate::{
    audit, auth,
    containers::{self, ContainerAction},
    error::{AppError, Result},
    operations::adapters::containers::{ComposeApplyInput, ComposeArtifactStore},
    AppState,
};
use axum::{
    extract::{ConnectInfo, Extension, Path, Query, State, WebSocketUpgrade},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

use super::{
    bearer_auth::AuthenticatedApiToken,
    operation_adoption::{self, CompatibilityResource, CompatibilityResult},
};

#[derive(Serialize)]
pub struct ContainersResponse {
    pub containers: Vec<containers::ContainerInfo>,
    pub docker_available: bool,
}

#[derive(Serialize)]
pub struct ImagesResponse {
    pub images: Vec<containers::ImageInfo>,
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub action: ContainerAction,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub tail: Option<usize>,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub lines: Vec<String>,
}

#[derive(Deserialize)]
pub struct ApplyComposeRequest {
    pub content: String,
    pub path: Option<String>,
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

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
) -> Result<Json<ContainersResponse>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Ok(Json(ContainersResponse {
            containers: vec![],
            docker_available: false,
        }));
    }

    let cs = containers::list_containers().await.map_err(|e| {
        tracing::warn!("Docker list error: {}", e);
        AppError::FeatureUnavailable(e.to_string())
    })?;

    Ok(Json(ContainersResponse {
        containers: cs,
        docker_available: true,
    }))
}

pub async fn action(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ActionRequest>,
) -> CompatibilityResult<Response> {
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    let action = canonical_action(&req.action);
    operation_adoption::authorize(&credential, action)?;
    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()).into());
    }
    let listed = containers::list_containers()
        .await
        .map_err(|error| AppError::FeatureUnavailable(error.to_string()))?;
    let container = select_container(&listed, &id)?;
    let resource = operation_adoption::observe_available(
        &state,
        &credential,
        CompatibilityResource {
            kind: "container",
            display_name: &container.name,
            node_id: None,
            provider: Some("docker"),
            namespace: "docker.container",
            scope_key: "local-engine",
            alias: &container.id,
        },
        &[action],
    )
    .await?;
    let input = serde_json::json!({});

    if req.dry_run {
        let prepared =
            operation_adoption::prepare(&state, &credential, &resource.id, action, input).await?;
        let view = prepared.view();
        let mut plan = serde_json::to_value(view.operation)
            .map_err(|error| AppError::Internal(error.into()))?;
        plan["risk"] = serde_json::Value::String(
            if matches!(req.action, ContainerAction::Remove) {
                "high"
            } else {
                "medium"
            }
            .into(),
        );
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": plan,
            "policy": view.policy,
            "resource": view.resource,
        }))
        .into_response());
    }
    operation_adoption::submit(&state, &credential, &resource.id, action, input, &headers).await
}

pub async fn logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<LogsResponse>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable(
            "Docker is not available".into(),
        ));
    }

    let tail = q.tail.unwrap_or(200);
    let lines = containers::get_container_logs(&id, tail)
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    Ok(Json(LogsResponse { lines }))
}

pub async fn images(State(state): State<AppState>, jar: CookieJar) -> Result<Json<ImagesResponse>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Ok(Json(ImagesResponse { images: vec![] }));
    }

    let images = containers::list_images()
        .await
        .map_err(|e| AppError::FeatureUnavailable(e.to_string()))?;

    Ok(Json(ImagesResponse { images }))
}

/// WebSocket live log tail — streams `docker logs --follow` output
pub async fn logs_ws(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> std::result::Result<impl IntoResponse, AppError> {
    require_user(&state, &jar).await?;
    Ok(ws.on_upgrade(move |socket| async move {
        use axum::extract::ws::Message;
        use bollard::container::{LogOutput, LogsOptions};
        use futures_util::{SinkExt, StreamExt};

        let (mut sink, mut stream) = socket.split();
        let Ok(docker) = bollard::Docker::connect_with_unix_defaults() else { return };
        let opts = LogsOptions::<String> {
            stdout: true, stderr: true, follow: true, tail: "200".into(),
            ..Default::default()
        };
        let mut logs = docker.logs(&id, Some(opts));
        loop {
            tokio::select! {
                chunk = logs.next() => {
                    match chunk {
                        Some(Ok(LogOutput::StdOut { message } | LogOutput::StdErr { message })) => {
                            let line = String::from_utf8_lossy(&message).trim_end().to_string();
                            if !line.is_empty() {
                                let json = serde_json::json!({"type":"log","line":line}).to_string();
                                if sink.send(Message::Text(json)).await.is_err() { break; }
                            }
                        }
                        None | Some(Err(_)) => break,
                        _ => {}
                    }
                }
                msg = stream.next() => {
                    if !matches!(msg, Some(Ok(Message::Text(_) | Message::Binary(_)))) { break; }
                }
            }
        }
    }))
}

/// WebSocket exec — spawns `docker exec -it <id> sh` in a PTY
pub async fn exec_ws(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(container_id): Path<String>,
    ws: WebSocketUpgrade,
) -> std::result::Result<impl IntoResponse, AppError> {
    let user = require_user(&state, &jar).await?;

    // Require at least operator role — this opens an interactive shell in the container
    super::role_guard::require_operator(&user)?;

    // Sanitise: only hex chars (short id) or alphanumeric/dash (name)
    if !container_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::BadRequest("Invalid container id".into()));
    }
    Ok(ws.on_upgrade(move |socket| async move {
        crate::terminal::handle_terminal_ws(
            socket,
            Some(format!("docker exec -it {container_id} sh")),
            String::new(),
        )
        .await
    }))
}

/// Read the compose file for a container (looks for label com.docker.compose.project.working_dir)
pub async fn get_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(container_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker unavailable".into()));
    }

    async fn inspect_label(id: &str, label: &str) -> String {
        let fmt = format!("{{{{index .Config.Labels \"{label}\"}}}}");
        tokio::process::Command::new("docker")
            .args(["inspect", "--format", &fmt, "--", id])
            .output()
            .await
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    // Try config_files label first — it gives the absolute path(s) directly.
    // Compose v2 sets this; may be comma-separated if multiple -f files were used.
    let config_files =
        inspect_label(&container_id, "com.docker.compose.project.config_files").await;
    if !config_files.is_empty() {
        let path = config_files
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if !path.is_empty() {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let working_dir = std::path::Path::new(&path)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                return Ok(Json(serde_json::json!({
                    "found": true,
                    "path": path,
                    "content": content,
                    "working_dir": working_dir,
                })));
            }
        }
    }

    // Fallback: working_dir label + search for compose filename
    let working_dir = inspect_label(&container_id, "com.docker.compose.project.working_dir").await;

    if working_dir.is_empty() {
        return Ok(Json(serde_json::json!({
            "found": false,
            "message": "Container was not started via docker compose (no compose labels found)"
        })));
    }

    let candidates = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ];
    for name in candidates {
        let path = format!("{working_dir}/{name}");
        if let Ok(content) = tokio::fs::read_to_string(&path).await {
            return Ok(Json(serde_json::json!({
                "found": true,
                "path": path,
                "content": content,
                "working_dir": working_dir,
            })));
        }
    }

    Ok(Json(serde_json::json!({
        "found": false,
        "working_dir": working_dir,
        "message": "Compose file not found in working directory"
    })))
}

/// Validate and preview a Compose file change without mutating host state.
pub async fn propose_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(container_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }

    let compose_path = body["path"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("path required".into()))?;
    let new_content = body["content"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("content required".into()))?;

    let preview = crate::operations::adapters::containers::preview_compose_change(
        &state.config.data_dir,
        &container_id,
        std::path::Path::new(compose_path),
        new_content.as_bytes(),
    )
    .await
    .map_err(|error| {
        AppError::BadRequest(crate::api::mcp::redact::redact_patterns(&error.to_string()))
    })?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "containers.propose_compose",
        Some("container"),
        Some(&container_id),
        "success",
        None,
        Some(&format!("preview +{} -{}", preview.added, preview.removed)),
    )
    .await;

    Ok(Json(
        serde_json::to_value(preview).map_err(|error| AppError::Internal(error.into()))?,
    ))
}

pub async fn apply_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(container_id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<ApplyComposeRequest>,
) -> CompatibilityResult<Response> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_admin(&user)?;
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, "container.compose.apply")?;
    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()).into());
    }
    let listed = containers::list_containers()
        .await
        .map_err(|error| AppError::FeatureUnavailable(error.to_string()))?;
    let container = select_container(&listed, &container_id)?;
    if let Some(path) = body.path.as_deref() {
        crate::operations::adapters::containers::preview_compose_change(
            &state.config.data_dir,
            &container.id,
            std::path::Path::new(path),
            body.content.as_bytes(),
        )
        .await
        .map_err(|error| {
            AppError::BadRequest(crate::api::mcp::redact::redact_patterns(&error.to_string()))
        })?;
    }
    let resource = operation_adoption::observe_available(
        &state,
        &credential,
        CompatibilityResource {
            kind: "container",
            display_name: &container.name,
            node_id: None,
            provider: Some("docker"),
            namespace: "docker.container",
            scope_key: "local-engine",
            alias: &container.id,
        },
        &["container.compose.apply"],
    )
    .await?;
    let idempotency_key = operation_adoption::idempotency_key(&headers)?;
    let artifact_identity = format!(
        "{}:{}:{}:{}",
        credential.idempotency_scope(),
        resource.id,
        "container.compose.apply",
        idempotency_key
    );
    let artifact = ComposeArtifactStore::new(&state.config.data_dir)
        .stage_for(&artifact_identity, body.content.as_bytes())
        .await
        .map_err(|error| {
            AppError::BadRequest(crate::api::mcp::redact::redact_patterns(&error.to_string()))
        })?;
    let input = serde_json::to_value(ComposeApplyInput {
        artifact_ref: artifact.artifact_ref,
        artifact_sha256: artifact.sha256,
    })
    .map_err(|error| AppError::Internal(error.into()))?;
    operation_adoption::submit_with_key(
        &state,
        &credential,
        &resource.id,
        "container.compose.apply",
        input,
        &idempotency_key,
    )
    .await
}

fn canonical_action(action: &ContainerAction) -> &'static str {
    match action {
        ContainerAction::Start => "container.start",
        ContainerAction::Stop => "container.stop",
        ContainerAction::Restart => "container.restart",
        ContainerAction::Remove => "container.remove",
    }
}

fn select_container(
    listed: &[containers::ContainerInfo],
    requested: &str,
) -> CompatibilityResult<containers::ContainerInfo> {
    if let Some(container) = listed
        .iter()
        .find(|container| container.id == requested || container.name == requested)
    {
        return Ok(container.clone());
    }
    let matches: Vec<_> = listed
        .iter()
        .filter(|container| container.id.starts_with(requested))
        .collect();
    match matches.as_slice() {
        [container] => Ok((*container).clone()),
        [] => Err(AppError::NotFound.into()),
        _ => Err(AppError::Conflict("Container id is ambiguous".into()).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(id: &str, name: &str) -> containers::ContainerInfo {
        containers::ContainerInfo {
            id: id.into(),
            short_id: id.chars().take(12).collect(),
            name: name.into(),
            image: "fixture:latest".into(),
            status: "Up".into(),
            state: "running".into(),
            created: 1,
            ports: vec![],
        }
    }

    #[test]
    fn compatibility_selection_canonicalizes_names_and_unambiguous_prefixes() {
        let listed = vec![
            container("abcdef0123456789", "web"),
            container("1234567890abcdef", "worker"),
        ];

        assert_eq!(select_container(&listed, "web").unwrap().id, listed[0].id);
        assert_eq!(
            select_container(&listed, "1234567890ab").unwrap().id,
            listed[1].id
        );
        assert!(select_container(&listed, "missing").is_err());
    }

    #[test]
    fn compatibility_selection_rejects_ambiguous_prefixes() {
        let listed = vec![
            container("abcdef-one", "one"),
            container("abcdef-two", "two"),
        ];
        assert!(select_container(&listed, "abcdef").is_err());
    }

    #[test]
    fn adopted_container_handlers_do_not_call_the_provider_mutation_helper() {
        let source = include_str!("containers.rs");
        let forbidden = ["containers::container_", "action("].concat();
        assert!(!source.contains(&forbidden));
    }
}
