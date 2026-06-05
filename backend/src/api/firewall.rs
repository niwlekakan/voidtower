use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{auth, error::{AppError, Result}, AppState};

#[derive(Serialize, Clone)]
pub struct FirewallRule {
    pub num: u32,
    pub to: String,
    pub action: String,
    pub from: String,
    pub ipv6: bool,
    pub comment: Option<String>,
}

#[derive(Serialize)]
pub struct FirewallStatus {
    pub backend: String,
    pub enabled: bool,
    pub rules: Vec<FirewallRule>,
    pub logging: Option<String>,
    pub error: Option<String>,
}

fn run_ufw(args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new("ufw").args(args).output()
}

fn parse_ufw_status(output: &str) -> (bool, Vec<FirewallRule>, Option<String>) {
    let mut enabled = false;
    let mut rules = Vec::new();
    let mut logging = None;

    for line in output.lines() {
        let l = line.trim();
        if l.starts_with("Status: active")   { enabled = true; }
        if l.starts_with("Status: inactive") { enabled = false; }
        if l.starts_with("Logging:") {
            logging = l.split_once(':').map(|x| x.1).map(|s| s.trim().to_string());
        }

        // Parse numbered rules: "[ 1] 22/tcp    ALLOW IN    Anywhere"
        if l.starts_with('[') {
            if let Some(end) = l.find(']') {
                let num_str = l[1..end].trim();
                let Ok(num) = num_str.parse::<u32>() else { continue };
                let rest = l[end + 1..].trim();

                // Detect IPv6 entries (contain "(v6)")
                let ipv6 = rest.contains("(v6)");
                let rest_clean = rest.replace("(v6)", "").replace("  ", " ");

                // Split on double-space or column-aligned whitespace
                let parts: Vec<&str> = rest_clean.split_whitespace().collect();
                if parts.len() < 3 { continue }

                // Format: TO  ACTION  FROM  or  TO  ACTION  IN/OUT  FROM
                // We need to find the action word (ALLOW/DENY/LIMIT/REJECT)
                let mut to = String::new();
                let mut action = String::new();
                let mut from = String::new();

                for (i, &p) in parts.iter().enumerate() {
                    let pu = p.to_uppercase();
                    if pu == "ALLOW" || pu == "DENY" || pu == "LIMIT" || pu == "REJECT" {
                        to = parts[..i].join(" ");
                        // skip IN/OUT direction word if present
                        let next = i + 1;
                        let skip = if parts.get(next).map(|s| s.eq_ignore_ascii_case("IN") || s.eq_ignore_ascii_case("OUT")).unwrap_or(false) { next + 1 } else { next };
                        let dir = if parts.get(i+1).map(|s| s.eq_ignore_ascii_case("OUT")).unwrap_or(false) { " OUT" } else { "" };
                        action = format!("{}{}", p, dir);
                        from = parts[skip..].join(" ");
                        break;
                    }
                }

                if action.is_empty() { continue }

                rules.push(FirewallRule { num, to, action, from, ipv6, comment: None });
            }
        }
    }

    (enabled, rules, logging)
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if user.role == "viewer" || user.role == "operator" { return Err(AppError::Forbidden); }
    Ok(user)
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)
}

fn ufw_available() -> bool {
    std::path::Path::new("/usr/sbin/ufw").exists() || std::path::Path::new("/usr/bin/ufw").exists()
}

pub async fn get_status(State(state): State<AppState>, jar: CookieJar) -> Result<Json<FirewallStatus>> {
    require_user(&state, &jar).await?;

    if !ufw_available() {
        return Ok(Json(FirewallStatus {
            backend: "none".into(),
            enabled: false,
            rules: vec![],
            logging: None,
            error: Some("No supported firewall backend found (ufw not installed)".into()),
        }));
    }

    let out = match run_ufw(&["status", "numbered", "verbose"]) {
        Ok(o) => o,
        Err(e) => return Ok(Json(FirewallStatus {
            backend: "ufw".into(), enabled: false, rules: vec![], logging: None,
            error: Some(format!("Failed to run ufw: {e}")),
        })),
    };

    let text = String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr);

    let perm_error = !out.status.success() && (text.contains("permission") || text.contains("sudo") || text.contains("root"));

    if perm_error {
        return Ok(Json(FirewallStatus {
            backend: "ufw".into(), enabled: false, rules: vec![], logging: None,
            error: Some("VoidTower needs elevated privileges to read firewall rules (run as root or add sudo permission for ufw)".into()),
        }));
    }

    let (enabled, rules, logging) = parse_ufw_status(&text);

    Ok(Json(FirewallStatus {
        backend: "ufw".into(),
        enabled,
        rules,
        logging,
        error: None,
    }))
}

#[derive(Deserialize)]
pub struct AddRuleRequest {
    pub action: String,   // "allow" | "deny" | "limit"
    pub port: Option<String>,
    pub proto: Option<String>, // "tcp" | "udp" | "any"
    pub from: Option<String>,  // IP or "Anywhere"
    pub direction: Option<String>, // "in" | "out"
    pub comment: Option<String>,
}

pub async fn add_rule(State(state): State<AppState>, jar: CookieJar, Json(req): Json<AddRuleRequest>) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let action = match req.action.to_lowercase().as_str() {
        "allow" | "deny" | "limit" => req.action.to_lowercase(),
        _ => return Err(AppError::BadRequest("action must be allow, deny, or limit".into())),
    };

    // Build ufw args — never shell-interpolate
    let mut args: Vec<String> = vec![action.clone()];

    if let Some(ref dir) = req.direction {
        match dir.to_lowercase().as_str() {
            "in" => args.push("in".into()),
            "out" => args.push("out".into()),
            _ => {}
        }
    }

    if let Some(ref from) = req.from {
        // Basic validation: reject anything that isn't an IP/CIDR or "anywhere"
        let f = from.trim();
        if !f.eq_ignore_ascii_case("anywhere") && !f.eq_ignore_ascii_case("any") {
            // Must look like an IP or CIDR
            if f.contains(';') || f.contains('&') || f.contains('|') || f.contains('`') {
                return Err(AppError::BadRequest("invalid from address".into()));
            }
            args.push("from".into());
            args.push(f.to_string());
        }
    }

    if let Some(ref port) = req.port {
        let p = port.trim();
        // Validate port: digits, comma-separated, or range with colon
        if !p.chars().all(|c| c.is_ascii_digit() || c == ',' || c == ':') {
            return Err(AppError::BadRequest("invalid port — digits, comma, or colon only".into()));
        }
        let spec = match req.proto.as_deref() {
            Some("tcp") => format!("{p}/tcp"),
            Some("udp") => format!("{p}/udp"),
            _ => p.to_string(),
        };
        if req.from.is_some() {
            args.push("to".into());
            args.push("any".into());
            args.push("port".into());
        }
        args.push(spec);
    }

    if let Some(ref comment) = req.comment {
        let c = comment.trim();
        if !c.is_empty() {
            args.push("comment".into());
            args.push(c.to_string());
        }
    }

    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let out = run_ufw(&arg_refs).map_err(|e| AppError::Internal(e.into()))?;

    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::FeatureUnavailable(if msg.is_empty() { "ufw add rule failed".into() } else { msg }));
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct DeleteRuleRequest {
    pub num: u32,
}

pub async fn delete_rule(State(state): State<AppState>, jar: CookieJar, Json(req): Json<DeleteRuleRequest>) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    // Pass --force to skip interactive confirmation
    let num = req.num.to_string();
    let out = run_ufw(&["--force", "delete", &num]).map_err(|e| AppError::Internal(e.into()))?;

    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::FeatureUnavailable(if msg.is_empty() { "ufw delete failed".into() } else { msg }));
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct FirewallActionRequest {
    pub action: String, // "enable" | "disable" | "reload" | "reset"
}

pub async fn firewall_action(State(state): State<AppState>, jar: CookieJar, Json(req): Json<FirewallActionRequest>) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let cmd = match req.action.as_str() {
        "enable"  => vec!["--force", "enable"],
        "disable" => vec!["--force", "disable"],
        "reload"  => vec!["reload"],
        "reset"   => vec!["--force", "reset"],
        _ => return Err(AppError::BadRequest("unknown action".into())),
    };

    let out = run_ufw(&cmd).map_err(|e| AppError::Internal(e.into()))?;
    let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::FeatureUnavailable(if err.is_empty() { "ufw action failed".into() } else { err }));
    }

    Ok(Json(serde_json::json!({ "ok": true, "message": msg })))
}
