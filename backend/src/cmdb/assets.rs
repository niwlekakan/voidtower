use super::{contracts::AssetRecord, identifiers};
use crate::{
    audit::{self, PendingAudit},
    operations::{
        canonical_json,
        contracts::ActorRef,
        events::{self, PendingEvent},
        unix_now,
    },
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

const ASSET_SELECT: &str =
    "SELECT a.resource_id, r.kind AS resource_kind, r.display_name, a.asset_id, \
            a.class_key, a.type_key, a.subtype, a.friendly_name, a.description, \
            a.manufacturer, a.model, a.serial_number, a.part_number, a.lifecycle_status, \
            a.discovery_status, a.condition_status, a.location_id, a.first_seen_at, \
            a.last_seen_at, a.metadata_json, a.notes, a.revision, a.created_at, a.updated_at \
     FROM cmdb_assets a JOIN resources r ON r.id = a.resource_id";

#[derive(Debug, Clone)]
pub struct CreateAssetInput {
    pub class_key: String,
    pub type_key: String,
    pub name: String,
    pub friendly_name: Option<String>,
    pub description: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub part_number: Option<String>,
    pub location_id: Option<String>,
    pub metadata: Value,
    pub notes: String,
}

#[derive(Debug, Clone)]
pub struct MutationContext {
    pub actor: ActorRef,
    pub correlation_id: String,
}

fn validate_text(value: &str, field: &str, maximum: usize, required: bool) -> Result<()> {
    let trimmed = value.trim();
    if required && trimmed.is_empty() {
        bail!("{field} is required");
    }
    if trimmed.len() > maximum {
        bail!("{field} exceeds {maximum} characters");
    }
    Ok(())
}

fn validate_optional(value: Option<&str>, field: &str, maximum: usize) -> Result<()> {
    if let Some(value) = value {
        validate_text(value, field, maximum, false)?;
    }
    Ok(())
}

fn actor_type(actor: &ActorRef) -> &'static str {
    actor.actor_type.as_str()
}

async fn resolve_in(
    transaction: &mut Transaction<'_, Sqlite>,
    selector: &str,
) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT a.resource_id FROM cmdb_assets a \
         WHERE a.resource_id = ? OR a.asset_id = ? OR EXISTS (\
             SELECT 1 FROM resource_aliases ra \
             WHERE ra.resource_id = a.resource_id AND ra.namespace = 'cmdb.asset_id' \
               AND ra.scope_key = 'global' AND ra.value = ?\
         ) \
         ORDER BY CASE WHEN a.resource_id = ? THEN 0 WHEN a.asset_id = ? THEN 1 ELSE 2 END \
         LIMIT 1",
    )
    .bind(selector)
    .bind(selector)
    .bind(selector)
    .bind(selector)
    .bind(selector)
    .fetch_optional(&mut **transaction)
    .await?)
}

pub async fn get(pool: &SqlitePool, selector: &str) -> Result<Option<AssetRecord>> {
    validate_text(selector, "asset selector", 128, true)?;
    let sql = format!(
        "{ASSET_SELECT} WHERE a.resource_id = ? OR a.asset_id = ? OR EXISTS (\
             SELECT 1 FROM resource_aliases ra \
             WHERE ra.resource_id = a.resource_id AND ra.namespace = 'cmdb.asset_id' \
               AND ra.scope_key = 'global' AND ra.value = ?\
         ) \
         ORDER BY CASE WHEN a.resource_id = ? THEN 0 WHEN a.asset_id = ? THEN 1 ELSE 2 END \
         LIMIT 1"
    );
    Ok(sqlx::query_as(&sql)
        .bind(selector)
        .bind(selector)
        .bind(selector)
        .bind(selector)
        .bind(selector)
        .fetch_optional(pool)
        .await?)
}

pub async fn list(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<AssetRecord>> {
    let sql = format!("{ASSET_SELECT} ORDER BY a.asset_id LIMIT ? OFFSET ?");
    Ok(sqlx::query_as(&sql)
        .bind(limit.clamp(1, 200))
        .bind(offset.max(0))
        .fetch_all(pool)
        .await?)
}

pub async fn create_manual(
    pool: &SqlitePool,
    input: CreateAssetInput,
    context: MutationContext,
) -> Result<AssetRecord> {
    identifiers::validate_key(&input.class_key, "class")?;
    identifiers::validate_key(&input.type_key, "type")?;
    validate_text(&input.name, "name", 160, true)?;
    validate_optional(input.friendly_name.as_deref(), "friendly_name", 160)?;
    validate_optional(input.description.as_deref(), "description", 2_000)?;
    validate_optional(input.manufacturer.as_deref(), "manufacturer", 160)?;
    validate_optional(input.model.as_deref(), "model", 160)?;
    validate_optional(input.serial_number.as_deref(), "serial_number", 256)?;
    validate_optional(input.part_number.as_deref(), "part_number", 256)?;
    validate_text(&input.notes, "notes", 16_384, false)?;
    let metadata_json = canonical_json::to_canonical_string(&input.metadata)?;
    let now = unix_now();
    let resource_id = uuid::Uuid::new_v4().to_string();
    let mut transaction = pool.begin().await?;
    let asset_id =
        identifiers::allocate(&mut transaction, &input.class_key, &input.type_key, now).await?;

    if let Some(location_id) = input.location_id.as_deref() {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_locations WHERE id = ?")
            .bind(location_id)
            .fetch_one(&mut *transaction)
            .await?;
        if exists != 1 {
            bail!("location does not exist");
        }
    }

    sqlx::query(
        "INSERT INTO resources \
         (id, kind, display_name, node_id, provider, lifecycle_state, revision, created_at, updated_at) \
         VALUES (?, 'cmdb_asset', ?, NULL, NULL, 'active', 0, ?, ?)",
    )
    .bind(&resource_id)
    .bind(input.name.trim())
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO cmdb_assets \
         (resource_id, asset_id, class_key, type_key, friendly_name, description, manufacturer, \
          model, serial_number, part_number, lifecycle_status, discovery_status, condition_status, \
          location_id, metadata_json, notes, revision, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'inventory', 'manual', 'unknown', ?, ?, ?, 0, ?, ?)",
    )
    .bind(&resource_id)
    .bind(&asset_id)
    .bind(&input.class_key)
    .bind(&input.type_key)
    .bind(input.friendly_name.as_deref())
    .bind(input.description.as_deref())
    .bind(input.manufacturer.as_deref())
    .bind(input.model.as_deref())
    .bind(input.serial_number.as_deref())
    .bind(input.part_number.as_deref())
    .bind(input.location_id.as_deref())
    .bind(metadata_json)
    .bind(input.notes.trim())
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO resource_aliases \
         (resource_id, namespace, scope_key, value, created_at, last_seen_at) \
         VALUES (?, 'cmdb.asset_id', 'global', ?, ?, ?)",
    )
    .bind(&resource_id)
    .bind(&asset_id)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await?;

    events::append(
        &mut transaction,
        PendingEvent {
            event_type: "cmdb.asset.created.v1".into(),
            actor: Some(context.actor.clone()),
            resource_id: Some(resource_id.clone()),
            job_id: None,
            approval_id: None,
            correlation_id: context.correlation_id.clone(),
            causation_id: None,
            payload: serde_json::json!({
                "asset_id": &asset_id,
                "class": &input.class_key,
                "type": &input.type_key,
                "source": "manual"
            }),
        },
    )
    .await?;
    let audit_details = format!("asset_id={asset_id}");
    audit::append(
        &mut transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: actor_type(&context.actor),
            action: "cmdb.asset.create",
            resource_type: Some("asset"),
            resource_id: Some(&resource_id),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: Some(&audit_details),
            source: context.actor.source.as_deref(),
        },
    )
    .await?;
    transaction.commit().await?;

    get(pool, &resource_id)
        .await?
        .context("created CMDB asset is missing")
}

pub async fn rename(
    pool: &SqlitePool,
    selector: &str,
    new_asset_id: &str,
    expected_revision: i64,
    context: MutationContext,
) -> Result<AssetRecord> {
    validate_text(new_asset_id, "asset_id", 128, true)?;
    if !new_asset_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("asset_id contains unsupported characters");
    }
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    let resource_id = resolve_in(&mut transaction, selector)
        .await?
        .context("asset not found")?;
    let (old_asset_id, revision): (String, i64) =
        sqlx::query_as("SELECT asset_id, revision FROM cmdb_assets WHERE resource_id = ?")
            .bind(&resource_id)
            .fetch_one(&mut *transaction)
            .await?;
    if revision != expected_revision {
        bail!("asset revision conflict");
    }
    if old_asset_id == new_asset_id {
        transaction.commit().await?;
        return get(pool, &resource_id)
            .await?
            .context("renamed CMDB asset is missing");
    }
    sqlx::query(
        "INSERT INTO resource_aliases \
         (resource_id, namespace, scope_key, value, created_at, last_seen_at) \
         VALUES (?, 'cmdb.asset_id', 'global', ?, ?, ?)",
    )
    .bind(&resource_id)
    .bind(new_asset_id)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE cmdb_assets SET asset_id = ?, revision = revision + 1, updated_at = ? \
         WHERE resource_id = ?",
    )
    .bind(new_asset_id)
    .bind(now)
    .bind(&resource_id)
    .execute(&mut *transaction)
    .await?;
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: "cmdb.asset.identifier_changed.v1".into(),
            actor: Some(context.actor.clone()),
            resource_id: Some(resource_id.clone()),
            job_id: None,
            approval_id: None,
            correlation_id: context.correlation_id.clone(),
            causation_id: None,
            payload: serde_json::json!({"old_asset_id": old_asset_id, "asset_id": new_asset_id}),
        },
    )
    .await?;
    audit::append(
        &mut transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: actor_type(&context.actor),
            action: "cmdb.asset.identifier_change",
            resource_type: Some("asset"),
            resource_id: Some(&resource_id),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: None,
            source: context.actor.source.as_deref(),
        },
    )
    .await?;
    transaction.commit().await?;
    get(pool, &resource_id)
        .await?
        .context("renamed CMDB asset is missing")
}

pub async fn set_retired(
    pool: &SqlitePool,
    selector: &str,
    retired: bool,
    expected_revision: i64,
    context: MutationContext,
) -> Result<AssetRecord> {
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    let resource_id = resolve_in(&mut transaction, selector)
        .await?
        .context("asset not found")?;
    let (current_lifecycle, revision): (String, i64) =
        sqlx::query_as("SELECT lifecycle_status, revision FROM cmdb_assets WHERE resource_id = ?")
            .bind(&resource_id)
            .fetch_one(&mut *transaction)
            .await?;
    if revision != expected_revision {
        bail!("asset revision conflict");
    }
    let lifecycle = if retired { "retired" } else { "inventory" };
    let resource_lifecycle = if retired { "retired" } else { "active" };
    if current_lifecycle == lifecycle {
        transaction.commit().await?;
        return get(pool, &resource_id)
            .await?
            .context("CMDB asset is missing");
    }
    if !retired && current_lifecycle != "retired" {
        bail!("only a retired asset can be restored");
    }
    sqlx::query(
        "UPDATE cmdb_assets SET lifecycle_status = ?, revision = revision + 1, updated_at = ? \
         WHERE resource_id = ?",
    )
    .bind(lifecycle)
    .bind(now)
    .bind(&resource_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE resources SET lifecycle_state = ?, revision = revision + 1, updated_at = ? \
         WHERE id = ?",
    )
    .bind(resource_lifecycle)
    .bind(now)
    .bind(&resource_id)
    .execute(&mut *transaction)
    .await?;
    let event_type = if retired {
        "cmdb.asset.retired.v1"
    } else {
        "cmdb.asset.restored.v1"
    };
    let action = if retired {
        "cmdb.asset.retire"
    } else {
        "cmdb.asset.restore"
    };
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(context.actor.clone()),
            resource_id: Some(resource_id.clone()),
            job_id: None,
            approval_id: None,
            correlation_id: context.correlation_id.clone(),
            causation_id: None,
            payload: serde_json::json!({
                "previous_lifecycle": current_lifecycle,
                "lifecycle": lifecycle
            }),
        },
    )
    .await?;
    audit::append(
        &mut transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: actor_type(&context.actor),
            action,
            resource_type: Some("asset"),
            resource_id: Some(&resource_id),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: None,
            source: context.actor.source.as_deref(),
        },
    )
    .await?;
    transaction.commit().await?;
    get(pool, &resource_id)
        .await?
        .context("updated CMDB asset is missing")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::contracts::ActorType;
    use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

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

    fn context() -> MutationContext {
        MutationContext {
            actor: ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner-id".into()),
                source: Some("test".into()),
            },
            correlation_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    fn input(type_key: &str, name: &str) -> CreateAssetInput {
        CreateAssetInput {
            class_key: "hw".into(),
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
        }
    }

    #[tokio::test]
    async fn manual_asset_uses_resource_uuid_and_commits_event_audit_and_alias() {
        let pool = pool().await;
        let asset = create_manual(&pool, input("hdd", "Shelf disk"), context())
            .await
            .unwrap();
        assert_eq!(asset.asset_id, "VT-hw-hdd-0001");
        assert_eq!(asset.resource_kind, "cmdb_asset");
        assert_eq!(asset.discovery_status, "manual");
        assert_eq!(
            get(&pool, &asset.asset_id).await.unwrap(),
            Some(asset.clone())
        );
        let observations: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_observations WHERE resource_id = ?")
                .bind(&asset.resource_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE resource_id = ? AND event_type = 'cmdb.asset.created.v1'",
        )
        .bind(&asset.resource_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE resource_id = ? AND action = 'cmdb.asset.create'",
        )
        .bind(&asset.resource_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((observations, events, audits), (0, 1, 1));
    }

    #[tokio::test]
    async fn rename_retains_old_identifier_and_rejects_stale_revision() {
        let pool = pool().await;
        let asset = create_manual(&pool, input("ssdm2", "M.2 spare"), context())
            .await
            .unwrap();
        let renamed = rename(
            &pool,
            &asset.resource_id,
            "HOME-hw-ssdm2-0042",
            0,
            context(),
        )
        .await
        .unwrap();
        assert_eq!(renamed.revision, 1);
        assert_eq!(
            get(&pool, "VT-hw-ssdm2-0001")
                .await
                .unwrap()
                .unwrap()
                .resource_id,
            asset.resource_id
        );
        assert!(rename(
            &pool,
            &asset.resource_id,
            "HOME-hw-ssdm2-0043",
            0,
            context()
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn type_counters_are_independent_and_lists_are_bounded() {
        let pool = pool().await;
        create_manual(&pool, input("hdd", "Disk one"), context())
            .await
            .unwrap();
        create_manual(&pool, input("hdd", "Disk two"), context())
            .await
            .unwrap();
        create_manual(&pool, input("ssdm2", "SSD"), context())
            .await
            .unwrap();
        let assets = list(&pool, 2, 0).await.unwrap();
        assert_eq!(assets.len(), 2);
        assert_eq!(
            get(&pool, "VT-hw-hdd-0002")
                .await
                .unwrap()
                .unwrap()
                .display_name,
            "Disk two"
        );
        assert!(get(&pool, "VT-hw-ssdm2-0001").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn retirement_is_soft_and_restore_reactivates_the_same_resource() {
        let pool = pool().await;
        let asset = create_manual(&pool, input("hdd", "Archive disk"), context())
            .await
            .unwrap();
        let retired = set_retired(&pool, &asset.asset_id, true, 0, context())
            .await
            .unwrap();
        assert_eq!(retired.lifecycle_status, "retired");
        let resource_state: String =
            sqlx::query_scalar("SELECT lifecycle_state FROM resources WHERE id = ?")
                .bind(&asset.resource_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(resource_state, "retired");
        let restored = set_retired(&pool, &asset.resource_id, false, 1, context())
            .await
            .unwrap();
        assert_eq!(restored.resource_id, asset.resource_id);
        assert_eq!(restored.lifecycle_status, "inventory");
        assert_eq!(restored.revision, 2);
    }

    #[tokio::test]
    async fn concurrent_manual_creates_receive_distinct_identifiers() {
        let db_path = std::env::temp_dir().join(format!(
            "voidtower-cmdb-identifiers-{}.db",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::init_pool(&db_path).await.unwrap();
        let (first, second) = tokio::join!(
            create_manual(&pool, input("hdd", "Concurrent one"), context()),
            create_manual(&pool, input("hdd", "Concurrent two"), context())
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_ne!(first.resource_id, second.resource_id);
        assert_ne!(first.asset_id, second.asset_id);
        assert_eq!(
            [first.asset_id, second.asset_id]
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            2
        );
        pool.close().await;
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("db-shm"));
        let _ = std::fs::remove_file(db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("db.migration.lock"));
    }
}
