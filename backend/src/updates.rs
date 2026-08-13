//! Low-level host update provider primitives.
//!
//! This module owns host command execution and restart markers. It does not own authorization,
//! approval, operation planning, durable job state, or HTTP response contracts.

use anyhow::{bail, ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Stdio,
};

const ODYSSEUS_INSTALL_DIR: &str = "/opt/odysseus";
const ODYSSEUS_BRANCH: &str = "odysseus-voidlink";
const MAX_PROVIDER_TEXT_CHARS: usize = 4 * 1024;
const MAX_PACKAGE_MANIFEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum UpdateTarget {
    VoidTower,
    Odysseus,
    DockerEngine,
    DockerImage { container_id: String },
    OperatingSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GitSnapshot {
    pub installed: bool,
    pub branch: String,
    pub current_commit: String,
    pub remote_commit: String,
    pub behind: usize,
    pub ahead: usize,
    pub backup_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DockerContainerSnapshot {
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub container_image_id: String,
    pub local_image_id: String,
    pub compose_project: String,
    pub compose_file: String,
    pub compose_service: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum UpdateSnapshot {
    VoidTowerGit(GitSnapshot),
    VoidTowerBinary {
        current_version: String,
        remote_version: String,
    },
    VoidTowerDocker(DockerContainerSnapshot),
    Odysseus(GitSnapshot),
    DockerEngine {
        containers: Vec<DockerContainerSnapshot>,
    },
    DockerImage(DockerContainerSnapshot),
    OperatingSystem {
        package_manager: String,
        packages: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateRequest {
    Check,
    Apply,
    Rollback { tag: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackPoint {
    pub operation_id: String,
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    Completed { message: String },
    RestartInitiated { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconciliationResult {
    Succeeded { message: String },
    Failed { message: String },
    StillUncertain { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OperationMarker {
    operation_id: String,
    target: String,
    rollback_kind: String,
    rollback_reference: String,
    expected_state: Option<String>,
    initiating_process: String,
}

pub async fn snapshot(target: &UpdateTarget) -> Result<UpdateSnapshot> {
    match target {
        UpdateTarget::VoidTower => voidtower_snapshot().await,
        UpdateTarget::Odysseus => Ok(UpdateSnapshot::Odysseus(
            git_snapshot(Path::new(ODYSSEUS_INSTALL_DIR), ODYSSEUS_BRANCH).await?,
        )),
        UpdateTarget::DockerEngine => Ok(UpdateSnapshot::DockerEngine {
            containers: list_running_containers().await?,
        }),
        UpdateTarget::DockerImage { container_id } => Ok(UpdateSnapshot::DockerImage(
            inspect_container(container_id).await?,
        )),
        UpdateTarget::OperatingSystem => {
            let package_manager =
                detect_package_manager().context("no supported package manager")?;
            let packages = list_upgradable_packages(package_manager).await?;
            Ok(UpdateSnapshot::OperatingSystem {
                package_manager: display_package_manager(package_manager).into(),
                packages,
            })
        }
    }
}

pub async fn prepare_rollback(target: &UpdateTarget, operation_id: &str) -> Result<RollbackPoint> {
    validate_operation_id(operation_id)?;
    let (kind, reference): (String, String) = match target {
        UpdateTarget::VoidTower if is_docker() => {
            let own = own_container().await?;
            ("docker_image".into(), own.container_image_id)
        }
        UpdateTarget::VoidTower if is_dev_install() => {
            let root = project_root().context("cannot locate VoidTower project root")?;
            let tag = format!("vt-backup-job-{}", operation_id.replace('-', ""));
            run_checked(&root, "git", &["tag", &tag]).await?;
            ("git_tag".into(), tag)
        }
        UpdateTarget::VoidTower => {
            let executable = std::env::current_exe().context("cannot locate VoidTower binary")?;
            let rollback_dir = marker_root().join("binaries");
            std::fs::create_dir_all(&rollback_dir)
                .context("cannot create VoidTower rollback directory")?;
            let backup = rollback_dir.join(format!("{operation_id}.voidtower"));
            std::fs::copy(&executable, &backup).context("cannot snapshot VoidTower binary")?;
            ("binary".into(), backup.to_string_lossy().into_owned())
        }
        UpdateTarget::Odysseus => {
            let tag = format!("vt-backup-job-{}", operation_id.replace('-', ""));
            run_checked(Path::new(ODYSSEUS_INSTALL_DIR), "git", &["tag", &tag]).await?;
            ("git_tag".into(), tag)
        }
        UpdateTarget::DockerImage { container_id } => {
            let container = inspect_container(container_id).await?;
            ("docker_image".into(), container.container_image_id)
        }
        UpdateTarget::OperatingSystem => {
            let manager = detect_package_manager().context("no supported package manager")?;
            let manifest = installed_package_manifest(manager).await?;
            let manifest_path = marker_root().join(format!("{operation_id}.packages"));
            create_marker_root()?;
            std::fs::write(&manifest_path, manifest)
                .context("cannot persist installed-package snapshot")?;
            (
                "package_manifest".into(),
                manifest_path.to_string_lossy().into_owned(),
            )
        }
        UpdateTarget::DockerEngine => bail!("Docker engine checks do not create rollback points"),
    };
    write_marker(&OperationMarker {
        operation_id: operation_id.into(),
        target: target_name(target).into(),
        rollback_kind: kind.clone(),
        rollback_reference: reference.clone(),
        expected_state: None,
        initiating_process: process_identity(),
    })?;
    Ok(RollbackPoint {
        operation_id: operation_id.into(),
        kind,
        reference,
    })
}

pub async fn execute(
    target: &UpdateTarget,
    request: &UpdateRequest,
    operation_id: &str,
) -> Result<ExecutionResult> {
    validate_operation_id(operation_id)?;
    match (target, request) {
        (UpdateTarget::VoidTower, UpdateRequest::Check) => check_voidtower().await,
        (UpdateTarget::DockerEngine, UpdateRequest::Check) => check_docker_images().await,
        (UpdateTarget::VoidTower, UpdateRequest::Apply) => apply_voidtower(operation_id).await,
        (UpdateTarget::VoidTower, UpdateRequest::Rollback { tag }) => {
            rollback_voidtower(operation_id, tag).await
        }
        (UpdateTarget::Odysseus, UpdateRequest::Apply) => apply_odysseus(operation_id).await,
        (UpdateTarget::DockerImage { container_id }, UpdateRequest::Apply) => {
            apply_docker_image(container_id, operation_id).await
        }
        (UpdateTarget::OperatingSystem, UpdateRequest::Apply) => apply_os(operation_id).await,
        _ => bail!("update target and request do not match"),
    }
}

pub async fn reconcile(
    target: &UpdateTarget,
    request: &UpdateRequest,
    operation_id: &str,
) -> Result<ReconciliationResult> {
    validate_operation_id(operation_id)?;
    let marker = read_marker(operation_id)?;
    match (target, request) {
        (UpdateTarget::VoidTower, UpdateRequest::Apply | UpdateRequest::Rollback { .. }) => {
            let expected = marker
                .expected_state
                .as_deref()
                .context("self-update marker has no expected state")?;
            let observed = current_voidtower_state().await?;
            let restarted = marker.initiating_process != process_identity();
            if restarted && observed == expected {
                Ok(ReconciliationResult::Succeeded {
                    message: "The restarted VoidTower process matches the requested version".into(),
                })
            } else if restarted {
                Ok(ReconciliationResult::Failed {
                    message: "VoidTower restarted but does not match the requested version".into(),
                })
            } else {
                Ok(ReconciliationResult::StillUncertain {
                    message: "Waiting for VoidTower to restart into the requested version".into(),
                })
            }
        }
        (UpdateTarget::Odysseus, UpdateRequest::Apply) => {
            let UpdateSnapshot::Odysseus(snapshot) = snapshot(target).await? else {
                unreachable!()
            };
            compare_git_snapshot(&snapshot, "Odysseus")
        }
        (UpdateTarget::DockerImage { .. }, UpdateRequest::Apply) => {
            let UpdateSnapshot::DockerImage(snapshot) = snapshot(target).await? else {
                unreachable!()
            };
            if !snapshot.local_image_id.is_empty()
                && snapshot.container_image_id == snapshot.local_image_id
            {
                Ok(ReconciliationResult::Succeeded {
                    message: "The container is running the current local image".into(),
                })
            } else {
                Ok(ReconciliationResult::StillUncertain {
                    message: "The container image does not yet match the pulled image".into(),
                })
            }
        }
        (UpdateTarget::OperatingSystem, UpdateRequest::Apply) => {
            let UpdateSnapshot::OperatingSystem { packages, .. } = snapshot(target).await? else {
                unreachable!()
            };
            if packages.is_empty() {
                Ok(ReconciliationResult::Succeeded {
                    message: "No remaining OS package updates were observed".into(),
                })
            } else {
                Ok(ReconciliationResult::StillUncertain {
                    message: "OS packages remain upgradable after the interrupted operation".into(),
                })
            }
        }
        (_, UpdateRequest::Check) => Ok(ReconciliationResult::StillUncertain {
            message: "A refresh check can be safely submitted as a new job".into(),
        }),
        _ => bail!("update target and request do not match"),
    }
}

pub async fn rollback_prepared(target: &UpdateTarget, operation_id: &str) -> Result<bool> {
    validate_operation_id(operation_id)?;
    if let Ok(marker) = read_marker(operation_id) {
        return Ok(marker.target == target_name(target) && !marker.rollback_reference.is_empty());
    }
    match target {
        UpdateTarget::VoidTower if is_dev_install() => {
            let root = project_root().context("cannot locate VoidTower project root")?;
            let tag = format!("vt-backup-job-{}", operation_id.replace('-', ""));
            Ok(run_allowing_status(
                &root,
                "git",
                &["rev-parse", "--verify", &format!("{tag}^{{commit}}")],
                &[0],
            )
            .await
            .is_ok())
        }
        UpdateTarget::Odysseus => {
            let tag = format!("vt-backup-job-{}", operation_id.replace('-', ""));
            Ok(run_allowing_status(
                Path::new(ODYSSEUS_INSTALL_DIR),
                "git",
                &["rev-parse", "--verify", &format!("{tag}^{{commit}}")],
                &[0],
            )
            .await
            .is_ok())
        }
        UpdateTarget::VoidTower => Ok(marker_root()
            .join("binaries")
            .join(format!("{operation_id}.voidtower"))
            .exists()),
        UpdateTarget::OperatingSystem => Ok(marker_root()
            .join(format!("{operation_id}.packages"))
            .exists()),
        UpdateTarget::DockerImage { .. } => Ok(false),
        UpdateTarget::DockerEngine => Ok(false),
    }
}

async fn voidtower_snapshot() -> Result<UpdateSnapshot> {
    if is_docker() {
        return Ok(UpdateSnapshot::VoidTowerDocker(own_container().await?));
    }
    if is_dev_install() {
        let root = project_root().context("cannot locate VoidTower project root")?;
        let branch = current_branch(&root).await?;
        return Ok(UpdateSnapshot::VoidTowerGit(
            git_snapshot(&root, &branch).await?,
        ));
    }
    let current_version = current_binary_version();
    let remote_version = latest_release_version()
        .await
        .unwrap_or_else(|_| "unknown".into());
    Ok(UpdateSnapshot::VoidTowerBinary {
        current_version,
        remote_version,
    })
}

async fn check_voidtower() -> Result<ExecutionResult> {
    if !is_docker() {
        let current = snapshot(&UpdateTarget::VoidTower).await?;
        return Ok(ExecutionResult::Completed {
            message: safe_text(&format!("VoidTower update state refreshed: {current:?}")),
        });
    }
    let before = own_container().await?;
    run_checked(Path::new("/"), "docker", &["pull", &before.image]).await?;
    let local_image_id = docker_image_id(&before.image).await?;
    Ok(ExecutionResult::Completed {
        message: if before.container_image_id == local_image_id {
            "VoidTower image is up to date".into()
        } else {
            "A newer VoidTower image is pulled and ready to apply".into()
        },
    })
}

async fn check_docker_images() -> Result<ExecutionResult> {
    let containers = list_running_containers().await?;
    let images: BTreeSet<String> = containers.into_iter().map(|item| item.image).collect();
    let mut updated = 0usize;
    for image in &images {
        let before = docker_image_id(image).await.unwrap_or_default();
        run_checked(Path::new("/"), "docker", &["pull", image]).await?;
        let after = docker_image_id(image).await?;
        updated += usize::from(!before.is_empty() && before != after);
    }
    Ok(ExecutionResult::Completed {
        message: format!(
            "Checked {} Docker image(s); {updated} newer image(s) were pulled",
            images.len()
        ),
    })
}

async fn apply_voidtower(operation_id: &str) -> Result<ExecutionResult> {
    if is_docker() {
        let own = own_container().await?;
        run_checked(Path::new("/"), "docker", &["pull", &own.image]).await?;
        let expected = docker_image_id(&own.image).await?;
        set_expected_state(operation_id, &expected)?;
        let mut command = if usable_compose(&own) {
            let mut command = std::process::Command::new("docker");
            command.args([
                "compose",
                "-p",
                &own.compose_project,
                "-f",
                &own.compose_file,
                "up",
                "-d",
                &own.compose_service,
            ]);
            command
        } else {
            let mut command = std::process::Command::new("docker");
            command.args(["restart", &own.container_id]);
            command
        };
        spawn_detached(&mut command, "VoidTower Docker restart")?;
        return Ok(ExecutionResult::RestartInitiated {
            message: "VoidTower container restart was initiated".into(),
        });
    }

    if is_dev_install() {
        let root = project_root().context("cannot locate VoidTower project root")?;
        let branch = current_branch(&root).await?;
        let expected = remote_commit(&root, &branch).await?;
        set_expected_state(operation_id, &expected)?;
        let script = marker_root().join(format!("{operation_id}.apply.sh"));
        let executable = root.join("backend/target/release/voidtower");
        let body = format!(
            "#!/bin/sh\nset -eu\ncd {}\ngit fetch origin {}\ngit merge --ff-only {}\ncargo build --manifest-path backend/Cargo.toml --release\nnpm --prefix frontend run build\nkill -TERM {}\nsleep 1\nexec {} >> /tmp/voidtower.log 2>&1\n",
            shell_quote(&root.to_string_lossy()),
            shell_quote(&branch),
            shell_quote(&expected),
            std::process::id(),
            shell_quote(&executable.to_string_lossy()),
        );
        write_script(&script, &body)?;
        spawn_script(&script, "VoidTower source update")?;
        return Ok(ExecutionResult::RestartInitiated {
            message: "VoidTower source update and restart were initiated".into(),
        });
    }

    let version = latest_release_version().await?;
    set_expected_state(operation_id, &version)?;
    let install_dir = install_dir();
    let script = marker_root().join(format!("{operation_id}.apply.sh"));
    let body = format!(
        "#!/bin/sh\nset -eu\narch=$(uname -m)\nversion={}\ninstall_dir={}\narchive=voidtower-$version-$arch-unknown-linux-musl.tar.gz\ncurl -fsSL --max-time 120 https://github.com/niwlekakan/voidtower/releases/download/v$version/$archive -o /tmp/vt-update-$version.tar.gz\ntar -xzf /tmp/vt-update-$version.tar.gz -C /tmp voidtower\ninstall -m 0755 /tmp/voidtower \"$install_dir/voidtower\"\nprintf '%s\\n' \"$version\" > \"$install_dir/.version\"\nkill -TERM {}\n",
        shell_quote(&version),
        shell_quote(&install_dir.to_string_lossy()),
        std::process::id(),
    );
    write_script(&script, &body)?;
    spawn_script(&script, "VoidTower binary update")?;
    Ok(ExecutionResult::RestartInitiated {
        message: "VoidTower binary update and restart were initiated".into(),
    })
}

async fn rollback_voidtower(operation_id: &str, tag: &str) -> Result<ExecutionResult> {
    validate_backup_tag(tag)?;
    ensure!(!is_docker(), "git rollback is unavailable in Docker mode");
    ensure!(
        is_dev_install(),
        "git rollback is unavailable for binary installs"
    );
    let root = project_root().context("cannot locate VoidTower project root")?;
    let branch = current_branch(&root).await?;
    run_checked(
        &root,
        "git",
        &["rev-parse", "--verify", &format!("{tag}^{{commit}}")],
    )
    .await?;
    let expected = run_checked(&root, "git", &["rev-parse", tag]).await?;
    set_expected_state(operation_id, expected.trim())?;
    let script = marker_root().join(format!("{operation_id}.rollback.sh"));
    let executable = root.join("backend/target/release/voidtower");
    let body = format!(
        "#!/bin/sh\nset -eu\ncd {}\ngit checkout {}\ngit reset --hard {}\ncargo build --manifest-path backend/Cargo.toml --release\nnpm --prefix frontend run build\nkill -TERM {}\nsleep 1\nexec {} >> /tmp/voidtower.log 2>&1\n",
        shell_quote(&root.to_string_lossy()),
        shell_quote(&branch),
        shell_quote(tag),
        std::process::id(),
        shell_quote(&executable.to_string_lossy()),
    );
    write_script(&script, &body)?;
    spawn_script(&script, "VoidTower rollback")?;
    Ok(ExecutionResult::RestartInitiated {
        message: format!("VoidTower rollback to {tag} was initiated"),
    })
}

async fn apply_odysseus(_operation_id: &str) -> Result<ExecutionResult> {
    let root = Path::new(ODYSSEUS_INSTALL_DIR);
    ensure!(root.join("app.py").exists(), "Odysseus is not installed");
    run_checked(
        root,
        "git",
        &["pull", "--ff-only", "origin", ODYSSEUS_BRANCH],
    )
    .await?;
    run_checked(
        root,
        &root.join("venv/bin/pip").to_string_lossy(),
        &["install", "--quiet", "-r", "requirements.txt"],
    )
    .await?;
    run_checked(root, "sudo", &["systemctl", "restart", "odysseus.service"]).await?;
    Ok(ExecutionResult::Completed {
        message: "Odysseus was updated and restarted".into(),
    })
}

async fn apply_docker_image(container_id: &str, _operation_id: &str) -> Result<ExecutionResult> {
    validate_container_selector(container_id)?;
    let container = inspect_container(container_id).await?;
    run_checked(Path::new("/"), "docker", &["pull", &container.image]).await?;
    if usable_compose(&container) {
        run_checked(
            Path::new("/"),
            "docker",
            &[
                "compose",
                "-p",
                &container.compose_project,
                "-f",
                &container.compose_file,
                "up",
                "-d",
                &container.compose_service,
            ],
        )
        .await?;
    } else {
        run_checked(Path::new("/"), "docker", &["restart", container_id]).await?;
    }
    Ok(ExecutionResult::Completed {
        message: format!("Updated container {}", container.container_name),
    })
}

async fn apply_os(_operation_id: &str) -> Result<ExecutionResult> {
    let manager = detect_package_manager().context("no supported package manager")?;
    let arguments: &[&str] = match manager {
        "apt-get" => &["apt-get", "-y", "upgrade", "-o", "Dpkg::Progress-Fancy=0"],
        "pacman" => &["pacman", "-Syu", "--noconfirm"],
        "dnf" => &["dnf", "upgrade", "-y"],
        "yum" => &["yum", "upgrade", "-y"],
        "zypper" => &["zypper", "--non-interactive", "update"],
        "apk" => &["apk", "upgrade"],
        "xbps-install" => &["xbps-install", "-Syu"],
        _ => bail!("unsupported package manager"),
    };
    run_checked(Path::new("/"), "sudo", arguments).await?;
    Ok(ExecutionResult::Completed {
        message: format!(
            "Applied {} operating-system updates",
            display_package_manager(manager)
        ),
    })
}

async fn current_voidtower_state() -> Result<String> {
    if is_docker() {
        Ok(own_container().await?.container_image_id)
    } else if is_dev_install() {
        let root = project_root().context("cannot locate VoidTower project root")?;
        Ok(run_checked(&root, "git", &["rev-parse", "HEAD"])
            .await?
            .trim()
            .into())
    } else {
        Ok(current_binary_version())
    }
}

fn compare_git_snapshot(snapshot: &GitSnapshot, name: &str) -> Result<ReconciliationResult> {
    if snapshot.current_commit == snapshot.remote_commit && !snapshot.current_commit.is_empty() {
        Ok(ReconciliationResult::Succeeded {
            message: format!("{name} matches its remote branch"),
        })
    } else {
        Ok(ReconciliationResult::StillUncertain {
            message: format!("{name} does not yet match its remote branch"),
        })
    }
}

async fn git_snapshot(root: &Path, configured_branch: &str) -> Result<GitSnapshot> {
    if !root.join(".git").exists() {
        return Ok(GitSnapshot {
            installed: false,
            branch: configured_branch.into(),
            current_commit: String::new(),
            remote_commit: String::new(),
            behind: 0,
            ahead: 0,
            backup_tags: Vec::new(),
        });
    }
    let current_branch = current_branch(root).await?;
    let branch = if configured_branch.is_empty() {
        current_branch
    } else {
        ensure!(
            current_branch == configured_branch,
            "git checkout is not on the configured update branch"
        );
        configured_branch.into()
    };
    let current_commit = run_checked(root, "git", &["rev-parse", "HEAD"])
        .await?
        .trim()
        .into();
    let remote_commit = remote_commit(root, &branch).await?;
    let behind = git_count(root, &format!("{current_commit}..{remote_commit}")).await;
    let ahead = git_count(root, &format!("{remote_commit}..{current_commit}")).await;
    let tags = run_checked(
        root,
        "git",
        &["tag", "--list", "vt-backup-*", "--sort=-creatordate"],
    )
    .await?;
    Ok(GitSnapshot {
        installed: true,
        branch,
        current_commit,
        remote_commit,
        behind,
        ahead,
        backup_tags: tags
            .lines()
            .filter(|line| !line.is_empty())
            .take(10)
            .map(str::to_owned)
            .collect(),
    })
}

async fn current_branch(root: &Path) -> Result<String> {
    let branch = run_checked(root, "git", &["rev-parse", "--abbrev-ref", "HEAD"])
        .await?
        .trim()
        .to_string();
    ensure!(
        !branch.is_empty() && branch != "HEAD",
        "git checkout has no branch"
    );
    ensure!(
        branch
            .chars()
            .all(|character| character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '/' | '.')),
        "git branch contains unsupported characters"
    );
    Ok(branch)
}

async fn remote_commit(root: &Path, branch: &str) -> Result<String> {
    let reference = format!("refs/heads/{branch}");
    let output = run_checked(root, "git", &["ls-remote", "origin", &reference]).await?;
    let commit = output.split_whitespace().next().unwrap_or_default();
    ensure!(is_commit(commit), "remote branch did not return a commit");
    Ok(commit.into())
}

async fn git_count(root: &Path, range: &str) -> usize {
    run_checked(root, "git", &["rev-list", range, "--count"])
        .await
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

async fn own_container() -> Result<DockerContainerSnapshot> {
    let hostname = std::env::var("HOSTNAME").context("HOSTNAME is unavailable")?;
    inspect_container(&hostname).await
}

async fn list_running_containers() -> Result<Vec<DockerContainerSnapshot>> {
    let output = run_checked(Path::new("/"), "docker", &["ps", "--format", "{{.ID}}"]).await?;
    let mut containers = Vec::new();
    for id in output.lines().filter(|line| !line.trim().is_empty()) {
        containers.push(inspect_container(id.trim()).await?);
    }
    containers.sort_by(|left, right| left.container_id.cmp(&right.container_id));
    Ok(containers)
}

async fn inspect_container(container_id: &str) -> Result<DockerContainerSnapshot> {
    validate_container_selector(container_id)?;
    let output = run_checked(
        Path::new("/"),
        "docker",
        &[
            "inspect",
            "--format",
            "{{.Id}}|{{.Name}}|{{.Config.Image}}|{{.Image}}|{{index .Config.Labels \"com.docker.compose.project\"}}|{{index .Config.Labels \"com.docker.compose.project.config_files\"}}|{{index .Config.Labels \"com.docker.compose.service\"}}",
            container_id,
        ],
    )
    .await?;
    let parts: Vec<&str> = output.trim().splitn(7, '|').collect();
    ensure!(
        parts.len() == 7,
        "Docker returned malformed container metadata"
    );
    Ok(DockerContainerSnapshot {
        container_id: parts[0].chars().take(12).collect(),
        container_name: parts[1].trim_start_matches('/').into(),
        image: parts[2].into(),
        container_image_id: parts[3].into(),
        local_image_id: docker_image_id(parts[2]).await.unwrap_or_default(),
        compose_project: nil_label(parts[4]),
        compose_file: nil_label(parts[5]),
        compose_service: nil_label(parts[6]),
    })
}

async fn docker_image_id(image: &str) -> Result<String> {
    Ok(run_checked(
        Path::new("/"),
        "docker",
        &["image", "inspect", image, "--format", "{{.Id}}"],
    )
    .await?
    .trim()
    .into())
}

fn usable_compose(container: &DockerContainerSnapshot) -> bool {
    !container.compose_project.is_empty()
        && !container.compose_file.is_empty()
        && !container.compose_service.is_empty()
        && Path::new(&container.compose_file).exists()
}

fn nil_label(value: &str) -> String {
    match value.trim() {
        "<no value>" | "<nil>" => String::new(),
        value => value.into(),
    }
}

fn detect_package_manager() -> Option<&'static str> {
    [
        "apt-get",
        "pacman",
        "dnf",
        "yum",
        "zypper",
        "apk",
        "xbps-install",
    ]
    .into_iter()
    .find(|command| command_exists(command))
}

async fn list_upgradable_packages(manager: &str) -> Result<Vec<String>> {
    let (command, arguments): (&str, &[&str]) = match manager {
        "apt-get" => ("apt-get", &["-s", "upgrade", "-V"]),
        "pacman" => ("pacman", &["-Qu"]),
        "dnf" | "yum" => (manager, &["check-update", "-q"]),
        "zypper" => ("zypper", &["--non-interactive", "list-updates"]),
        "apk" => ("apk", &["list", "--upgradable"]),
        "xbps-install" => ("xbps-install", &["-nu"]),
        _ => bail!("unsupported package manager"),
    };
    let output = run_allowing_status(Path::new("/"), command, arguments, &[0, 100]).await?;
    let packages = output
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !line.starts_with("Reading ")
                && !line.starts_with("Building ")
                && !line.starts_with("Calculating ")
                && !line.starts_with("Last metadata")
                && !line.starts_with("Listing")
        })
        .map(safe_text)
        .collect();
    Ok(packages)
}

async fn installed_package_manifest(manager: &str) -> Result<String> {
    let (command, arguments): (&str, &[&str]) = match manager {
        "apt-get" => ("dpkg-query", &["-W", "-f=${Package}=${Version}\\n"]),
        "pacman" => ("pacman", &["-Q"]),
        "dnf" | "yum" | "zypper" => ("rpm", &["-qa", "--qf", "%{NAME}=%{VERSION}-%{RELEASE}\\n"]),
        "apk" => ("apk", &["info", "-vv"]),
        "xbps-install" => ("xbps-query", &["-l"]),
        _ => bail!("unsupported package manager"),
    };
    let output = tokio::process::Command::new(command)
        .args(arguments)
        .current_dir("/")
        .output()
        .await
        .with_context(|| format!("failed to snapshot packages with {command}"))?;
    ensure!(
        output.status.success(),
        "{command} package snapshot failed: {}",
        safe_text(&String::from_utf8_lossy(&output.stderr))
    );
    ensure!(
        output.stdout.len() <= MAX_PACKAGE_MANIFEST_BYTES,
        "installed-package snapshot exceeds the size limit"
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn display_package_manager(manager: &str) -> &str {
    if manager == "apt-get" {
        "apt"
    } else {
        manager
    }
}

async fn latest_release_version() -> Result<String> {
    let response = reqwest::Client::new()
        .get("https://api.github.com/repos/niwlekakan/voidtower/releases/latest")
        .header(reqwest::header::USER_AGENT, "voidtower-update")
        .send()
        .await?
        .error_for_status()?;
    let value: serde_json::Value = response.json().await?;
    let tag = value["tag_name"]
        .as_str()
        .context("latest release has no tag name")?;
    let version = tag.strip_prefix('v').unwrap_or(tag);
    ensure!(
        !version.is_empty()
            && version
                .chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || matches!(character, '.' | '-' | '_')),
        "latest release version is invalid"
    );
    Ok(version.into())
}

fn current_binary_version() -> String {
    let dir = install_dir();
    std::fs::read_to_string(dir.join(".version"))
        .or_else(|_| std::fs::read_to_string(dir.join(".commit")))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
        .trim()
        .into()
}

fn project_root() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()?
        .parent()?
        .parent()?
        .parent()
        .map(Path::to_path_buf)
}

fn install_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("/opt/voidtower"))
}

fn is_docker() -> bool {
    Path::new("/.dockerenv").exists()
}

fn is_dev_install() -> bool {
    std::env::current_exe()
        .ok()
        .is_some_and(|path| path.to_string_lossy().contains("/target/"))
}

fn command_exists(command: &str) -> bool {
    std::process::Command::new("which")
        .arg(command)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

async fn run_checked(root: &Path, command: &str, arguments: &[&str]) -> Result<String> {
    run_allowing_status(root, command, arguments, &[0]).await
}

async fn run_allowing_status(
    root: &Path,
    command: &str,
    arguments: &[&str],
    allowed_codes: &[i32],
) -> Result<String> {
    let output = tokio::process::Command::new(command)
        .args(arguments)
        .current_dir(root)
        .output()
        .await
        .with_context(|| format!("failed to execute {command}"))?;
    let code = output.status.code().unwrap_or(-1);
    if !allowed_codes.contains(&code) {
        let diagnostic = safe_text(&String::from_utf8_lossy(&output.stderr));
        bail!(
            "{command} failed: {}",
            if diagnostic.is_empty() {
                "provider returned no diagnostic"
            } else {
                &diagnostic
            }
        );
    }
    Ok(safe_text(&String::from_utf8_lossy(&output.stdout)))
}

fn marker_root() -> PathBuf {
    std::env::var_os("VOIDTOWER_UPDATE_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/voidtower/update-operations"))
}

fn create_marker_root() -> Result<()> {
    std::fs::create_dir_all(marker_root()).context("cannot create update operation directory")
}

fn marker_path(operation_id: &str) -> PathBuf {
    marker_root().join(format!("{operation_id}.json"))
}

fn write_marker(marker: &OperationMarker) -> Result<()> {
    create_marker_root()?;
    let path = marker_path(&marker.operation_id);
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(marker)?;
    std::fs::write(&temporary, bytes).context("cannot write update operation marker")?;
    std::fs::rename(temporary, path).context("cannot commit update operation marker")
}

fn read_marker(operation_id: &str) -> Result<OperationMarker> {
    let bytes = std::fs::read(marker_path(operation_id))
        .context("update operation marker is unavailable")?;
    let marker: OperationMarker = serde_json::from_slice(&bytes)?;
    ensure!(
        marker.operation_id == operation_id,
        "update operation marker mismatch"
    );
    Ok(marker)
}

fn set_expected_state(operation_id: &str, expected: &str) -> Result<()> {
    ensure!(
        !expected.trim().is_empty(),
        "expected update state is empty"
    );
    let mut marker = read_marker(operation_id)?;
    marker.expected_state = Some(expected.trim().into());
    write_marker(&marker)
}

fn write_script(path: &Path, body: &str) -> Result<()> {
    create_marker_root()?;
    std::fs::write(path, body).context("cannot write update helper script")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn spawn_script(path: &Path, label: &str) -> Result<()> {
    let mut command = std::process::Command::new("sh");
    command.arg(path);
    spawn_detached(&mut command, label)
}

fn spawn_detached(command: &mut std::process::Command, label: &str) -> Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot start {label}"))?;
    Ok(())
}

fn validate_operation_id(operation_id: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(operation_id).is_ok(),
        "update operation ID must be a UUID"
    );
    Ok(())
}

pub fn validate_backup_tag(tag: &str) -> Result<()> {
    ensure!(
        tag.starts_with("vt-backup-"),
        "invalid VoidTower backup tag"
    );
    ensure!(
        tag.chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-'),
        "invalid VoidTower backup tag"
    );
    Ok(())
}

fn validate_container_selector(container_id: &str) -> Result<()> {
    ensure!(
        (1..=128).contains(&container_id.len())
            && container_id
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_alphanumeric())
            && container_id
                .chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || matches!(character, '.' | '_' | '-')),
        "invalid Docker container selector"
    );
    Ok(())
}

fn is_commit(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn process_identity() -> String {
    let start_ticks = std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| stat.rsplit_once(") ").map(|(_, fields)| fields.to_string()))
        .and_then(|fields| fields.split_whitespace().nth(19).map(str::to_owned))
        .unwrap_or_else(|| "unknown".into());
    format!("{}:{start_ticks}", std::process::id())
}

fn target_name(target: &UpdateTarget) -> &'static str {
    match target {
        UpdateTarget::VoidTower => "voidtower",
        UpdateTarget::Odysseus => "odysseus",
        UpdateTarget::DockerEngine => "docker_engine",
        UpdateTarget::DockerImage { .. } => "docker_image",
        UpdateTarget::OperatingSystem => "operating_system",
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn safe_text(value: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(value.trim());
    let mut characters = redacted.chars();
    let mut bounded: String = characters.by_ref().take(MAX_PROVIDER_TEXT_CHARS).collect();
    if characters.next().is_some() {
        bounded.push_str("…[truncated]");
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_used_in_provider_paths_are_strictly_validated() {
        assert!(validate_operation_id("00000000-0000-4000-8000-000000000001").is_ok());
        assert!(validate_operation_id("../../escape").is_err());
        assert!(validate_container_selector("abcdef012345").is_ok());
        assert!(validate_container_selector("container-name").is_ok());
        assert!(validate_container_selector("--format").is_err());
        assert!(validate_backup_tag("vt-backup-1700000000").is_ok());
        assert!(validate_backup_tag("vt-backup-../../main").is_err());
    }

    #[test]
    fn shell_arguments_are_single_quoted() {
        assert_eq!(shell_quote("a b'c"), "'a b'\\''c'");
    }

    #[test]
    fn provider_text_is_redacted_and_bounded() {
        let value = format!("token=known-secret-value {}", "x".repeat(5000));
        let safe = safe_text(&value);
        assert!(!safe.contains("known-secret-value"));
        assert!(safe.ends_with("[truncated]"));
    }

    #[test]
    fn compatibility_routes_do_not_own_update_helper_scripts() {
        let source = include_str!("api/updates.rs");
        assert!(!source.contains("voidtower-update.sh"));
        assert!(!source.contains("odysseus-update.sh"));
        assert!(!source.contains("voidtower-rollback.sh"));
        assert!(!source.contains("Command::new"));
        assert!(!source.contains("std::fs::"));
        assert!(!source.contains(".spawn()"));
        assert!(source.contains("update_provider::prepare_rollback("));
        assert!(source.contains("update_provider::execute("));
    }
}
