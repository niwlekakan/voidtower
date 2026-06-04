use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub description: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
}

pub fn is_systemd_available() -> bool {
    std::path::Path::new("/run/systemd/private").exists()
        || std::path::Path::new("/sys/fs/cgroup/systemd").exists()
        || std::path::Path::new("/run/systemd/system").exists()
}

pub fn list_services() -> Result<Vec<ServiceInfo>> {
    if !is_systemd_available() {
        return Ok(vec![]);
    }

    let output = Command::new("systemctl")
        .args(["list-units", "--type=service", "--all", "--no-pager", "--output=json"])
        .output()?;

    if !output.status.success() {
        return Ok(vec![]);
    }

    #[derive(Deserialize)]
    struct SystemdUnit {
        unit: String,
        description: String,
        load: String,
        active: String,
        sub: String,
    }

    let units: Vec<SystemdUnit> = serde_json::from_slice(&output.stdout)
        .unwrap_or_default();

    let services = units
        .into_iter()
        .map(|u| {
            let name = u.unit.trim_end_matches(".service").to_string();
            let enabled = is_service_enabled(&u.unit);
            ServiceInfo {
                name,
                description: u.description,
                load_state: u.load,
                active_state: u.active,
                sub_state: u.sub,
                enabled,
            }
        })
        .collect();

    Ok(services)
}

pub fn get_service(name: &str) -> Result<Option<ServiceInfo>> {
    if !is_systemd_available() {
        return Ok(None);
    }
    let unit = if name.ends_with(".service") {
        name.to_string()
    } else {
        format!("{}.service", name)
    };

    let output = Command::new("systemctl")
        .args(["show", &unit, "--no-pager",
               "--property=Description,LoadState,ActiveState,SubState,UnitFileState"])
        .output()?;

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut props = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            props.insert(k, v);
        }
    }

    let active_state = props.get("ActiveState").unwrap_or(&"unknown").to_string();
    if active_state == "unknown" && !props.contains_key("Description") {
        return Ok(None);
    }

    Ok(Some(ServiceInfo {
        name: name.trim_end_matches(".service").to_string(),
        description: props.get("Description").unwrap_or(&"").to_string(),
        load_state: props.get("LoadState").unwrap_or(&"").to_string(),
        active_state,
        sub_state: props.get("SubState").unwrap_or(&"").to_string(),
        enabled: props.get("UnitFileState").map(|s| *s == "enabled").unwrap_or(false),
    }))
}

pub fn run_service_action(name: &str, action: ServiceAction) -> Result<()> {
    if !is_systemd_available() {
        return Err(anyhow::anyhow!("systemd is not available on this system"));
    }

    let unit = if name.ends_with(".service") {
        name.to_string()
    } else {
        format!("{}.service", name)
    };

    let cmd = match action {
        ServiceAction::Start => "start",
        ServiceAction::Stop => "stop",
        ServiceAction::Restart => "restart",
        ServiceAction::Enable => "enable",
        ServiceAction::Disable => "disable",
    };

    let output = Command::new("systemctl")
        .args([cmd, &unit])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("systemctl {} failed: {}", cmd, stderr));
    }
    Ok(())
}

pub fn get_service_logs(name: &str, lines: usize) -> Result<Vec<String>> {
    if !is_systemd_available() {
        return Ok(vec![]);
    }
    let unit = if name.ends_with(".service") {
        name.to_string()
    } else {
        format!("{}.service", name)
    };

    let output = Command::new("journalctl")
        .args(["-u", &unit, "--no-pager", "-n", &lines.to_string(), "--output=short-iso"])
        .output()?;

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().map(str::to_string).collect())
}

fn is_service_enabled(unit: &str) -> bool {
    Command::new("systemctl")
        .args(["is-enabled", unit, "--quiet"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
