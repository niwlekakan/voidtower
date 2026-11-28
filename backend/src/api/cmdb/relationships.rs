use super::support::{self, Access};
use crate::{
    cmdb::relationships::{self as service, RelationshipError, RelationshipRecord},
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

#[derive(Deserialize)]
pub struct CreateRequest {
    destination_selector: String,
    type_key: String,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Serialize)]
pub struct PublicRelationship {
    pub id: String,
    pub source_resource_id: String,
    pub destination_resource_id: String,
    pub type_key: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub active: bool,
    pub metadata: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

fn public_relationship(record: RelationshipRecord) -> Result<PublicRelationship> {
    let metadata = serde_json::from_str(&record.metadata_json).map_err(|error| {
        tracing::error!("invalid persisted CMDB relationship metadata: {error}");
        AppError::Internal(anyhow::anyhow!("CMDB relationship encoding failed"))
    })?;
    Ok(PublicRelationship {
        id: record.id,
        source_resource_id: record.source_resource_id,
        destination_resource_id: record.destination_resource_id,
        type_key: record.type_key,
        started_at: record.started_at,
        ended_at: record.ended_at,
        active: record.active,
        metadata,
        created_at: record.created_at,
        updated_at: record.updated_at,
    })
}

fn error(error: RelationshipError) -> AppError {
    match error {
        RelationshipError::Invalid(message) => AppError::BadRequest(message),
        RelationshipError::NotFound
        | RelationshipError::TypeNotFound
        | RelationshipError::SourceNotFound
        | RelationshipError::DestinationNotFound => AppError::NotFound,
        RelationshipError::TypeDisabled => {
            AppError::BadRequest("relationship type is disabled".into())
        }
        RelationshipError::Conflict(_) => {
            AppError::Conflict("relationship request conflicts with current state".into())
        }
        RelationshipError::Internal(error) => {
            tracing::error!("CMDB relationship operation failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB relationship operation failed"))
        }
    }
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    uri: Uri,
) -> Result<Json<Value>> {
    support::require_user(&state, &jar, Access::Read).await?;
    let query: ListQuery = support::parse_query(&uri)?;
    support::validate_page(query.limit, query.offset)?;
    let asset = support::resolve_asset(&state, &selector).await?;
    let relationships =
        service::list_by_asset(&state.db, &asset.resource_id, query.limit, query.offset)
            .await
            .map_err(error)?
            .into_iter()
            .map(public_relationship)
            .collect::<Result<Vec<_>>>()?;
    Ok(Json(json!({
        "resource_id": asset.resource_id,
        "relationships": relationships,
        "limit": query.limit,
        "offset": query.offset
    })))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<Json<PublicRelationship>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let request: CreateRequest = support::parse_json(body)?;
    let source = support::resolve_asset(&state, &selector).await?;
    let destination = support::resolve_asset(&state, &request.destination_selector).await?;
    let relationship = service::start(
        &state.db,
        service::StartRelationshipInput {
            source_resource_id: source.resource_id,
            destination_resource_id: destination.resource_id,
            type_key: request.type_key,
            metadata: request.metadata,
        },
        support::mutation_context(&user),
    )
    .await
    .map_err(error)?;
    Ok(Json(public_relationship(relationship)?))
}

pub async fn end(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<PublicRelationship>> {
    let user = support::require_user(&state, &jar, Access::Write).await?;
    let relationship = service::end(&state.db, &id, support::mutation_context(&user))
        .await
        .map_err(error)?;
    Ok(Json(public_relationship(relationship)?))
}
