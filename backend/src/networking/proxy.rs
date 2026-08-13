//! Low-level nginx-proxy host primitives.
//!
//! This module owns Docker command execution and writes to the nginx bind mount. It does not own
//! authorization, policy, approval, planning, job state, or proxy-rule database records.

use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const NGINX_CONF_DIR: &str = "/var/lib/voidtower/nginx/conf.d";
const MAX_PROVIDER_MESSAGE_CHARS: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NginxAction {
    Start,
    Stop,
    Restart,
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NginxSnapshot {
    pub container_id: Option<String>,
    pub state: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NginxMutationResult {
    pub message: String,
}

pub fn container_id() -> Option<String> {
    find_container(false)
}

pub fn running_container_id() -> Option<String> {
    find_container(true)
}

pub fn snapshot() -> Result<NginxSnapshot> {
    let Some(id) = container_id() else {
        return Ok(NginxSnapshot {
            container_id: None,
            state: "not deployed".into(),
            active: false,
        });
    };
    let output = std::process::Command::new("docker")
        .args(["inspect", "--format", "{{.State.Status}}", &id])
        .output()
        .context("failed to inspect nginx-proxy container")?;
    if !output.status.success() {
        bail!(
            "nginx-proxy inspect failed: {}",
            safe_message(&String::from_utf8_lossy(&output.stderr))
        );
    }
    let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(NginxSnapshot {
        container_id: Some(id),
        active: state == "running",
        state,
    })
}

pub fn execute(action: NginxAction) -> Result<NginxMutationResult> {
    let id = container_id()
        .context("nginx-proxy container is not deployed — deploy it from App Vault")?;
    let output = match action {
        NginxAction::Reload => {
            ensure!(snapshot()?.active, "nginx-proxy container is not running");
            std::process::Command::new("docker")
                .args(["exec", &id, "nginx", "-s", "reload"])
                .output()
                .context("failed to execute nginx reload")?
        }
        NginxAction::Start | NginxAction::Stop | NginxAction::Restart => {
            let command = match action {
                NginxAction::Start => "start",
                NginxAction::Stop => "stop",
                NginxAction::Restart => "restart",
                NginxAction::Reload => unreachable!(),
            };
            std::process::Command::new("docker")
                .args([command, &id])
                .output()
                .with_context(|| format!("failed to execute nginx-proxy {command}"))?
        }
    };
    if !output.status.success() {
        let diagnostic = safe_message(&String::from_utf8_lossy(&output.stderr));
        bail!(
            "nginx-proxy mutation failed: {}",
            if diagnostic.is_empty() {
                "provider returned no diagnostic"
            } else {
                &diagnostic
            }
        );
    }
    let action_name = match action {
        NginxAction::Start => "started",
        NginxAction::Stop => "stopped",
        NginxAction::Restart => "restarted",
        NginxAction::Reload => "reloaded",
    };
    Ok(NginxMutationResult {
        message: format!("nginx-proxy {action_name}"),
    })
}

pub fn test_configuration() -> Result<String> {
    let id = running_container_id().context("nginx-proxy container is not running")?;
    let output = std::process::Command::new("docker")
        .args(["exec", &id, "nginx", "-t"])
        .output()
        .context("failed to test nginx configuration")?;
    let message = safe_message(
        &(String::from_utf8_lossy(&output.stdout).to_string()
            + &String::from_utf8_lossy(&output.stderr)),
    );
    ensure!(
        output.status.success(),
        "nginx configuration is invalid: {message}"
    );
    Ok(message)
}

pub fn conf_path(domain: &str) -> Result<PathBuf> {
    validate_domain_path_component(domain)?;
    Ok(Path::new(NGINX_CONF_DIR).join(format!("voidtower-{domain}.conf")))
}

pub fn htpasswd_path(domain: &str) -> Result<PathBuf> {
    validate_domain_path_component(domain)?;
    Ok(Path::new(NGINX_CONF_DIR).join(format!("voidtower-{domain}.htpasswd")))
}

pub fn port_conf_path(slug: &str) -> Result<PathBuf> {
    validate_slug_path_component(slug)?;
    Ok(Path::new(NGINX_CONF_DIR).join(format!("voidtower-embed-port-{slug}.conf")))
}

pub fn write_conf(domain: &str, content: &str) -> Result<()> {
    write_file(
        &conf_path(domain)?,
        content.as_bytes(),
        "nginx configuration",
    )
}

pub fn write_port_conf(slug: &str, content: &str) -> Result<()> {
    write_file(
        &port_conf_path(slug)?,
        content.as_bytes(),
        "nginx port configuration",
    )
}

pub fn remove_conf(domain: &str) -> Result<()> {
    remove_file(&conf_path(domain)?, "nginx configuration")
}

pub fn write_htpasswd(domain: &str, user: &str, pass_hash: &str) -> Result<()> {
    ensure!(
        !user.contains(['\n', '\r', ':']) && !pass_hash.contains(['\n', '\r']),
        "basic-auth credential contains an invalid control character"
    );
    write_file(
        &htpasswd_path(domain)?,
        format!("{user}:{pass_hash}\n").as_bytes(),
        "nginx basic-auth file",
    )
}

pub fn remove_htpasswd(domain: &str) -> Result<()> {
    remove_file(&htpasswd_path(domain)?, "nginx basic-auth file")
}

fn find_container(running_only: bool) -> Option<String> {
    let mut command = std::process::Command::new("docker");
    command.args([
        "ps",
        "-a",
        "--filter",
        "label=com.docker.compose.project=vt-nginx-proxy",
    ]);
    if running_only {
        command.args(["--filter", "status=running"]);
    }
    let output = command
        .args(["--format", "{{.ID}}", "--latest"])
        .output()
        .ok()?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!id.is_empty()).then_some(id)
}

fn validate_domain_path_component(domain: &str) -> Result<()> {
    ensure!(!domain.is_empty(), "proxy domain is empty");
    ensure!(
        !domain.contains(['/', '\\', '\0', '\n', '\r']),
        "proxy domain is not a safe path component"
    );
    Ok(())
}

fn validate_slug_path_component(slug: &str) -> Result<()> {
    ensure!(!slug.is_empty(), "proxy slug is empty");
    ensure!(
        slug.chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_')),
        "proxy slug is not a safe path component"
    );
    Ok(())
}

fn write_file(path: &Path, content: &[u8], label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {label} directory"))?;
    }
    std::fs::write(path, content).with_context(|| format!("cannot write {label}"))
}

fn remove_file(path: &Path, label: &str) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("cannot remove {label}")),
    }
}

fn safe_message(message: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(message.trim());
    let mut characters = redacted.chars();
    let mut bounded: String = characters
        .by_ref()
        .take(MAX_PROVIDER_MESSAGE_CHARS)
        .collect();
    if characters.next().is_some() {
        bounded.push_str("…[truncated]");
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_paths_cannot_escape_the_nginx_directory() {
        assert!(conf_path("app.example.test").is_ok());
        assert!(conf_path("../../etc/passwd").is_err());
        assert!(htpasswd_path("bad\\name").is_err());
        assert!(port_conf_path("../../../bad").is_err());
    }

    #[test]
    fn provider_diagnostics_are_redacted_and_bounded() {
        let message = format!("token=known-secret-value {}", "x".repeat(5000));
        let safe = safe_message(&message);
        assert!(!safe.contains("known-secret-value"));
        assert!(safe.ends_with("[truncated]"));
    }

    #[test]
    fn compatibility_routes_cannot_mutate_nginx_outside_provider_boundary() {
        let source = include_str!("../api/proxy.rs");
        assert!(!source.contains("args([\"exec\", &id, \"nginx\", \"-s\", \"reload\"])"));
        assert!(!source.contains("std::fs::write(&path, content)"));
        assert!(source.contains("proxy_provider::execute("));
        assert!(source.contains("proxy_provider::write_conf("));
    }
}
