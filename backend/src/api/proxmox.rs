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

// ─── helpers ─────────────────────────────────────────────────────────────────

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ─── types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProxmoxHost {
    pub id: String,
    pub name: String,
    pub url: String,
    pub node: String,
    pub fingerprint: Option<String>,
    pub created_at: String,
}

#[derive(Deserialize)]
pub struct AddHostRequest {
    pub name: String,
    pub url: String,
    pub node: Option<String>,
    pub fingerprint: Option<String>,
    pub token: String,
}

// ─── host management ─────────────────────────────────────────────────────────

pub async fn list_hosts(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let hosts = sqlx::query_as::<_, ProxmoxHost>(
        "SELECT id, name, url, node, fingerprint, created_at FROM proxmox_hosts ORDER BY name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "hosts": hosts })))
}

pub async fn add_host(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AddHostRequest>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name required".into()));
    }
    if req.token.is_empty() {
        return Err(AppError::BadRequest("token required".into()));
    }
    if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
        return Err(AppError::BadRequest("url must start with http:// or https://".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let node = req.node.as_deref().unwrap_or("pve");

    sqlx::query(
        "INSERT INTO proxmox_hosts (id, name, url, node, fingerprint) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.url)
    .bind(node)
    .bind(&req.fingerprint)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    // Store the API token in VoidTower's secrets table (encrypted).
    // Key convention: proxmox_token_{host_id}
    let secret_name = format!("proxmox_token_{id}");
    let enc = encrypt_secret(&state.secrets_key, &req.token)
        .map_err(AppError::Internal)?;
    let now = now_ts();
    sqlx::query(
        "INSERT OR REPLACE INTO secrets (id, name, description, value_enc, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&secret_name)
    .bind(format!("Proxmox API token for host {}", req.name))
    .bind(&enc)
    .bind(now)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn delete_host(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;

    let rows = sqlx::query("DELETE FROM proxmox_hosts WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    // Remove the stored token as well (best-effort).
    let secret_name = format!("proxmox_token_{id}");
    let _ = sqlx::query("DELETE FROM secrets WHERE name = ?")
        .bind(&secret_name)
        .execute(&state.db)
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ─── proxy helpers ────────────────────────────────────────────────────────────

async fn get_host(state: &AppState, host_id: &str) -> Result<ProxmoxHost> {
    sqlx::query_as::<_, ProxmoxHost>(
        "SELECT id, name, url, node, fingerprint, created_at FROM proxmox_hosts WHERE id = ?",
    )
    .bind(host_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)
}

async fn get_token(state: &AppState, host_id: &str) -> Result<String> {
    let secret_name = format!("proxmox_token_{host_id}");
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT value_enc FROM secrets WHERE name = ?",
    )
    .bind(&secret_name)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::BadRequest("No token configured for this host".into()))?;

    decrypt_secret(&state.secrets_key, &row.0).map_err(AppError::Internal)
}

fn build_client() -> reqwest::Result<reqwest::Client> {
    // TODO: support per-host TLS fingerprint verification instead of blanket
    // accept_invalid_certs. Proxmox uses self-signed certs by default.
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(15))
        .build()
}

async fn proxmox_get(
    state: &AppState,
    host_id: &str,
    path: &str,
) -> Result<serde_json::Value> {
    let host = get_host(state, host_id).await?;
    let token = get_token(state, host_id).await?;

    let url = format!("{}/api2/json/{}", host.url.trim_end_matches('/'), path);
    let client = build_client()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("reqwest build: {e}")))?;

    let resp = client
        .get(&url)
        .header("Authorization", format!("PVEAPIToken={token}"))
        .send()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("upstream request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::Internal(anyhow::anyhow!(
            "Proxmox returned {status}: {body}"
        )));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("parse response: {e}")))?;

    let data = json.get("data").cloned().unwrap_or(serde_json::Value::Null);
    Ok(data)
}

// ─── read-only proxy routes ───────────────────────────────────────────────────

pub async fn get_nodes(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let data = proxmox_get(&state, &host_id, "nodes").await?;
    Ok(Json(data))
}

pub async fn get_vms(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let host = get_host(&state, &host_id).await?;
    let node = host.node.clone();

    let qemu_path = format!("nodes/{node}/qemu");
    let lxc_path = format!("nodes/{node}/lxc");
    let (qemu, lxc) = tokio::join!(
        proxmox_get(&state, &host_id, &qemu_path),
        proxmox_get(&state, &host_id, &lxc_path),
    );

    let mut vms: Vec<serde_json::Value> = Vec::new();

    if let Ok(serde_json::Value::Array(arr)) = qemu {
        for mut v in arr {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("type".into(), serde_json::json!("qemu"));
            }
            vms.push(v);
        }
    }

    if let Ok(serde_json::Value::Array(arr)) = lxc {
        for mut v in arr {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("type".into(), serde_json::json!("lxc"));
            }
            vms.push(v);
        }
    }

    Ok(Json(serde_json::json!(vms)))
}

pub async fn get_storage(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let host = get_host(&state, &host_id).await?;
    let node = host.node.clone();
    let data = proxmox_get(&state, &host_id, &format!("nodes/{node}/storage")).await?;
    Ok(Json(data))
}

pub async fn get_tasks(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(host_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let host = get_host(&state, &host_id).await?;
    let node = host.node.clone();
    let data = proxmox_get(&state, &host_id, &format!("nodes/{node}/tasks?limit=50")).await?;
    Ok(Json(data))
}

// ─── crypto (mirrors secrets.rs) ─────────────────────────────────────────────

fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
    use aes_gcm::{aead::{Aead, KeyInit, OsRng}, Aes256Gcm, Nonce};
    use aes_gcm::aead::rand_core::RngCore;
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ciphertext);
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &blob,
    ))
}

fn decrypt_secret(key: &[u8; 32], encoded: &str) -> anyhow::Result<String> {
    use aes_gcm::{aead::{Aead, KeyInit}, Aes256Gcm, Nonce};
    let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| anyhow::anyhow!("base64 decode failed"))?;
    anyhow::ensure!(blob.len() > 12, "ciphertext too short");
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    String::from_utf8(plaintext).map_err(Into::into)
}
