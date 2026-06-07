use crate::{
    audit,
    auth,
    error::{AppError, Result},
    terminal,
    AppState,
};
use aes_gcm::{aead::{Aead, KeyInit, OsRng, rand_core::RngCore}, Aes256Gcm, Nonce};
use axum::{
    extract::{ws::WebSocketUpgrade, ConnectInfo, Path, Query, State},
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

fn encrypt_secret(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| anyhow::anyhow!("encrypt failed"))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ct);
    Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blob))
}

fn decrypt_secret(key: &[u8; 32], encoded: &str) -> anyhow::Result<String> {
    let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| anyhow::anyhow!("base64 decode failed"))?;
    anyhow::ensure!(blob.len() > 12, "ciphertext too short");
    let (nonce_bytes, ct) = blob.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let pt = cipher.decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| anyhow::anyhow!("decrypt failed"))?;
    String::from_utf8(pt).map_err(Into::into)
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Response> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }

    let user_id = user.id.clone();
    let db = state.db.clone();
    let ip = addr.ip().to_string();

    audit::log(&db, Some(&user.id), "human", "terminal.session.start",
        Some("terminal"), None, "success", Some(&ip), None).await;

    let db2 = db.clone();
    let user_id2 = user_id.clone();
    let ip2 = ip.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        terminal::handle_terminal_ws(socket, None, user_id.clone()).await;
        audit::log(&db2, Some(&user_id2), "human", "terminal.session.end",
            Some("terminal"), None, "success", Some(&ip2), None).await;
    }))
}

// ── SSH sessions ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct SshSession {
    pub id:           String,
    pub label:        String,
    pub host:         String,
    pub port:         i64,
    pub username:     String,
    pub key_path:     Option<String>,
    pub password_enc: Option<String>,
    pub created_at:   i64,
    pub last_used:    Option<i64>,
}

// Outbound type — never expose password_enc to client
#[derive(Serialize)]
pub struct SshSessionOut {
    pub id:           String,
    pub label:        String,
    pub host:         String,
    pub port:         i64,
    pub username:     String,
    pub key_path:     Option<String>,
    pub password_set: bool,
    pub created_at:   i64,
    pub last_used:    Option<i64>,
}

impl From<SshSession> for SshSessionOut {
    fn from(s: SshSession) -> Self {
        SshSessionOut {
            password_set: s.password_enc.is_some(),
            id: s.id, label: s.label, host: s.host, port: s.port,
            username: s.username, key_path: s.key_path,
            created_at: s.created_at, last_used: s.last_used,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateSshSession {
    pub label:    String,
    pub host:     String,
    pub port:     Option<i64>,
    pub username: String,
    pub key_path: Option<String>,
    pub password: Option<String>,
}

async fn require_operator(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if matches!(user.role.as_str(), "viewer") { return Err(AppError::Forbidden); }
    Ok(user)
}

pub async fn list_ssh_sessions(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<SshSessionOut>>> {
    require_operator(&state, &jar).await?;
    let sessions = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_enc, created_at, last_used FROM ssh_sessions ORDER BY last_used DESC NULLS LAST, created_at DESC"
    ).fetch_all(&state.db).await?;
    Ok(Json(sessions.into_iter().map(Into::into).collect()))
}

pub async fn create_ssh_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateSshSession>,
) -> Result<Json<SshSessionOut>> {
    require_operator(&state, &jar).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let port = req.port.unwrap_or(22);
    let password_enc = req.password
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|p| encrypt_secret(&state.secrets_key, p))
        .transpose()
        .map_err(|e| AppError::Internal(e))?;

    sqlx::query(
        "INSERT INTO ssh_sessions (id, label, host, port, username, key_path, password_enc) VALUES (?,?,?,?,?,?,?)"
    ).bind(&id).bind(&req.label).bind(&req.host).bind(port)
     .bind(&req.username).bind(&req.key_path).bind(&password_enc)
     .execute(&state.db).await?;

    let s = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_enc, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&id).fetch_one(&state.db).await?;
    Ok(Json(s.into()))
}

pub async fn update_ssh_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<CreateSshSession>,
) -> Result<Json<SshSessionOut>> {
    require_operator(&state, &jar).await?;
    let port = req.port.unwrap_or(22);

    // Fetch existing to preserve password_enc if no new password given
    let existing = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_enc, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound)?;

    let password_enc = match req.password.as_deref() {
        Some(p) if !p.is_empty() => Some(encrypt_secret(&state.secrets_key, p).map_err(AppError::Internal)?),
        Some(_) => None, // empty string = clear password
        None => existing.password_enc, // not provided = keep existing
    };

    sqlx::query(
        "UPDATE ssh_sessions SET label=?, host=?, port=?, username=?, key_path=?, password_enc=? WHERE id=?"
    ).bind(&req.label).bind(&req.host).bind(port)
     .bind(&req.username).bind(&req.key_path).bind(&password_enc).bind(&id)
     .execute(&state.db).await?;

    let s = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_enc, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&id).fetch_one(&state.db).await?;
    Ok(Json(s.into()))
}

pub async fn delete_ssh_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_operator(&state, &jar).await?;
    sqlx::query("DELETE FROM ssh_sessions WHERE id = ?").bind(&id).execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SshConnectQuery {
    pub session_id: String,
}

pub async fn ssh_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(q): Query<SshConnectQuery>,
) -> Result<Response> {
    let user = require_operator(&state, &jar).await?;
    let session = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_enc, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&q.session_id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound)?;

    sqlx::query("UPDATE ssh_sessions SET last_used = unixepoch() WHERE id = ?")
        .bind(&session.id).execute(&state.db).await.ok();

    audit::log(&state.db, Some(&user.id), "human", "terminal.ssh.connect",
        Some("ssh_session"), Some(&session.id), "success", Some(&addr.ip().to_string()),
        Some(&format!("{}@{}:{}", session.username, session.host, session.port)),
    ).await;

    let host     = session.host.clone();
    let port     = session.port as u16;
    let username = session.username.clone();
    let key_path = session.key_path.clone();
    let password = session.password_enc.as_deref()
        .and_then(|enc| decrypt_secret(&state.secrets_key, enc).ok());

    Ok(ws.on_upgrade(move |socket| async move {
        terminal::handle_ssh_ws(socket, host, port, username, key_path, password).await;
    }))
}
