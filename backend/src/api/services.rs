use crate::{
    auth,
    error::{AppError, Result},
    services::{self, ServiceAction, ServiceInfo},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct ServicesResponse {
    pub services: Vec<ServiceInfo>,
    pub systemd_available: bool,
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub action: ServiceAction,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub lines: Vec<String>,
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<ServicesResponse>> {
    let user = require_user(&state, &jar).await?;
    let _ = user;

    let available = services::is_systemd_available();
    let svcs = if available {
        services::list_services().unwrap_or_default()
    } else {
        vec![]
    };

    Ok(Json(ServicesResponse {
        services: svcs,
        systemd_available: available,
    }))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<ServiceInfo>> {
    require_user(&state, &jar).await?;
    services::get_service(&name)
        .map_err(AppError::Internal)?
        .ok_or(AppError::NotFound)
        .map(Json)
}

pub async fn action(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    // Require at least operator role for mutations.
    super::role_guard::require_operator(&user)?;

    let _ = name;
    Err(AppError::FeatureUnavailable(
        "service mutations require a canonical operation adapter".into(),
    ))
}

pub async fn logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<LogsResponse>> {
    require_user(&state, &jar).await?;
    let lines = services::get_service_logs(&name, 200)
        .map_err(AppError::Internal)?;
    Ok(Json(LogsResponse { lines }))
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<crate::auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::Cookie;

    #[tokio::test]
    async fn service_mutation_fails_closed_until_canonical_adapter_exists() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let state = crate::api::mcp::test_support::build(pool);
        let jar = CookieJar::new().add(Cookie::new("vt_session", session));

        let result = action(
            State(state),
            jar,
            Path("fixture.service".into()),
        )
        .await;

        assert!(
            matches!(result, Err(AppError::FeatureUnavailable(ref message)) if message.contains("canonical operation")),
            "service mutation must fail closed instead of executing systemctl: {result:?}"
        );
    }
}
