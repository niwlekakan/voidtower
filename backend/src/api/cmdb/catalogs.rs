use super::support::{self, Access};
use crate::{
    cmdb::catalog_admin::{self as service, CatalogError},
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

fn error(error: CatalogError) -> AppError {
    match error {
        CatalogError::Invalid(message) => AppError::BadRequest(message),
        CatalogError::NotFound | CatalogError::ClassNotFound => AppError::NotFound,
        CatalogError::Conflict(_) => {
            AppError::Conflict("catalog request conflicts with current state".into())
        }
        CatalogError::Internal(error) => {
            tracing::error!("CMDB catalog operation failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB catalog operation failed"))
        }
    }
}

#[derive(Deserialize)]
pub struct CreateClassRequest {
    key: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "enabled")]
    enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateClassRequest {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: support::Nullable<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateTypeRequest {
    key: String,
    class_key: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default = "enabled")]
    enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateTypeRequest {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: support::Nullable<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

fn enabled() -> bool {
    true
}

type BodyResult = std::result::Result<Bytes, BytesRejection>;

pub async fn list_classes(
    State(state): State<AppState>,
    jar: CookieJar,
    uri: Uri,
) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let classes = service::list_classes(&state.db, query.limit, query.offset)
        .await
        .map_err(error)?;
    Ok(Json(json!({
        "classes": classes,
        "limit": query.limit,
        "offset": query.offset
    })))
}

pub async fn create_class(
    State(state): State<AppState>,
    jar: CookieJar,
    body: BodyResult,
) -> Result<Json<service::ClassRecord>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: CreateClassRequest = support::parse_json(body)?;
    let record = service::create_class(
        &state.db,
        service::CreateClassInput {
            key: request.key,
            label: request.label,
            description: request.description,
            enabled: request.enabled,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(record))
}

pub async fn update_class(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(key): Path<String>,
    body: BodyResult,
) -> Result<Json<service::ClassRecord>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: UpdateClassRequest = support::parse_json(body)?;
    let record = service::update_class(
        &state.db,
        &key,
        service::UpdateClassInput {
            label: request.label,
            description: request.description.0,
            enabled: request.enabled,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(record))
}

pub async fn delete_class(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(key): Path<String>,
) -> Result<StatusCode> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    service::delete_class(&state.db, &key, support::mutation_context(&user))
        .await
        .map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_types(
    State(state): State<AppState>,
    jar: CookieJar,
    uri: Uri,
) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let types = service::list_types(&state.db, query.limit, query.offset)
        .await
        .map_err(error)?;
    Ok(Json(json!({
        "types": types,
        "limit": query.limit,
        "offset": query.offset
    })))
}

pub async fn create_type(
    State(state): State<AppState>,
    jar: CookieJar,
    body: BodyResult,
) -> Result<Json<service::TypeRecord>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: CreateTypeRequest = support::parse_json(body)?;
    let record = service::create_type(
        &state.db,
        service::CreateTypeInput {
            key: request.key,
            class_key: request.class_key,
            label: request.label,
            description: request.description,
            enabled: request.enabled,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(record))
}

pub async fn update_type(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(key): Path<String>,
    body: BodyResult,
) -> Result<Json<service::TypeRecord>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: UpdateTypeRequest = support::parse_json(body)?;
    let record = service::update_type(
        &state.db,
        &key,
        service::UpdateTypeInput {
            label: request.label,
            description: request.description.0,
            enabled: request.enabled,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(record))
}

pub async fn delete_type(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(key): Path<String>,
) -> Result<StatusCode> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    service::delete_type(&state.db, &key, support::mutation_context(&user))
        .await
        .map_err(error)?;
    Ok(StatusCode::NO_CONTENT)
}
