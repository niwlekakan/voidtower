use crate::{
    audit::{self, PendingAudit},
    cmdb::assets::MutationContext,
    operations::unix_now,
};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;

pub const MAX_LOCATION_DEPTH: i64 = 16;
const MAX_NAME_LEN: usize = 160;
const MAX_DESCRIPTION_LEN: usize = 2_000;

#[derive(Debug, thiserror::Error)]
pub enum LocationError {
    #[error("invalid location: {0}")]
    Invalid(String),
    #[error("location not found")]
    NotFound,
    #[error("parent location not found")]
    ParentNotFound,
    #[error("location conflict: {0}")]
    Conflict(String),
    #[error("location operation failed")]
    Internal(#[source] anyhow::Error),
}

impl From<sqlx::Error> for LocationError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl From<anyhow::Error> for LocationError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

pub type Result<T> = std::result::Result<T, LocationError>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, sqlx::FromRow)]
pub struct LocationRecord {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[sqlx(default)]
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct CreateLocationInput {
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateLocationInput {
    pub parent_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
}

fn normalize_required(value: &str, field: &str, maximum: usize) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(LocationError::Invalid(format!("{field} is required")));
    }
    if value.chars().count() > maximum {
        return Err(LocationError::Invalid(format!(
            "{field} exceeds {maximum} characters"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_optional(value: Option<&str>, field: &str, maximum: usize) -> Result<Option<String>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            if value.chars().count() > maximum {
                Err(LocationError::Invalid(format!(
                    "{field} exceeds {maximum} characters"
                )))
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()
}

fn actor_type(context: &MutationContext) -> &'static str {
    context.actor.actor_type.as_str()
}

async fn ensure_parent(
    transaction: &mut Transaction<'_, Sqlite>,
    parent_id: Option<&str>,
) -> Result<i64> {
    let Some(parent_id) = parent_id else {
        return Ok(0);
    };
    let depth: Option<i64> = sqlx::query_scalar(
        "WITH RECURSIVE ancestors(id, parent_id, depth) AS (\
             SELECT id, parent_id, 1 FROM cmdb_locations WHERE id = ? \
             UNION ALL \
             SELECT parent.id, parent.parent_id, ancestors.depth + 1 \
             FROM cmdb_locations parent JOIN ancestors ON parent.id = ancestors.parent_id\
         ) SELECT MAX(depth) FROM ancestors",
    )
    .bind(parent_id)
    .fetch_one(&mut **transaction)
    .await?;
    depth.ok_or(LocationError::ParentNotFound)
}

async fn ensure_name_available(
    transaction: &mut Transaction<'_, Sqlite>,
    parent_id: Option<&str>,
    name: &str,
    excluding_id: Option<&str>,
) -> Result<()> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cmdb_locations \
         WHERE ((parent_id = ?) OR (parent_id IS NULL AND ? IS NULL)) AND name = ? \
           AND (? IS NULL OR id != ?)",
    )
    .bind(parent_id)
    .bind(parent_id)
    .bind(name)
    .bind(excluding_id)
    .bind(excluding_id)
    .fetch_one(&mut **transaction)
    .await?;
    if exists != 0 {
        return Err(LocationError::Conflict(
            "a location with that name already exists under the same parent".into(),
        ));
    }
    Ok(())
}

async fn append_audit(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    action: &'static str,
    location_id: &str,
) -> Result<()> {
    audit::append(
        transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: actor_type(context),
            action,
            resource_type: Some("location"),
            resource_id: Some(location_id),
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

pub async fn create_in(
    transaction: &mut Transaction<'_, Sqlite>,
    input: &CreateLocationInput,
    context: &MutationContext,
) -> Result<String> {
    let name = normalize_required(&input.name, "name", MAX_NAME_LEN)?;
    let description = normalize_optional(
        input.description.as_deref(),
        "description",
        MAX_DESCRIPTION_LEN,
    )?;
    let parent_id = input
        .parent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let parent_depth = ensure_parent(transaction, parent_id).await?;
    if parent_depth + 1 > MAX_LOCATION_DEPTH {
        return Err(LocationError::Invalid(format!(
            "location hierarchy exceeds maximum depth {MAX_LOCATION_DEPTH}"
        )));
    }
    ensure_name_available(transaction, parent_id, &name, None).await?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    sqlx::query(
        "INSERT INTO cmdb_locations (id, parent_id, name, description, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(parent_id)
    .bind(name)
    .bind(description)
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    append_audit(transaction, context, "cmdb.location.create", &id).await?;
    Ok(id)
}

pub async fn create(
    pool: &SqlitePool,
    input: CreateLocationInput,
    context: MutationContext,
) -> Result<LocationRecord> {
    let mut transaction = pool.begin().await?;
    let id = create_in(&mut transaction, &input, &context).await?;
    transaction.commit().await?;
    get(pool, &id).await?.ok_or(LocationError::NotFound)
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<LocationRecord>> {
    Ok(sqlx::query_as(
        "WITH RECURSIVE tree \
             (id, parent_id, name, description, created_at, updated_at, path) AS (\
                 SELECT id, parent_id, name, description, created_at, updated_at, name \
                 FROM cmdb_locations WHERE parent_id IS NULL \
                 UNION ALL \
                 SELECT child.id, child.parent_id, child.name, child.description, \
                        child.created_at, child.updated_at, tree.path || ' / ' || child.name \
                 FROM cmdb_locations child JOIN tree ON child.parent_id = tree.id\
             ) \
         SELECT id, parent_id, name, description, created_at, updated_at, path \
         FROM tree WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<LocationRecord>> {
    Ok(sqlx::query_as(
        "WITH RECURSIVE tree \
             (id, parent_id, name, description, created_at, updated_at, path) AS (\
                 SELECT id, parent_id, name, description, created_at, updated_at, name \
                 FROM cmdb_locations WHERE parent_id IS NULL \
                 UNION ALL \
                 SELECT child.id, child.parent_id, child.name, child.description, \
                        child.created_at, child.updated_at, tree.path || ' / ' || child.name \
                 FROM cmdb_locations child JOIN tree ON child.parent_id = tree.id\
             ) \
         SELECT id, parent_id, name, description, created_at, updated_at, path \
         FROM tree ORDER BY path, id LIMIT ? OFFSET ?",
    )
    .bind(limit.clamp(1, 200))
    .bind(offset.max(0))
    .fetch_all(pool)
    .await?)
}

pub async fn update_in(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &str,
    input: &UpdateLocationInput,
    context: &MutationContext,
) -> Result<()> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_locations WHERE id = ?")
        .bind(id)
        .fetch_one(&mut **transaction)
        .await?;
    if exists == 0 {
        return Err(LocationError::NotFound);
    }
    let name = normalize_required(&input.name, "name", MAX_NAME_LEN)?;
    let description = normalize_optional(
        input.description.as_deref(),
        "description",
        MAX_DESCRIPTION_LEN,
    )?;
    let parent_id = input
        .parent_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if parent_id == Some(id) {
        return Err(LocationError::Invalid(
            "a location cannot be its own parent".into(),
        ));
    }
    let parent_depth = ensure_parent(transaction, parent_id).await?;
    if let Some(parent_id) = parent_id {
        let parent_is_descendant: i64 = sqlx::query_scalar(
            "WITH RECURSIVE descendants(id) AS (\
                 SELECT id FROM cmdb_locations WHERE parent_id = ? \
                 UNION ALL \
                 SELECT child.id FROM cmdb_locations child \
                 JOIN descendants ON child.parent_id = descendants.id\
             ) SELECT COUNT(*) FROM descendants WHERE id = ?",
        )
        .bind(id)
        .bind(parent_id)
        .fetch_one(&mut **transaction)
        .await?;
        if parent_is_descendant != 0 {
            return Err(LocationError::Invalid(
                "moving the location would create a hierarchy cycle".into(),
            ));
        }
    }
    let subtree_height: i64 = sqlx::query_scalar(
        "WITH RECURSIVE descendants(id, depth) AS (\
             SELECT id, 1 FROM cmdb_locations WHERE id = ? \
             UNION ALL \
             SELECT child.id, descendants.depth + 1 FROM cmdb_locations child \
             JOIN descendants ON child.parent_id = descendants.id\
         ) SELECT MAX(depth) FROM descendants",
    )
    .bind(id)
    .fetch_one(&mut **transaction)
    .await?;
    if parent_depth + subtree_height > MAX_LOCATION_DEPTH {
        return Err(LocationError::Invalid(format!(
            "location hierarchy exceeds maximum depth {MAX_LOCATION_DEPTH}"
        )));
    }
    ensure_name_available(transaction, parent_id, &name, Some(id)).await?;
    sqlx::query(
        "UPDATE cmdb_locations SET parent_id = ?, name = ?, description = ?, updated_at = ? \
         WHERE id = ?",
    )
    .bind(parent_id)
    .bind(name)
    .bind(description)
    .bind(unix_now())
    .bind(id)
    .execute(&mut **transaction)
    .await?;
    append_audit(transaction, context, "cmdb.location.update", id).await
}

pub async fn update(
    pool: &SqlitePool,
    id: &str,
    input: UpdateLocationInput,
    context: MutationContext,
) -> Result<LocationRecord> {
    let mut transaction = pool.begin().await?;
    update_in(&mut transaction, id, &input, &context).await?;
    transaction.commit().await?;
    get(pool, id).await?.ok_or(LocationError::NotFound)
}

pub async fn delete_in(
    transaction: &mut Transaction<'_, Sqlite>,
    id: &str,
    context: &MutationContext,
) -> Result<()> {
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_locations WHERE id = ?")
        .bind(id)
        .fetch_one(&mut **transaction)
        .await?;
    if exists == 0 {
        return Err(LocationError::NotFound);
    }
    let references: HashMap<String, i64> = sqlx::query_as::<_, (String, i64)>(
        "SELECT 'children', COUNT(*) FROM cmdb_locations WHERE parent_id = ? \
         UNION ALL SELECT 'assets', COUNT(*) FROM cmdb_assets WHERE location_id = ?",
    )
    .bind(id)
    .bind(id)
    .fetch_all(&mut **transaction)
    .await?
    .into_iter()
    .collect();
    if references.get("children").copied().unwrap_or(0) != 0 {
        return Err(LocationError::Conflict(
            "location still has child locations".into(),
        ));
    }
    if references.get("assets").copied().unwrap_or(0) != 0 {
        return Err(LocationError::Conflict(
            "location is still referenced by assets".into(),
        ));
    }
    sqlx::query("DELETE FROM cmdb_locations WHERE id = ?")
        .bind(id)
        .execute(&mut **transaction)
        .await?;
    append_audit(transaction, context, "cmdb.location.delete", id).await
}

pub async fn delete(pool: &SqlitePool, id: &str, context: MutationContext) -> Result<()> {
    let mut transaction = pool.begin().await?;
    delete_in(&mut transaction, id, &context).await?;
    transaction.commit().await?;
    Ok(())
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

    fn input(parent_id: Option<&str>, name: &str) -> CreateLocationInput {
        CreateLocationInput {
            parent_id: parent_id.map(str::to_owned),
            name: name.into(),
            description: None,
        }
    }

    async fn make(pool: &SqlitePool, parent_id: Option<&str>, name: &str) -> LocationRecord {
        create(pool, input(parent_id, name), context())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn root_and_nested_locations_have_derived_paths_and_bounded_audits() {
        let pool = pool().await;
        let root = create(
            &pool,
            CreateLocationInput {
                parent_id: None,
                name: "  Home  ".into(),
                description: Some("  Main site  ".into()),
            },
            context(),
        )
        .await
        .unwrap();
        let room = make(&pool, Some(&root.id), "Office").await;
        let shelf = make(&pool, Some(&room.id), "Shelf A").await;

        assert_eq!(root.path, "Home");
        assert_eq!(root.description.as_deref(), Some("Main site"));
        assert_eq!(shelf.path, "Home / Office / Shelf A");
        let listed = list(&pool, 200, 0).await.unwrap();
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[2].path, "Home / Office / Shelf A");
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'cmdb.location.create'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE event_type LIKE 'cmdb.location.%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((audits, events), (3, 0));
    }

    #[tokio::test]
    async fn duplicate_names_missing_parents_self_parenting_and_cycles_are_rejected() {
        let pool = pool().await;
        let first_root = make(&pool, None, "Site").await;
        assert!(matches!(
            create(&pool, input(None, "Site"), context()).await,
            Err(LocationError::Conflict(_))
        ));
        let other_root = make(&pool, None, "Other").await;
        let first_child = make(&pool, Some(&first_root.id), "Rack").await;
        make(&pool, Some(&other_root.id), "Rack").await;
        assert!(matches!(
            create(&pool, input(Some(&first_root.id), "Rack"), context()).await,
            Err(LocationError::Conflict(_))
        ));
        assert!(matches!(
            create(&pool, input(Some("missing"), "Nowhere"), context()).await,
            Err(LocationError::ParentNotFound)
        ));
        assert!(matches!(
            update(
                &pool,
                &first_root.id,
                UpdateLocationInput {
                    parent_id: Some(first_root.id.clone()),
                    name: first_root.name.clone(),
                    description: None,
                },
                context(),
            )
            .await,
            Err(LocationError::Invalid(_))
        ));
        let grandchild = make(&pool, Some(&first_child.id), "Bay").await;
        assert!(matches!(
            update(
                &pool,
                &first_root.id,
                UpdateLocationInput {
                    parent_id: Some(grandchild.id),
                    name: first_root.name,
                    description: None,
                },
                context(),
            )
            .await,
            Err(LocationError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn maximum_depth_is_enforced_on_create_and_move() {
        let pool = pool().await;
        let root = make(&pool, None, "Depth 1").await;
        let mut parent = root.id;
        for depth in 2..=MAX_LOCATION_DEPTH {
            parent = make(&pool, Some(&parent), &format!("Depth {depth}"))
                .await
                .id;
        }
        assert!(matches!(
            create(&pool, input(Some(&parent), "Too deep"), context()).await,
            Err(LocationError::Invalid(_))
        ));

        let subtree = make(&pool, None, "Subtree").await;
        make(&pool, Some(&subtree.id), "Subtree child").await;
        assert!(matches!(
            update(
                &pool,
                &subtree.id,
                UpdateLocationInput {
                    parent_id: Some(parent),
                    name: subtree.name,
                    description: None,
                },
                context(),
            )
            .await,
            Err(LocationError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn deletion_is_blocked_by_children_and_assets_but_allows_an_unreferenced_leaf() {
        let pool = pool().await;
        let root = make(&pool, None, "Root").await;
        let child = make(&pool, Some(&root.id), "Child").await;
        assert!(matches!(
            delete(&pool, &root.id, context()).await,
            Err(LocationError::Conflict(_))
        ));

        assets::create_manual(
            &pool,
            CreateAssetInput {
                class_key: "hw".into(),
                type_key: "hdd".into(),
                name: "Located disk".into(),
                friendly_name: None,
                description: None,
                manufacturer: None,
                model: None,
                serial_number: None,
                part_number: None,
                location_id: Some(child.id.clone()),
                metadata: serde_json::json!({}),
                notes: String::new(),
            },
            context(),
        )
        .await
        .unwrap();
        assert!(matches!(
            delete(&pool, &child.id, context()).await,
            Err(LocationError::Conflict(_))
        ));
        let leaf = make(&pool, Some(&root.id), "Disposable").await;
        delete(&pool, &leaf.id, context()).await.unwrap();
        assert!(get(&pool, &leaf.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn caller_owned_transaction_rollback_removes_location_and_audit() {
        let pool = pool().await;
        let mut transaction = pool.begin().await.unwrap();
        let id = create_in(&mut transaction, &input(None, "Rolled back"), &context())
            .await
            .unwrap();
        transaction.rollback().await.unwrap();

        let locations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_locations WHERE id = ?")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let audits: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE resource_id = ? AND action = 'cmdb.location.create'",
        )
        .bind(&id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((locations, audits), (0, 0));
    }
}
