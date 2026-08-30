use crate::{
    audit::{self, PendingAudit},
    cmdb::assets::MutationContext,
    operations::{
        canonical_json,
        events::{self, PendingEvent},
        unix_now,
    },
};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

const MAX_METADATA_BYTES: usize = 16 * 1024;
const MAX_METADATA_DEPTH: usize = 8;
const MAX_METADATA_VALUES: usize = 128;
const MAX_METADATA_STRING_LEN: usize = 2_048;
const MAX_WRITE_ATTEMPTS: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum RelationshipError {
    #[error("invalid relationship: {0}")]
    Invalid(String),
    #[error("relationship not found")]
    NotFound,
    #[error("relationship type not found")]
    TypeNotFound,
    #[error("relationship type is disabled")]
    TypeDisabled,
    #[error("source CMDB asset not found")]
    SourceNotFound,
    #[error("destination CMDB asset not found")]
    DestinationNotFound,
    #[error("relationship conflict: {0}")]
    Conflict(String),
    #[error("relationship operation failed")]
    Internal(#[source] anyhow::Error),
}

impl From<sqlx::Error> for RelationshipError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<anyhow::Error> for RelationshipError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

pub type Result<T> = std::result::Result<T, RelationshipError>;

fn is_sqlite_busy(error: &RelationshipError) -> bool {
    match error {
        RelationshipError::Internal(error) => {
            let message = format!("{error:#}");
            message.contains("database is locked") || message.contains("database is busy")
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RelationshipRecord {
    pub id: String,
    pub source_resource_id: String,
    pub destination_resource_id: String,
    pub type_key: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub active: bool,
    pub metadata_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct StartRelationshipInput {
    pub source_resource_id: String,
    pub destination_resource_id: String,
    pub type_key: String,
    pub metadata: Value,
}

fn actor_type(context: &MutationContext) -> &'static str {
    context.actor.actor_type.as_str()
}

fn validate_metadata_value(value: &Value, depth: usize, value_count: &mut usize) -> Result<()> {
    if depth > MAX_METADATA_DEPTH {
        return Err(RelationshipError::Invalid(format!(
            "metadata exceeds maximum depth {MAX_METADATA_DEPTH}"
        )));
    }
    *value_count += 1;
    if *value_count > MAX_METADATA_VALUES {
        return Err(RelationshipError::Invalid(format!(
            "metadata exceeds maximum value count {MAX_METADATA_VALUES}"
        )));
    }
    match value {
        Value::String(value) if value.chars().count() > MAX_METADATA_STRING_LEN => {
            Err(RelationshipError::Invalid(format!(
                "metadata string exceeds {MAX_METADATA_STRING_LEN} characters"
            )))
        }
        Value::Array(values) => {
            if values.len() > MAX_METADATA_VALUES {
                return Err(RelationshipError::Invalid(format!(
                    "metadata array exceeds {MAX_METADATA_VALUES} values"
                )));
            }
            for value in values {
                validate_metadata_value(value, depth + 1, value_count)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_METADATA_VALUES {
                return Err(RelationshipError::Invalid(format!(
                    "metadata object exceeds {MAX_METADATA_VALUES} fields"
                )));
            }
            for (key, value) in values {
                if key.chars().count() > MAX_METADATA_STRING_LEN {
                    return Err(RelationshipError::Invalid(format!(
                        "metadata key exceeds {MAX_METADATA_STRING_LEN} characters"
                    )));
                }
                validate_metadata_value(value, depth + 1, value_count)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn canonical_metadata(metadata: &Value) -> Result<String> {
    if !metadata.is_object() {
        return Err(RelationshipError::Invalid(
            "metadata must be a JSON object".into(),
        ));
    }
    validate_metadata_value(metadata, 1, &mut 0)?;
    let encoded = canonical_json::to_canonical_string(metadata)
        .map_err(|error| RelationshipError::Invalid(error.to_string()))?;
    if encoded.len() > MAX_METADATA_BYTES {
        return Err(RelationshipError::Invalid(format!(
            "metadata exceeds {MAX_METADATA_BYTES} bytes"
        )));
    }
    Ok(encoded)
}

async fn require_type(transaction: &mut Transaction<'_, Sqlite>, type_key: &str) -> Result<()> {
    let enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM cmdb_relationship_types WHERE key = ?")
            .bind(type_key)
            .fetch_optional(&mut **transaction)
            .await?;
    match enabled {
        None => Err(RelationshipError::TypeNotFound),
        Some(false) => Err(RelationshipError::TypeDisabled),
        Some(true) => Ok(()),
    }
}

async fn require_asset(
    transaction: &mut Transaction<'_, Sqlite>,
    resource_id: &str,
    missing: RelationshipError,
) -> Result<()> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_assets WHERE resource_id = ?")
        .bind(resource_id)
        .fetch_one(&mut **transaction)
        .await?;
    if exists == 1 {
        Ok(())
    } else {
        Err(missing)
    }
}

async fn append_endpoint_event(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    endpoint_id: &str,
    event_type: &'static str,
    payload: Value,
) -> Result<()> {
    events::append(
        transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(context.actor.clone()),
            resource_id: Some(endpoint_id.into()),
            job_id: None,
            approval_id: None,
            correlation_id: context.correlation_id.clone(),
            causation_id: None,
            payload,
        },
    )
    .await?;
    Ok(())
}

async fn append_audit(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    action: &'static str,
    relationship_id: &str,
) -> Result<()> {
    audit::append(
        transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: actor_type(context),
            action,
            resource_type: Some("relationship"),
            resource_id: Some(relationship_id),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: None,
            source: context.actor.source.as_deref(),
        },
    )
    .await?;
    Ok(())
}

async fn append_started(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    relationship: &RelationshipRecord,
) -> Result<()> {
    let payload = serde_json::json!({
        "relationship_id": relationship.id,
        "source_resource_id": relationship.source_resource_id,
        "destination_resource_id": relationship.destination_resource_id,
        "type": relationship.type_key,
        "started_at": relationship.started_at,
    });
    for endpoint in [
        &relationship.source_resource_id,
        &relationship.destination_resource_id,
    ] {
        append_endpoint_event(
            transaction,
            context,
            endpoint,
            "cmdb.asset.relationship_started.v1",
            payload.clone(),
        )
        .await?;
    }
    append_audit(
        transaction,
        context,
        "cmdb.relationship.start",
        &relationship.id,
    )
    .await
}

async fn end_record(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    relationship: &mut RelationshipRecord,
    ended_at: i64,
    reason: &'static str,
) -> Result<()> {
    if !relationship.active {
        return Ok(());
    }
    sqlx::query(
        "UPDATE cmdb_relationships SET active = 0, ended_at = ?, updated_at = ? \
         WHERE id = ? AND active = 1",
    )
    .bind(ended_at)
    .bind(ended_at)
    .bind(&relationship.id)
    .execute(&mut **transaction)
    .await?;
    relationship.active = false;
    relationship.ended_at = Some(ended_at);
    relationship.updated_at = ended_at;
    let payload = serde_json::json!({
        "relationship_id": relationship.id,
        "source_resource_id": relationship.source_resource_id,
        "destination_resource_id": relationship.destination_resource_id,
        "type": relationship.type_key,
        "started_at": relationship.started_at,
        "ended_at": ended_at,
        "reason": reason,
    });
    for endpoint in [
        &relationship.source_resource_id,
        &relationship.destination_resource_id,
    ] {
        append_endpoint_event(
            transaction,
            context,
            endpoint,
            "cmdb.asset.relationship_ended.v1",
            payload.clone(),
        )
        .await?;
    }
    append_audit(
        transaction,
        context,
        "cmdb.relationship.end",
        &relationship.id,
    )
    .await
}

pub async fn start_in(
    transaction: &mut Transaction<'_, Sqlite>,
    input: &StartRelationshipInput,
    context: &MutationContext,
) -> Result<String> {
    let source = input.source_resource_id.trim();
    let destination = input.destination_resource_id.trim();
    let type_key = input.type_key.trim();
    if source.is_empty() || destination.is_empty() || type_key.is_empty() {
        return Err(RelationshipError::Invalid(
            "source, destination, and type are required".into(),
        ));
    }
    if source == destination {
        return Err(RelationshipError::Invalid(
            "a relationship cannot connect an asset to itself".into(),
        ));
    }
    let metadata_json = canonical_metadata(&input.metadata)?;
    require_type(transaction, type_key).await?;
    require_asset(transaction, source, RelationshipError::SourceNotFound).await?;
    require_asset(
        transaction,
        destination,
        RelationshipError::DestinationNotFound,
    )
    .await?;

    let duplicate: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cmdb_relationships \
         WHERE source_resource_id = ? AND destination_resource_id = ? \
           AND type_key = ? AND active = 1",
    )
    .bind(source)
    .bind(destination)
    .bind(type_key)
    .fetch_one(&mut **transaction)
    .await?;
    if duplicate != 0 {
        return Err(RelationshipError::Conflict(
            "an identical active relationship already exists".into(),
        ));
    }

    let now = unix_now();
    if type_key == "installed_in" {
        let mut prior: Vec<RelationshipRecord> = sqlx::query_as(
            "SELECT id, source_resource_id, destination_resource_id, type_key, started_at, \
                    ended_at, active, metadata_json, created_at, updated_at \
             FROM cmdb_relationships \
             WHERE source_resource_id = ? AND type_key = 'installed_in' AND active = 1 \
             ORDER BY created_at, id",
        )
        .bind(source)
        .fetch_all(&mut **transaction)
        .await?;
        for relationship in &mut prior {
            end_record(transaction, context, relationship, now, "moved").await?;
        }
    }

    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO cmdb_relationships \
         (id, source_resource_id, destination_resource_id, type_key, started_at, ended_at, \
          active, metadata_json, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, NULL, 1, ?, ?, ?)",
    )
    .bind(&id)
    .bind(source)
    .bind(destination)
    .bind(type_key)
    .bind(now)
    .bind(&metadata_json)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await
    .map_err(|error| {
        if error
            .as_database_error()
            .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
        {
            RelationshipError::Conflict("an active relationship already exists".into())
        } else {
            RelationshipError::from(error)
        }
    })?;
    let relationship = RelationshipRecord {
        id: id.clone(),
        source_resource_id: source.into(),
        destination_resource_id: destination.into(),
        type_key: type_key.into(),
        started_at: now,
        ended_at: None,
        active: true,
        metadata_json,
        created_at: now,
        updated_at: now,
    };
    append_started(transaction, context, &relationship).await?;
    Ok(id)
}

pub async fn start(
    pool: &SqlitePool,
    input: StartRelationshipInput,
    context: MutationContext,
) -> Result<RelationshipRecord> {
    for attempt in 0..MAX_WRITE_ATTEMPTS {
        let result = async {
            let mut transaction = pool.begin().await?;
            let id = start_in(&mut transaction, &input, &context).await?;
            transaction.commit().await?;
            get(pool, &id).await?.ok_or(RelationshipError::NotFound)
        }
        .await;
        match result {
            Err(error) if is_sqlite_busy(&error) && attempt + 1 < MAX_WRITE_ATTEMPTS => {
                tokio::time::sleep(std::time::Duration::from_millis(
                    5 * u64::try_from(attempt + 1).unwrap_or(1),
                ))
                .await;
            }
            result => return result,
        }
    }
    unreachable!("bounded relationship start attempts always return")
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<RelationshipRecord>> {
    Ok(sqlx::query_as(
        "SELECT id, source_resource_id, destination_resource_id, type_key, started_at, ended_at, \
                active, metadata_json, created_at, updated_at \
         FROM cmdb_relationships WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn end_in(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &str,
    context: &MutationContext,
) -> Result<()> {
    let mut relationship: RelationshipRecord = sqlx::query_as(
        "SELECT id, source_resource_id, destination_resource_id, type_key, started_at, ended_at, \
                active, metadata_json, created_at, updated_at \
         FROM cmdb_relationships WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or(RelationshipError::NotFound)?;
    if relationship.active {
        end_record(
            transaction,
            context,
            &mut relationship,
            unix_now(),
            "explicit",
        )
        .await?;
    }
    Ok(())
}

pub async fn end(
    pool: &SqlitePool,
    id: &str,
    context: MutationContext,
) -> Result<RelationshipRecord> {
    let mut transaction = pool.begin().await?;
    end_in(&mut transaction, id, &context).await?;
    transaction.commit().await?;
    get(pool, id).await?.ok_or(RelationshipError::NotFound)
}

pub async fn list_by_asset(
    pool: &SqlitePool,
    resource_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<RelationshipRecord>> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_assets WHERE resource_id = ?")
        .bind(resource_id)
        .fetch_one(pool)
        .await?;
    if exists == 0 {
        return Err(RelationshipError::SourceNotFound);
    }
    Ok(sqlx::query_as(
        "SELECT id, source_resource_id, destination_resource_id, type_key, started_at, ended_at, \
                active, metadata_json, created_at, updated_at \
         FROM cmdb_relationships \
         WHERE source_resource_id = ? OR destination_resource_id = ? \
         ORDER BY started_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(resource_id)
    .bind(resource_id)
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cmdb::assets::{self, CreateAssetInput},
        operations::contracts::{ActorRef, ActorType},
    };
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let mut transaction = pool.begin().await.unwrap();
        crate::cmdb::catalog::seed(&mut transaction, 1)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        pool
    }

    fn context_with(correlation_id: &str) -> MutationContext {
        MutationContext {
            actor: ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner-id".into()),
                source: Some("test".into()),
            },
            correlation_id: correlation_id.into(),
        }
    }

    fn context() -> MutationContext {
        context_with(&uuid::Uuid::new_v4().to_string())
    }

    async fn asset(pool: &SqlitePool, type_key: &str, name: &str) -> String {
        assets::create_manual(
            pool,
            CreateAssetInput {
                class_key: if type_key == "host" { "sys" } else { "hw" }.into(),
                type_key: type_key.into(),
                name: name.into(),
                friendly_name: None,
                description: None,
                manufacturer: None,
                model: None,
                serial_number: None,
                part_number: None,
                location_id: None,
                metadata: serde_json::json!({}),
                notes: String::new(),
            },
            context(),
        )
        .await
        .unwrap()
        .resource_id
    }

    fn input(source: &str, destination: &str, type_key: &str) -> StartRelationshipInput {
        StartRelationshipInput {
            source_resource_id: source.into(),
            destination_resource_id: destination.into(),
            type_key: type_key.into(),
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn type_endpoint_and_self_edge_validation_has_stable_domain_errors() {
        let pool = pool().await;
        let source = asset(&pool, "hdd", "Disk").await;
        let destination = asset(&pool, "host", "Host").await;
        assert!(matches!(
            start(&pool, input(&source, &destination, "unknown"), context()).await,
            Err(RelationshipError::TypeNotFound)
        ));
        sqlx::query("UPDATE cmdb_relationship_types SET enabled = 0 WHERE key = 'contains'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            start(&pool, input(&source, &destination, "contains"), context()).await,
            Err(RelationshipError::TypeDisabled)
        ));
        assert!(matches!(
            start(
                &pool,
                input("missing", &destination, "connected_to"),
                context()
            )
            .await,
            Err(RelationshipError::SourceNotFound)
        ));
        assert!(matches!(
            start(&pool, input(&source, "missing", "connected_to"), context()).await,
            Err(RelationshipError::DestinationNotFound)
        ));
        assert!(matches!(
            start(&pool, input(&source, &source, "connected_to"), context()).await,
            Err(RelationshipError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn duplicate_active_edges_are_rejected_but_ended_history_allows_a_later_edge() {
        let pool = pool().await;
        let source = asset(&pool, "hdd", "Disk").await;
        let destination = asset(&pool, "host", "Host").await;
        let first = start(
            &pool,
            input(&source, &destination, "attached_to"),
            context(),
        )
        .await
        .unwrap();
        assert!(matches!(
            start(
                &pool,
                input(&source, &destination, "attached_to"),
                context()
            )
            .await,
            Err(RelationshipError::Conflict(_))
        ));
        let ended = end(&pool, &first.id, context()).await.unwrap();
        assert!(!ended.active);
        let event_count_before_retry: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE event_type = 'cmdb.asset.relationship_ended.v1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        end(&pool, &first.id, context()).await.unwrap();
        let event_count_after_retry: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE event_type = 'cmdb.asset.relationship_ended.v1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event_count_before_retry, event_count_after_retry);
        let second = start(
            &pool,
            input(&source, &destination, "attached_to"),
            context(),
        )
        .await
        .unwrap();
        assert_ne!(first.id, second.id);
        let history = list_by_asset(&pool, &destination, 20, 0).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history.iter().filter(|row| row.active).count(), 1);
    }

    #[tokio::test]
    async fn metadata_must_be_bounded_object_and_is_stored_canonically() {
        let pool = pool().await;
        let source = asset(&pool, "hdd", "Disk").await;
        let destination = asset(&pool, "host", "Host").await;
        let mut valid = input(&source, &destination, "installed_in");
        valid.metadata = serde_json::json!({"slot": {"z": 2, "a": 1}, "bay": "B2"});
        let relationship = start(&pool, valid, context()).await.unwrap();
        assert_eq!(
            relationship.metadata_json,
            r#"{"bay":"B2","slot":{"a":1,"z":2}}"#
        );

        let mut scalar = input(&destination, &source, "attached_to");
        scalar.metadata = serde_json::json!("not an object");
        assert!(matches!(
            start(&pool, scalar, context()).await,
            Err(RelationshipError::Invalid(_))
        ));
        let mut oversized = input(&destination, &source, "attached_to");
        oversized.metadata = serde_json::json!({"value": "x".repeat(MAX_METADATA_STRING_LEN + 1)});
        assert!(matches!(
            start(&pool, oversized, context()).await,
            Err(RelationshipError::Invalid(_))
        ));
        let mut deep = serde_json::json!({});
        for _ in 0..=MAX_METADATA_DEPTH {
            deep = serde_json::json!({"nested": deep});
        }
        let mut too_deep = input(&destination, &source, "attached_to");
        too_deep.metadata = deep;
        assert!(matches!(
            start(&pool, too_deep, context()).await,
            Err(RelationshipError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn installed_in_move_preserves_history_and_emits_correlated_events_for_all_endpoints() {
        let pool = pool().await;
        let disk = asset(&pool, "hdd", "Disk").await;
        let host_a = asset(&pool, "host", "Host A").await;
        let host_b = asset(&pool, "host", "Host B").await;
        start(&pool, input(&disk, &host_a, "installed_in"), context())
            .await
            .unwrap();
        let correlation = "move-correlation";
        let moved = start(
            &pool,
            input(&disk, &host_b, "installed_in"),
            context_with(correlation),
        )
        .await
        .unwrap();
        assert!(moved.active);
        let rows = list_by_asset(&pool, &disk, 20, 0).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().filter(|row| row.active).count(), 1);
        assert_eq!(
            rows.iter()
                .find(|row| row.active)
                .unwrap()
                .destination_resource_id,
            host_b
        );
        assert_eq!(list_by_asset(&pool, &host_a, 20, 0).await.unwrap().len(), 1);
        assert_eq!(list_by_asset(&pool, &host_b, 20, 0).await.unwrap().len(), 1);

        let events: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT event_type, resource_id, correlation_id, payload_json FROM events \
             WHERE correlation_id = ? ORDER BY sequence",
        )
        .bind(correlation)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(
            events
                .iter()
                .map(|event| event.0.as_str())
                .collect::<Vec<_>>(),
            vec![
                "cmdb.asset.relationship_ended.v1",
                "cmdb.asset.relationship_ended.v1",
                "cmdb.asset.relationship_started.v1",
                "cmdb.asset.relationship_started.v1",
            ]
        );
        assert_eq!(
            events
                .iter()
                .map(|event| event.1.as_str())
                .collect::<std::collections::HashSet<_>>(),
            [disk.as_str(), host_a.as_str(), host_b.as_str()]
                .into_iter()
                .collect()
        );
        for (_, _, correlation_id, payload) in events {
            assert_eq!(correlation_id, correlation);
            let payload: Value = serde_json::from_str(&payload).unwrap();
            assert_eq!(payload["type"], "installed_in");
            assert_eq!(payload["source_resource_id"], disk);
        }
    }

    #[tokio::test]
    async fn caller_owned_transaction_rollback_removes_relationship_events_and_audit() {
        let pool = pool().await;
        let source = asset(&pool, "hdd", "Disk").await;
        let destination = asset(&pool, "host", "Host").await;
        let correlation = "rollback-relationship";
        let mut transaction = pool.begin().await.unwrap();
        let id = start_in(
            &mut transaction,
            &input(&source, &destination, "installed_in"),
            &context_with(correlation),
        )
        .await
        .unwrap();
        transaction.rollback().await.unwrap();

        let relationships: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_relationships WHERE id = ?")
                .bind(&id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let events: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE correlation_id = ?")
                .bind(correlation)
                .fetch_one(&pool)
                .await
                .unwrap();
        let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE request_id = ?")
            .bind(correlation)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!((relationships, events, audits), (0, 0, 0));
    }

    #[tokio::test]
    async fn concurrent_installed_in_moves_leave_exactly_one_active_host() {
        let db_path = std::env::temp_dir().join(format!(
            "voidtower-cmdb-relationships-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::init_pool(&db_path).await.unwrap();
        let disk = asset(&pool, "hdd", "Concurrent disk").await;
        let host_a = asset(&pool, "host", "Concurrent host A").await;
        let host_b = asset(&pool, "host", "Concurrent host B").await;
        let (first, second) = tokio::join!(
            start(&pool, input(&disk, &host_a, "installed_in"), context()),
            start(&pool, input(&disk, &host_b, "installed_in"), context())
        );
        first.unwrap();
        second.unwrap();
        let active: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_relationships \
             WHERE source_resource_id = ? AND type_key = 'installed_in' AND active = 1",
        )
        .bind(&disk)
        .fetch_one(&pool)
        .await
        .unwrap();
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_relationships \
             WHERE source_resource_id = ? AND type_key = 'installed_in'",
        )
        .bind(&disk)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((active, total), (1, 2));
        pool.close().await;
        for path in [
            db_path.clone(),
            db_path.with_extension("db-shm"),
            db_path.with_extension("db-wal"),
            db_path.with_extension("db.migration.lock"),
        ] {
            let _ = std::fs::remove_file(path);
        }
    }
}
