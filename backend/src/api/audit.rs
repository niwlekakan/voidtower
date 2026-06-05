use crate::{
    audit,
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 { 50 }

#[derive(Serialize)]
pub struct AuditResponse {
    pub entries: Vec<audit::AuditEntry>,
    pub limit: i64,
    pub offset: i64,
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Json<AuditResponse>> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    if user.role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let limit = query.limit.clamp(1, 500);
    let entries = audit::list(&state.db, limit, query.offset)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(AuditResponse {
        entries,
        limit,
        offset: query.offset,
    }))
}
