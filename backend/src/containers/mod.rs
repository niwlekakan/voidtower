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

// Deploy a Docker Compose app from a compose file string
pub async fn deploy_compose(
    project_name: &str,
    compose_path: &std::path::Path,
) -> Result<()> {
    let output = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["up", "-d", "--build"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose failed: {}", stderr);
    }
    Ok(())
}

pub async fn pull_compose(project_name: &str, compose_path: &std::path::Path) -> Result<()> {
    let out = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .arg("pull")
        .output()
        .await?;
    if !out.status.success() {
        anyhow::bail!("docker compose pull failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

pub async fn stop_compose(project_name: &str, compose_path: &std::path::Path) -> Result<()> {
    tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .arg("down")
        .output()
        .await?;
    Ok(())
}

pub async fn restart_compose(project_name: &str, compose_path: &std::path::Path) -> Result<()> {
    let out = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .arg("restart")
        .output()
        .await?;
    if !out.status.success() {
        anyhow::bail!("docker compose restart failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

pub async fn remove_compose(project_name: &str, compose_path: &std::path::Path) -> Result<()> {
    tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["down", "--volumes", "--remove-orphans"])
        .output()
        .await?;
    Ok(())
}

pub async fn logs_compose(project_name: &str, compose_path: &std::path::Path, tail: usize) -> Result<String> {
    let out = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["logs", "--no-color", "--tail"])
        .arg(tail.to_string())
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr))
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
    let out = tokio::process::Command::new("docker")
        .args(["compose", "-p", project_name, "-f"])
        .arg(compose_path)
        .args(["ps", "--format", "json"])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut containers = Vec::new();

    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let ports: Vec<String> = v["Publishers"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|p| {
                let pub_port = p["PublishedPort"].as_u64()?;
                let target   = p["TargetPort"].as_u64()?;
                let proto    = p["Protocol"].as_str().unwrap_or("tcp");
                if pub_port > 0 {
                    Some(format!("{pub_port}->{target}/{proto}"))
                } else {
                    None
                }
            }).collect())
            .unwrap_or_default();

        containers.push(ComposeContainer {
            name:    v["Name"].as_str().unwrap_or("").to_string(),
            service: v["Service"].as_str().unwrap_or("").to_string(),
            image:   v["Image"].as_str().unwrap_or("").to_string(),
            state:   v["State"].as_str().unwrap_or("unknown").to_string(),
            status:  v["Status"].as_str().unwrap_or("").to_string(),
            ports,
        });
    }
    Ok(containers)
}
