use super::support::{self, Access};
use crate::{
    cmdb::settings::{self as service, SettingsError},
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Bytes,
    extract::{rejection::BytesRejection, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    separator: Option<String>,
    #[serde(default)]
    number_width: Option<i64>,
    #[serde(default)]
    starting_number: Option<i64>,
    #[serde(default)]
    counter_scope: Option<String>,
    #[serde(default)]
    letter_case: Option<String>,
    #[serde(default)]
    discovery_policy: Option<String>,
}

fn error(error: SettingsError) -> AppError {
    match error {
        SettingsError::Invalid(message) => AppError::BadRequest(message),
        SettingsError::NotFound => AppError::NotFound,
        SettingsError::Internal(error) => {
            tracing::error!("CMDB settings operation failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB settings operation failed"))
        }
    }
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<crate::cmdb::contracts::IdentifierSettings>> {
    support::require_user(&state, &jar, Access::Read).await?;
    Ok(Json(service::get(&state.db).await.map_err(error)?))
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<crate::cmdb::contracts::IdentifierSettings>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: UpdateRequest = support::parse_json(body)?;
    let settings = service::update(
        &state.db,
        service::UpdateSettingsInput {
            prefix: request.prefix,
            template: request.template,
            separator: request.separator,
            number_width: request.number_width,
            starting_number: request.starting_number,
            counter_scope: request.counter_scope,
            letter_case: request.letter_case,
            discovery_policy: request.discovery_policy,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(settings))
}
