use super::support::{self, Access};
use crate::{
    cmdb::locations::{self as service, LocationError},
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Bytes,
    extract::{rejection::BytesRejection, Path, State},
    http::{StatusCode, Uri},
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
pub struct CreateRequest {
    #[serde(default)]
    parent_id: Option<String>,
    name: String,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    #[serde(default)]
    parent_id: support::Nullable<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: support::Nullable<String>,
}

fn error(error: LocationError) -> AppError {
    match error {
        LocationError::Invalid(message) => AppError::BadRequest(message),
        LocationError::NotFound | LocationError::ParentNotFound => AppError::NotFound,
        LocationError::Conflict(_) => {
            AppError::Conflict("location request conflicts with current state".into())
        }
        LocationError::Internal(error) => {
            tracing::error!("CMDB location operation failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB location operation failed"))
        }
    }
}

pub async fn list(State(state): State<AppState>, jar: CookieJar, uri: Uri) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let locations = service::list(&state.db, query.limit, query.offset)
        .await
        .map_err(error)?;
    Ok(Json(json!({
        "locations": locations,
        "limit": query.limit,
        "offset": query.offset
    })))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<service::LocationRecord>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: CreateRequest = support::parse_json(body)?;
    let location = service::create(
        &state.db,
        service::CreateLocationInput {
            parent_id: request.parent_id,
            name: request.name,
            description: request.description,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(location))
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<service::LocationRecord>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: UpdateRequest = support::parse_json(body)?;
    let current = service::get(&state.db, &id)
        .await
        .map_err(error)?
        .ok_or(AppError::NotFound)?;
    let location = service::update(
        &state.db,
        &id,
        service::UpdateLocationInput {
            parent_id: request.parent_id.0.unwrap_or(current.parent_id),
            name: request.name.unwrap_or(current.name),
            description: request.description.0.unwrap_or(current.description),
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(location))
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<StatusCode> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    service::delete(&state.db, &id, support::mutation_context(&user))
        .await
        .map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}
