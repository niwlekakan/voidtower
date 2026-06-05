use crate::{
    audit,
    auth,
    error::{AppError, Result},
    terminal,
    AppState,
};
use axum::{
    extract::{ws::WebSocketUpgrade, ConnectInfo, Path, Query, State},
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Result<Response> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;

    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    // Require operator or higher for terminal
    if user.role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let user_id = user.id.clone();
    let db = state.db.clone();
    let ip = addr.ip().to_string();

    audit::log(
        &db,
        Some(&user.id),
        "human",
        "terminal.session.start",
        Some("terminal"),
        None,
        "success",
        Some(&ip),
        None,
    ).await;

    let db2 = db.clone();
    let user_id2 = user_id.clone();
    let ip2 = ip.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        terminal::handle_terminal_ws(socket, None, user_id.clone()).await;
        audit::log(
            &db2,
            Some(&user_id2),
            "human",
            "terminal.session.end",
            Some("terminal"),
            None,
            "success",
            Some(&ip2),
            None,
        ).await;
    }))
}

// ── SSH sessions ──────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct SshSession {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub key_path: Option<String>,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateSshSession {
    pub label: String,
    pub host: String,
    pub port: Option<i64>,
    pub username: String,
    pub key_path: Option<String>,
}

async fn require_operator(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if matches!(user.role.as_str(), "viewer") { return Err(AppError::Forbidden); }
    Ok(user)
}

pub async fn list_ssh_sessions(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<SshSession>>> {
    require_operator(&state, &jar).await?;
    let sessions = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, created_at, last_used FROM ssh_sessions ORDER BY last_used DESC NULLS LAST, created_at DESC"
    ).fetch_all(&state.db).await?;
    Ok(Json(sessions))
}

pub async fn create_ssh_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateSshSession>,
) -> Result<Json<SshSession>> {
    require_operator(&state, &jar).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let port = req.port.unwrap_or(22);
    sqlx::query(
        "INSERT INTO ssh_sessions (id, label, host, port, username, key_path) VALUES (?,?,?,?,?,?)"
    ).bind(&id).bind(&req.label).bind(&req.host).bind(port).bind(&req.username).bind(&req.key_path)
    .execute(&state.db).await?;
    let s = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&id).fetch_one(&state.db).await?;
    Ok(Json(s))
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
        "SELECT id, label, host, port, username, key_path, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&q.session_id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound)?;

    // Update last_used
    sqlx::query("UPDATE ssh_sessions SET last_used = unixepoch() WHERE id = ?")
        .bind(&session.id).execute(&state.db).await.ok();

    audit::log(&state.db, Some(&user.id), "human", "terminal.ssh.connect",
        Some("ssh_session"), Some(&session.id), "success", Some(&addr.ip().to_string()),
        Some(&format!("{}@{}:{}", session.username, session.host, session.port)),
    ).await;

    let host = session.host.clone();
    let port = session.port as u16;
    let username = session.username.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        terminal::handle_ssh_ws(socket, host, port, username).await;
    }))
}
