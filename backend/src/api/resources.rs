use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::{
    api::version::{
        ResourceCapabilitiesEnvelopeV1, ResourceListEnvelopeV1, ResourceReadEnvelopeV1,
        MAX_PAGE_LIMIT, RESOURCE_ENVELOPE_SCHEMA_VERSION,
    },
    auth,
    error::{AppError, Result},
    operations::resources,
    AppState,
};

#[derive(Deserialize)]
pub struct ListQuery {
    limit: Option<String>,
}

fn default_limit() -> i64 {
    100
}

fn parse_limit(value: Option<&str>) -> Result<i64> {
    let limit = value
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| AppError::BadRequest("limit must be an integer between 1 and 500".into()))?
        .unwrap_or_else(default_limit);
    if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest(
            "limit must be an integer between 1 and 500".into(),
        ));
    }
    Ok(limit)
}

async fn require_session(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Json<ResourceListEnvelopeV1<crate::operations::contracts::ResourceRef>>> {
    require_session(&state, &jar).await?;
    let resources = resources::list(&state.db, parse_limit(query.limit.as_deref())?)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(ResourceListEnvelopeV1 {
        schema_version: RESOURCE_ENVELOPE_SCHEMA_VERSION,
        resources,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<
    Json<
        ResourceReadEnvelopeV1<
            crate::operations::contracts::ResourceRef,
            crate::operations::contracts::ResourceAlias,
            crate::operations::contracts::ResourceCapability,
        >,
    >,
> {
    require_session(&state, &jar).await?;
    let resource = resources::get(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let aliases = resources::aliases(&state.db, &id)
        .await
        .map_err(AppError::Internal)?;
    let capabilities = resources::capabilities(&state.db, &id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(ResourceReadEnvelopeV1 {
        schema_version: RESOURCE_ENVELOPE_SCHEMA_VERSION,
        resource,
        aliases,
        capabilities,
    }))
}

pub async fn capabilities(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<ResourceCapabilitiesEnvelopeV1<crate::operations::contracts::ResourceCapability>>>
{
    require_session(&state, &jar).await?;
    if resources::get(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .is_none()
    {
        return Err(AppError::NotFound);
    }
    let capabilities = resources::capabilities(&state.db, &id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(ResourceCapabilitiesEnvelopeV1 {
        schema_version: RESOURCE_ENVELOPE_SCHEMA_VERSION,
        resource_id: id,
        capabilities,
    }))
}
