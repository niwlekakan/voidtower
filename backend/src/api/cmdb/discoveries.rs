use super::{
    records::{public_observation, PublicObservation},
    support::{self, Access},
};
use crate::{
    cmdb::observations::{self as service, ObservationError},
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Bytes,
    extract::{rejection::BytesRejection, Path, State},
    http::Uri,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
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

#[derive(Deserialize)]
pub struct DecisionRequest {
    expected_fingerprint: String,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Deserialize)]
pub struct LinkRequest {
    expected_fingerprint: String,
    asset_selector: String,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    expected_fingerprint: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    class_key: Option<String>,
    #[serde(default)]
    type_key: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

fn error(error: ObservationError) -> AppError {
    match error {
        ObservationError::Invalid(message) => AppError::BadRequest(message),
        ObservationError::SourceNotFound
        | ObservationError::AssetNotFound
        | ObservationError::ObservationNotFound => AppError::NotFound,
        ObservationError::NodeNotTrusted => AppError::Unauthorized,
        ObservationError::Processing | ObservationError::Conflict(_) => {
            AppError::Conflict("discovery request conflicts with current evidence".into())
        }
        ObservationError::Internal(error) => {
            tracing::error!("CMDB discovery operation failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB discovery operation failed"))
        }
    }
}

pub async fn list(State(state): State<AppState>, jar: CookieJar, uri: Uri) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let discoveries = service::list_discoveries(&state.db, query.limit, query.offset)
        .await
        .map_err(error)?
        .into_iter()
        .map(public_observation)
        .collect::<Result<Vec<_>>>()?;
    Ok(Json(json!({
        "discoveries": discoveries,
        "limit": query.limit,
        "offset": query.offset
    })))
}

pub async fn ignore(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<PublicObservation>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: DecisionRequest = support::parse_json(body)?;
    let record = service::ignore_discovery(
        &state.db,
        &id,
        &request.expected_fingerprint,
        request.notes.as_deref(),
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(public_observation(record)?))
}

pub async fn link(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<PublicObservation>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: LinkRequest = support::parse_json(body)?;
    let target = support::resolve_asset(&state, &request.asset_selector).await?;
    let record = service::link_discovery(
        &state.db,
        &id,
        &request.expected_fingerprint,
        &target.resource_id,
        request.notes.as_deref(),
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(public_observation(record)?))
}

pub async fn register(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<PublicObservation>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: RegisterRequest = support::parse_json(body)?;
    let record = service::register_discovery(
        &state.db,
        &id,
        &request.expected_fingerprint,
        service::RegisterDiscoveryInput {
            name: request.name,
            class_key: request.class_key,
            type_key: request.type_key,
            subtype: request.subtype,
            manufacturer: request.manufacturer,
            model: request.model,
            serial_number: request.serial_number,
        },
        request.notes.as_deref(),
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(public_observation(record)?))
}
