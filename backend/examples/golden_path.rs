//! Explicit real-infrastructure acceptance harness for P1-06.
//!
//! Run only in a disposable environment prepared with Docker, Docker Compose,
//! and restic:
//!
//! ```text
//! cargo build --bin voidtower
//! cargo run --example golden_path
//! ```
//!
//! This is an example target rather than an auto-discovered integration test
//! because normal `cargo test` runs must never deploy or delete host resources.

use anyhow::{bail, Context, Result};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{json, Value};
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

const VAULTWARDEN_PORT: u16 = 8085;

#[tokio::main]
async fn main() -> Result<()> {
    assert_dependencies_available()?;

    let server = TestServer::spawn().await?;
    app_vault_deploy_reaches_healthy_state_end_to_end(&server).await?;
    app_vault_teardown_removes_containers_and_optionally_volumes(&server).await?;
    restic_backup_then_restore_test_reports_confidence(&server).await?;
    container_lifecycle_start_stop_restart_reflect_real_docker_state(&server).await?;

    println!("golden path: all real-infrastructure checks passed");
    Ok(())
}

fn assert_dependencies_available() -> Result<()> {
    run_checked("docker", &["info"]).context("Docker daemon is unavailable")?;
    run_checked("docker", &["compose", "version"]).context("Docker Compose v2 is unavailable")?;
    run_checked("restic", &["version"]).context("restic is unavailable")?;
    Ok(())
}

fn run_checked(program: &str, args: &[&str]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("start {program}"))?;
    if !output.status.success() {
        bail!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn voidtower_bin() -> Result<PathBuf> {
    let example = std::env::current_exe().context("resolve golden-path executable")?;
    let debug_dir = example
        .parent()
        .and_then(Path::parent)
        .context("derive Cargo target directory")?;
    let path = debug_dir.join("voidtower");
    if !path.is_file() {
        bail!(
            "VoidTower binary is missing at {}; run `cargo build --bin voidtower` first",
            path.display()
        );
    }
    Ok(path)
}

fn catalog_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../app-vault/apps")
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind an ephemeral port")?;
    Ok(listener.local_addr().context("read ephemeral port")?.port())
}

fn assert_port_available(port: u16) -> Result<()> {
    TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("required App Vault host port {port} is already in use"))?;
    Ok(())
}

fn unique_name(label: &str) -> String {
    format!("vt-golden-{label}-{}", uuid::Uuid::new_v4().simple())
}

async fn expect_success(response: Response, operation: &str) -> Result<Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    bail!("{operation} failed with {status}: {body}")
}

async fn wait_for_http_ok(url: &str, timeout: Duration) -> Result<()> {
    let client = Client::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(response) = client.get(url).send().await {
            if response.status().is_success() {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("{url} did not return success within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn docker_inspect(container: &str, template: &str) -> Result<Option<String>> {
    let output = tokio::process::Command::new("docker")
        .args(["inspect", "--format", template, container])
        .output()
        .await
        .context("run docker inspect")?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).trim().to_string()))
}

async fn wait_for_container_status(container: &str, expected: &str, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let status = docker_inspect(container, "{{.State.Status}}").await?;
        if status.as_deref() == Some(expected) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "container {container} did not reach '{expected}' within {timeout:?}; last state: {status:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_container_removed(container: &str, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if docker_inspect(container, "{{.Id}}").await?.is_none() {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("container {container} was not removed within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn docker_volume_exists(volume: &str) -> Result<bool> {
    let status = tokio::process::Command::new("docker")
        .args(["volume", "inspect", volume])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .context("inspect Docker volume")?;
    Ok(status.success())
}

struct TestServer {
    child: Child,
    client: Client,
    base_url: String,
    session_cookie: String,
    data_dir: PathBuf,
}

impl TestServer {
    async fn spawn() -> Result<Self> {
        let data_dir = std::env::temp_dir().join(unique_name("state"));
        let config_dir = data_dir.join("config");
        std::fs::create_dir_all(&config_dir).context("create test configuration directory")?;

        let username = unique_name("admin");
        let password = unique_name("password");
        let restic_password =
            std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| unique_name("restic"));
        let binary = voidtower_bin()?;

        let status = Command::new(&binary)
            .env("VOIDTOWER_DATA_DIR", &data_dir)
            .env("VOIDTOWER_CONFIG_DIR", &config_dir)
            .args([
                "user",
                "create",
                "--username",
                &username,
                "--password",
                &password,
                "--role",
                "admin",
            ])
            .stdout(Stdio::null())
            .status()
            .context("create golden-path admin")?;
        if !status.success() {
            bail!("VoidTower CLI failed to create the golden-path admin");
        }

        let port = free_port()?;
        let child = Command::new(&binary)
            .env("VOIDTOWER_DATA_DIR", &data_dir)
            .env("VOIDTOWER_HOST_DATA_DIR", &data_dir)
            .env("VOIDTOWER_CONFIG_DIR", &config_dir)
            .env("VOIDTOWER_CATALOG_DIR", catalog_dir())
            .env("VOIDTOWER_BIND", "127.0.0.1")
            .env("VOIDTOWER_PORT", port.to_string())
            .env("RESTIC_PASSWORD", restic_password)
            .env("RUST_LOG", "warn")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start VoidTower server")?;

        let base_url = format!("http://127.0.0.1:{port}");
        wait_for_http_ok(&format!("{base_url}/api/health"), Duration::from_secs(30)).await?;

        let client = Client::new();
        let response = client
            .post(format!("{base_url}/api/auth/login"))
            .json(&json!({ "username": username, "password": password }))
            .send()
            .await
            .context("send login request")?;
        let response = expect_success(response, "login").await?;
        let set_cookie = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .find_map(|value| value.to_str().ok())
            .context("login response omitted Set-Cookie")?;
        let session_cookie = set_cookie
            .split(';')
            .next()
            .and_then(|cookie| cookie.strip_prefix("vt_session="))
            .context("login response omitted vt_session")?
            .to_string();

        Ok(Self { child, client, base_url, session_cookie, data_dir })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn cookie(&self) -> String {
        format!("vt_session={}", self.session_cookie)
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        self.client
            .request(method, self.url(path))
            .header("Cookie", self.cookie())
    }

    fn compose_path(&self, project: &str) -> PathBuf {
        self.data_dir.join("apps").join(project).join("docker-compose.yml")
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

struct ComposeProjectCleanup {
    project: String,
    compose_path: PathBuf,
}

impl Drop for ComposeProjectCleanup {
    fn drop(&mut self) {
        if self.compose_path.is_file() {
            let _ = Command::new("docker")
                .args(["compose", "-p", &self.project, "-f"])
                .arg(&self.compose_path)
                .args(["down", "--volumes", "--remove-orphans"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

struct ContainerCleanup(String);

impl Drop for ContainerCleanup {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

struct VaultwardenDeployment {
    project: String,
    container: String,
    volume: String,
    _cleanup: ComposeProjectCleanup,
}

async fn deploy_vaultwarden(server: &TestServer) -> Result<VaultwardenDeployment> {
    assert_port_available(VAULTWARDEN_PORT)?;

    let project = unique_name("app");
    let cleanup = ComposeProjectCleanup {
        compose_path: server.compose_path(&project),
        project: project.clone(),
    };

    let response = server
        .request(Method::POST, "/api/apps/deploy")
        .json(&json!({ "app_id": "vaultwarden", "project_name": project.clone() }))
        .send()
        .await
        .context("send App Vault deploy request")?;
    expect_success(response, "App Vault deploy").await?;

    let container = format!("{project}-vaultwarden-1");
    let volume = format!("{project}_vaultwarden_data");
    wait_for_container_status(&container, "running", Duration::from_secs(90)).await?;
    wait_for_docker_health_if_configured(&container, Duration::from_secs(90)).await?;
    // Vaultwarden currently has no Compose healthcheck, so its real `/alive`
    // endpoint is also required as the service-readiness boundary.
    wait_for_http_ok(
        &format!("http://127.0.0.1:{VAULTWARDEN_PORT}/alive"),
        Duration::from_secs(90),
    )
    .await?;
    if !docker_volume_exists(&volume).await? {
        bail!("Vaultwarden named volume was not created");
    }

    Ok(VaultwardenDeployment { project, container, volume, _cleanup: cleanup })
}

async fn wait_for_docker_health_if_configured(container: &str, timeout: Duration) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let health = docker_inspect(
            container,
            "{{if .State.Health}}{{.State.Health.Status}}{{else}}not-configured{{end}}",
        )
        .await?
        .context("deployed container disappeared before health check")?;
        match health.as_str() {
            "healthy" | "not-configured" => return Ok(()),
            "unhealthy" => bail!("container {container} entered Docker's unhealthy state"),
            "starting" if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            other => bail!("container {container} did not become healthy: {other}"),
        }
    }
}

async fn app_vault_deploy_reaches_healthy_state_end_to_end(server: &TestServer) -> Result<()> {
    println!("golden path: App Vault deploy reaches service readiness");
    let _deployment = deploy_vaultwarden(server).await?;
    Ok(())
}

async fn app_vault_teardown_removes_containers_and_optionally_volumes(
    server: &TestServer,
) -> Result<()> {
    println!("golden path: App Vault stop retains data and purge removes it");
    let deployment = deploy_vaultwarden(server).await?;

    let response = server
        .request(Method::POST, &format!("/api/apps/{}/stop", deployment.project))
        .send()
        .await
        .context("send App Vault stop request")?;
    expect_success(response, "App Vault stop").await?;
    wait_for_container_removed(&deployment.container, Duration::from_secs(45)).await?;
    if !docker_volume_exists(&deployment.volume).await? {
        bail!("stopping an app unexpectedly removed its named volume");
    }

    let response = server
        .request(Method::POST, &format!("/api/apps/{}/purge", deployment.project))
        .send()
        .await
        .context("send App Vault purge request")?;
    expect_success(response, "App Vault purge").await?;
    if docker_volume_exists(&deployment.volume).await? {
        bail!("purging an app did not remove its named volume");
    }

    Ok(())
}

async fn restic_backup_then_restore_test_reports_confidence(server: &TestServer) -> Result<()> {
    println!("golden path: restic backup, check, restore-test, and confidence");

    let source_path = server.data_dir.join("backup-source");
    let repo_path = server.data_dir.join("backup-repository");
    std::fs::create_dir_all(&source_path).context("create backup source")?;
    std::fs::write(source_path.join("evidence.txt"), b"voidtower golden path\n")
        .context("write backup evidence")?;

    let response = server
        .request(Method::POST, "/api/backups")
        .json(&json!({
            "name": unique_name("backup"),
            "source_path": source_path,
            "repo_path": repo_path,
        }))
        .send()
        .await
        .context("send backup creation request")?;
    let created: Value = expect_success(response, "backup creation").await?.json().await?;
    let id = created["id"].as_str().context("backup creation omitted id")?;

    let response = server
        .request(Method::POST, &format!("/api/backups/{id}/run"))
        .send()
        .await
        .context("send backup run request")?;
    let run: Value = expect_success(response, "backup run").await?.json().await?;
    if run["status"] != "success" {
        bail!("backup run did not succeed: {run}");
    }

    let response = server
        .request(Method::POST, &format!("/api/backups/{id}/check"))
        .send()
        .await
        .context("send restic check request")?;
    let check: Value = expect_success(response, "restic check").await?.json().await?;
    if check["status"] != "ok" {
        bail!("restic check did not succeed: {check}");
    }

    let response = server
        .request(Method::POST, &format!("/api/backups/{id}/restore-test"))
        .send()
        .await
        .context("send restore-test request")?;
    let restore: Value = expect_success(response, "restore-test").await?.json().await?;
    if restore["status"] != "ok" {
        bail!("restore-test did not succeed: {restore}");
    }

    let response = server
        .request(Method::GET, "/api/backups")
        .send()
        .await
        .context("send backup list request")?;
    let list: Value = expect_success(response, "backup list").await?.json().await?;
    let config = list["configs"]
        .as_array()
        .context("backup list omitted configs")?
        .iter()
        .find(|config| config["id"].as_str() == Some(id))
        .context("created backup missing from list")?;

    if config["last_status"] != "success"
        || config["last_check_status"] != "ok"
        || config["last_restore_test_status"] != "ok"
        || config["confidence"] != "high"
    {
        bail!("backup confidence fields are incomplete: {config}");
    }

    Ok(())
}

async fn container_lifecycle_start_stop_restart_reflect_real_docker_state(
    server: &TestServer,
) -> Result<()> {
    println!("golden path: container start, stop, and verified restart");

    let name = unique_name("container");
    let _cleanup = ContainerCleanup(name.clone());
    let status = Command::new("docker")
        .args(["run", "-d", "--name", &name, "busybox:1.36", "sleep", "300"])
        .stdout(Stdio::null())
        .status()
        .context("start lifecycle fixture container")?;
    if !status.success() {
        bail!("Docker failed to start the lifecycle fixture container");
    }
    wait_for_container_status(&name, "running", Duration::from_secs(45)).await?;

    container_action(server, &name, ContainerAction::Stop).await?;
    wait_for_container_status(&name, "exited", Duration::from_secs(45)).await?;

    container_action(server, &name, ContainerAction::Start).await?;
    wait_for_container_status(&name, "running", Duration::from_secs(45)).await?;

    let started_before = docker_inspect(&name, "{{.State.StartedAt}}")
        .await?
        .context("fixture container disappeared before restart")?;
    container_action(server, &name, ContainerAction::Restart).await?;
    wait_for_container_status(&name, "running", Duration::from_secs(45)).await?;
    let started_after = docker_inspect(&name, "{{.State.StartedAt}}")
        .await?
        .context("fixture container disappeared after restart")?;
    if started_before == started_after {
        bail!("container restart did not change Docker's StartedAt timestamp");
    }

    Ok(())
}

#[derive(Clone, Copy)]
enum ContainerAction {
    Start,
    Stop,
    Restart,
}

impl ContainerAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

async fn container_action(
    server: &TestServer,
    container: &str,
    action: ContainerAction,
) -> Result<()> {
    let action_name = action.as_str();
    let response = server
        .request(Method::POST, &format!("/api/containers/{container}/action"))
        .json(&json!({ "action": action_name }))
        .send()
        .await
        .with_context(|| format!("send container {action_name} request"))?;
    expect_success(response, &format!("container {action_name}")).await?;
    Ok(())
}
