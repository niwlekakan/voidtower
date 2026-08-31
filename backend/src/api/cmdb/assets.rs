use super::support::{self, Access};
use crate::{
    cmdb::{assets as service, contracts::AssetRecord},
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
use serde_json::Value;

fn domain_error(error: anyhow::Error) -> AppError {
    let message = error.to_string();
    if message.contains("not found") || message.contains("does not exist") {
        AppError::NotFound
    } else if message.contains("conflict")
        || message.contains("already exists")
        || message.contains("UNIQUE")
    {
        AppError::Conflict("CMDB request conflicts with current state".into())
    } else {
        AppError::BadRequest(message)
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 {
    100
}

pub async fn list(State(state): State<AppState>, jar: CookieJar, uri: Uri) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let q: ListQuery = support::parse_query(&uri)?;
    support::validate_page(q.limit, q.offset)?;
    let rows = service::list(&state.db, q.limit, q.offset)
        .await
        .map_err(domain_error)?;
    Ok(Json(
        serde_json::json!({"assets": rows, "limit": q.limit, "offset": q.offset}),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
) -> Result<Json<AssetRecord>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let record = service::get(&state.db, &selector)
        .await
        .map_err(domain_error)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub class_key: String,
    pub type_key: String,
    pub name: String,
    #[serde(default)]
    pub friendly_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
    #[serde(default)]
    pub part_number: Option<String>,
    #[serde(default)]
    pub location_id: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub notes: String,
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<AssetRecord>> {
    let u = support::require_user(&state, &jar, Access::Write).await?;
    let req: CreateRequest = support::parse_json(body)?;
    let input = service::CreateAssetInput {
        class_key: req.class_key,
        type_key: req.type_key,
        name: req.name,
        friendly_name: req.friendly_name,
        description: req.description,
        manufacturer: req.manufacturer,
        model: req.model,
        serial_number: req.serial_number,
        part_number: req.part_number,
        location_id: req.location_id,
        metadata: req.metadata,
        notes: req.notes,
    };
    let record = service::create_manual(&state.db, input, support::mutation_context(&u))
        .await
        .map_err(domain_error)?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub asset_id: String,
    pub expected_revision: i64,
}
pub async fn rename(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<AssetRecord>> {
    let u = support::require_user(&state, &jar, Access::Write).await?;
    let req: RenameRequest = support::parse_json(body)?;
    if req.expected_revision < 0 {
        return Err(AppError::BadRequest(
            "expected_revision must be non-negative".into(),
        ));
    }
    let record = service::rename(
        &state.db,
        &selector,
        &req.asset_id,
        req.expected_revision,
        support::mutation_context(&u),
    )
    .await
    .map_err(domain_error)?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct RetirementRequest {
    pub retired: bool,
    pub expected_revision: i64,
}
pub async fn retirement(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<AssetRecord>> {
    let u = support::require_user(&state, &jar, Access::Write).await?;
    let req: RetirementRequest = support::parse_json(body)?;
    let record = service::set_retired(
        &state.db,
        &selector,
        req.retired,
        req.expected_revision,
        support::mutation_context(&u),
    )
    .await
    .map_err(domain_error)?;
    Ok(Json(record))
}
