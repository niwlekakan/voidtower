use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct LanNeighbor {
    pub ip: String,
    pub mac: String,
    pub iface: String,
    pub state: String,
    pub hostname: Option<String>,
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

// Parse /proc/net/arp — fast, no subprocess, no root required
fn read_proc_arp() -> Vec<LanNeighbor> {
    let content = std::fs::read_to_string("/proc/net/arp").unwrap_or_default();
    let mut neighbors = Vec::new();
    for line in content.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 { continue; }
        let ip = parts[0].to_string();
        let flags = parts[2];
        let mac = parts[3].to_string();
        let iface = parts[5].to_string();
        // flags 0x0 = incomplete, 0x2 = complete, 0x6 = complete+perm
        if mac == "00:00:00:00:00:00" || flags == "0x0" { continue; }
        let state = if flags == "0x6" { "permanent" } else { "reachable" }.to_string();
        neighbors.push(LanNeighbor { ip, mac, iface, state, hostname: None });
    }
    neighbors
}

// Supplement with `ip neigh show` for STALE entries /proc/net/arp may miss
fn read_ip_neigh() -> Vec<LanNeighbor> {
    let Ok(out) = std::process::Command::new("ip").args(["neigh", "show"]).output() else { return vec![] };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut neighbors = Vec::new();
    for line in stdout.lines() {
        // format: <ip> dev <iface> lladdr <mac> <state>
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 { continue; }
        let ip = parts[0].to_string();
        let iface = if parts.len() > 2 { parts[2].to_string() } else { String::new() };
        let mac_idx = parts.iter().position(|&p| p == "lladdr").map(|i| i + 1);
        let Some(mac_pos) = mac_idx else { continue };
        let mac = parts.get(mac_pos).unwrap_or(&"").to_string();
        if mac.is_empty() || mac == "00:00:00:00:00:00" { continue; }
        let state = parts.last().unwrap_or(&"unknown").to_lowercase();
        if state == "failed" || state == "incomplete" { continue; }
        neighbors.push(LanNeighbor { ip, mac, iface, state, hostname: None });
    }
    neighbors
}

// Reverse-DNS lookup for hostnames (best-effort, non-blocking)
fn lookup_hostnames(neighbors: &mut [LanNeighbor]) {
    for n in neighbors.iter_mut() {
        // Quick timeout lookup via `getent hosts` — synchronous but fast on LAN
        if let Ok(out) = std::process::Command::new("getent")
            .args(["hosts", &n.ip])
            .output()
        {
            let line = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let host = parts[1].to_string();
                if !host.is_empty() && host != n.ip {
                    n.hostname = Some(host);
                }
            }
        }
    }
}

pub async fn neighbors(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;

    let mut seen: HashMap<String, LanNeighbor> = HashMap::new();

    // Merge /proc/net/arp (most reliable) with ip neigh (catches more states)
    for n in read_proc_arp().into_iter().chain(read_ip_neigh()) {
        seen.entry(n.ip.clone()).or_insert(n);
    }

    let mut neighbors: Vec<LanNeighbor> = seen.into_values().collect();

    // Sort: by last octet so 192.168.1.1 comes before 192.168.1.100
    neighbors.sort_by(|a, b| {
        let parse_last = |ip: &str| ip.split('.').next_back().and_then(|s| s.parse::<u32>().ok()).unwrap_or(999);
        parse_last(&a.ip).cmp(&parse_last(&b.ip))
    });

    // Hostname lookup (async spawn so we don't block the event loop)
    let mut neighbors = tokio::task::spawn_blocking(move || {
        lookup_hostnames(&mut neighbors);
        neighbors
    })
    .await
    .unwrap_or_default();

    // Final sort by IP numerically
    neighbors.sort_by(|a, b| {
        let to_u32 = |ip: &str| -> u32 {
            let parts: Vec<u32> = ip.split('.').filter_map(|s| s.parse().ok()).collect();
            if parts.len() == 4 { (parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3] } else { 0 }
        };
        to_u32(&a.ip).cmp(&to_u32(&b.ip))
    });

    Ok(Json(serde_json::json!({ "neighbors": neighbors, "count": neighbors.len() })))
}
