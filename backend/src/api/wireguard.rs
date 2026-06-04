use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

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

// ─── Key generation (native Curve25519 — no wg binary required) ───────────────

fn generate_keypair() -> (String, String) {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    (B64.encode(secret.as_bytes()), B64.encode(public.as_bytes()))
}

// ─── wg command helpers ───────────────────────────────────────────────────────

fn wg_cmd(args: &[&str]) -> std::result::Result<String, String> {
    let out = std::process::Command::new("wg")
        .args(args)
        .output()
        .map_err(|e| format!("wg not found: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).to_lowercase();
        if err.contains("permission") || err.contains("operation not permitted") {
            Err("Permission denied — VoidTower needs root or CAP_NET_ADMIN to manage WireGuard".to_string())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        }
    }
}

fn wg_available() -> bool {
    std::process::Command::new("wg")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── Parse `wg show all dump` ─────────────────────────────────────────────────
//
// Interface line (5 tab-separated fields):
//   iface  private-key  public-key  listen-port  fwmark
//
// Peer line (9 tab-separated fields):
//   iface  public-key  preshared-key  endpoint  allowed-ips  latest-handshake  rx  tx  persistent-keepalive

#[derive(Default)]
struct WgDumpIface {
    public_key: String,
    listen_port: u16,
}

#[derive(Default)]
struct WgDumpPeer {
    endpoint: Option<String>,
    latest_handshake: Option<i64>,
    rx_bytes: u64,
    tx_bytes: u64,
}

fn parse_wg_dump(output: &str) -> (HashMap<String, WgDumpIface>, HashMap<String, WgDumpPeer>) {
    let mut ifaces: HashMap<String, WgDumpIface> = HashMap::new();
    let mut peers: HashMap<String, WgDumpPeer> = HashMap::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(10, '\t').collect();
        match parts.len() {
            5 => {
                // Interface line
                ifaces.insert(parts[0].to_string(), WgDumpIface {
                    public_key: parts[2].to_string(),
                    listen_port: parts[3].parse().unwrap_or(51820),
                });
            }
            9 => {
                // Peer line
                let endpoint = if parts[3] == "(none)" { None } else { Some(parts[3].to_string()) };
                let handshake = parts[5].parse::<i64>().ok().filter(|&v| v > 0);
                let rx = parts[6].parse::<u64>().unwrap_or(0);
                let tx = parts[7].parse::<u64>().unwrap_or(0);
                peers.insert(parts[1].to_string(), WgDumpPeer { endpoint, latest_handshake: handshake, rx_bytes: rx, tx_bytes: tx });
            }
            _ => {}
        }
    }
    (ifaces, peers)
}

// ─── Config file helpers ──────────────────────────────────────────────────────

fn conf_path(iface: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("/etc/wireguard/{iface}.conf"))
}

fn read_conf(iface: &str) -> String {
    std::fs::read_to_string(conf_path(iface)).unwrap_or_default()
}

fn write_conf(iface: &str, content: &str) -> std::result::Result<(), String> {
    std::fs::write(conf_path(iface), content)
        .map_err(|e| format!("Cannot write /etc/wireguard/{iface}.conf: {e}"))
}

// Parse [Interface] Address = x.x.x.1/24 → returns ("10.0.0", 24, 1) or default
fn parse_server_addr(conf: &str) -> (String, u8, u32) {
    for line in conf.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Address") {
            let val = rest.trim_start_matches('=').trim().split(',').next().unwrap_or("").trim();
            if let Some((ip, mask)) = val.split_once('/') {
                let mask: u8 = mask.parse().unwrap_or(24);
                let parts: Vec<&str> = ip.split('.').collect();
                if parts.len() == 4 {
                    let prefix = format!("{}.{}.{}", parts[0], parts[1], parts[2]);
                    let last: u32 = parts[3].parse().unwrap_or(1);
                    return (prefix, mask, last);
                }
            }
        }
    }
    ("10.13.37".to_string(), 24, 1)
}

fn parse_server_pubkey(conf: &str) -> Option<String> {
    // Try to get from running wg first; fall back to deriving from private key in conf
    for line in conf.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("PrivateKey") {
            let priv_b64 = rest.trim_start_matches('=').trim();
            if let Ok(bytes) = B64.decode(priv_b64) {
                if bytes.len() == 32 {
                    let arr: [u8; 32] = bytes.try_into().ok()?;
                    let secret = StaticSecret::from(arr);
                    let public = PublicKey::from(&secret);
                    return Some(B64.encode(public.as_bytes()));
                }
            }
        }
    }
    None
}

fn append_peer_to_conf(iface: &str, name: &str, pubkey: &str, ip: &str) -> std::result::Result<(), String> {
    let mut conf = read_conf(iface);
    if !conf.ends_with('\n') && !conf.is_empty() { conf.push('\n'); }
    conf.push_str(&format!(
        "\n[Peer]\n# {name}\nPublicKey = {pubkey}\nAllowedIPs = {ip}/32\nPersistentKeepalive = 25\n"
    ));
    write_conf(iface, &conf)
}

fn remove_peer_from_conf(iface: &str, pubkey: &str) -> std::result::Result<(), String> {
    let conf = read_conf(iface);
    let mut out = String::new();
    let mut skip = false;
    let mut pending = String::new();

    for line in conf.lines() {
        let trimmed = line.trim();
        if trimmed == "[Peer]" {
            if !pending.is_empty() && !skip {
                out.push_str(&pending);
            }
            pending = format!("{line}\n");
            skip = false;
        } else if trimmed.starts_with('[') && trimmed != "[Peer]" {
            if !pending.is_empty() && !skip {
                out.push_str(&pending);
            }
            pending = String::new();
            skip = false;
            out.push_str(&format!("{line}\n"));
        } else if pending.is_empty() {
            out.push_str(&format!("{line}\n"));
        } else {
            let is_key_line = trimmed.starts_with("PublicKey")
                && trimmed.replace(" ", "").contains(&format!("={pubkey}"));
            if is_key_line { skip = true; }
            pending.push_str(&format!("{line}\n"));
        }
    }
    if !pending.is_empty() && !skip {
        out.push_str(&pending);
    }
    write_conf(iface, &out)
}

// ─── IP allocation ────────────────────────────────────────────────────────────

fn allocate_ip(prefix: &str, used: &[String]) -> String {
    for host in 2u32..=254 {
        let candidate = format!("{prefix}.{host}");
        if !used.contains(&candidate) {
            return candidate;
        }
    }
    format!("{prefix}.2") // fallback; shouldn't happen in practice
}

// ─── Client config builder ────────────────────────────────────────────────────

fn client_config(
    private_key: &str,
    client_ip: &str,
    mask: u8,
    server_pubkey: &str,
    server_endpoint: &str,
    listen_port: u16,
) -> String {
    format!(
        "[Interface]\nPrivateKey = {private_key}\nAddress = {client_ip}/{mask}\nDNS = 1.1.1.1\n\
         \n[Peer]\nPublicKey = {server_pubkey}\nEndpoint = {server_endpoint}:{listen_port}\n\
         AllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n"
    )
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
struct PeerRow {
    id: String,
    interface: String,
    name: String,
    public_key: String,
    allocated_ip: String,
    created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct WgPeer {
    id: String,
    interface: String,
    name: String,
    public_key: String,
    allocated_ip: String,
    endpoint: Option<String>,
    latest_handshake: Option<i64>,
    rx_bytes: u64,
    tx_bytes: u64,
    connected: bool,
    created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct WgInterface {
    name: String,
    public_key: String,
    listen_port: u16,
}

#[derive(Deserialize)]
pub struct AddPeerRequest {
    pub name: String,
    #[serde(default = "default_iface")]
    pub interface: String,
    pub server_endpoint: Option<String>,
}
fn default_iface() -> String { "wg0".to_string() }

// ─── Handlers ─────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let _ = require_admin(&state, &jar).await?;

    let available = wg_available();
    let mut interfaces: Vec<WgInterface> = Vec::new();
    let mut live_ifaces: HashMap<String, WgDumpIface> = HashMap::new();
    let mut live_peers: HashMap<String, WgDumpPeer> = HashMap::new();
    let mut wg_error: Option<String> = None;

    if available {
        match wg_cmd(&["show", "all", "dump"]) {
            Ok(dump) => {
                let (i, p) = parse_wg_dump(&dump);
                for (name, data) in &i {
                    interfaces.push(WgInterface {
                        name: name.clone(),
                        public_key: data.public_key.clone(),
                        listen_port: data.listen_port,
                    });
                }
                live_ifaces = i;
                live_peers = p;
            }
            Err(e) => wg_error = Some(e),
        }
    }

    // If no interfaces found from wg show, look for conf files
    if interfaces.is_empty() {
        if let Ok(entries) = std::fs::read_dir("/etc/wireguard") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".conf") {
                    let iface = name.trim_end_matches(".conf").to_string();
                    let conf = read_conf(&iface);
                    let pubkey = parse_server_pubkey(&conf)
                        .or_else(|| live_ifaces.get(&iface).map(|i| i.public_key.clone()))
                        .unwrap_or_default();
                    interfaces.push(WgInterface {
                        name: iface,
                        public_key: pubkey,
                        listen_port: 51820,
                    });
                }
            }
        }
    }

    // Merge DB peers with live data
    let db_peers: Vec<PeerRow> = sqlx::query_as(
        "SELECT id, interface, name, public_key, allocated_ip, created_at FROM wireguard_peers ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let now = unix_now();
    let peers: Vec<WgPeer> = db_peers
        .into_iter()
        .map(|row| {
            let live = live_peers.get(&row.public_key);
            let last_hs = live.and_then(|l| l.latest_handshake);
            let connected = last_hs.map(|ts| (now - ts) < 180).unwrap_or(false);
            WgPeer {
                id: row.id,
                interface: row.interface,
                name: row.name,
                public_key: row.public_key,
                allocated_ip: row.allocated_ip,
                endpoint: live.and_then(|l| l.endpoint.clone()),
                latest_handshake: last_hs,
                rx_bytes: live.map(|l| l.rx_bytes).unwrap_or(0),
                tx_bytes: live.map(|l| l.tx_bytes).unwrap_or(0),
                connected,
                created_at: row.created_at,
            }
        })
        .collect();

    Ok(Json(serde_json::json!({
        "available": available,
        "error": wg_error,
        "interfaces": interfaces,
        "peers": peers,
    })))
}

pub async fn add_peer(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AddPeerRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Peer name is required".into()));
    }

    let iface = req.interface.trim().to_string();

    // Get server info
    let conf = read_conf(&iface);
    let (prefix, mask, _server_last) = parse_server_addr(&conf);

    let server_pubkey = wg_cmd(&["show", &iface, "public-key"])
        .ok()
        .or_else(|| parse_server_pubkey(&conf))
        .unwrap_or_else(|| "(unknown — run wg show to verify)".to_string());

    let listen_port = wg_cmd(&["show", &iface, "listen-port"])
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(51820);

    // Collect used IPs
    let used_ips: Vec<String> = sqlx::query_scalar(
        "SELECT allocated_ip FROM wireguard_peers WHERE interface = ?",
    )
    .bind(&iface)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let client_ip = allocate_ip(&prefix, &used_ips);

    // Generate keypair
    let (private_key, public_key) = generate_keypair();

    // Add to running WireGuard (best-effort — might need root)
    let wg_result = wg_cmd(&[
        "set", &iface, "peer", &public_key,
        "allowed-ips", &format!("{client_ip}/32"),
        "persistent-keepalive", "25",
    ]);

    // Write to conf file (best-effort)
    let conf_result = append_peer_to_conf(&iface, &req.name, &public_key, &client_ip);

    // Save to DB regardless (so we track it even if wg command had issues)
    let id = Uuid::new_v4().to_string();
    let now = unix_now();
    sqlx::query(
        "INSERT INTO wireguard_peers (id, interface, name, public_key, allocated_ip, created_at) VALUES (?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&iface)
    .bind(&req.name)
    .bind(&public_key)
    .bind(&client_ip)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    // Build client config
    let endpoint = req.server_endpoint.as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| std::env::var("VOIDTOWER_WG_ENDPOINT").ok())
        .unwrap_or_else(|| "YOUR_SERVER_IP".to_string());

    let config_str = client_config(&private_key, &client_ip, mask, &server_pubkey, &endpoint, listen_port);

    audit::log(
        &state.db, Some(&user.id), "human", "wireguard.peer_add",
        Some("wireguard_peer"), Some(&id), "success", None,
        Some(&format!("name={},ip={client_ip},iface={iface}", req.name)),
    ).await;

    let warnings: Vec<String> = [
        wg_result.err().map(|e| format!("wg set: {e}")),
        conf_result.err().map(|e| format!("conf update: {e}")),
    ]
    .into_iter()
    .flatten()
    .collect();

    Ok(Json(serde_json::json!({
        "id": id,
        "public_key": public_key,
        "allocated_ip": client_ip,
        "client_config": config_str,
        "warnings": warnings,
    })))
}

pub async fn delete_peer(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(peer_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let row: Option<PeerRow> = sqlx::query_as(
        "SELECT id, interface, name, public_key, allocated_ip, created_at FROM wireguard_peers WHERE id = ?",
    )
    .bind(&peer_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let peer = row.ok_or_else(|| AppError::BadRequest("Peer not found".into()))?;

    // Remove from running WireGuard
    let wg_result = wg_cmd(&["set", &peer.interface, "peer", &peer.public_key, "remove"]);

    // Remove from conf file
    let conf_result = remove_peer_from_conf(&peer.interface, &peer.public_key);

    // Remove from DB
    sqlx::query("DELETE FROM wireguard_peers WHERE id = ?")
        .bind(&peer_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db, Some(&user.id), "human", "wireguard.peer_remove",
        Some("wireguard_peer"), Some(&peer_id), "success", None,
        Some(&format!("name={},ip={}", peer.name, peer.allocated_ip)),
    ).await;

    let warnings: Vec<String> = [
        wg_result.err().map(|e| format!("wg set: {e}")),
        conf_result.err().map(|e| format!("conf update: {e}")),
    ]
    .into_iter()
    .flatten()
    .collect();

    Ok(Json(serde_json::json!({ "ok": true, "warnings": warnings })))
}
