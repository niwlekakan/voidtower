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
        contracts::{ActorRef, ActorType, ApprovalViewV1, JobSummaryV1},
    },
    AppState,
};

use super::version::{ApprovalListEnvelopeV1, ApprovalReadEnvelopeV1, JobSuccessEnvelopeV1};

const MAX_DECISION_COMMENT_CHARS: usize = 500;

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

fn validate_decision_comment(comment: Option<&str>) -> Result<()> {
    if comment.is_some_and(|value| value.chars().count() > MAX_DECISION_COMMENT_CHARS) {
        return Err(AppError::BadRequest(
            "Approval comments must be 500 characters or fewer.".into(),
        ));
    }
    Ok(())
}

fn map_decision_error(error: anyhow::Error) -> AppError {
    if error
        .chain()
        .any(|cause| cause.to_string() == "approval not found")
    {
        AppError::NotFound
    } else if error
        .chain()
        .any(|cause| cause.to_string() == "approval is no longer pending")
    {
        AppError::ApprovalConflict
    } else {
        AppError::Internal(error)
    }
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<ListQuery>,
) -> Result<Json<ApprovalListEnvelopeV1<ApprovalViewV1>>> {
    require_admin(&state, &jar).await?;
    let approvals = approvals::list(&state.db, query.status.as_deref(), query.limit)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(ApprovalListEnvelopeV1::new(approvals)))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<ApprovalReadEnvelopeV1<ApprovalViewV1>>> {
    require_admin(&state, &jar).await?;
    let approval = approvals::get(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(ApprovalReadEnvelopeV1::new(approval)))
}

pub async fn approve(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(request): Json<DecisionRequest>,
) -> Result<Json<JobSuccessEnvelopeV1<JobSummaryV1>>> {
    let user = require_admin(&state, &jar).await?;
    validate_decision_comment(request.comment.as_deref())?;
    let job = approvals::approve(
        &state.db,
        &state.operation_adapters,
        &id,
        actor(user),
        request.comment.as_deref(),
    )
    .await
    .map_err(map_decision_error)?;
    Ok(Json(JobSuccessEnvelopeV1::new(
        job.resource.id.clone(),
        job.action.clone(),
        job,
    )))
}

pub async fn reject(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(request): Json<DecisionRequest>,
) -> Result<Json<JobSuccessEnvelopeV1<JobSummaryV1>>> {
    let user = require_admin(&state, &jar).await?;
    validate_decision_comment(request.comment.as_deref())?;
    let job = approvals::reject(&state.db, &id, actor(user), request.comment.as_deref())
        .await
        .map_err(map_decision_error)?;
    Ok(Json(JobSuccessEnvelopeV1::new(
        job.resource.id.clone(),
        job.action.clone(),
        job,
    )))
}
