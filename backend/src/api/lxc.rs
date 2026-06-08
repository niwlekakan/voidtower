use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

pub fn is_pct_available() -> bool {
    std::path::Path::new("/usr/sbin/pct").exists()
        || std::path::Path::new("/usr/bin/pct").exists()
}

// ── types ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LxcContainer {
    pub vmid: u32,
    pub name: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct LxcConfig {
    pub hostname: String,
    pub memory: u64,
    pub cores: u32,
    pub arch: String,
    pub rootfs: String,
    pub raw: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub available: bool,
    pub containers: Vec<LxcContainer>,
}

// ── parsers ───────────────────────────────────────────────────────────────────

fn parse_pct_list(output: &str) -> Vec<LxcContainer> {
    let mut containers = Vec::new();
    let mut past_header = false;
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if trimmed.starts_with("VMID") { past_header = true; continue; }
        if !past_header { continue; }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let Some(vmid) = parts.first().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let status = parts.get(1).unwrap_or(&"unknown").to_string();
        // Name is always the last field; lock (if present) sits between status and name.
        // Skip cases where only VMID+status were parsed (no name field present).
        let name = if parts.len() >= 3 {
            parts.last().map(|s| s.to_string()).unwrap_or_default()
        } else {
            String::new()
        };
        containers.push(LxcContainer { vmid, name, status });
    }
    containers
}

fn parse_pct_config(output: &str) -> LxcConfig {
    let mut raw: HashMap<String, String> = HashMap::new();
    for line in output.lines() {
        if let Some((k, v)) = line.split_once(": ") {
            raw.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    LxcConfig {
        hostname: raw.get("hostname").cloned().unwrap_or_default(),
        memory:   raw.get("memory").and_then(|v| v.parse().ok()).unwrap_or(512),
        cores:    raw.get("cores").and_then(|v| v.parse().ok()).unwrap_or(1),
        arch:     raw.get("arch").cloned().unwrap_or_else(|| "amd64".to_string()),
        rootfs:   raw.get("rootfs").cloned().unwrap_or_default(),
        raw,
    }
}

// ── handlers ──────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<ListResponse>> {
    require_admin(&state, &jar).await?;
    if !is_pct_available() {
        return Ok(Json(ListResponse { available: false, containers: vec![] }));
    }
    let out = std::process::Command::new("pct")
        .arg("list")
        .output()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(Json(ListResponse {
        available: true,
        containers: parse_pct_list(&stdout),
    }))
}

pub async fn get_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(vmid): Path<u32>,
) -> Result<Json<LxcConfig>> {
    require_admin(&state, &jar).await?;
    if !is_pct_available() {
        return Err(AppError::BadRequest("pct not available".into()));
    }
    let out = std::process::Command::new("pct")
        .args(["config", &vmid.to_string()])
        .output()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::BadRequest(msg));
    }
    Ok(Json(parse_pct_config(&String::from_utf8_lossy(&out.stdout))))
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub action: String,
}

pub async fn action(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(vmid): Path<u32>,
    Json(req): Json<ActionRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if !is_pct_available() {
        return Err(AppError::BadRequest("pct not available".into()));
    }
    let pct_cmd = match req.action.as_str() {
        "start"    => "start",
        "stop"     => "stop",
        "shutdown" => "shutdown",
        "restart"  => "restart",
        _ => return Err(AppError::BadRequest(format!("unknown action: {}", req.action))),
    };
    let out = std::process::Command::new("pct")
        .args([pct_cmd, &vmid.to_string()])
        .output()
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        Ok(Json(serde_json::json!({ "ok": true, "message": stdout })))
    } else {
        Ok(Json(serde_json::json!({ "ok": false, "message": if stderr.is_empty() { stdout } else { stderr } })))
    }
}
