use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use crate::{
    auth,
    error::{AppError, Result},
    operations::{
        contracts::{ActorType, JobState},
        invocation::{self, CredentialContext},
        jobs, worker,
    },
    AppState,
};

use super::{
    actions::CanonicalApiError,
    bearer_auth::AuthenticatedApiToken,
    version::{JobReadEnvelopeV1, JobSuccessEnvelopeV1},
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

pub async fn get_by_idempotency(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(key): Path<String>,
) -> std::result::Result<
    Json<JobReadEnvelopeV1<crate::operations::contracts::JobSummaryV1>>,
    CanonicalApiError,
> {
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    invocation::validate_idempotency_key(&key)?;
    let job = jobs::get_by_idempotency(&state.db, &credential.idempotency_scope(), &key)
        .await
        .map_err(|_| CanonicalApiError::internal())?
        .ok_or_else(CanonicalApiError::job_not_found)?;
    let action = crate::api::mcp::action_registry::action(&job.action)
        .ok_or_else(CanonicalApiError::forbidden)?;
    invocation::authorize_action(action, &credential)?;
    Ok(Json(JobReadEnvelopeV1::new(
        job.resource.id.clone(),
        job.action.clone(),
        job,
    )))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<JobReadEnvelopeV1<crate::operations::contracts::JobSummaryV1>>> {
    require_operator(&state, &jar).await?;
    let job = jobs::get(&state.db, &id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(JobReadEnvelopeV1::new(
        job.resource.id.clone(),
        job.action.clone(),
        job,
    )))
}

pub async fn cancel(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(id): Path<String>,
) -> std::result::Result<Response, CanonicalApiError> {
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    let job = jobs::get(&state.db, &id)
        .await
        .map_err(|_| CanonicalApiError::internal())?
        .ok_or_else(CanonicalApiError::job_not_found)?;
    let action = crate::api::mcp::action_registry::action(&job.action)
        .ok_or_else(CanonicalApiError::forbidden)?;
    invocation::authorize_action(action, &credential)?;

    if let CredentialContext::Bearer { token_id, .. } = &credential {
        if job.actor.actor_type != ActorType::ApiToken
            || job.actor.id.as_deref() != Some(token_id.as_str())
        {
            return Err(CanonicalApiError::forbidden());
        }
    }
    if !matches!(job.state, JobState::Queued | JobState::Running) {
        return Err(CanonicalApiError::invalid_job_state());
    }

    worker::request_cancellation(
        &state.db,
        &id,
        credential.actor(),
        crate::operations::unix_now(),
    )
    .await
    .map_err(|error| match error {
        worker::CancellationError::NotFound => CanonicalApiError::job_not_found(),
        worker::CancellationError::InvalidState => CanonicalApiError::invalid_job_state(),
        worker::CancellationError::Internal => CanonicalApiError::internal(),
    })?;
    let job = jobs::get(&state.db, &id)
        .await
        .map_err(|_| CanonicalApiError::internal())?
        .ok_or_else(CanonicalApiError::job_not_found)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(JobSuccessEnvelopeV1::new(
            job.resource.id.clone(),
            job.action.clone(),
            job,
        )),
    )
        .into_response())
}
