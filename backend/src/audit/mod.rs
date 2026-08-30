use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};
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
    pub source: Option<String>,
}

pub struct PendingAudit<'a> {
    pub user_id: Option<&'a str>,
    pub actor_type: &'a str,
    pub action: &'a str,
    pub resource_type: Option<&'a str>,
    pub resource_id: Option<&'a str>,
    pub outcome: &'a str,
    pub ip_address: Option<&'a str>,
    pub request_id: Option<&'a str>,
    pub details: Option<&'a str>,
    pub source: Option<&'a str>,
}

pub async fn append(
    transaction: &mut Transaction<'_, Sqlite>,
    pending: PendingAudit<'_>,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO audit_log \
         (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, \
          ip_address, request_id, details, source) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(unix_now())
    .bind(pending.user_id)
    .bind(pending.actor_type)
    .bind(pending.action)
    .bind(pending.resource_type)
    .bind(pending.resource_id)
    .bind(pending.outcome)
    .bind(pending.ip_address)
    .bind(pending.request_id)
    .bind(pending.details)
    .bind(pending.source)
    .execute(&mut **transaction)
    .await?;
    Ok(id)
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

/// Like `log` but also records an optional source tag (e.g. `"odysseus"`).
#[allow(clippy::too_many_arguments)]
pub async fn log_sourced(
    pool: &SqlitePool,
    user_id: Option<&str>,
    actor_type: &str,
    action: &str,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    outcome: &str,
    ip: Option<&str>,
    details: Option<&str>,
    source: Option<&str>,
) {
    let id = Uuid::new_v4().to_string();
    let timestamp = unix_now();
    if let Err(e) = sqlx::query(
        "INSERT INTO audit_log
         (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, ip_address, details, source)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .bind(source)
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
                outcome, ip_address, request_id, details, source
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
