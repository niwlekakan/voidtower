use crate::{
    audit,
    auth,
    containers::{self, ContainerAction},
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{ConnectInfo, Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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
}

#[derive(Deserialize)]
pub struct LogsQuery {
    pub tail: Option<usize>,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub lines: Vec<String>,
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

    Ok(Json(ContainersResponse { containers: cs, docker_available: true }))
}

pub async fn action(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<ActionRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let ip = addr.ip().to_string();

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }

    let action_str = format!("{:?}", req.action).to_lowercase();
    containers::container_action(&id, req.action).await.map_err(|e| {
        AppError::FeatureUnavailable(e.to_string())
    })?;

    audit::log(
        &state.db,
        Some(&user.id.to_string()),
        &user.username,
        &format!("container.{}", action_str),
        Some("container"),
        Some(&id),
        "success",
        Some(&ip),
        None,
    )
    .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<LogsResponse>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Err(AppError::FeatureUnavailable("Docker is not available".into()));
    }

    let tail = q.tail.unwrap_or(200);
    let lines = containers::get_container_logs(&id, tail).await.map_err(|e| {
        AppError::FeatureUnavailable(e.to_string())
    })?;

    Ok(Json(LogsResponse { lines }))
}

pub async fn images(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<ImagesResponse>> {
    require_user(&state, &jar).await?;

    if !containers::is_docker_available() {
        return Ok(Json(ImagesResponse { images: vec![] }));
    }

    let images = containers::list_images().await.map_err(|e| {
        AppError::FeatureUnavailable(e.to_string())
    })?;

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
        let Ok(docker) = (|| bollard::Docker::connect_with_unix_defaults())() else { return };
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
    require_user(&state, &jar).await?;
    // Sanitise: only hex chars (short id) or alphanumeric/dash (name)
    if !container_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
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
            .args(["inspect", "--format", &fmt, id])
            .output().await
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    // Try config_files label first — it gives the absolute path(s) directly.
    // Compose v2 sets this; may be comma-separated if multiple -f files were used.
    let config_files = inspect_label(&container_id, "com.docker.compose.project.config_files").await;
    if !config_files.is_empty() {
        let path = config_files.split(',').next().unwrap_or("").trim().to_string();
        if !path.is_empty() {
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                let working_dir = std::path::Path::new(&path)
                    .parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
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

    let candidates = ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"];
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

/// Write (stage) a compose file change — writes to a .proposed file, returns diff
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

    let compose_path = body["path"].as_str()
        .ok_or_else(|| AppError::BadRequest("path required".into()))?;
    let new_content = body["content"].as_str()
        .ok_or_else(|| AppError::BadRequest("content required".into()))?;

    // Basic path guard
    if !compose_path.ends_with(".yml") && !compose_path.ends_with(".yaml") {
        return Err(AppError::BadRequest("Path must be a YAML file".into()));
    }

    let current = tokio::fs::read_to_string(compose_path).await
        .unwrap_or_default();

    // Write proposed version alongside original
    let proposed_path = format!("{compose_path}.proposed");
    tokio::fs::write(&proposed_path, new_content).await
        .map_err(|e| AppError::BadRequest(format!("Write failed: {e}")))?;

    // Generate simple unified diff summary (line counts)
    let old_lines: Vec<&str> = current.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();
    let added   = new_lines.iter().filter(|l| !old_lines.contains(l)).count();
    let removed = old_lines.iter().filter(|l| !new_lines.contains(l)).count();

    audit::log(
        &state.db, Some(&user.id), "human", "containers.propose_compose",
        Some("container"), Some(&container_id), "success", None,
        Some(&format!("path={compose_path} +{added} -{removed}")),
    ).await;

    Ok(Json(serde_json::json!({
        "proposed_path": proposed_path,
        "added": added,
        "removed": removed,
        "current_lines": old_lines.len(),
        "new_lines": new_lines.len(),
    })))
}

/// Apply a proposed compose change — moves .proposed → original, restarts stack
pub async fn apply_compose(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(container_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }

    let proposed_path = body["proposed_path"].as_str()
        .ok_or_else(|| AppError::BadRequest("proposed_path required".into()))?;

    if !proposed_path.ends_with(".proposed") {
        return Err(AppError::BadRequest("Not a proposed file".into()));
    }

    let original = &proposed_path[..proposed_path.len() - ".proposed".len()];

    tokio::fs::rename(proposed_path, original).await
        .map_err(|e| AppError::BadRequest(format!("Rename failed: {e}")))?;

    // Restart via docker compose up -d in the working dir
    let working_dir = std::path::Path::new(original)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".into());

    let out = tokio::process::Command::new("docker")
        .args(["compose", "up", "-d", "--remove-orphans"])
        .current_dir(&working_dir)
        .output()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&user.id), "human", "containers.apply_compose",
        Some("container"), Some(&container_id), "success", None,
        Some(&format!("path={original}")),
    ).await;

    Ok(Json(serde_json::json!({
        "ok": out.status.success(),
        "stdout": String::from_utf8_lossy(&out.stdout),
        "stderr": String::from_utf8_lossy(&out.stderr),
    })))
}
