use crate::{
    auth,
    backups::{self, BackupConfigInput},
    error::{AppError, Result},
    operations::backup_adoption::{self, BackupSelector},
    AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

use super::{
    bearer_auth::AuthenticatedApiToken,
    operation_adoption::{self, CompatibilityResult},
};

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let configs = backups::list_configs(&state.db)
        .await
        .map_err(AppError::Internal)?;

    let configs_with_confidence: Vec<serde_json::Value> = configs
        .iter()
        .map(|c| {
            let mut v = serde_json::to_value(c).unwrap_or_default();
            v["confidence"] = serde_json::Value::String(backups::confidence(c).to_string());
            v
        })
        .collect();

    Ok(Json(serde_json::json!({
        "configs": configs_with_confidence,
        "restic_available": backups::is_restic_available(),
    })))
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub source_path: String,
    pub repo_path: String,
    pub schedule: Option<String>,
    pub retention_days: Option<i64>,
    pub restore_test_schedule: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(&state, &jar, None).await?;
    let input = BackupConfigInput {
        name: req.name,
        source_path: req.source_path,
        repo_path: req.repo_path,
        schedule: req.schedule,
        retention_days: req.retention_days.unwrap_or(30),
        restore_test_schedule: req.restore_test_schedule,
    };
    input
        .validate()
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    let resource = backup_adoption::resolve_create_target(&state.db, &credential).await?;
    let input = serde_json::to_value(input).map_err(|error| AppError::Internal(error.into()))?;
    operation_adoption::submit(
        &state,
        &credential,
        &resource.id,
        "backup.config.create",
        input,
        &headers,
    )
    .await
}

pub async fn run_now(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_existing(
        &state,
        &jar,
        token.map(|Extension(token)| token),
        &id,
        "backup.run",
        &headers,
    )
    .await
}

pub async fn check(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_existing(&state, &jar, None, &id, "backup.check", &headers).await
}

pub async fn restore_test(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_existing(&state, &jar, None, &id, "backup.restore_test", &headers).await
}

pub async fn delete_plan(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "backup.config.delete";
    let credential = super::actions::credential(&state, &jar, None).await?;
    let adopted = backup_adoption::resolve_config_target(
        &state.db,
        &credential,
        BackupSelector::Id(&id),
        ACTION,
    )
    .await?;
    let prepared = operation_adoption::prepare(
        &state,
        &credential,
        &adopted.resource.id,
        ACTION,
        serde_json::json!({}),
    )
    .await?;
    let view = prepared.view();
    Ok(Json(serde_json::json!({
        "dry_run": true,
        "plan": view.operation,
        "policy": view.policy,
        "resource": view.resource,
    }))
    .into_response())
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_existing(&state, &jar, None, &id, "backup.config.delete", &headers).await
}

async fn submit_existing(
    state: &AppState,
    jar: &CookieJar,
    token: Option<AuthenticatedApiToken>,
    config_id: &str,
    action: &str,
    headers: &HeaderMap,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(state, jar, token).await?;
    let adopted = backup_adoption::resolve_config_target(
        &state.db,
        &credential,
        BackupSelector::Id(config_id),
        action,
    )
    .await?;
    operation_adoption::submit(
        state,
        &credential,
        &adopted.resource.id,
        action,
        serde_json::json!({}),
        headers,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::StatusCode};
    use axum_extra::extract::cookie::{Cookie, CookieJar};

    async fn state_and_jar() -> (AppState, CookieJar) {
        let pool = crate::api::mcp::test_support::setup_db().await;
        crate::operations::resources::observe(
            &pool,
            crate::operations::resources::ObserveResource {
                kind: "system",
                display_name: "This VoidTower",
                node_id: None,
                provider: Some("local"),
                namespace: "voidtower.singleton",
                scope_key: "local",
                alias: "system",
            },
            None,
            "seed",
        )
        .await
        .unwrap();
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let state = crate::api::mcp::test_support::build(pool);
        let jar = CookieJar::new().add(Cookie::new("vt_session", session));
        (state, jar)
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn create_submits_a_durable_job_without_creating_the_config_inline() {
        let (state, jar) = state_and_jar().await;
        let response = create(
            State(state.clone()),
            jar,
            HeaderMap::new(),
            Json(CreateRequest {
                name: "Daily".into(),
                source_path: "/srv/data".into(),
                repo_path: "/srv/restic".into(),
                schedule: None,
                retention_days: Some(30),
                restore_test_schedule: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = response_json(response).await;
        assert_eq!(body["job"]["action"], "backup.config.create");
        let configs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM backup_configs")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(configs, 0);
    }

    #[tokio::test]
    async fn delete_plan_is_adapter_produced_and_advisory() {
        let (state, jar) = state_and_jar().await;
        sqlx::query(
            "INSERT INTO backup_configs \
             (id, name, source_path, repo_path, retention_days, enabled, created_at) \
             VALUES ('config-1', 'Daily', '/srv/data', '/srv/restic', 30, 1, 0)",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let response = delete_plan(State(state.clone()), jar, Path("config-1".into()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["dry_run"], true);
        assert_eq!(body["plan"]["title"], "Delete backup configuration");
        assert_eq!(body["plan"]["risk"], "destructive");
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
            .fetch_one(&state.db)
            .await
            .unwrap();
        let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM approvals")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!((jobs, approvals), (0, 0));
    }
}
