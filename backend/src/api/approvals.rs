use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::{
    auth,
    error::{AppError, Result},
    operations::{
        approvals,
        contracts::{ActorRef, ActorType},
    },
    AppState,
};

#[derive(Deserialize)]
pub struct ListQuery {
    status: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

#[derive(Deserialize)]
pub struct DecisionRequest {
    comment: Option<String>,
}

fn default_limit() -> i64 {
    50
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    super::role_guard::require_admin(&user)?;
    Ok(user)
}

fn actor(user: auth::User) -> ActorRef {
    ActorRef {
        actor_type: ActorType::Human,
        id: Some(user.id),
        source: Some("web".into()),
    }
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let approvals = approvals::list(&state.db, query.status.as_deref(), query.limit)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({"approvals": approvals})))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let approval = approvals::get(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(serde_json::json!({"approval": approval})))
}

pub async fn approve(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(request): Json<DecisionRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let job = approvals::approve(
        &state.db,
        &state.operation_adapters,
        &id,
        actor(user),
        request.comment.as_deref(),
    )
    .await
    .map_err(|error| AppError::Conflict(error.to_string()))?;
    Ok(Json(serde_json::json!({"job": job})))
}

pub async fn reject(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(request): Json<DecisionRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;
    let job = approvals::reject(&state.db, &id, actor(user), request.comment.as_deref())
        .await
        .map_err(|error| AppError::Conflict(error.to_string()))?;
    Ok(Json(serde_json::json!({"job": job})))
}
