use crate::{
    audit,
    auth,
    error::{AppError, Result},
    services::{self, ServiceAction, ServiceInfo},
    AppState,
};
use axum::{
    extract::{ConnectInfo, Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

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
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::NotFound)
        .map(Json)
}

pub async fn action(
    State(state): State<AppState>,
    jar: CookieJar,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(name): Path<String>,
    Json(req): Json<ActionRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    // Require at least operator role for mutations
    if user.role == "viewer" {
        return Err(AppError::Forbidden);
    }

    let action_str = format!("{:?}", req.action).to_lowercase();
    services::run_service_action(&name, req.action)
        .map_err(|e| AppError::Internal(e))?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        &format!("service.{}", action_str),
        Some("service"),
        Some(&name),
        "success",
        Some(&addr.ip().to_string()),
        None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<LogsResponse>> {
    require_user(&state, &jar).await?;
    let lines = services::get_service_logs(&name, 200)
        .map_err(|e| AppError::Internal(e))?;
    Ok(Json(LogsResponse { lines }))
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<crate::auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::Unauthorized)
}
