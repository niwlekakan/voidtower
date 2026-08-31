use crate::{
    audit::{self, PendingAudit},
    cmdb::{assets::MutationContext, contracts::IdentifierSettings, identifiers},
    operations::unix_now,
};
use sqlx::SqlitePool;

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("invalid CMDB settings: {0}")]
    Invalid(String),
    #[error("CMDB settings not found")]
    NotFound,
    #[error("CMDB settings operation failed")]
    Internal(#[source] anyhow::Error),
}

impl From<sqlx::Error> for SettingsError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

pub type Result<T> = std::result::Result<T, SettingsError>;

#[derive(Debug, Clone, Default)]
pub struct UpdateSettingsInput {
    pub prefix: Option<String>,
    pub template: Option<String>,
    pub separator: Option<String>,
    pub number_width: Option<i64>,
    pub starting_number: Option<i64>,
    pub counter_scope: Option<String>,
    pub letter_case: Option<String>,
    pub discovery_policy: Option<String>,
}

pub async fn get(pool: &SqlitePool) -> Result<IdentifierSettings> {
    sqlx::query_as(
        "SELECT prefix, template, separator, number_width, starting_number, counter_scope, \
                letter_case, discovery_policy, updated_at \
         FROM cmdb_identifier_settings WHERE id = 'default'",
    )
    .fetch_optional(pool)
    .await?
    .ok_or(SettingsError::NotFound)
}

pub async fn update(
    pool: &SqlitePool,
    input: UpdateSettingsInput,
    context: MutationContext,
) -> Result<IdentifierSettings> {
    let mut transaction = pool.begin().await?;
    let current: IdentifierSettings = sqlx::query_as(
        "SELECT prefix, template, separator, number_width, starting_number, counter_scope, \
                letter_case, discovery_policy, updated_at \
         FROM cmdb_identifier_settings WHERE id = 'default'",
    )
    .fetch_optional(&mut *transaction)
    .await?
    .ok_or(SettingsError::NotFound)?;
    let updated_at = unix_now().max(current.updated_at.saturating_add(1));
    let updated = IdentifierSettings {
        prefix: input.prefix.unwrap_or(current.prefix),
        template: input.template.unwrap_or(current.template),
        separator: input.separator.unwrap_or(current.separator),
        number_width: input.number_width.unwrap_or(current.number_width),
        starting_number: input.starting_number.unwrap_or(current.starting_number),
        counter_scope: input.counter_scope.unwrap_or(current.counter_scope),
        letter_case: input.letter_case.unwrap_or(current.letter_case),
        discovery_policy: input.discovery_policy.unwrap_or(current.discovery_policy),
        updated_at,
    };
    identifiers::validate_settings(&updated)
        .map_err(|error| SettingsError::Invalid(error.to_string()))?;
    sqlx::query(
        "UPDATE cmdb_identifier_settings SET prefix = ?, template = ?, separator = ?, \
         number_width = ?, starting_number = ?, counter_scope = ?, letter_case = ?, \
         discovery_policy = ?, updated_at = ? WHERE id = 'default'",
    )
    .bind(&updated.prefix)
    .bind(&updated.template)
    .bind(&updated.separator)
    .bind(updated.number_width)
    .bind(updated.starting_number)
    .bind(&updated.counter_scope)
    .bind(&updated.letter_case)
    .bind(&updated.discovery_policy)
    .bind(updated.updated_at)
    .execute(&mut *transaction)
    .await?;
    audit::append(
        &mut transaction,
        PendingAudit {
            user_id: context.actor.id.as_deref(),
            actor_type: context.actor.actor_type.as_str(),
            action: "cmdb.settings.update",
            resource_type: Some("cmdb_settings"),
            resource_id: Some("default"),
            outcome: "success",
            ip_address: None,
            request_id: Some(&context.correlation_id),
            details: None,
            source: context.actor.source.as_deref(),
        },
    )
    .await
    .map_err(SettingsError::Internal)?;
    transaction.commit().await?;
    get(pool).await
}
