use crate::{
    audit::{self, PendingAudit},
    cmdb::{assets::MutationContext, identifiers},
    operations::unix_now,
};
use serde::Serialize;
use sqlx::{Sqlite, SqlitePool, Transaction};

const MAX_LABEL_LEN: usize = 160;
const MAX_DESCRIPTION_LEN: usize = 2_000;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("invalid catalog record: {0}")]
    Invalid(String),
    #[error("catalog record not found")]
    NotFound,
    #[error("catalog class not found")]
    ClassNotFound,
    #[error("catalog conflict: {0}")]
    Conflict(String),
    #[error("catalog operation failed")]
    Internal(#[source] anyhow::Error),
}

impl From<sqlx::Error> for CatalogError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

pub type Result<T> = std::result::Result<T, CatalogError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct ClassRecord {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub is_builtin: bool,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct TypeRecord {
    pub key: String,
    pub class_key: String,
    pub label: String,
    pub description: Option<String>,
    pub is_builtin: bool,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone)]
pub struct CreateClassInput {
    pub key: String,
    pub label: String,
    pub description: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateClassInput {
    pub label: Option<String>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct CreateTypeInput {
    pub key: String,
    pub class_key: String,
    pub label: String,
    pub description: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateTypeInput {
    pub label: Option<String>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
}

fn text(value: &str, field: &str, maximum: usize, required: bool) -> Result<String> {
    let value = value.trim();
    if required && value.is_empty() {
        return Err(CatalogError::Invalid(format!("{field} is required")));
    }
    if value.chars().count() > maximum {
        return Err(CatalogError::Invalid(format!(
            "{field} exceeds {maximum} characters"
        )));
    }
    Ok(value.to_owned())
}

fn description(value: Option<&str>) -> Result<Option<String>> {
    value
        .map(|value| text(value, "description", MAX_DESCRIPTION_LEN, false))
        .transpose()
        .map(|value| value.filter(|value| !value.is_empty()))
}

fn key(value: &str, field: &str) -> Result<String> {
    let value = value.trim();
    identifiers::validate_key(value, field)
        .map_err(|error| CatalogError::Invalid(error.to_string()))?;
    Ok(value.to_owned())
}

async fn append_audit(
    transaction: &mut Transaction<'_, Sqlite>,
    context: &MutationContext,
    action: &'static str,
    resource_type: &'static str,
    resource_id: &str,
) -> Result<()> {
    audit::append(
        transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: context.actor.actor_type.as_str(),
            action,
            resource_type: Some(resource_type),
            resource_id: Some(resource_id),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: None,
            source: context.actor.source.as_deref(),
        },
    )
    .await
    .map(|_| ())
    .map_err(CatalogError::Internal)
}

fn map_insert(error: sqlx::Error, kind: &str) -> CatalogError {
    if error
        .as_database_error()
        .is_some_and(sqlx::error::DatabaseError::is_unique_violation)
    {
        CatalogError::Conflict(format!("{kind} key already exists"))
    } else {
        CatalogError::Internal(error.into())
    }
}

pub async fn list_classes(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<ClassRecord>> {
    Ok(sqlx::query_as(
        "SELECT key, label, description, is_builtin, enabled, created_at, updated_at \
         FROM cmdb_classes ORDER BY label, key LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn get_class(pool: &SqlitePool, class_key: &str) -> Result<Option<ClassRecord>> {
    Ok(sqlx::query_as(
        "SELECT key, label, description, is_builtin, enabled, created_at, updated_at \
         FROM cmdb_classes WHERE key = ?",
    )
    .bind(class_key)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_class(
    pool: &SqlitePool,
    input: CreateClassInput,
    context: MutationContext,
) -> Result<ClassRecord> {
    let key = key(&input.key, "class")?;
    let label = text(&input.label, "label", MAX_LABEL_LEN, true)?;
    let description = description(input.description.as_deref())?;
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    sqlx::query(
        "INSERT INTO cmdb_classes \
         (key, label, description, is_builtin, enabled, created_at, updated_at) \
         VALUES (?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(&key)
    .bind(label)
    .bind(description)
    .bind(input.enabled)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| map_insert(error, "class"))?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.catalog.class.create",
        "cmdb_class",
        &key,
    )
    .await?;
    transaction.commit().await?;
    get_class(pool, &key).await?.ok_or(CatalogError::NotFound)
}

pub async fn update_class(
    pool: &SqlitePool,
    class_key: &str,
    input: UpdateClassInput,
    context: MutationContext,
) -> Result<ClassRecord> {
    let class_key = key(class_key, "class")?;
    let mut transaction = pool.begin().await?;
    let current: ClassRecord = sqlx::query_as(
        "SELECT key, label, description, is_builtin, enabled, created_at, updated_at \
         FROM cmdb_classes WHERE key = ?",
    )
    .bind(&class_key)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(CatalogError::NotFound)?;
    let label = input
        .label
        .as_deref()
        .map(|value| text(value, "label", MAX_LABEL_LEN, true))
        .transpose()?
        .unwrap_or(current.label);
    let description = match input.description {
        Some(value) => description(value.as_deref())?,
        None => current.description,
    };
    let enabled = input.enabled.unwrap_or(current.enabled);
    if !enabled && current.enabled {
        let references: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_assets WHERE class_key = ? \
             AND lifecycle_status NOT IN ('retired', 'disposed', 'lost')",
        )
        .bind(&class_key)
        .fetch_one(&mut *transaction)
        .await?;
        if references != 0 {
            return Err(CatalogError::Conflict(
                "class is referenced by active assets".into(),
            ));
        }
    }
    sqlx::query(
        "UPDATE cmdb_classes SET label = ?, description = ?, enabled = ?, updated_at = ? \
         WHERE key = ?",
    )
    .bind(label)
    .bind(description)
    .bind(enabled)
    .bind(unix_now())
    .bind(&class_key)
    .execute(&mut *transaction)
    .await?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.catalog.class.update",
        "cmdb_class",
        &class_key,
    )
    .await?;
    transaction.commit().await?;
    get_class(pool, &class_key)
        .await?
        .ok_or(CatalogError::NotFound)
}

pub async fn delete_class(
    pool: &SqlitePool,
    class_key: &str,
    context: MutationContext,
) -> Result<()> {
    let class_key = key(class_key, "class")?;
    let mut transaction = pool.begin().await?;
    let builtin: Option<bool> =
        sqlx::query_scalar("SELECT is_builtin FROM cmdb_classes WHERE key = ?")
            .bind(&class_key)
            .fetch_optional(&mut *transaction)
            .await?;
    let builtin = builtin.ok_or(CatalogError::NotFound)?;
    if builtin {
        return Err(CatalogError::Conflict(
            "built-in classes cannot be deleted".into(),
        ));
    }
    let references: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM cmdb_types WHERE class_key = ?), \
                (SELECT COUNT(*) FROM cmdb_assets WHERE class_key = ?)",
    )
    .bind(&class_key)
    .bind(&class_key)
    .fetch_one(&mut *transaction)
    .await?;
    if references != (0, 0) {
        return Err(CatalogError::Conflict(
            "class is still referenced by types or assets".into(),
        ));
    }
    sqlx::query("DELETE FROM cmdb_classes WHERE key = ?")
        .bind(&class_key)
        .execute(&mut *transaction)
        .await?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.catalog.class.delete",
        "cmdb_class",
        &class_key,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn list_types(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<TypeRecord>> {
    Ok(sqlx::query_as(
        "SELECT key, class_key, label, description, is_builtin, enabled, created_at, updated_at \
         FROM cmdb_types ORDER BY label, key LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn get_type(pool: &SqlitePool, type_key: &str) -> Result<Option<TypeRecord>> {
    Ok(sqlx::query_as(
        "SELECT key, class_key, label, description, is_builtin, enabled, created_at, updated_at \
         FROM cmdb_types WHERE key = ?",
    )
    .bind(type_key)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_type(
    pool: &SqlitePool,
    input: CreateTypeInput,
    context: MutationContext,
) -> Result<TypeRecord> {
    let type_key = key(&input.key, "type")?;
    let class_key = key(&input.class_key, "class")?;
    let label = text(&input.label, "label", MAX_LABEL_LEN, true)?;
    let description = description(input.description.as_deref())?;
    let mut transaction = pool.begin().await?;
    let class_enabled: Option<bool> =
        sqlx::query_scalar("SELECT enabled FROM cmdb_classes WHERE key = ?")
            .bind(&class_key)
            .fetch_optional(&mut *transaction)
            .await?;
    match class_enabled {
        None => return Err(CatalogError::ClassNotFound),
        Some(false) if input.enabled => {
            return Err(CatalogError::Conflict(
                "an enabled type requires an enabled class".into(),
            ));
        }
        Some(_) => {}
    }
    let now = unix_now();
    sqlx::query(
        "INSERT INTO cmdb_types \
         (key, class_key, label, description, is_builtin, enabled, created_at, updated_at) \
         VALUES (?, ?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(&type_key)
    .bind(&class_key)
    .bind(label)
    .bind(description)
    .bind(input.enabled)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| map_insert(error, "type"))?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.catalog.type.create",
        "cmdb_type",
        &type_key,
    )
    .await?;
    transaction.commit().await?;
    get_type(pool, &type_key)
        .await?
        .ok_or(CatalogError::NotFound)
}

pub async fn update_type(
    pool: &SqlitePool,
    type_key: &str,
    input: UpdateTypeInput,
    context: MutationContext,
) -> Result<TypeRecord> {
    let type_key = key(type_key, "type")?;
    let mut transaction = pool.begin().await?;
    let current: TypeRecord = sqlx::query_as(
        "SELECT key, class_key, label, description, is_builtin, enabled, created_at, updated_at \
         FROM cmdb_types WHERE key = ?",
    )
    .bind(&type_key)
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(CatalogError::NotFound)?;
    let label = input
        .label
        .as_deref()
        .map(|value| text(value, "label", MAX_LABEL_LEN, true))
        .transpose()?
        .unwrap_or(current.label);
    let description = match input.description {
        Some(value) => description(value.as_deref())?,
        None => current.description,
    };
    let enabled = input.enabled.unwrap_or(current.enabled);
    if enabled && !current.enabled {
        let class_enabled: bool =
            sqlx::query_scalar("SELECT enabled FROM cmdb_classes WHERE key = ?")
                .bind(&current.class_key)
                .fetch_one(&mut *transaction)
                .await?;
        if !class_enabled {
            return Err(CatalogError::Conflict(
                "an enabled type requires an enabled class".into(),
            ));
        }
    }
    if !enabled && current.enabled {
        let references: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cmdb_assets WHERE type_key = ? \
             AND lifecycle_status NOT IN ('retired', 'disposed', 'lost')",
        )
        .bind(&type_key)
        .fetch_one(&mut *transaction)
        .await?;
        if references != 0 {
            return Err(CatalogError::Conflict(
                "type is referenced by active assets".into(),
            ));
        }
    }
    sqlx::query(
        "UPDATE cmdb_types SET label = ?, description = ?, enabled = ?, updated_at = ? WHERE key = ?",
    )
    .bind(label)
    .bind(description)
    .bind(enabled)
    .bind(unix_now())
    .bind(&type_key)
    .execute(&mut *transaction)
    .await?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.catalog.type.update",
        "cmdb_type",
        &type_key,
    )
    .await?;
    transaction.commit().await?;
    get_type(pool, &type_key)
        .await?
        .ok_or(CatalogError::NotFound)
}

pub async fn delete_type(
    pool: &SqlitePool,
    type_key: &str,
    context: MutationContext,
) -> Result<()> {
    let type_key = key(type_key, "type")?;
    let mut transaction = pool.begin().await?;
    let builtin: Option<bool> =
        sqlx::query_scalar("SELECT is_builtin FROM cmdb_types WHERE key = ?")
            .bind(&type_key)
            .fetch_optional(&mut *transaction)
            .await?;
    let builtin = builtin.ok_or(CatalogError::NotFound)?;
    if builtin {
        return Err(CatalogError::Conflict(
            "built-in types cannot be deleted".into(),
        ));
    }
    let references: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cmdb_assets WHERE type_key = ?")
        .bind(&type_key)
        .fetch_one(&mut *transaction)
        .await?;
    if references != 0 {
        return Err(CatalogError::Conflict(
            "type is still referenced by assets".into(),
        ));
    }
    sqlx::query("DELETE FROM cmdb_types WHERE key = ?")
        .bind(&type_key)
        .execute(&mut *transaction)
        .await?;
    append_audit(
        &mut transaction,
        &context,
        "cmdb.catalog.type.delete",
        "cmdb_type",
        &type_key,
    )
    .await?;
    transaction.commit().await?;
    Ok(())
}
