use axum::{extract::State, Json};
use serde::Serialize;

use crate::AppState;

#[derive(Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
    Info,
}

#[derive(Serialize, Clone)]
pub struct DiagCheck {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub status: CheckStatus,
    pub message: String,
    pub detail: Option<String>,
}

impl DiagCheck {
    fn pass(id: &'static str, name: &'static str, cat: &'static str, msg: String) -> Self {
        Self { id, name, category: cat, status: CheckStatus::Pass, message: msg, detail: None }
    }
    fn warn(id: &'static str, name: &'static str, cat: &'static str, msg: String, detail: Option<&'static str>) -> Self {
        Self { id, name, category: cat, status: CheckStatus::Warn, message: msg, detail: detail.map(str::to_string) }
    }
    fn fail(id: &'static str, name: &'static str, cat: &'static str, msg: String, detail: Option<&'static str>) -> Self {
        Self { id, name, category: cat, status: CheckStatus::Fail, message: msg, detail: detail.map(str::to_string) }
    }
    fn info(id: &'static str, name: &'static str, cat: &'static str, msg: &'static str) -> Self {
        Self { id, name, category: cat, status: CheckStatus::Info, message: msg.to_string(), detail: None }
    }
}

fn check_config_dir(cfg: &crate::config::Config) -> DiagCheck {
    let p = &cfg.config_dir;
    if !p.exists() {
        DiagCheck::fail("config_dir", "Config directory", "Config",
            format!("{} does not exist", p.display()), None)
    } else {
        let test = p.join(".vt_write_test");
        let writable = std::fs::write(&test, b"").is_ok();
        let _ = std::fs::remove_file(&test);
        if writable {
            DiagCheck::pass("config_dir", "Config directory", "Config",
                format!("{} exists and is writable", p.display()))
        } else {
            DiagCheck::warn("config_dir", "Config directory", "Config",
                format!("{} exists but is not writable", p.display()), None)
        }
    }
}

fn check_data_dir(cfg: &crate::config::Config) -> DiagCheck {
    let p = &cfg.data_dir;
    if !p.exists() {
        DiagCheck::fail("data_dir", "Data directory", "Config",
            format!("{} does not exist", p.display()), None)
    } else {
        DiagCheck::pass("data_dir", "Data directory", "Config",
            format!("{} exists", p.display()))
    }
}

fn check_apps_dir(cfg: &crate::config::Config) -> DiagCheck {
    let p = cfg.apps_dir();
    if p.exists() {
        DiagCheck::pass("apps_dir", "Apps directory", "Config",
            format!("{} exists", p.display()))
    } else {
        DiagCheck::warn("apps_dir", "Apps directory", "Config",
            format!("{} does not exist — App Vault deployments may fail", p.display()),
            Some("VoidTower creates this directory on startup."))
    }
}

fn check_db(cfg: &crate::config::Config) -> DiagCheck {
    let p = cfg.db_path();
    if p.exists() {
        let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
        DiagCheck::pass("db_file", "Database file", "Database",
            format!("voidtower.db  ({:.1} KB)", size as f64 / 1024.0))
    } else {
        DiagCheck::fail("db_file", "Database file", "Database",
            format!("{} not found", p.display()),
            Some("Database has not been initialised. Start VoidTower normally to create it."))
    }
}

fn check_bootstrap_token(cfg: &crate::config::Config) -> DiagCheck {
    let p = cfg.bootstrap_token_path();
    if p.exists() {
        DiagCheck::warn("bootstrap_token", "Bootstrap token", "Config",
            "Bootstrap token file still exists — setup may not be complete".to_string(),
            Some("Complete first-run setup at /bootstrap; the token is consumed on success."))
    } else {
        DiagCheck::pass("bootstrap_token", "Bootstrap token", "Config",
            "Bootstrap token consumed — setup complete".to_string())
    }
}

fn check_frontend(cfg: &crate::config::Config) -> DiagCheck {
    let p = &cfg.frontend_dir;
    if !p.exists() {
        DiagCheck::fail("frontend", "Frontend assets", "Config",
            format!("{} does not exist", p.display()),
            Some("Run `npm run build` in the frontend/ directory, or install from a release package."))
    } else if !p.join("index.html").exists() {
        DiagCheck::warn("frontend", "Frontend assets", "Config",
            format!("{} exists but index.html is missing", p.display()), None)
    } else {
        DiagCheck::pass("frontend", "Frontend assets", "Config",
            format!("{}/index.html found", p.display()))
    }
}

fn check_disk_space(cfg: &crate::config::Config) -> DiagCheck {
    let path = cfg.data_dir.to_string_lossy().to_string();
    let out = match std::process::Command::new("df").args(["-B1", &path]).output() {
        Ok(o) => o,
        Err(_) => return DiagCheck::info("disk_space", "Disk space", "System",
            "Could not run df to check disk space"),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let line = match text.lines().nth(1) {
        Some(l) => l,
        None => return DiagCheck::info("disk_space", "Disk space", "System",
            "Could not parse df output"),
    };
    let cols: Vec<&str> = line.split_whitespace().collect();
    let (Ok(total), Ok(avail)) = (
        cols.get(1).and_then(|s| s.parse::<u64>().ok()).ok_or(()),
        cols.get(3).and_then(|s| s.parse::<u64>().ok()).ok_or(()),
    ) else {
        return DiagCheck::info("disk_space", "Disk space", "System",
            "Could not parse df column values");
    };
    if total == 0 {
        return DiagCheck::info("disk_space", "Disk space", "System", "Filesystem reports size 0");
    }
    let pct_free = avail as f64 / total as f64 * 100.0;
    let free_gb  = avail as f64 / 1_073_741_824.0;
    let total_gb = total as f64 / 1_073_741_824.0;
    let msg = format!("{:.1} GB free of {:.1} GB ({:.0}% free)", free_gb, total_gb, pct_free);
    if pct_free < 5.0 {
        DiagCheck::fail("disk_space", "Disk space", "System", msg,
            Some("Data directory filesystem is critically low on space."))
    } else if pct_free < 10.0 {
        DiagCheck::warn("disk_space", "Disk space", "System", msg,
            Some("Data directory filesystem is running low on space."))
    } else {
        DiagCheck::pass("disk_space", "Disk space", "System", msg)
    }
}

fn check_systemd() -> DiagCheck {
    if crate::services::is_systemd_available() {
        DiagCheck::pass("systemd", "systemd", "Services", "systemd is available".to_string())
    } else {
        DiagCheck::warn("systemd", "systemd", "Services",
            "systemd not detected — service management unavailable".to_string(),
            Some("VoidTower requires systemd for service management."))
    }
}

fn check_docker() -> DiagCheck {
    if !crate::containers::is_docker_available() {
        return DiagCheck::warn("docker", "Docker daemon", "Containers",
            "Docker socket not found at /var/run/docker.sock".to_string(),
            Some("Install Docker to enable container management."));
    }
    match std::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
    {
        Ok(o) if o.status.success() => {
            let version = String::from_utf8_lossy(&o.stdout).trim().to_string();
            DiagCheck::pass("docker", "Docker daemon", "Containers",
                format!("Docker daemon responding (server {})", version))
        }
        _ => DiagCheck::fail("docker", "Docker daemon", "Containers",
            "Docker socket exists but daemon not responding".to_string(),
            Some("Run: systemctl start docker")),
    }
}

fn check_restic() -> DiagCheck {
    if !crate::backups::is_restic_available() {
        return DiagCheck::warn("restic", "restic", "Backups",
            "restic not found — backup features unavailable".to_string(),
            Some("Install restic: apt install restic"));
    }
    let ver = std::process::Command::new("restic")
        .arg("version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    DiagCheck::pass("restic", "restic", "Backups", ver.trim().to_string())
}

fn check_nginx() -> DiagCheck {
    let found = std::process::Command::new("which")
        .arg("nginx")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !found {
        return DiagCheck::info("nginx", "nginx", "Networking",
            "nginx not found — proxy manager will be unavailable");
    }
    let conf_ok = std::process::Command::new("nginx")
        .arg("-t")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if conf_ok {
        DiagCheck::pass("nginx", "nginx", "Networking",
            "nginx found and config test passed".to_string())
    } else {
        DiagCheck::warn("nginx", "nginx", "Networking",
            "nginx found but config test failed".to_string(),
            Some("Run `nginx -t` for details."))
    }
}

fn check_port(cfg: &crate::config::Config) -> DiagCheck {
    use std::net::TcpListener;
    match TcpListener::bind(format!("{}:{}", cfg.bind, cfg.port)) {
        Ok(_) => DiagCheck::warn("port", "Bind port", "Network",
            format!("Port {} is free — VoidTower is not listening in this process context", cfg.port),
            None),
        Err(_) => DiagCheck::pass("port", "Bind port", "Network",
            format!("Port {} is in use — VoidTower (or another process) is listening", cfg.port)),
    }
}

pub fn run_all_checks(cfg: &crate::config::Config) -> Vec<DiagCheck> {
    vec![
        check_config_dir(cfg),
        check_data_dir(cfg),
        check_apps_dir(cfg),
        check_db(cfg),
        check_bootstrap_token(cfg),
        check_frontend(cfg),
        check_disk_space(cfg),
        check_systemd(),
        check_docker(),
        check_restic(),
        check_nginx(),
        check_port(cfg),
    ]
}

pub async fn get_diagnostics(state: State<AppState>) -> Json<serde_json::Value> {
    let checks = run_all_checks(&state.config);

    let pass = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
    let warn = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let fail = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let info = checks.iter().filter(|c| c.status == CheckStatus::Info).count();
    let overall = if fail > 0 { "fail" } else if warn > 0 { "warn" } else { "pass" };

    Json(serde_json::json!({
        "checks": checks,
        "summary": { "pass": pass, "warn": warn, "fail": fail, "info": info, "overall": overall }
    }))
}
