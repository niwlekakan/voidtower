//! Low-level UFW provider primitives shared by compatibility routes and durable adapters.
//!
//! This module owns host command execution but not authorization, policy, approval, planning, or
//! job state. Arguments are structured and passed directly to `Command`; no shell is involved.

use anyhow::{bail, Context, Result};
use serde::Serialize;

const MAX_COMMAND_MESSAGE_CHARS: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FirewallSnapshot {
    pub backend: &'static str,
    pub enabled: bool,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirewallMutation {
    AddRule(Vec<String>),
    DeleteRule(u32),
    Enable,
    Disable,
    Reload,
    Reset,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationResult {
    pub message: String,
}

pub fn available() -> bool {
    std::path::Path::new("/usr/sbin/ufw").exists() || std::path::Path::new("/usr/bin/ufw").exists()
}

pub async fn snapshot() -> Result<FirewallSnapshot> {
    let status = status_text().await?;
    let enabled = status
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case("Status: active"));
    let rules = status
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('['))
        .map(normalize_rule)
        .collect();
    Ok(FirewallSnapshot {
        backend: "ufw",
        enabled,
        rules,
    })
}

pub async fn status_text() -> Result<String> {
    let output = tokio::process::Command::new("ufw")
        .args(["status", "numbered", "verbose"])
        .output()
        .await
        .context("failed to execute ufw status")?;
    let combined = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!("ufw status failed: {}", safe_message(&combined));
    }
    Ok(combined)
}

pub async fn execute(mutation: FirewallMutation) -> Result<MutationResult> {
    let arguments = match mutation {
        FirewallMutation::AddRule(arguments) => {
            validate_add_rule_arguments(&arguments)?;
            arguments
        }
        FirewallMutation::DeleteRule(number) => {
            if number == 0 {
                bail!("firewall rule number must be positive");
            }
            vec!["--force".into(), "delete".into(), number.to_string()]
        }
        FirewallMutation::Enable => vec!["--force".into(), "enable".into()],
        FirewallMutation::Disable => vec!["--force".into(), "disable".into()],
        FirewallMutation::Reload => vec!["reload".into()],
        FirewallMutation::Reset => vec!["--force".into(), "reset".into()],
    };
    let output = tokio::process::Command::new("ufw")
        .args(&arguments)
        .output()
        .await
        .context("failed to execute ufw mutation")?;
    if !output.status.success() {
        let stderr = safe_message(&String::from_utf8_lossy(&output.stderr));
        bail!(
            "ufw mutation failed: {}",
            if stderr.is_empty() {
                "provider returned no diagnostic"
            } else {
                &stderr
            }
        );
    }
    Ok(MutationResult {
        message: safe_message(&String::from_utf8_lossy(&output.stdout)),
    })
}

pub fn validate_add_rule_arguments(arguments: &[String]) -> Result<()> {
    let Some(action) = arguments.first() else {
        bail!("firewall rule action is required");
    };
    if !matches!(action.as_str(), "allow" | "deny" | "limit") {
        bail!("firewall rule action must be allow, deny, or limit");
    }
    if arguments
        .iter()
        .any(|argument| argument.contains([';', '&', '|', '`', '\n', '\r']) || argument.is_empty())
    {
        bail!("firewall rule contains an invalid argument");
    }
    Ok(())
}

fn normalize_rule(line: &str) -> String {
    let without_number = line.find(']').map(|end| &line[end + 1..]).unwrap_or(line);
    without_number
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn safe_message(message: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(message.trim());
    let mut characters = redacted.chars();
    let mut bounded: String = characters
        .by_ref()
        .take(MAX_COMMAND_MESSAGE_CHARS)
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
    fn normalized_snapshot_rules_ignore_unstable_ufw_numbers() {
        assert_eq!(
            normalize_rule("[ 7] 22/tcp     ALLOW IN    Anywhere"),
            "22/tcp ALLOW IN Anywhere"
        );
    }

    #[test]
    fn add_rule_arguments_reject_shell_control_characters() {
        assert!(validate_add_rule_arguments(&["allow".into(), "22/tcp".into()]).is_ok());
        assert!(validate_add_rule_arguments(&["allow".into(), "22; reboot".into()]).is_err());
    }

    #[test]
    fn provider_messages_are_redacted_and_bounded() {
        let message = format!("api_key=known-secret-value {}", "x".repeat(5000));
        let safe = safe_message(&message);
        assert!(!safe.contains("known-secret-value"));
        assert!(safe.ends_with("[truncated]"));
    }

    #[test]
    fn compatibility_routes_cannot_execute_ufw_outside_provider_boundary() {
        let source = include_str!("../api/firewall.rs");
        assert!(!source.contains("Command::new(\"ufw\")"));
        assert_eq!(source.matches("firewall_provider::execute(").count(), 3);
    }
}
