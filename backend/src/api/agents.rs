use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

// ─── Auth helpers ───────────────────────────────────────────────────────────

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let user = require_user(state, jar).await?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentWithStatus {
    pub id: String,
    pub name: String,
    pub source: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub state: String,
    pub activity: Option<String>,
    pub task_id: Option<String>,
    pub status_updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub source: String,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub enabled: Option<bool>,
}

/// Portable representation of a registry entry, used by export/import.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExportedAgent {
    pub name: String,
    pub source: String,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct ImportAgentsRequest {
    pub agents: Vec<ExportedAgent>,
}

#[derive(Debug, Deserialize)]
pub struct StatusUpdateRequest {
    pub state: String,
    pub activity: Option<String>,
    pub task_id: Option<String>,
}

/// Broadcast payload sent over `/api/agents/ws`.
#[derive(Debug, Clone, Serialize)]
pub struct AgentStatusUpdate {
    pub agent_id: String,
    pub name: String,
    pub state: String,
    pub activity: Option<String>,
    pub task_id: Option<String>,
    pub updated_at: i64,
}

pub type AgentBroadcaster = broadcast::Sender<AgentStatusUpdate>;

const VALID_STATES: &[&str] = &["working", "idle", "error", "offline"];

fn validate_state(state: &str) -> Result<()> {
    if VALID_STATES.contains(&state) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Invalid state '{state}' — must be one of {VALID_STATES:?}"
        )))
    }
}

/// How long an agent can go without a status update before it's marked offline.
const OFFLINE_TIMEOUT_SECS: i64 = 90;
/// How often the heartbeat loop checks for stale agents.
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Background task: periodically marks agents `offline` if they haven't sent a
/// status update within `OFFLINE_TIMEOUT_SECS`, broadcasting the change.
pub async fn run_heartbeat_loop(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(HEARTBEAT_INTERVAL_SECS));
    loop {
        interval.tick().await;
        let cutoff = unix_now() - OFFLINE_TIMEOUT_SECS;

        let stale = sqlx::query_as::<_, (String, String)>(
            r#"SELECT r.id, r.name FROM agent_registry r
            JOIN agent_status s ON s.agent_id = r.id
            WHERE s.state != 'offline' AND s.updated_at < ?"#,
        )
        .bind(cutoff)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for (id, name) in stale {
            let now = unix_now();
            let _ = sqlx::query("UPDATE agent_status SET state = 'offline', updated_at = ? WHERE agent_id = ?")
                .bind(now)
                .bind(&id)
                .execute(&state.db)
                .await;

            let _ = state.agents_tx.send(AgentStatusUpdate {
                agent_id: id,
                name,
                state: "offline".to_string(),
                activity: None,
                task_id: None,
                updated_at: now,
            });
        }
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<AgentWithStatus>>> {
    require_user(&state, &jar).await?;

    let agents = sqlx::query_as::<_, AgentWithStatus>(
        r#"SELECT
            r.id, r.name, r.source, r.icon, r.color, r.enabled, r.created_at,
            COALESCE(s.state, 'offline') AS state,
            s.activity,
            s.task_id,
            s.updated_at AS status_updated_at
        FROM agent_registry r
        LEFT JOIN agent_status s ON s.agent_id = r.id
        ORDER BY r.created_at ASC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(agents))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<AgentWithStatus>> {
    require_admin(&state, &jar).await?;

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Agent name is required".to_string()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();

    sqlx::query(
        "INSERT INTO agent_registry (id, name, source, icon, color, enabled, created_at) VALUES (?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.source)
    .bind(&req.icon)
    .bind(&req.color)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    sqlx::query("INSERT INTO agent_status (agent_id, state, updated_at) VALUES (?, 'offline', ?)")
        .bind(&id)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(AgentWithStatus {
        id,
        name: req.name,
        source: req.source,
        icon: req.icon,
        color: req.color,
        enabled: true,
        created_at: now,
        state: "offline".to_string(),
        activity: None,
        task_id: None,
        status_updated_at: Some(now),
    }))
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let existing = sqlx::query_as::<_, (String, Option<String>, Option<String>, bool)>(
        "SELECT name, icon, color, enabled FROM agent_registry WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

    let name = req.name.unwrap_or(existing.0);
    let icon = req.icon.or(existing.1);
    let color = req.color.or(existing.2);
    let enabled = req.enabled.unwrap_or(existing.3);

    sqlx::query("UPDATE agent_registry SET name = ?, icon = ?, color = ?, enabled = ? WHERE id = ?")
        .bind(&name)
        .bind(&icon)
        .bind(&color)
        .bind(enabled)
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let result = sqlx::query("DELETE FROM agent_registry WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/agents/export — admin-only dump of the agent registry (no runtime status).
pub async fn export(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<ExportedAgent>>> {
    require_admin(&state, &jar).await?;

    let agents = sqlx::query_as::<_, ExportedAgent>(
        "SELECT name, source, icon, color, enabled FROM agent_registry ORDER BY created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(agents))
}

/// POST /api/agents/import — admin-only bulk upsert by `name`, creating registry + status rows.
pub async fn import(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ImportAgentsRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let now = unix_now();
    let mut imported: u32 = 0;

    for agent in &req.agents {
        if agent.name.trim().is_empty() {
            return Err(AppError::BadRequest("Agent name is required".to_string()));
        }

        let existing_id: Option<String> = sqlx::query_scalar("SELECT id FROM agent_registry WHERE name = ?")
            .bind(&agent.name)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::Database)?;

        let _id = match existing_id {
            Some(id) => {
                sqlx::query("UPDATE agent_registry SET source = ?, icon = ?, color = ?, enabled = ? WHERE id = ?")
                    .bind(&agent.source)
                    .bind(&agent.icon)
                    .bind(&agent.color)
                    .bind(agent.enabled)
                    .bind(&id)
                    .execute(&state.db)
                    .await
                    .map_err(AppError::Database)?;
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO agent_registry (id, name, source, icon, color, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&agent.name)
                .bind(&agent.source)
                .bind(&agent.icon)
                .bind(&agent.color)
                .bind(agent.enabled)
                .bind(now)
                .execute(&state.db)
                .await
                .map_err(AppError::Database)?;

                sqlx::query("INSERT INTO agent_status (agent_id, state, updated_at) VALUES (?, 'offline', ?)")
                    .bind(&id)
                    .bind(now)
                    .execute(&state.db)
                    .await
                    .map_err(AppError::Database)?;
                id
            }
        };
        imported += 1;
    }

    Ok(Json(serde_json::json!({ "ok": true, "imported": imported })))
}

pub async fn get_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<AgentStatusUpdate>> {
    require_user(&state, &jar).await?;

    let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, i64)>(
        r#"SELECT r.id, COALESCE(s.state, 'offline'), s.activity, s.task_id, COALESCE(s.updated_at, r.created_at)
        FROM agent_registry r
        LEFT JOIN agent_status s ON s.agent_id = r.id
        WHERE r.id = ?"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

    let name: String = sqlx::query_scalar("SELECT name FROM agent_registry WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(AgentStatusUpdate {
        agent_id: row.0,
        name,
        state: row.1,
        activity: row.2,
        task_id: row.3,
        updated_at: row.4,
    }))
}

/// Status ingest endpoint — agents (e.g. Odysseus) push their current activity here.
pub async fn post_status(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<StatusUpdateRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    validate_state(&req.state)?;

    let name: String = sqlx::query_scalar("SELECT name FROM agent_registry WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::NotFound)?;

    let now = unix_now();

    sqlx::query(
        r#"INSERT INTO agent_status (agent_id, state, activity, task_id, updated_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(agent_id) DO UPDATE SET state = excluded.state, activity = excluded.activity, task_id = excluded.task_id, updated_at = excluded.updated_at"#,
    )
    .bind(&id)
    .bind(&req.state)
    .bind(&req.activity)
    .bind(&req.task_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    let _ = state.agents_tx.send(AgentStatusUpdate {
        agent_id: id,
        name,
        state: req.state,
        activity: req.activity,
        task_id: req.task_id,
        updated_at: now,
    });

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Response> {
    require_user(&state, &jar).await?;
    let rx = state.agents_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, rx)))
}

async fn handle_ws(socket: WebSocket, mut rx: broadcast::Receiver<AgentStatusUpdate>) {
    let (mut sink, mut stream) = socket.split();

    loop {
        tokio::select! {
            Ok(update) = rx.recv() => {
                let json = match serde_json::to_string(&update) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if sink.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn test_create_and_list_agent() {
        let pool = setup_db().await;

        let now = unix_now();
        sqlx::query("INSERT INTO agent_registry (id, name, source, icon, color, enabled, created_at) VALUES ('a1', 'Odysseus', 'odysseus', NULL, NULL, 1, ?)")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO agent_status (agent_id, state, updated_at) VALUES ('a1', 'offline', ?)")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let agents = sqlx::query_as::<_, AgentWithStatus>(
            r#"SELECT
                r.id, r.name, r.source, r.icon, r.color, r.enabled, r.created_at,
                COALESCE(s.state, 'offline') AS state,
                s.activity, s.task_id, s.updated_at AS status_updated_at
            FROM agent_registry r
            LEFT JOIN agent_status s ON s.agent_id = r.id
            ORDER BY r.created_at ASC"#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "Odysseus");
        assert_eq!(agents[0].state, "offline");
    }

    #[tokio::test]
    async fn test_status_upsert() {
        let pool = setup_db().await;
        let now = unix_now();

        sqlx::query("INSERT INTO agent_registry (id, name, source, enabled, created_at) VALUES ('a1', 'Odysseus', 'odysseus', 1, ?)")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        for state in ["offline", "working"] {
            sqlx::query(
                r#"INSERT INTO agent_status (agent_id, state, activity, task_id, updated_at)
                VALUES ('a1', ?, NULL, NULL, ?)
                ON CONFLICT(agent_id) DO UPDATE SET state = excluded.state, updated_at = excluded.updated_at"#,
            )
            .bind(state)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        }

        let final_state: String = sqlx::query_scalar("SELECT state FROM agent_status WHERE agent_id = 'a1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(final_state, "working");
    }

    #[test]
    fn test_validate_state() {
        assert!(validate_state("working").is_ok());
        assert!(validate_state("idle").is_ok());
        assert!(validate_state("bogus").is_err());
    }

    #[tokio::test]
    async fn test_export_and_import_roundtrip() {
        let pool = setup_db().await;
        let now = unix_now();

        sqlx::query("INSERT INTO agent_registry (id, name, source, icon, color, enabled, created_at) VALUES ('a1', 'Odysseus', 'odysseus', NULL, '#fff', 1, ?)")
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let exported = sqlx::query_as::<_, ExportedAgent>(
            "SELECT name, source, icon, color, enabled FROM agent_registry ORDER BY created_at ASC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].name, "Odysseus");

        // Import into a fresh database — should create a new registry + status row
        let pool2 = setup_db().await;
        for agent in &exported {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO agent_registry (id, name, source, icon, color, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(&agent.name)
                .bind(&agent.source)
                .bind(&agent.icon)
                .bind(&agent.color)
                .bind(agent.enabled)
                .bind(now)
                .execute(&pool2)
                .await
                .unwrap();
            sqlx::query("INSERT INTO agent_status (agent_id, state, updated_at) VALUES (?, 'offline', ?)")
                .bind(&id)
                .bind(now)
                .execute(&pool2)
                .await
                .unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_registry")
            .fetch_one(&pool2)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
}
