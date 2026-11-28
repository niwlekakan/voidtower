use super::{
    canonical_json,
    contracts::{ActorRef, EventEnvelopeV1},
    unix_now,
};
use anyhow::{bail, Result};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventBounds {
    pub earliest: Option<i64>,
    pub latest: i64,
}

pub const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;
pub const MAX_EVENT_FRAME_BYTES: usize = 128 * 1024;
pub const MAX_EVENT_SEQUENCE: i64 = 9_007_199_254_740_991;
const MAX_EVENT_FIELD_CHARS: usize = 256;
const MAX_EVENT_PAYLOAD_STRING_CHARS: usize = 4_160;
const MAX_EVENT_PAYLOAD_ITEMS: usize = 64;
const MAX_EVENT_PAYLOAD_KEY_CHARS: usize = 128;
const MAX_EVENT_PAYLOAD_DEPTH: usize = 8;

pub fn sse_frame_fits(event_name: &str, data_bytes: usize, id: Option<i64>) -> bool {
    let id_bytes = id.map(|value| 5 + value.to_string().len()).unwrap_or(0);
    id_bytes + 8 + event_name.len() + 7 + data_bytes < MAX_EVENT_FRAME_BYTES
}

pub async fn append(
    transaction: &mut Transaction<'_, Sqlite>,
    event: PendingEvent,
) -> Result<EventEnvelopeV1> {
    let event_id = uuid::Uuid::new_v4().to_string();
    let occurred_at = unix_now();
    let payload_json = canonical_json::to_canonical_string(&event.payload)?;
    if payload_json.len() > MAX_EVENT_PAYLOAD_BYTES {
        bail!("event payload exceeds {MAX_EVENT_PAYLOAD_BYTES} bytes");
    }
    validate_event_payload(&event.payload, 0)?;
    validate_event_field("event type", &event.event_type)?;
    validate_event_field("correlation id", &event.correlation_id)?;
    for (name, value) in [
        ("resource id", event.resource_id.as_deref()),
        ("job id", event.job_id.as_deref()),
        ("approval id", event.approval_id.as_deref()),
        ("causation id", event.causation_id.as_deref()),
    ] {
        if let Some(value) = value {
            validate_event_field(name, value)?;
        }
    }
    if let Some(actor) = event.actor.as_ref() {
        validate_event_field("actor type", actor.actor_type.as_str())?;
        if let Some(value) = actor.id.as_deref() {
            validate_event_field("actor id", value)?;
        }
        if let Some(value) = actor.source.as_deref() {
            validate_event_field("actor source", value)?;
        }
    }
    let latest_sequence: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sequence), 0) FROM events")
        .fetch_one(&mut **transaction)
        .await?;
    if latest_sequence >= MAX_EVENT_SEQUENCE {
        bail!("durable event sequence is exhausted");
    }
    let sequence_counter: Option<i64> =
        sqlx::query_scalar("SELECT seq FROM sqlite_sequence WHERE name = 'events'")
            .fetch_optional(&mut **transaction)
            .await?;
    if sequence_counter.unwrap_or_default() >= MAX_EVENT_SEQUENCE {
        bail!("durable event sequence is exhausted");
    }
    let candidate = EventEnvelopeV1 {
        sequence: i64::MAX,
        event_id: event_id.clone(),
        schema_version: 1,
        event_type: event.event_type.clone(),
        occurred_at,
        actor: event.actor.clone(),
        resource_id: event.resource_id.clone(),
        job_id: event.job_id.clone(),
        approval_id: event.approval_id.clone(),
        correlation_id: event.correlation_id.clone(),
        causation_id: event.causation_id.clone(),
        payload: event.payload.clone(),
    };
    let candidate_bytes = serde_json::to_vec(&candidate)?;
    if !sse_frame_fits("durable_event", candidate_bytes.len(), Some(i64::MAX)) {
        bail!("event frame exceeds {MAX_EVENT_FRAME_BYTES} bytes");
    }
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
    if !(0..=MAX_EVENT_SEQUENCE).contains(&sequence) {
        bail!("durable event sequence is invalid");
    }

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

    rows.into_iter().map(decode_event_row).collect()
}

pub async fn list_for_resource(
    pool: &SqlitePool,
    resource_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<EventEnvelopeV1>> {
    let rows: Vec<EventRow> = sqlx::query_as(
        "SELECT sequence, event_id, schema_version, event_type, occurred_at, actor_type, \
                actor_id, actor_source, resource_id, job_id, approval_id, correlation_id, \
                causation_id, payload_json FROM events WHERE resource_id = ? \
         ORDER BY sequence DESC LIMIT ? OFFSET ?",
    )
    .bind(resource_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(decode_event_row).collect()
}

fn decode_event_row(row: EventRow) -> Result<EventEnvelopeV1> {
    let (
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
    ) = row;

    if !(0..=MAX_EVENT_SEQUENCE).contains(&sequence) {
        bail!("durable event sequence is invalid");
    }
    if !(0..=MAX_EVENT_SEQUENCE).contains(&occurred_at) {
        bail!("durable event timestamp is invalid");
    }
    if schema_version != 1 {
        bail!("unsupported durable event schema");
    }
    for (name, value) in [
        ("event id", &event_id),
        ("event type", &event_type),
        ("correlation id", &correlation_id),
    ] {
        validate_event_field(name, value)?;
    }
    for (name, value) in [
        ("resource id", resource_id.as_deref()),
        ("job id", job_id.as_deref()),
        ("approval id", approval_id.as_deref()),
        ("causation id", causation_id.as_deref()),
    ] {
        if let Some(value) = value {
            validate_event_field(name, value)?;
        }
    }
    if payload_json.len() > MAX_EVENT_PAYLOAD_BYTES {
        bail!("durable event payload exceeds {MAX_EVENT_PAYLOAD_BYTES} bytes");
    }
    let payload: Value = serde_json::from_str(&payload_json)?;
    validate_event_payload(&payload, 0)?;
    let actor = match actor_type {
        Some(actor_type) => {
            validate_event_field("actor type", &actor_type)?;
            if let Some(value) = actor_id.as_deref() {
                validate_event_field("actor id", value)?;
            }
            if let Some(value) = actor_source.as_deref() {
                validate_event_field("actor source", value)?;
            }
            Some(ActorRef {
                actor_type: parse_actor_type(&actor_type)?,
                id: actor_id,
                source: actor_source,
            })
        }
        None if actor_id.is_some() || actor_source.is_some() => {
            bail!("durable event actor metadata is incomplete")
        }
        None => None,
    };
    let envelope = EventEnvelopeV1 {
        sequence,
        event_id,
        schema_version: 1,
        event_type,
        occurred_at,
        actor,
        resource_id,
        job_id,
        approval_id,
        correlation_id,
        causation_id,
        payload,
    };
    let frame_bytes = serde_json::to_vec(&envelope)?;
    if !sse_frame_fits("durable_event", frame_bytes.len(), Some(sequence)) {
        bail!("durable event frame exceeds {MAX_EVENT_FRAME_BYTES} bytes");
    }
    Ok(envelope)
}

fn validate_event_field(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > MAX_EVENT_FIELD_CHARS {
        bail!("durable event {name} is invalid");
    }
    Ok(())
}

fn validate_event_payload(value: &Value, depth: usize) -> Result<()> {
    if depth > MAX_EVENT_PAYLOAD_DEPTH {
        bail!("durable event payload is too deeply nested");
    }
    if serde_json::to_vec(value)?.len() > MAX_EVENT_PAYLOAD_BYTES {
        bail!("durable event payload exceeds {MAX_EVENT_PAYLOAD_BYTES} bytes");
    }
    match value {
        Value::Array(items) => {
            if items.len() > MAX_EVENT_PAYLOAD_ITEMS {
                bail!("durable event payload has too many array items");
            }
            for item in items {
                validate_event_payload(item, depth + 1)?;
            }
        }
        Value::Object(fields) => {
            if fields.len() > MAX_EVENT_PAYLOAD_ITEMS {
                bail!("durable event payload has too many object fields");
            }
            for (key, item) in fields {
                if key.chars().count() > MAX_EVENT_PAYLOAD_KEY_CHARS {
                    bail!("durable event payload object key is too long");
                }
                validate_event_payload(item, depth + 1)?;
            }
        }
        Value::String(text) if text.chars().count() > MAX_EVENT_PAYLOAD_STRING_CHARS => {
            bail!("durable event payload string is too long")
        }
        _ => {}
    }
    Ok(())
}

pub fn history_is_contiguous_after(after: i64, events: &[EventEnvelopeV1]) -> bool {
    let mut expected = after;
    events.iter().all(|event| {
        let Some(next) = expected.checked_add(1) else {
            return false;
        };
        if event.sequence != next {
            return false;
        }
        expected = next;
        true
    })
}

pub async fn bounds(pool: &SqlitePool) -> Result<EventBounds> {
    let (earliest, latest): (Option<i64>, Option<i64>) =
        sqlx::query_as("SELECT MIN(sequence), MAX(sequence) FROM events")
            .fetch_one(pool)
            .await?;
    if earliest.is_some_and(|value| !(0..=MAX_EVENT_SEQUENCE).contains(&value))
        || latest.is_some_and(|value| !(0..=MAX_EVENT_SEQUENCE).contains(&value))
    {
        bail!("durable event sequence is invalid");
    }
    Ok(EventBounds {
        earliest,
        latest: latest.unwrap_or(0),
    })
}

fn parse_actor_type(value: &str) -> Result<super::contracts::ActorType> {
    use super::contracts::ActorType;
    Ok(match value {
        "human" => ActorType::Human,
        "api_token" => ActorType::ApiToken,
        "automation" => ActorType::Automation,
        "plugin" => ActorType::Plugin,
        "node" => ActorType::Node,
        "ai" => ActorType::Ai,
        "system" => ActorType::System,
        _ => bail!("unknown durable event actor type"),
    })
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
    async fn oversized_payload_is_rejected_before_insert() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let result = append(
            &mut transaction,
            PendingEvent {
                event_type: "test.oversized.v1".into(),
                actor: None,
                resource_id: None,
                job_id: None,
                approval_id: None,
                correlation_id: "test".into(),
                causation_id: None,
                payload: serde_json::json!({
                    "blob": "x".repeat(MAX_EVENT_PAYLOAD_BYTES + 1),
                }),
            },
        )
        .await;
        assert!(result.is_err());
        transaction.commit().await.unwrap();
        assert!(list_after(&pool, 0, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn oversized_frame_metadata_is_rejected_before_insert() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let result = append(
            &mut transaction,
            PendingEvent {
                event_type: "x".repeat(MAX_EVENT_FRAME_BYTES),
                actor: None,
                resource_id: None,
                job_id: None,
                approval_id: None,
                correlation_id: "test".into(),
                causation_id: None,
                payload: serde_json::json!({"safe": true}),
            },
        )
        .await;
        assert!(result.is_err());
        transaction.commit().await.unwrap();
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

    #[tokio::test]
    async fn unknown_persisted_actor_type_is_rejected() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO events
             (event_id, schema_version, event_type, occurred_at, actor_type, correlation_id, payload_json)
             VALUES (?, 1, 'test.unknown_actor.v1', 1, 'future_actor', 'test', '{}')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let error = list_after(&pool, 0, 10).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("unknown durable event actor type"));
    }

    #[tokio::test]
    async fn append_rejects_malformed_metadata_before_insert() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let error = append(
            &mut transaction,
            PendingEvent {
                event_type: "".into(),
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
        .unwrap_err();
        assert!(error.to_string().contains("event type is invalid"));
        transaction.rollback().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn append_rejects_payloads_outside_frontend_structural_bounds() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let error = append(
            &mut transaction,
            PendingEvent {
                event_type: "test.payload_bounds.v1".into(),
                actor: None,
                resource_id: None,
                job_id: None,
                approval_id: None,
                correlation_id: "test".into(),
                causation_id: None,
                payload: serde_json::json!({
                    "text": "x".repeat(MAX_EVENT_PAYLOAD_STRING_CHARS + 1),
                }),
            },
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("durable event payload string is too long"));
        transaction.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn persisted_negative_timestamp_is_rejected_on_read() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO events
             (event_id, schema_version, event_type, occurred_at, correlation_id, payload_json)
             VALUES (?, 1, 'test.invalid_timestamp.v1', -1, 'test', '{}')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let error = list_after(&pool, 0, 10).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("durable event timestamp is invalid"));
    }

    #[tokio::test]
    async fn persisted_unsafe_sequence_is_rejected_on_read() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO events
             (sequence, event_id, schema_version, event_type, occurred_at, correlation_id, payload_json)
             VALUES (?, ?, 1, 'test.unsafe_sequence.v1', 1, 'test', '{}')",
        )
        .bind(MAX_EVENT_SEQUENCE + 1)
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let error = list_after(&pool, 0, 10).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("durable event sequence is invalid"));
    }

    #[tokio::test]
    async fn bounds_rejects_negative_persisted_sequence() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO events
             (sequence, event_id, schema_version, event_type, occurred_at, correlation_id, payload_json)
             VALUES (-1, ?, 1, 'test.negative_sequence.v1', 1, 'test', '{}')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let error = bounds(&pool).await.unwrap_err();
        assert!(error
            .to_string()
            .contains("durable event sequence is invalid"));
    }

    #[tokio::test]
    async fn append_rejects_an_exhausted_sequence_counter() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO events
             (sequence, event_id, schema_version, event_type, occurred_at, correlation_id, payload_json)
             VALUES (?, ?, 1, 'test.exhausted_sequence.v1', 1, 'test', '{}')",
        )
        .bind(MAX_EVENT_SEQUENCE)
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM events WHERE sequence = ?")
            .bind(MAX_EVENT_SEQUENCE)
            .execute(&pool)
            .await
            .unwrap();
        let mut transaction = pool.begin().await.unwrap();

        let error = append(
            &mut transaction,
            PendingEvent {
                event_type: "test.rejected_sequence.v1".into(),
                actor: None,
                resource_id: None,
                job_id: None,
                approval_id: None,
                correlation_id: "test".into(),
                causation_id: None,
                payload: serde_json::json!({}),
            },
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("durable event sequence is exhausted"));
        transaction.rollback().await.unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn oversized_persisted_payload_is_rejected_on_read() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO events
             (event_id, schema_version, event_type, occurred_at, correlation_id, payload_json)
             VALUES (?, 1, 'test.oversized_read.v1', 1, 'test', ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(
            serde_json::to_string(&serde_json::json!({
                "blob": "x".repeat(MAX_EVENT_PAYLOAD_BYTES + 1),
            }))
            .unwrap(),
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = list_after(&pool, 0, 10).await.unwrap_err();
        assert!(error.to_string().contains("durable event payload exceeds"));
    }

    #[tokio::test]
    async fn malformed_persisted_metadata_is_rejected_on_resource_read() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO resources (id, kind, display_name, created_at, updated_at)
             VALUES ('resource-1', 'test', 'test resource', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events
             (event_id, schema_version, event_type, occurred_at, resource_id, correlation_id, payload_json)
             VALUES (?, 2, 'test.invalid_read.v1', 1, 'resource-1', 'test', '{}')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();

        let error = list_for_resource(&pool, "resource-1", 10, 0)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported durable event schema"));
    }

    #[tokio::test]
    async fn bounds_cover_empty_populated_and_discontinuous_history() {
        let pool = pool().await;
        assert_eq!(
            bounds(&pool).await.unwrap(),
            EventBounds {
                earliest: None,
                latest: 0,
            }
        );

        let mut transaction = pool.begin().await.unwrap();
        for number in 1..=3 {
            append(
                &mut transaction,
                PendingEvent {
                    event_type: "test.bounds.v1".into(),
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
        assert_eq!(
            bounds(&pool).await.unwrap(),
            EventBounds {
                earliest: Some(1),
                latest: 3,
            }
        );

        sqlx::query("DELETE FROM events WHERE sequence = 2")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            bounds(&pool).await.unwrap(),
            EventBounds {
                earliest: Some(1),
                latest: 3,
            }
        );
        let listed = list_after(&pool, 1, 10).await.unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![3]
        );
    }
}
