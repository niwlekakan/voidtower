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

use anyhow::{anyhow, bail, Context, Result};
use reqwest::{Client, Method, RequestBuilder, Response};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::{CString, OsString},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

#[cfg(unix)]
use nix::{
    fcntl::{open, openat, OFlag},
    sys::stat::{fstat, SFlag},
};
#[cfg(unix)]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::process::CommandExt;

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

struct PackagedBinary {
    path: PathBuf,
    #[cfg(unix)]
    _identity_fds: Vec<OwnedFd>,
}

impl PackagedBinary {
    #[cfg(test)]
    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(unix)]
fn descriptor_command(
    binary: &PackagedBinary,
    args: &[&str],
    overrides: &[(&str, &str)],
) -> Command {
    let mut command = Command::new("/bin/true");
    let mut argv = vec![binary.path.as_os_str().as_bytes().to_vec()];
    argv.extend(args.iter().map(|argument| argument.as_bytes().to_vec()));
    let argv = argv
        .into_iter()
        .map(|argument| CString::new(argument).expect("golden-path argument has no NUL"))
        .collect::<Vec<_>>();
    let mut environment = std::env::vars_os().collect::<BTreeMap<OsString, OsString>>();
    for (key, value) in overrides {
        environment.insert(OsString::from(key), OsString::from(value));
    }
    let environment = environment
        .into_iter()
        .map(|(key, value)| {
            let mut entry = key.as_os_str().as_bytes().to_vec();
            entry.push(b'=');
            entry.extend(value.as_os_str().as_bytes());
            CString::new(entry).expect("golden-path environment has no NUL")
        })
        .collect::<Vec<_>>();
    let argv_ptrs = argv
        .iter()
        .map(|argument| argument.as_ptr() as usize)
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let environment_ptrs = environment
        .iter()
        .map(|entry| entry.as_ptr() as usize)
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let binary_fd = binary
        ._identity_fds
        .last()
        .expect("selected binary descriptor is retained")
        .as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            // Keep the CString storage alive until exec without allocating after fork.
            let _ = (&argv, &environment);
            let result = nix::libc::syscall(
                nix::libc::SYS_execveat,
                binary_fd,
                c"".as_ptr(),
                argv_ptrs.as_ptr() as *const *const nix::libc::c_char,
                environment_ptrs.as_ptr() as *const *const nix::libc::c_char,
                nix::libc::AT_EMPTY_PATH,
            );
            if result == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                unreachable!("execveat returned after successful execution")
            }
        });
    }
    command
}

#[cfg(not(unix))]
fn descriptor_command(
    binary: &PackagedBinary,
    args: &[&str],
    overrides: &[(&str, &str)],
) -> Command {
    let mut command = Command::new(&binary.path);
    command.args(args).envs(overrides.iter().copied());
    command
}

#[cfg(unix)]
fn open_executable(path: &Path, label: &str) -> Result<PackagedBinary> {
    let mut components = path.components();
    if !matches!(components.next(), Some(std::path::Component::RootDir)) {
        bail!("{label} must be an absolute path");
    }
    let mut path_components: Vec<_> = components.collect();
    let std::path::Component::Normal(binary_name) = path_components
        .pop()
        .context("executable path has no final component")?
    else {
        bail!("{label} must name a regular executable");
    };

    let directory_flags = OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW;
    let root_fd = open("/", directory_flags, nix::sys::stat::Mode::empty())
        .context("open filesystem root without following links")?;
    let mut identity_fds = vec![unsafe { OwnedFd::from_raw_fd(root_fd) }];
    for component in path_components {
        let std::path::Component::Normal(name) = component else {
            bail!("packaged binary path contains an unsafe parent component");
        };
        let parent_fd = identity_fds
            .last()
            .context("packaged binary parent descriptor is missing")?;
        let next_fd = openat(
            parent_fd.as_raw_fd(),
            name,
            directory_flags,
            nix::sys::stat::Mode::empty(),
        )
        .with_context(|| {
            format!("open packaged binary directory {name:?} without following links")
        })?;
        identity_fds.push(unsafe { OwnedFd::from_raw_fd(next_fd) });
    }

    let parent_fd = identity_fds
        .last()
        .context("packaged binary directory descriptor is missing")?;
    let binary_fd = openat(
        parent_fd.as_raw_fd(),
        binary_name,
        OFlag::O_RDONLY | OFlag::O_NOFOLLOW,
        nix::sys::stat::Mode::empty(),
    )
    .with_context(|| format!("open {label} without following links"))?;
    let binary_fd = unsafe { OwnedFd::from_raw_fd(binary_fd) };
    let binary_stat = fstat(binary_fd.as_raw_fd()).context("inspect packaged binary identity")?;
    let binary_type = SFlag::from_bits_truncate(binary_stat.st_mode);
    if !binary_type.contains(SFlag::S_IFREG) || binary_stat.st_mode & 0o111 == 0 {
        bail!("{label} is not a regular executable");
    }
    let descriptor_path = PathBuf::from(format!("/proc/self/fd/{}", binary_fd.as_raw_fd()));
    identity_fds.push(binary_fd);
    Ok(PackagedBinary {
        path: descriptor_path,
        _identity_fds: identity_fds,
    })
}

#[cfg(unix)]
fn packaged_binary_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Cargo manifest directory must have a repository parent")
        .join("release-package")
        .join("extracted")
        .join("voidtower")
}

#[cfg(unix)]
fn packaged_binary_is_present(path: &Path) -> Result<bool> {
    let mut current = PathBuf::from("/");
    for component in path.components().skip(1) {
        let std::path::Component::Normal(name) = component else {
            bail!("packaged binary path contains an unsafe component");
        };
        current.push(name);
        let metadata = match std::fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error).with_context(|| format!("inspect {current:?}")),
        };
        if metadata.file_type().is_symlink() {
            bail!("packaged binary path contains a symlink: {current:?}");
        }
        if current != path && !metadata.is_dir() {
            bail!("packaged binary parent is not a directory: {current:?}");
        }
    }
    Ok(true)
}

#[cfg(unix)]
fn open_packaged_binary_at(path: &Path) -> Result<PackagedBinary> {
    open_executable(path, "packaged VoidTower binary")
}

#[cfg(unix)]
fn resolve_voidtower_binary(
    packaged_path: &Path,
    debug_path: &Path,
    require_packaged: bool,
) -> Result<PackagedBinary> {
    if packaged_binary_is_present(packaged_path)? {
        return open_packaged_binary_at(packaged_path).with_context(|| {
            format!(
                "packaged VoidTower binary is present but missing or unsafe at {}",
                packaged_path.display()
            )
        });
    }
    if require_packaged {
        bail!("packaged VoidTower binary is required but missing");
    }
    open_executable(debug_path, "debug VoidTower binary").with_context(|| {
        format!(
            "open debug VoidTower binary at {}; run `cargo build --bin voidtower` first",
            debug_path.display()
        )
    })
}

fn voidtower_bin() -> Result<PackagedBinary> {
    let require_packaged = std::env::var("VOIDTOWER_REQUIRE_PACKAGED").as_deref() == Ok("1");

    #[cfg(unix)]
    {
        let packaged_path = packaged_binary_path();
        let example = std::env::current_exe().context("resolve golden-path executable")?;
        let debug_dir = example
            .parent()
            .and_then(Path::parent)
            .context("derive Cargo target directory")?;
        resolve_voidtower_binary(
            &packaged_path,
            &debug_dir.join("voidtower"),
            require_packaged,
        )
    }

    #[cfg(not(unix))]
    bail!("secure VoidTower binary resolution is unavailable on this platform");
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
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

async fn wait_for_container_status(
    container: &str,
    expected: &str,
    timeout: Duration,
) -> Result<()> {
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
    _binary: PackagedBinary,
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

        let mut create_user = descriptor_command(
            &binary,
            &[
                "user",
                "create",
                "--username",
                &username,
                "--password",
                &password,
                "--role",
                "admin",
            ],
            &[
                ("VOIDTOWER_DATA_DIR", data_dir.to_str().expect("data path")),
                (
                    "VOIDTOWER_CONFIG_DIR",
                    config_dir.to_str().expect("config path"),
                ),
            ],
        );
        let status = create_user
            .stdout(Stdio::null())
            .status()
            .context("create golden-path admin")?;
        if !status.success() {
            bail!("VoidTower CLI failed to create the golden-path admin");
        }

        let port = free_port()?;
        let mut server_command = descriptor_command(
            &binary,
            &[],
            &[
                ("VOIDTOWER_DATA_DIR", data_dir.to_str().expect("data path")),
                (
                    "VOIDTOWER_HOST_DATA_DIR",
                    data_dir.to_str().expect("host data path"),
                ),
                (
                    "VOIDTOWER_CONFIG_DIR",
                    config_dir.to_str().expect("config path"),
                ),
                (
                    "VOIDTOWER_CATALOG_DIR",
                    catalog_dir().to_str().expect("catalog path"),
                ),
                ("VOIDTOWER_BIND", "127.0.0.1"),
                ("VOIDTOWER_PORT", &port.to_string()),
                ("RESTIC_PASSWORD", &restic_password),
                ("RUST_LOG", "warn"),
            ],
        );
        let child = server_command
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

        Ok(Self {
            child,
            client,
            base_url,
            session_cookie,
            data_dir,
            _binary: binary,
        })
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
        self.data_dir
            .join("apps")
            .join(project)
            .join("docker-compose.yml")
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

/// App Vault deployment is gated behind the canonical operation adapter in this
/// tree, so `POST /api/apps/deploy` fails closed with `feature_unavailable`.
/// Exactly that response means "not available in this build", which the golden
/// path reports and skips rather than treating as a regression.
const APP_VAULT_UNAVAILABLE: &str = "app-vault-deployment-unavailable";

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
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE && body.contains("feature_unavailable") {
            return Err(anyhow!(APP_VAULT_UNAVAILABLE));
        }
        bail!("App Vault deploy failed with {status}: {body}");
    }

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

    Ok(VaultwardenDeployment {
        project,
        container,
        volume,
        _cleanup: cleanup,
    })
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

fn is_app_vault_unavailable(error: &anyhow::Error) -> bool {
    error.to_string() == APP_VAULT_UNAVAILABLE
}

async fn app_vault_deploy_reaches_healthy_state_end_to_end(server: &TestServer) -> Result<()> {
    println!("golden path: App Vault deploy reaches service readiness");
    let _deployment = match deploy_vaultwarden(server).await {
        Ok(deployment) => deployment,
        Err(error) if is_app_vault_unavailable(&error) => {
            println!(
                "golden path: App Vault deployment is unavailable in this build \
                 (feature_unavailable); skipping the App Vault section"
            );
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    Ok(())
}

async fn app_vault_teardown_removes_containers_and_optionally_volumes(
    server: &TestServer,
) -> Result<()> {
    println!("golden path: App Vault stop retains data and purge removes it");
    let deployment = match deploy_vaultwarden(server).await {
        Ok(deployment) => deployment,
        Err(error) if is_app_vault_unavailable(&error) => {
            println!(
                "golden path: App Vault stop and purge are unavailable in this build \
                 (feature_unavailable); skipping"
            );
            return Ok(());
        }
        Err(error) => return Err(error),
    };

    let response = server
        .request(
            Method::POST,
            &format!("/api/apps/{}/stop", deployment.project),
        )
        .send()
        .await
        .context("send App Vault stop request")?;
    expect_success(response, "App Vault stop").await?;
    wait_for_container_removed(&deployment.container, Duration::from_secs(45)).await?;
    if !docker_volume_exists(&deployment.volume).await? {
        bail!("stopping an app unexpectedly removed its named volume");
    }

    let response = server
        .request(
            Method::POST,
            &format!("/api/apps/{}/purge", deployment.project),
        )
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
    let created: Value = expect_success(response, "backup creation")
        .await?
        .json()
        .await?;
    let id = created["id"]
        .as_str()
        .context("backup creation omitted id")?;

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
    let check: Value = expect_success(response, "restic check")
        .await?
        .json()
        .await?;
    if check["status"] != "ok" {
        bail!("restic check did not succeed: {check}");
    }

    let response = server
        .request(Method::POST, &format!("/api/backups/{id}/restore-test"))
        .send()
        .await
        .context("send restore-test request")?;
    let restore: Value = expect_success(response, "restore-test")
        .await?
        .json()
        .await?;
    if restore["status"] != "ok" {
        bail!("restore-test did not succeed: {restore}");
    }

    let response = server
        .request(Method::GET, "/api/backups")
        .send()
        .await
        .context("send backup list request")?;
    let list: Value = expect_success(response, "backup list")
        .await?
        .json()
        .await?;
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

    let response = server
        .request(Method::DELETE, &format!("/api/backups/{id}"))
        .send()
        .await
        .context("send backup configuration deletion request")?;
    expect_success(response, "backup configuration deletion").await?;

    let response = server
        .request(Method::GET, "/api/backups")
        .send()
        .await
        .context("list backups after configuration deletion")?;
    let list: Value = expect_success(response, "backup list after deletion")
        .await?
        .json()
        .await?;
    if list["configs"]
        .as_array()
        .context("backup list omitted configs")?
        .iter()
        .any(|config| config["id"].as_str() == Some(id))
    {
        bail!("deleted backup configuration remained visible");
    }

    let snapshots = Command::new("restic")
        .args(["-r"])
        .arg(&repo_path)
        .args(["snapshots", "--json"])
        .output()
        .context("inspect restic repository after config deletion")?;
    if !snapshots.status.success()
        || serde_json::from_slice::<Vec<Value>>(&snapshots.stdout)
            .map(|values| values.is_empty())
            .unwrap_or(true)
    {
        bail!("deleting backup configuration removed or damaged repository data");
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

#[cfg(all(test, unix))]
mod binary_selection_tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock is after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("voidtower-golden-selection-{nonce}"));
        fs::create_dir_all(&root).expect("create binary-selection fixture root");
        root
    }

    fn executable_working_root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock is after the Unix epoch")
            .as_nanos();
        let root = std::env::current_dir()
            .expect("read test working directory")
            .join(format!(".voidtower-golden-selection-{nonce}"));
        fs::create_dir_all(&root).expect("create executable fixture root");
        root
    }

    fn executable(path: &Path) {
        fs::write(path, b"fixture binary").expect("write executable fixture");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("make executable fixture executable");
    }

    #[test]
    fn missing_package_uses_only_the_descriptor_bound_debug_fallback() {
        let root = temporary_root();
        let packaged = root.join("release-package/extracted/voidtower");
        let debug = root.join("target/debug/voidtower");
        fs::create_dir_all(packaged.parent().expect("packaged parent")).expect("packaged dirs");
        fs::create_dir_all(debug.parent().expect("debug parent")).expect("debug dirs");
        executable(&debug);

        let selected = resolve_voidtower_binary(&packaged, &debug, false).expect("debug fallback");
        assert!(selected.path().starts_with("/proc/self/fd/"));
        assert_eq!(
            fs::read(selected.path()).expect("read retained debug descriptor"),
            b"fixture binary"
        );
        fs::remove_dir_all(root).expect("remove binary-selection fixture");
    }

    #[test]
    fn present_unsafe_package_fails_closed_instead_of_using_debug_fallback() {
        let root = temporary_root();
        let packaged_parent = root.join("release-package/extracted");
        let packaged = packaged_parent.join("voidtower");
        let debug = root.join("target/debug/voidtower");
        fs::create_dir_all(&packaged_parent).expect("packaged dirs");
        fs::create_dir_all(debug.parent().expect("debug parent")).expect("debug dirs");
        executable(&debug);
        let outside = root.join("outside");
        executable(&outside);
        symlink(&outside, &packaged).expect("unsafe packaged symlink");

        let error = resolve_voidtower_binary(&packaged, &debug, false)
            .err()
            .expect("unsafe package");
        assert!(error.to_string().contains("symlink"));
        fs::remove_dir_all(root).expect("remove binary-selection fixture");
    }

    #[test]
    fn required_package_does_not_fall_back_when_missing() {
        let root = temporary_root();
        let packaged = root.join("release-package/extracted/voidtower");
        let debug = root.join("target/debug/voidtower");
        fs::create_dir_all(packaged.parent().expect("packaged parent")).expect("packaged dirs");
        fs::create_dir_all(debug.parent().expect("debug parent")).expect("debug dirs");
        executable(&debug);

        let error = resolve_voidtower_binary(&packaged, &debug, true)
            .err()
            .expect("required package");
        assert!(error.to_string().contains("required but missing"));
        fs::remove_dir_all(root).expect("remove binary-selection fixture");
    }

    #[test]
    fn selected_packaged_binary_executes_by_descriptor_after_path_replacement() {
        let root = executable_working_root();
        let packaged_parent = root.join("release-package/extracted");
        let packaged = packaged_parent.join("voidtower");
        let debug = root.join("target/debug/voidtower");
        fs::create_dir_all(&packaged_parent).expect("packaged dirs");
        fs::create_dir_all(debug.parent().expect("debug parent")).expect("debug dirs");
        fs::copy("/bin/true", &packaged).expect("packaged executable");
        fs::set_permissions(&packaged, fs::Permissions::from_mode(0o700))
            .expect("package executable");
        fs::copy("/bin/false", &debug).expect("debug executable");
        fs::set_permissions(&debug, fs::Permissions::from_mode(0o700)).expect("debug executable");

        let selected = resolve_voidtower_binary(&packaged, &debug, true).expect("packaged binary");
        fs::rename(&packaged, packaged.with_extension("replaced"))
            .expect("move original package path");
        fs::copy("/bin/false", &packaged).expect("replacement executable");
        fs::set_permissions(&packaged, fs::Permissions::from_mode(0o700))
            .expect("replacement executable");
        let mut command = descriptor_command(&selected, &[], &[]);
        let output = command
            .output()
            .expect("execute descriptor-selected package");
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stdout.is_empty());
        fs::remove_dir_all(root).expect("remove binary-selection fixture");
    }
}
