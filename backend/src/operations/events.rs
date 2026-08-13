use super::{
    canonical_json,
    contracts::{ActorRef, EventEnvelopeV1},
    unix_now,
};
use anyhow::Result;
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

type EventRow = (
    i64,
    String,
    i64,
    String,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
);

#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub event_type: String,
    pub actor: Option<ActorRef>,
    pub resource_id: Option<String>,
    pub job_id: Option<String>,
    pub approval_id: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub payload: Value,
}

pub async fn append(
    transaction: &mut Transaction<'_, Sqlite>,
    event: PendingEvent,
) -> Result<EventEnvelopeV1> {
    let event_id = uuid::Uuid::new_v4().to_string();
    let occurred_at = unix_now();
    let payload_json = canonical_json::to_canonical_string(&event.payload)?;
    let actor_type = event.actor.as_ref().map(|actor| actor.actor_type.as_str());
    let actor_id = event.actor.as_ref().and_then(|actor| actor.id.as_deref());
    let actor_source = event
        .actor
        .as_ref()
        .and_then(|actor| actor.source.as_deref());
    let sequence: i64 = sqlx::query_scalar(
        "INSERT INTO events \
         (event_id, schema_version, event_type, occurred_at, actor_type, actor_id, actor_source, \
          resource_id, job_id, approval_id, correlation_id, causation_id, payload_json) \
         VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING sequence",
    )
    .bind(&event_id)
    .bind(&event.event_type)
    .bind(occurred_at)
    .bind(actor_type)
    .bind(actor_id)
    .bind(actor_source)
    .bind(event.resource_id.as_deref())
    .bind(event.job_id.as_deref())
    .bind(event.approval_id.as_deref())
    .bind(&event.correlation_id)
    .bind(event.causation_id.as_deref())
    .bind(payload_json)
    .fetch_one(&mut **transaction)
    .await?;

    Ok(EventEnvelopeV1 {
        sequence,
        event_id,
        schema_version: 1,
        event_type: event.event_type,
        occurred_at,
        actor: event.actor,
        resource_id: event.resource_id,
        job_id: event.job_id,
        approval_id: event.approval_id,
        correlation_id: event.correlation_id,
        causation_id: event.causation_id,
        payload: event.payload,
    })
}

pub async fn list_after(pool: &SqlitePool, after: i64, limit: i64) -> Result<Vec<EventEnvelopeV1>> {
    let limit = limit.clamp(1, 500);
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT sequence, event_id, schema_version, event_type, occurred_at, actor_type, \
                actor_id, actor_source, resource_id, job_id, approval_id, correlation_id, causation_id, payload_json \
         FROM events WHERE sequence > ? ORDER BY sequence LIMIT ?",
    )
    .bind(after.max(0))
    .bind(limit)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(
            |(
                sequence,
                event_id,
                schema_version,
                event_type,
                occurred_at,
                actor_type,
                actor_id,
                actor_source,
                resource_id,
                job_id,
                approval_id,
                correlation_id,
                causation_id,
                payload_json,
            )| {
                Ok(EventEnvelopeV1 {
                    sequence,
                    event_id,
                    schema_version: u16::try_from(schema_version)?,
                    event_type,
                    occurred_at,
                    actor: actor_type.map(|actor_type| ActorRef {
                        actor_type: parse_actor_type(&actor_type),
                        id: actor_id,
                        source: actor_source,
                    }),
                    resource_id,
                    job_id,
                    approval_id,
                    correlation_id,
                    causation_id,
                    payload: serde_json::from_str(&payload_json)?,
                })
            },
        )
        .collect()
}

fn parse_actor_type(value: &str) -> super::contracts::ActorType {
    use super::contracts::ActorType;
    match value {
        "human" => ActorType::Human,
        "api_token" => ActorType::ApiToken,
        "automation" => ActorType::Automation,
        "plugin" => ActorType::Plugin,
        "node" => ActorType::Node,
        "ai" => ActorType::Ai,
        _ => ActorType::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn event_is_absent_when_its_transaction_rolls_back() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        append(
            &mut transaction,
            PendingEvent {
                event_type: "test.rolled_back.v1".into(),
                actor: None,
                resource_id: None,
                job_id: None,
                approval_id: None,
                correlation_id: "test".into(),
                causation_id: None,
                payload: serde_json::json!({"safe": true}),
            },
        )
        .await
        .unwrap();
        transaction.rollback().await.unwrap();
        assert!(list_after(&pool, 0, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn cursor_listing_is_ordered_and_resumable() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        for number in 1..=3 {
            append(
                &mut transaction,
                PendingEvent {
                    event_type: "test.cursor.v1".into(),
                    actor: None,
                    resource_id: None,
                    job_id: None,
                    approval_id: None,
                    correlation_id: "test".into(),
                    causation_id: None,
                    payload: serde_json::json!({"number": number}),
                },
            )
            .await
            .unwrap();
        }
        transaction.commit().await.unwrap();
        let first = list_after(&pool, 0, 2).await.unwrap();
        assert_eq!(first.len(), 2);
        let rest = list_after(&pool, first[1].sequence, 10).await.unwrap();
        assert_eq!(rest.len(), 1);
        assert!(rest[0].sequence > first[1].sequence);
    }
}
