use super::support::{self, Access};
use crate::{
    cmdb::observations::{self, ObservationError, ObservationRecord},
    error::{AppError, Result},
    operations::events,
    AppState,
};
use axum::{
    extract::{Path, State},
    http::Uri,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    100
}

#[derive(Debug, Serialize)]
pub struct PublicObservation {
    pub id: String,
    pub resource_id: Option<String>,
    pub source_resource_id: String,
    pub provider: String,
    pub entity_key: String,
    pub entity_type: String,
    pub schema_version: i64,
    pub identities: Value,
    pub attributes: Value,
    pub runtime: Value,
    pub health: Value,
    pub provider_observed_at: Option<i64>,
    pub received_at: i64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub state: String,
    pub fingerprint: String,
}

pub fn public_observation(record: ObservationRecord) -> Result<PublicObservation> {
    let parse = |field: &str, value: &str| {
        serde_json::from_str(value).map_err(|error| {
            tracing::error!("invalid persisted CMDB observation {field}: {error}");
            AppError::Internal(anyhow::anyhow!("CMDB observation encoding failed"))
        })
    };
    Ok(PublicObservation {
        id: record.id,
        resource_id: record.resource_id,
        source_resource_id: record.source_resource_id,
        provider: record.provider,
        entity_key: record.entity_key,
        entity_type: record.entity_type,
        schema_version: record.schema_version,
        identities: parse("identity", &record.identity_json)?,
        attributes: parse("attributes", &record.attributes_json)?,
        runtime: parse("runtime", &record.runtime_json)?,
        health: parse("health", &record.health_json)?,
        provider_observed_at: record.provider_observed_at,
        received_at: record.received_at,
        first_seen_at: record.first_seen_at,
        last_seen_at: record.last_seen_at,
        state: record.state,
        fingerprint: record.fingerprint,
    })
}

fn observation_error(error: ObservationError) -> AppError {
    match error {
        ObservationError::Invalid(message) => AppError::BadRequest(message),
        ObservationError::SourceNotFound
        | ObservationError::AssetNotFound
        | ObservationError::ObservationNotFound => AppError::NotFound,
        ObservationError::NodeNotTrusted => AppError::Unauthorized,
        ObservationError::Processing | ObservationError::Conflict(_) => {
            AppError::Conflict("observation request conflicts with current state".into())
        }
        ObservationError::Internal(error) => {
            tracing::error!("CMDB observation read failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB observation read failed"))
        }
    }
}

pub async fn history(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    uri: Uri,
) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let asset = support::resolve_asset(&state, &selector).await?;
    let history =
        events::list_for_resource(&state.db, &asset.resource_id, query.limit, query.offset)
            .await
            .map_err(|error| {
                tracing::error!("CMDB history read failed: {error:#}");
                AppError::Internal(anyhow::anyhow!("CMDB history read failed"))
            })?;
    Ok(Json(json!({
        "resource_id": asset.resource_id,
        "events": history,
        "limit": query.limit,
        "offset": query.offset
    })))
}

pub async fn observations(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    uri: Uri,
) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let asset = support::resolve_asset(&state, &selector).await?;
    let records =
        observations::list_by_asset(&state.db, &asset.resource_id, query.limit, query.offset)
            .await
            .map_err(observation_error)?;
    let records = records
        .into_iter()
        .map(public_observation)
        .collect::<Result<Vec<_>>>()?;
    Ok(Json(json!({
        "resource_id": asset.resource_id,
        "observations": records,
        "limit": query.limit,
        "offset": query.offset
    })))
}
