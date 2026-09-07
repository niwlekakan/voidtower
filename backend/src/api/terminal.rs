use crate::{
    api::secrets,
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

#[derive(Deserialize)]
pub struct LocalWsQuery {
    pub session_id: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(q): Query<LocalWsQuery>,
) -> Result<Response> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    super::role_guard::require_operator(&user)?;

    let user_id = user.id.clone();
    let db = state.db.clone();
    let ip = addr.ip().to_string();

    if let Some(ref local_sid) = q.session_id {
        sqlx::query("UPDATE local_sessions SET last_used = unixepoch() WHERE id = ?")
            .bind(local_sid).execute(&db).await.ok();
    }

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

// ── Local sessions ────────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
pub struct LocalSession {
    pub id:         String,
    pub label:      String,
    pub category:   Option<String>,
    pub created_at: i64,
    pub last_used:  Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateLocalSession {
    pub label:    String,
    pub category: Option<String>,
}

pub async fn list_local_sessions(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<LocalSession>>> {
    require_operator(&state, &jar).await?;
    let sessions = sqlx::query_as::<_, LocalSession>(
        "SELECT id, label, category, created_at, last_used FROM local_sessions ORDER BY last_used DESC NULLS LAST, created_at DESC"
    ).fetch_all(&state.db).await?;
    Ok(Json(sessions))
}

pub async fn create_local_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateLocalSession>,
) -> Result<Json<LocalSession>> {
    require_operator(&state, &jar).await?;
    let id = uuid::Uuid::new_v4().to_string();
    let category = req.category.as_deref().filter(|s| !s.is_empty()).map(str::to_string);
    sqlx::query("INSERT INTO local_sessions (id, label, category) VALUES (?,?,?)")
        .bind(&id).bind(&req.label).bind(&category)
        .execute(&state.db).await?;
    let s = sqlx::query_as::<_, LocalSession>(
        "SELECT id, label, category, created_at, last_used FROM local_sessions WHERE id = ?"
    ).bind(&id).fetch_one(&state.db).await?;
    Ok(Json(s))
}

pub async fn update_local_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<CreateLocalSession>,
) -> Result<Json<LocalSession>> {
    require_operator(&state, &jar).await?;
    let category = req.category.as_deref().filter(|s| !s.is_empty()).map(str::to_string);
    sqlx::query("UPDATE local_sessions SET label=?, category=? WHERE id=?")
        .bind(&req.label).bind(&category).bind(&id)
        .execute(&state.db).await?;
    let s = sqlx::query_as::<_, LocalSession>(
        "SELECT id, label, category, created_at, last_used FROM local_sessions WHERE id = ?"
    ).bind(&id).fetch_one(&state.db).await?;
    Ok(Json(s))
}

pub async fn delete_local_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_operator(&state, &jar).await?;
    sqlx::query("DELETE FROM local_sessions WHERE id = ?").bind(&id).execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── SSH sessions ──────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
pub struct SshSession {
    pub id:                String,
    pub label:             String,
    pub host:              String,
    pub port:              i64,
    pub username:          String,
    pub key_path:          Option<String>,
    pub password_secret_id: Option<String>,
    pub created_at:        i64,
    pub last_used:         Option<i64>,
}

// Outbound type — never expose secret values or secret storage details to client
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
            password_set: s.password_secret_id.is_some(),
            id: s.id,
            label: s.label,
            host: s.host,
            port: s.port,
            username: s.username,
            key_path: s.key_path,
            created_at: s.created_at,
            last_used: s.last_used,
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
    super::role_guard::require_operator(&user)?;
    Ok(user)
}

fn validate_ssh_password(password: &str) -> Result<()> {
    if password.len() > secrets::MAX_SECRET_VALUE_BYTES {
        return Err(AppError::BadRequest("SSH password exceeds size limit".into()));
    }
    Ok(())
}

pub(crate) async fn resolve_ssh_password(
    db: &sqlx::SqlitePool,
    key: &[u8; 32],
    secret_id: Option<&str>,
) -> std::result::Result<Option<String>, secrets::ResolveError> {
    match secret_id {
        Some(secret_id) => secrets::resolve(db, key, secret_id, "terminal_ssh")
            .await
            .map(Some),
        None => Ok(None),
    }
}

pub async fn list_ssh_sessions(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<SshSessionOut>>> {
    require_operator(&state, &jar).await?;
    let sessions = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_secret_id, created_at, last_used FROM ssh_sessions ORDER BY last_used DESC NULLS LAST, created_at DESC"
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
    let mut tx = state.db.begin().await?;
    let password_secret_id = if let Some(password) = req.password.as_deref().filter(|p| !p.is_empty()) {
        validate_ssh_password(password)?;
        let secret_id = uuid::Uuid::new_v4().to_string();
        let encrypted = secrets::encrypt(&state.secrets_key, password).map_err(AppError::Internal)?;
        let now = secrets::now_ts();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) \
             VALUES (?, ?, 'Terminal SSH password', ?, ?, ?)",
        )
        .bind(&secret_id)
        .bind(format!("terminal-ssh-password-{id}-{secret_id}"))
        .bind(encrypted)
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        Some(secret_id)
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO ssh_sessions (id, label, host, port, username, key_path, password_secret_id) VALUES (?,?,?,?,?,?,?)"
    ).bind(&id).bind(&req.label).bind(&req.host).bind(port)
     .bind(&req.username).bind(&req.key_path).bind(&password_secret_id)
     .execute(&mut *tx).await?;
    tx.commit().await?;

    let s = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_secret_id, created_at, last_used FROM ssh_sessions WHERE id = ?"
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
    let mut tx = state.db.begin().await?;
    let existing = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_secret_id, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&id).fetch_optional(&mut *tx).await?.ok_or(AppError::NotFound)?;
    let password_secret_id = match req.password.as_deref() {
        Some(password) if !password.is_empty() => {
            validate_ssh_password(password)?;
            if let Some(secret_id) = existing.password_secret_id.as_deref() {
                let encrypted = secrets::encrypt(&state.secrets_key, password).map_err(AppError::Internal)?;
                let updated = sqlx::query(
                    "UPDATE secrets SET value_enc=?, updated_at=?, version=version+1 WHERE id=? AND disabled=0",
                )
                .bind(encrypted)
                .bind(secrets::now_ts())
                .bind(secret_id)
                .execute(&mut *tx)
                .await?;
                if updated.rows_affected() == 0 {
                    let replacement_id = uuid::Uuid::new_v4().to_string();
                    let replacement_encrypted =
                        secrets::encrypt(&state.secrets_key, password).map_err(AppError::Internal)?;
                    let now = secrets::now_ts();
                    sqlx::query(
                        "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) \
                         VALUES (?, ?, 'Terminal SSH password', ?, ?, ?)",
                    )
                    .bind(&replacement_id)
                    .bind(format!("terminal-ssh-password-{id}-{replacement_id}"))
                    .bind(replacement_encrypted)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                    Some(replacement_id)
                } else {
                    Some(secret_id.to_string())
                }
            } else {
                let secret_id = uuid::Uuid::new_v4().to_string();
                let encrypted = secrets::encrypt(&state.secrets_key, password).map_err(AppError::Internal)?;
                let now = secrets::now_ts();
                sqlx::query(
                    "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) \
                     VALUES (?, ?, 'Terminal SSH password', ?, ?, ?)",
                )
                .bind(&secret_id)
                .bind(format!("terminal-ssh-password-{id}-{secret_id}"))
                .bind(encrypted)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                Some(secret_id)
            }
        }
        Some(_) => {
            if let Some(secret_id) = existing.password_secret_id.as_deref() {
                sqlx::query("UPDATE secrets SET disabled = 1, updated_at = ?, version = version + 1 WHERE id = ?")
                    .bind(secrets::now_ts())
                    .bind(secret_id)
                    .execute(&mut *tx)
                    .await?;
            }
            None
        }
        None => existing.password_secret_id.clone(),
    };

    sqlx::query(
        "UPDATE ssh_sessions SET label=?, host=?, port=?, username=?, key_path=?, password_secret_id=? WHERE id=?"
    ).bind(&req.label).bind(&req.host).bind(port)
     .bind(&req.username).bind(&req.key_path).bind(&password_secret_id).bind(&id)
     .execute(&mut *tx).await?;
    tx.commit().await?;

    let s = sqlx::query_as::<_, SshSession>(
        "SELECT id, label, host, port, username, key_path, password_secret_id, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&id).fetch_one(&state.db).await?;
    Ok(Json(s.into()))
}

pub async fn delete_ssh_session(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_operator(&state, &jar).await?;
    let mut tx = state.db.begin().await?;
    let password_secret_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT password_secret_id FROM ssh_sessions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let deleted = sqlx::query("DELETE FROM ssh_sessions WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    if deleted.rows_affected() == 0 {
        tx.commit().await?;
        return Ok(Json(serde_json::json!({ "ok": true })));
    }
    if let Some(secret_id) = password_secret_id {
        sqlx::query("UPDATE secrets SET disabled = 1, updated_at = ?, version = version + 1 WHERE id = ?")
            .bind(secrets::now_ts())
            .bind(secret_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
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
        "SELECT id, label, host, port, username, key_path, password_secret_id, created_at, last_used FROM ssh_sessions WHERE id = ?"
    ).bind(&q.session_id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound)?;

    let password = resolve_ssh_password(
        &state.db,
        &state.secrets_key,
        session.password_secret_id.as_deref(),
    )
    .await
    .map_err(|_| AppError::FeatureUnavailable("SSH credential unavailable".into()))?;

    sqlx::query("UPDATE ssh_sessions SET last_used = unixepoch() WHERE id = ?")
        .bind(&session.id).execute(&state.db).await?;

    audit::log(&state.db, Some(&user.id), "human", "terminal.ssh.connect",
        Some("ssh_session"), Some(&session.id), "success", Some(&addr.ip().to_string()),
        Some(&format!("{}@{}:{}", session.username, session.host, session.port)),
    ).await;

    let host     = session.host.clone();
    let port     = session.port as u16;
    let username = session.username.clone();
    let key_path = session.key_path.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        terminal::handle_ssh_ws(socket, host, port, username, key_path, password).await;
    }))
}

#[cfg(test)]
mod tests {
    use super::resolve_ssh_password;
    use crate::{api::secrets, db};
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn ssh_password_resolver_fails_closed_for_invalid_secret_states() {
        let db = setup_db().await;
        let key = [0u8; 32];
        assert_eq!(resolve_ssh_password(&db, &key, None).await, Ok(None));
        assert_eq!(
            resolve_ssh_password(&db, &key, Some("missing-secret"))
                .await
                .unwrap_err(),
            secrets::ResolveError::Missing
        );

        let valid = secrets::encrypt(&key, "ssh-password-fixture").unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) \
             VALUES ('ssh-valid', 'ssh-valid', ?, 1, 1)",
        )
        .bind(valid)
        .execute(&db)
        .await
        .unwrap();
        assert_eq!(
            resolve_ssh_password(&db, &key, Some("ssh-valid"))
                .await
                .unwrap(),
            Some("ssh-password-fixture".into())
        );

        sqlx::query("UPDATE secrets SET disabled = 1 WHERE id = 'ssh-valid'")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            resolve_ssh_password(&db, &key, Some("ssh-valid"))
                .await
                .unwrap_err(),
            secrets::ResolveError::Disabled
        );

        sqlx::query("INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES ('ssh-corrupt', 'ssh-corrupt', 'not-ciphertext', 1, 1)")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            resolve_ssh_password(&db, &key, Some("ssh-corrupt"))
                .await
                .unwrap_err(),
            secrets::ResolveError::Corrupt
        );

        let oversized = secrets::encrypt(&key, &"x".repeat(secrets::MAX_SECRET_VALUE_BYTES + 1)).unwrap();
        sqlx::query("INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES ('ssh-oversized', 'ssh-oversized', ?, 1, 1)")
            .bind(oversized)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            resolve_ssh_password(&db, &key, Some("ssh-oversized"))
                .await
                .unwrap_err(),
            secrets::ResolveError::TooLarge
        );
    }

    #[tokio::test]
    async fn ssh_session_handlers_store_and_rotate_only_secret_references() {
        use axum::{extract::Path, extract::State, Json};
        use axum_extra::extract::cookie::{Cookie, CookieJar};
        use super::{create_ssh_session, delete_ssh_session, update_ssh_session, CreateSshSession};

        let db = setup_db().await;
        let now = secrets::now_ts();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
             VALUES ('terminal-user', 'terminal-user', 'test-hash', 'operator', ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, expires_at, created_at) \
             VALUES ('terminal-session', 'terminal-user', ?, ?)",
        )
        .bind(now + 3600)
        .bind(now)
        .execute(&db)
        .await
        .unwrap();

        let state = crate::api::mcp::test_support::build(db.clone());
        let jar = CookieJar::new().add(Cookie::new("vt_session", "terminal-session"));
        let created = create_ssh_session(
            State(state.clone()),
            jar.clone(),
            Json(CreateSshSession {
                label: "fixture".into(),
                host: "192.0.2.12".into(),
                port: Some(22),
                username: "fixture-user".into(),
                key_path: None,
                password: Some("initial-password".into()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(created.password_set);

        let stored: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT password_enc, password_secret_id FROM ssh_sessions WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(stored.0.is_none());
        let secret_id = stored.1.expect("canonical SSH secret reference");
        let encrypted: String = sqlx::query_scalar("SELECT value_enc FROM secrets WHERE id = ?")
            .bind(&secret_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(secrets::decrypt(&[0u8; 32], &encrypted).unwrap(), "initial-password");

        let updated = update_ssh_session(
            State(state.clone()),
            jar.clone(),
            Path(created.id.clone()),
            Json(CreateSshSession {
                label: "fixture-updated".into(),
                host: "192.0.2.12".into(),
                port: Some(22),
                username: "fixture-user".into(),
                key_path: None,
                password: Some("rotated-password".into()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(updated.password_set);
        let rotated: String = sqlx::query_scalar("SELECT value_enc FROM secrets WHERE id = ?")
            .bind(&secret_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(secrets::decrypt(&[0u8; 32], &rotated).unwrap(), "rotated-password");
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT password_enc FROM ssh_sessions WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&db)
        .await
        .unwrap()
        .is_none());

        sqlx::query("UPDATE secrets SET disabled = 1 WHERE id = ?")
            .bind(&secret_id)
            .execute(&db)
            .await
            .unwrap();
        let rotated_after_disable = update_ssh_session(
            State(state.clone()),
            jar.clone(),
            Path(created.id.clone()),
            Json(CreateSshSession {
                label: "fixture-disabled-rotated".into(),
                host: "192.0.2.12".into(),
                port: Some(22),
                username: "fixture-user".into(),
                key_path: None,
                password: Some("rotated-after-disable".into()),
            }),
        )
        .await
        .unwrap()
        .0;
        assert!(rotated_after_disable.password_set);
        let replacement_secret_id: String = sqlx::query_scalar(
            "SELECT password_secret_id FROM ssh_sessions WHERE id = ?",
        )
        .bind(&created.id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_ne!(replacement_secret_id, secret_id);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT disabled FROM secrets WHERE id = ?")
                .bind(&replacement_secret_id)
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            resolve_ssh_password(&db, &[0u8; 32], Some(&replacement_secret_id))
                .await
                .unwrap()
                .as_deref(),
            Some("rotated-after-disable")
        );

        let _ = update_ssh_session(
            State(state.clone()),
            jar.clone(),
            Path(created.id.clone()),
            Json(CreateSshSession {
                label: "fixture-cleared".into(),
                host: "192.0.2.12".into(),
                port: Some(22),
                username: "fixture-user".into(),
                key_path: None,
                password: Some(String::new()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT disabled FROM secrets WHERE id = ?")
                .bind(&secret_id)
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );

        let recreated = update_ssh_session(
            State(state.clone()),
            jar.clone(),
            Path(created.id.clone()),
            Json(CreateSshSession {
                label: "fixture-recreated".into(),
                host: "192.0.2.12".into(),
                port: Some(22),
                username: "fixture-user".into(),
                key_path: None,
                password: Some("delete-password".into()),
            }),
        )
        .await
        .unwrap()
        .0;
        let recreated_secret_id: String = sqlx::query_scalar(
            "SELECT password_secret_id FROM ssh_sessions WHERE id = ?",
        )
        .bind(&recreated.id)
        .fetch_one(&db)
        .await
        .unwrap();
        let _ = delete_ssh_session(State(state), jar, Path(recreated.id))
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT disabled FROM secrets WHERE id = ?")
                .bind(recreated_secret_id)
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
    }
}
