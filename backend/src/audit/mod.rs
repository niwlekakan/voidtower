use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: String,
    pub timestamp: i64,
    pub user_id: Option<String>,
    pub actor_type: String,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub ip_address: Option<String>,
    pub request_id: Option<String>,
    pub details: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub async fn log(
    pool: &SqlitePool,
    user_id: Option<&str>,
    actor_type: &str,
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    outcome: &str,
    ip: Option<&str>,
    details: Option<&str>,
) {
    let id = Uuid::new_v4().to_string();
    let timestamp = unix_now();
    if let Err(e) = sqlx::query(
        "INSERT INTO audit_log
         (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, ip_address, details)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(timestamp)
    .bind(user_id)
    .bind(actor_type)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(outcome)
    .bind(ip)
    .bind(details)
    .execute(pool)
    .await
    {
        tracing::error!("Failed to write audit log: {e}");
    }
}

pub async fn list(
    pool: &SqlitePool,
    limit: i64,
    offset: i64,
) -> anyhow::Result<Vec<AuditEntry>> {
    let entries = sqlx::query_as::<_, AuditEntry>(
        "SELECT id, timestamp, user_id, actor_type, action, resource_type, resource_id,
                outcome, ip_address, request_id, details
         FROM audit_log ORDER BY timestamp DESC LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(entries)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
