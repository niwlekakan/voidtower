use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::{
    auth,
    error::{AppError, Result},
    operations::resources,
    AppState,
};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    100
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
) -> Result<Json<serde_json::Value>> {
    require_session(&state, &jar).await?;
    let resources = resources::list(&state.db, query.limit)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({"resources": resources})))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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
    Ok(Json(serde_json::json!({
        "resource": resource,
        "aliases": aliases,
        "capabilities": capabilities,
    })))
}

pub async fn capabilities(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
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
    Ok(Json(serde_json::json!({"capabilities": capabilities})))
}
