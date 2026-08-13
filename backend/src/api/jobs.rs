use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::{
    auth,
    error::{AppError, Result},
    operations::jobs,
    AppState,
};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    50
}

async fn require_operator(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    super::role_guard::require_operator(&user)?;
    Ok(user)
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>> {
    require_operator(&state, &jar).await?;
    let jobs = jobs::list(&state.db, query.limit)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({"jobs": jobs})))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_operator(&state, &jar).await?;
    let job = jobs::get(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(serde_json::json!({"job": job})))
}
