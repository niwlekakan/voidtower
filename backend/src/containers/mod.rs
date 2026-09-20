use anyhow::Result;
use bollard::{
    container::{
        ListContainersOptions, LogOutput, LogsOptions, RemoveContainerOptions,
        RestartContainerOptions, StartContainerOptions, StopContainerOptions,
    },
    image::ListImagesOptions,
    Docker,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, process::Stdio, sync::Arc};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::Mutex;

/// Tracks the OS pid of each in-flight `docker compose` deploy, keyed by project name,
/// so a deploy can be cancelled gracefully via `cancel_deploy` instead of killing the
/// whole VoidTower process (which would otherwise take the docker child down with it
/// and can corrupt the containerd content store mid-pull).
pub type DeployRegistry = Arc<Mutex<HashMap<String, u32>>>;

/// Gracefully cancel an in-flight deploy: SIGTERM, then SIGKILL after a short grace
/// period if it hasn't exited. Returns true if a running deploy was found and signalled.
pub async fn cancel_deploy(registry: &DeployRegistry, project_name: &str) -> bool {
    let pid = match registry.lock().await.get(project_name).copied() {
        Some(pid) => pid,
        None => return false,
    };
    #[cfg(unix)]
    {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let nix_pid = Pid::from_raw(pid as i32);
        let nix_group = Pid::from_raw(-(pid as i32));
        let _ = kill(nix_group, Signal::SIGTERM);
        let _ = kill(nix_pid, Signal::SIGTERM);
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            if kill(nix_pid, None).is_err() && kill(nix_group, None).is_err() {
                return true; // process exited
            }
        }
        let _ = kill(nix_group, Signal::SIGKILL);
        let _ = kill(nix_pid, Signal::SIGKILL);
        for _ in 0..20 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if kill(nix_pid, None).is_err() && kill(nix_group, None).is_err() {
                return true;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).output();
    }
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub ports: Vec<PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: Option<u16>,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Remove,
}

pub fn is_docker_available() -> bool {
    std::path::Path::new("/var/run/docker.sock").exists()
}

#[allow(dead_code)]
pub fn is_lxc_available() -> bool {
    std::process::Command::new("which")
        .arg("lxc-ls")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn connect() -> Result<Docker> {
    Ok(Docker::connect_with_unix_defaults()?)
}

pub async fn list_containers() -> Result<Vec<ContainerInfo>> {
    let docker = connect()?;
    let opts = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };
    let containers = docker.list_containers(Some(opts)).await?;

    let result = containers
        .into_iter()
        .map(|c| {
            let id = c.id.unwrap_or_default();
            let short_id: String = id.chars().take(12).collect();
            let name = c
                .names
                .unwrap_or_default()
                .into_iter()
                .next()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();
            let image = c.image.unwrap_or_default();
            let status = c.status.unwrap_or_default();
            let state = c.state.unwrap_or_default();
            let created = c.created.unwrap_or(0);

            let ports = c
                .ports
                .unwrap_or_default()
                .into_iter()
                .map(|p| {
                    let protocol = p.typ
                        .map(|t| format!("{:?}", t).to_lowercase())
                        .unwrap_or_else(|| "tcp".into());
                    PortMapping {
                        host_port: p.public_port,
                        container_port: p.private_port,
                        protocol,
                    }
                })
                .collect();

            ContainerInfo { id, short_id, name, image, status, state, created, ports }
        })
        .collect();

    Ok(result)
}

pub async fn container_action(id: &str, action: ContainerAction) -> Result<()> {
    let docker = connect()?;
    match action {
        ContainerAction::Start => {
            docker
                .start_container(id, None::<StartContainerOptions<String>>)
                .await?;
        }
        ContainerAction::Stop => {
            docker
                .stop_container(id, Some(StopContainerOptions { t: 10 }))
                .await?;
        }
        ContainerAction::Restart => {
            docker
                .restart_container(id, Some(RestartContainerOptions { t: 10 }))
                .await?;
        }
        ContainerAction::Remove => {
            docker
                .remove_container(
                    id,
                    Some(RemoveContainerOptions { force: true, ..Default::default() }),
                )
                .await?;
        }
    }
    Ok(())
}

pub async fn get_container_logs(id: &str, tail: usize) -> Result<Vec<String>> {
    let docker = connect()?;
    let opts = LogsOptions::<String> {
        stdout: true,
        stderr: true,
        tail: format!("{}", tail),
        ..Default::default()
    };

    let mut stream = docker.logs(id, Some(opts));
    let mut lines = Vec::new();
    while let Some(Ok(chunk)) = stream.next().await {
        let line = match chunk {
            LogOutput::StdOut { message } | LogOutput::StdErr { message } => {
                String::from_utf8_lossy(&message).trim_end().to_string()
            }
            _ => continue,
        };
        if !line.is_empty() {
            lines.push(line);
        }
    }
    Ok(lines)
}

pub async fn list_images() -> Result<Vec<ImageInfo>> {
    let docker = connect()?;
    let images = docker
        .list_images(Some(ListImagesOptions::<String> { all: false, ..Default::default() }))
        .await?;
    Ok(images
        .into_iter()
        .map(|img| {
            let id: String = img.id.strip_prefix("sha256:").unwrap_or(&img.id).chars().take(12).collect();
            ImageInfo {
                id,
                tags: img.repo_tags,
                size: img.size,
                created: img.created,
            }
        })
        .collect())
}

/// Like `deploy_compose`, but registers the spawned process's pid in `registry` for the
/// duration of the call so it can be cancelled gracefully via `cancel_deploy`. Used by the
/// interactive deploy flow, which exposes a Cancel button while this is in flight.
#[allow(dead_code)]
pub async fn deploy_compose_cancellable(
    project_name: &str,
    compose_path: &std::path::Path,
    registry: &DeployRegistry,
) -> Result<()> {
    let child = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["up", "-d", "--build"])
        .process_group(0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    if let Some(pid) = child.id() {
        registry.lock().await.insert(project_name.to_string(), pid);
    }
    let output = run_bounded_command(child, MAX_COMPOSE_LOG_STREAM_BYTES, MAX_COMPOSE_LOG_STREAM_BYTES).await;
    registry.lock().await.remove(project_name);
    let (stdout, stderr, status) = output?;

    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        anyhow::bail!("docker compose failed ({}): {}", status, stderr);
    }
    let _ = stdout;
    Ok(())
}

const MAX_COMPOSE_LOG_STREAM_BYTES: usize = 64 * 1024;
const COMPOSE_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

async fn read_bounded_output<R: AsyncRead + Unpin>(mut reader: R, max_bytes: usize) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(max_bytes);
    let mut buffer = [0u8; 8192];
    let mut total = 0usize;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        if output.len() < max_bytes {
            let keep = (max_bytes - output.len()).min(read);
            output.extend_from_slice(&buffer[..keep]);
        }
        if total > max_bytes {
            anyhow::bail!("docker compose output exceeded its bound");
        }
    }
    if total > max_bytes {
        anyhow::bail!("docker compose output exceeded its bound");
    }
    Ok(output)
}

async fn terminate_command_tree(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;
        let _ = kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL);
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

async fn run_bounded_command(
    mut child: tokio::process::Child,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<(Vec<u8>, Vec<u8>, std::process::ExitStatus)> {
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let mut stdout_task = tokio::spawn(read_bounded_output(stdout, stdout_limit));
    let mut stderr_task = tokio::spawn(read_bounded_output(stderr, stderr_limit));
    let collect = async {
        tokio::select! {
            stdout_result = &mut stdout_task => {
                let stdout = match stdout_result {
                    Ok(Ok(output)) => output,
                    Ok(Err(error)) => {
                        terminate_command_tree(&mut child).await;
                        let _ = stderr_task.await;
                        return Err(error);
                    }
                    Err(error) => {
                        terminate_command_tree(&mut child).await;
                        let _ = stderr_task.await;
                        return Err(anyhow::anyhow!("docker stdout reader failed: {error}"));
                    }
                };
                let status = child.wait().await?;
                let stderr = stderr_task.await.map_err(|error| anyhow::anyhow!("docker stderr reader failed: {error}"))??;
                Ok((stdout, stderr, status))
            }
            stderr_result = &mut stderr_task => {
                let stderr = match stderr_result {
                    Ok(Ok(output)) => output,
                    Ok(Err(error)) => {
                        terminate_command_tree(&mut child).await;
                        let _ = stdout_task.await;
                        return Err(error);
                    }
                    Err(error) => {
                        terminate_command_tree(&mut child).await;
                        let _ = stdout_task.await;
                        return Err(anyhow::anyhow!("docker stderr reader failed: {error}"));
                    }
                };
                let status = child.wait().await?;
                let stdout = stdout_task.await.map_err(|error| anyhow::anyhow!("docker stdout reader failed: {error}"))??;
                Ok((stdout, stderr, status))
            }
            status = child.wait() => {
                let status = status?;
                let stdout = stdout_task.await.map_err(|error| anyhow::anyhow!("docker stdout reader failed: {error}"))??;
                let stderr = stderr_task.await.map_err(|error| anyhow::anyhow!("docker stderr reader failed: {error}"))??;
                Ok((stdout, stderr, status))
            }
        }
    };
    match tokio::time::timeout(COMPOSE_COMMAND_TIMEOUT, collect).await {
        Ok(result) => result,
        Err(_) => {
            terminate_command_tree(&mut child).await;
            Err(anyhow::anyhow!("docker compose command timed out"))
        }
    }
}

pub async fn list_external_containers() -> Result<Vec<u8>> {
    const MAX_EXTERNAL_DETECT_BYTES: usize = 1024 * 1024;
    let child = tokio::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let (stdout, _stderr, status) = run_bounded_command(child, MAX_EXTERNAL_DETECT_BYTES, MAX_COMPOSE_LOG_STREAM_BYTES).await?;
    if !status.success() {
        anyhow::bail!("docker container discovery failed");
    }
    Ok(stdout)
}

pub async fn logs_compose(project_name: &str, compose_path: &std::path::Path, tail: usize) -> Result<String> {
    let child = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["logs", "--no-color", "--tail"])
        .arg(tail.to_string())
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let (stdout, stderr, status) = run_bounded_command(child, MAX_COMPOSE_LOG_STREAM_BYTES, MAX_COMPOSE_LOG_STREAM_BYTES).await?;
    if !status.success() {
        anyhow::bail!("docker compose logs failed");
    }
    let _stderr = stderr;
    Ok(String::from_utf8(stdout)?)
}

#[derive(Debug, serde::Serialize)]
pub struct ComposeContainer {
    pub name: String,
    pub service: String,
    pub image: String,
    pub state: String,
    pub status: String,
    pub ports: Vec<String>,
}

pub async fn status_compose(project_name: &str, compose_path: &std::path::Path) -> Result<Vec<ComposeContainer>> {
    let child = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["ps", "--format", "json"])
        .process_group(0)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let (stdout, _stderr, status) = run_bounded_command(child, MAX_COMPOSE_LOG_STREAM_BYTES, MAX_COMPOSE_LOG_STREAM_BYTES).await?;
    if !status.success() {
        anyhow::bail!("docker compose status failed");
    }

    let stdout_bytes = stdout;
    let stdout = String::from_utf8(stdout_bytes)
        .map_err(|error| anyhow::anyhow!("docker compose status returned invalid UTF-8: {error}"))?;
    parse_compose_status_output(&stdout)
}

fn parse_compose_status_output(stdout: &str) -> Result<Vec<ComposeContainer>> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let values = if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        match value {
            serde_json::Value::Array(values) => values,
            serde_json::Value::Object(_) => vec![value],
            _ => anyhow::bail!("docker compose status returned a non-object JSON value"),
        }
    } else {
        stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str::<serde_json::Value>(line.trim()))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| anyhow::anyhow!("invalid docker compose status JSON: {error}"))?
    };

    values
        .into_iter()
        .map(|value| {
            let object = value
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("docker compose status record is not an object"))?;
            for key in ["Name", "Service", "Image", "State", "Status"] {
                if !object.get(key).is_some_and(serde_json::Value::is_string) {
                    anyhow::bail!("docker compose status record is missing a required field");
                }
            }
            let ports = match object.get("Publishers") {
                None => Vec::new(),
                Some(value) => value
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("docker compose Publishers is not an array"))?
                    .iter()
                    .map(|publisher| {
                        let publisher = publisher
                            .as_object()
                            .ok_or_else(|| anyhow::anyhow!("docker compose publisher is not an object"))?;
                        let published = publisher
                            .get("PublishedPort")
                            .and_then(serde_json::Value::as_u64)
                            .filter(|port| *port > 0 && *port <= u16::MAX as u64)
                            .ok_or_else(|| anyhow::anyhow!("docker compose publisher has an invalid published port"))?;
                        let target = publisher
                            .get("TargetPort")
                            .and_then(serde_json::Value::as_u64)
                            .filter(|port| *port > 0 && *port <= u16::MAX as u64)
                            .ok_or_else(|| anyhow::anyhow!("docker compose publisher has an invalid target port"))?;
                        let protocol = publisher
                            .get("Protocol")
                            .and_then(serde_json::Value::as_str)
                            .ok_or_else(|| anyhow::anyhow!("docker compose publisher has an invalid protocol"))?;
                        Ok(format!("{published}->{target}/{protocol}"))
                    })
                    .collect::<Result<Vec<_>>>()?,
            };
            Ok(ComposeContainer {
                name: object.get("Name").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                service: object.get("Service").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                image: object.get("Image").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                state: object.get("State").and_then(serde_json::Value::as_str).unwrap_or("unknown").to_string(),
                status: object.get("Status").and_then(serde_json::Value::as_str).unwrap_or_default().to_string(),
                ports,
            })
        })
        .collect()
}

#[cfg(test)]
mod log_output_tests {
    #[test]
    fn status_parser_rejects_malformed_records_and_accepts_arrays() {
        assert!(super::parse_compose_status_output("{not-json}").is_err());
        assert!(super::parse_compose_status_output("[{\"Name\":\"missing-fields\"}]").is_err());
        let parsed = super::parse_compose_status_output(
            r#"[{"Name":"app-1","Service":"app","Image":"image","State":"running","Status":"Up","Publishers":[]}]"#
        )
        .unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "app-1");
    }

    #[tokio::test]
    async fn bounded_reader_drains_but_rejects_oversized_output() {
        let result = super::read_bounded_output(std::io::Cursor::new(
            vec![b'x'; super::MAX_COMPOSE_LOG_STREAM_BYTES + 1],
        ), super::MAX_COMPOSE_LOG_STREAM_BYTES)
        .await;

        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn bounded_command_kills_a_writer_after_stream_overflow() {
        let child = tokio::process::Command::new("sh")
            .args(["-c", "yes"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            super::run_bounded_command(
                child,
                super::MAX_COMPOSE_LOG_STREAM_BYTES,
                super::MAX_COMPOSE_LOG_STREAM_BYTES,
            ),
        )
        .await
        .expect("overflow should terminate before the test timeout");

        assert!(result.is_err());
    }
}
