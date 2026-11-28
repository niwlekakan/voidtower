use crate::{
    auth,
    error::{AppError, Result},
    services::{self, ServiceAction, ServiceInfo},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::HeaderMap,
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use super::{
    actions,
    bearer_auth::AuthenticatedApiToken,
    operation_adoption::{self, CompatibilityResult},
};
use crate::operations::{adapters::CompatibilityObservationRequest, invocation};

#[derive(Serialize)]
pub struct ServicesResponse {
    pub services: Vec<ServiceInfo>,
    pub systemd_available: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    pub action: ServiceAction,
}

#[derive(Serialize)]
pub struct LogsResponse {
    pub lines: Vec<String>,
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<ServicesResponse>> {
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
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    request: std::result::Result<Json<ActionRequest>, JsonRejection>,
) -> CompatibilityResult<Response> {
    let credential = actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, "service.restart")?;
    let Json(req) =
        request.map_err(|_| AppError::BadRequest("invalid service action request".into()))?;
    let action = match req.action {
        ServiceAction::Restart => "service.restart",
        _ => {
            return Err(AppError::FeatureUnavailable(
                "only service restart is available through the canonical operation adapter".into(),
            )
            .into());
        }
    };
    let idempotency_key = operation_adoption::idempotency_key(&headers)?;
    let unit = services::canonical_unit_name(&name)
        .map_err(|_| AppError::BadRequest("invalid service name".into()))?;
    let _compatibility_guard = state.compatibility_idempotency_lock.lock().await;
    let idempotency_scope = credential.idempotency_scope();
    let _cross_process_guard = crate::db::lock::acquire_scoped(
        &state.config.db_path(),
        &idempotency_scope,
        &idempotency_key,
        std::time::Duration::from_secs(30),
    )
    .await
    .map_err(|_| {
        AppError::FeatureUnavailable(
            "service operation is busy; retry with the same Idempotency-Key".into(),
        )
    })?;
    match invocation::lookup_compatibility_idempotency(
        &state.db,
        invocation::CompatibilityIdempotencyRequest {
            credential: &credential,
            action,
            expected_kind: "service",
            namespace: "systemd.service",
            scope_key: "local",
            alias: &unit,
            input: &serde_json::json!({}),
            idempotency_key: &idempotency_key,
        },
    )
    .await?
    {
        invocation::CompatibilityIdempotency::Existing(job) => {
            return Ok(operation_adoption::accepted_job(*job));
        }
        invocation::CompatibilityIdempotency::Conflict => {
            return Err(invocation::InvocationError::IdempotencyConflict.into());
        }
        invocation::CompatibilityIdempotency::Missing { .. } => {}
    }
    operation_adoption::preflight_policy(
        &state,
        &credential,
        action,
        "service",
        "systemd.service",
        "local",
        &unit,
    )
    .await?;
    let adapter = state.operation_adapters.for_action(action).map_err(|_| {
        AppError::FeatureUnavailable("service operation adapter unavailable".into())
    })?;
    let observation = adapter
        .observe_compatibility(CompatibilityObservationRequest {
            action,
            kind: "service",
            namespace: "systemd.service",
            scope_key: "local",
            alias: &unit,
        })
        .await
        .map_err(|_| {
            AppError::FeatureUnavailable("systemd service inspection is unavailable".into())
        })?;
    let resource = operation_adoption::observe_available(
        &state,
        &credential,
        operation_adoption::CompatibilityResource {
            kind: "service",
            display_name: &observation.display_name,
            node_id: None,
            provider: Some("systemd"),
            namespace: "systemd.service",
            scope_key: "local",
            alias: &observation.canonical_alias,
        },
        &[action],
    )
    .await?;
    operation_adoption::submit_with_key(
        &state,
        &credential,
        &resource.id,
        action,
        serde_json::json!({}),
        &idempotency_key,
    )
    .await
}

pub async fn logs(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(name): Path<String>,
) -> Result<Json<LogsResponse>> {
    require_user(&state, &jar).await?;
    let lines = services::get_service_logs(&name, 200).map_err(AppError::Internal)?;
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
    use crate::operations::adapters::services::{RestartOutcome, ServiceProvider, ServiceSnapshot};
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use axum_extra::extract::cookie::Cookie;
    use sha2::{Digest, Sha256};
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    #[derive(Debug)]
    struct RouterProvider {
        snapshot: ServiceSnapshot,
        inspect_calls: Mutex<usize>,
        restart_calls: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl ServiceProvider for RouterProvider {
        async fn inspect(&self, _unit: &str) -> anyhow::Result<Option<ServiceSnapshot>> {
            *self.inspect_calls.lock().unwrap() += 1;
            Ok(Some(self.snapshot.clone()))
        }

        async fn restart(&self, _unit: &str) -> anyhow::Result<RestartOutcome> {
            *self.restart_calls.lock().unwrap() += 1;
            Ok(RestartOutcome::Succeeded)
        }
    }

    fn router_provider() -> Arc<RouterProvider> {
        Arc::new(RouterProvider {
            snapshot: ServiceSnapshot {
                unit: "fixture.service".into(),
                load_state: "loaded".into(),
                active_state: "active".into(),
                sub_state: "running".into(),
                unit_file_state: "enabled".into(),
                active_enter_timestamp_monotonic: "100".into(),
            },
            inspect_calls: Mutex::new(0),
            restart_calls: Mutex::new(0),
        })
    }

    fn action_request_for(session: &str, key: Option<&str>, name: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(format!("/api/services/{name}/action"))
            .header("content-type", "application/json")
            .header("x-voidtower-api-version", "1")
            .header("cookie", format!("vt_session={session}"));
        if let Some(key) = key {
            builder = builder.header("Idempotency-Key", key);
        }
        builder.body(Body::from(r#"{"action":"restart"}"#)).unwrap()
    }

    fn action_request(session: &str, key: Option<&str>) -> Request<Body> {
        action_request_for(session, key, "fixture.service")
    }

    #[test]
    fn service_aliases_are_normalized_before_provider_use() {
        assert_eq!(
            services::canonical_unit_name("nginx").unwrap(),
            "nginx.service"
        );
        assert!(services::canonical_unit_name("../../etc").is_err());
    }

    #[tokio::test]
    async fn unsupported_service_mutations_remain_fail_closed() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let state = crate::api::mcp::test_support::build(pool);
        let jar = CookieJar::new().add(Cookie::new("vt_session", session));

        let result = action(
            State(state),
            jar,
            None,
            Path("fixture.service".into()),
            HeaderMap::new(),
            Ok(Json(ActionRequest {
                action: ServiceAction::Start,
            })),
        )
        .await;

        assert!(
            matches!(
                result,
                Err(super::operation_adoption::CompatibilityError::Legacy(
                    AppError::FeatureUnavailable(ref message)
                )) if message.contains("only service restart")
            ),
            "unsupported service mutations must fail closed: {result:?}"
        );
    }

    #[tokio::test]
    async fn real_router_adopts_service_restart_and_replays_same_job() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let app = crate::api::router(state);

        let first = app
            .clone()
            .oneshot(action_request(&session, Some("service-test-1")))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::ACCEPTED);
        let first_body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
        let first_json: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
        assert_eq!(first_json["action"], "service.restart");
        assert!(!first_json["resource_id"].as_str().unwrap().is_empty());
        let first_job = first_json["job"]["id"].as_str().unwrap().to_owned();
        let resource_id = first_json["resource_id"].as_str().unwrap();
        let inspections_after_first = *provider.inspect_calls.lock().unwrap();
        assert!(inspections_after_first > 0);
        sqlx::query("UPDATE resources SET lifecycle_state = ? WHERE id = ?")
            .bind("retired")
            .bind(resource_id)
            .execute(&db)
            .await
            .unwrap();

        let replay = app
            .oneshot(action_request(&session, Some("service-test-1")))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::ACCEPTED);
        let replay_body = to_bytes(replay.into_body(), usize::MAX).await.unwrap();
        let replay_json: serde_json::Value = serde_json::from_slice(&replay_body).unwrap();
        assert_eq!(replay_json["job"]["id"], first_job);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM jobs WHERE action = 'service.restart'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            *provider.inspect_calls.lock().unwrap(),
            inspections_after_first
        );
        assert_eq!(*provider.restart_calls.lock().unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM audit_log WHERE action = 'service.restart'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM events WHERE event_type = 'job.submitted.v1'"
            )
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
        let audit: (String, String, String, String, String) = sqlx::query_as(
            "SELECT user_id, resource_id, outcome, source, request_id
             FROM audit_log WHERE action = 'service.restart' LIMIT 1",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(!audit.0.is_empty());
        assert_eq!(audit.1, first_json["resource_id"]);
        assert_eq!(audit.2, "queued");
        assert_eq!(audit.3, "http_session");
        assert_eq!(audit.4, first_job);
        let event: (String, String, String, String) = sqlx::query_as(
            "SELECT resource_id, job_id, actor_id, correlation_id
             FROM events WHERE event_type = 'job.submitted.v1' LIMIT 1",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(event.0, first_json["resource_id"]);
        assert_eq!(event.1, first_job);
        assert!(!event.2.is_empty());
        assert_eq!(event.3, first_job);
    }

    #[tokio::test]
    async fn changed_service_intent_conflicts_before_provider_observation() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let app = crate::api::router(state);

        let first = app
            .clone()
            .oneshot(action_request(&session, Some("service-conflict-1")))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::ACCEPTED);
        let inspections_after_first = *provider.inspect_calls.lock().unwrap();
        assert!(inspections_after_first > 0);

        let conflict = app
            .oneshot(action_request_for(
                &session,
                Some("service-conflict-1"),
                "other.service",
            ))
            .await
            .unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        assert_eq!(
            *provider.inspect_calls.lock().unwrap(),
            inspections_after_first
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn bearer_token_is_denied_for_service_restart() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let provider = router_provider();
        let user_id = "service-bearer-operator";
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at)
             VALUES (?, ?, 'x', 'operator', 0, 0)",
        )
        .bind(user_id)
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();
        // API-token mutations default-deny. Scope is necessary but an explicit
        // policy allow is also required for this positive authorization fixture.
        sqlx::query(
            "INSERT INTO policy_rules
             (id, name, actor_type, action, resource_type, effect, priority, enabled, created_at)
             VALUES ('allow-scoped-service-token', 'allow scoped service token', 'api_token', 'service.restart', 'service', 'allow', 1, 1, 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let raw_token = "vt_service_restart_scope";
        let hash = hex::encode(Sha256::digest(raw_token.as_bytes()));
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('service-scope-token', ?, 'service', ?, '[\"services:restart\"]', 0)",
        )
        .bind(user_id)
        .bind(hash)
        .execute(&db)
        .await
        .unwrap();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let app = crate::api::router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/services/fixture.service/action")
                    .header("content-type", "application/json")
                    .header("x-voidtower-api-version", "1")
                    .header("authorization", format!("Bearer {raw_token}"))
                    .header("Idempotency-Key", "bearer-service-1")
                    .body(Body::from(r#"{"action":"restart"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        let response_status = response.status();
        let response_body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            response_status,
            StatusCode::FORBIDDEN,
            "unexpected response: {}",
            String::from_utf8_lossy(&response_body),
        );
        assert_eq!(*provider.inspect_calls.lock().unwrap(), 0);
        assert_eq!(*provider.restart_calls.lock().unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM resources")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn bearer_without_service_restart_scope_is_denied_before_observation() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let provider = router_provider();
        let user_id = "service-bearer-read-only";
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at)
             VALUES (?, ?, 'x', 'operator', 0, 0)",
        )
        .bind(user_id)
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();
        let raw_token = "vt_service_read_scope";
        let hash = hex::encode(Sha256::digest(raw_token.as_bytes()));
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('service-read-token', ?, 'service-read', ?, '[\"services:read\"]', 0)",
        )
        .bind(user_id)
        .bind(hash)
        .execute(&db)
        .await
        .unwrap();
        let state =
            crate::api::mcp::test_support::build_with_service_provider(db, provider.clone());
        let response = crate::api::router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/services/fixture.service/action")
                    .header("content-type", "application/json")
                    .header("x-voidtower-api-version", "1")
                    .header("authorization", format!("Bearer {raw_token}"))
                    .header("Idempotency-Key", "bearer-service-read-only-1")
                    .body(Body::from(r#"{"action":"restart"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(*provider.inspect_calls.lock().unwrap(), 0);
        assert_eq!(*provider.restart_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn policy_denial_preflights_without_provider_or_resource_side_effects() {
        let db = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO policy_rules
             (id, name, actor_type, action, resource_type, effect, priority, enabled, created_at)
             VALUES ('deny-service-restart', 'deny service restart', 'user', 'service.restart', 'service', 'deny', 1, 1, 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let response = crate::api::router(state)
            .oneshot(action_request(&session, Some("service-policy-deny-1")))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(*provider.inspect_calls.lock().unwrap(), 0);
        assert_eq!(*provider.restart_calls.lock().unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM resources")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM resource_aliases")
                .fetch_one(&db)
                .await
                .unwrap(),
            0
        );
        let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE action = ?")
            .bind("service.restart")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(job_count, 0);
        let blocked_audit_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action = ? AND outcome = ?")
                .bind("service.restart")
                .bind("blocked")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(blocked_audit_count, 1);
    }

    #[tokio::test]
    async fn concurrent_first_requests_observe_provider_once_and_create_one_job() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let app = crate::api::router(state);
        let (left, right) = tokio::join!(
            app.clone()
                .oneshot(action_request(&session, Some("concurrent-service-1"))),
            app.oneshot(action_request(&session, Some("concurrent-service-1"))),
        );
        assert_eq!(left.unwrap().status(), StatusCode::ACCEPTED);
        assert_eq!(right.unwrap().status(), StatusCode::ACCEPTED);
        // One request plans and validates the provider snapshot three times; the
        // idempotency lock makes the overlapping duplicate return that job
        // without entering a second observation/planning path.
        assert_eq!(*provider.inspect_calls.lock().unwrap(), 3);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM jobs")
                .fetch_one(&db)
                .await
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn assisted_policy_returns_approval_bound_to_service_restart_job() {
        let db = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at)
             VALUES ('global', 'assisted', 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let app = crate::api::router(state);
        let response = app
            .oneshot(action_request(&session, Some("service-approval-1")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["job"]["state"], "awaiting_approval");
        let job_id = json["job"]["id"].as_str().unwrap();
        let approval_id = json["job"]["approval_id"].as_str().unwrap();
        let approval_job: String = sqlx::query_scalar("SELECT job_id FROM approvals WHERE id = ?")
            .bind(approval_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(approval_job, job_id);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM events WHERE event_type = 'approval.requested.v1' AND job_id = ? AND approval_id = ?",
            )
            .bind(job_id)
            .bind(approval_id)
            .fetch_one(&db)
            .await
            .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn real_router_denies_viewer_before_service_observation_and_requires_idempotency() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "viewer").await;
        let operator = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state =
            crate::api::mcp::test_support::build_with_service_provider(db, provider.clone());
        let app = crate::api::router(state);

        let denied = app
            .clone()
            .oneshot(action_request(&session, Some("viewer-key")))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        assert_eq!(*provider.inspect_calls.lock().unwrap(), 0);

        let missing_key = app.oneshot(action_request(&operator, None)).await.unwrap();
        assert_eq!(missing_key.status(), StatusCode::BAD_REQUEST);
        assert_eq!(*provider.inspect_calls.lock().unwrap(), 0);
    }

    /// M1-02 residual — the adopted `POST /api/services/:name/action` route is not a machine
    /// ingress. `service.restart` is `BearerPolicy::Denied`, so only a human `Session` reaches it,
    /// and a session is admitted by the machine refusal guard on purpose. The route therefore never
    /// renders `409 unresolved_operation`: asserting that documented claim observes `202 Accepted`
    /// with a queued job. What actually protects the uncertain resource is the worker's
    /// `concurrency_key` precedence: the deferred restart stays `queued`, no provider restart
    /// happens, and `claim_next` returns nothing until the earlier job is reconciled. `docs/api.md`
    /// must not name this route as a refusal site.
    #[tokio::test]
    async fn adopted_service_route_defers_a_second_session_restart_behind_an_unresolved_job() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_role_session(&db, "operator").await;
        let provider = router_provider();
        let state = crate::api::mcp::test_support::build_with_service_provider(
            db.clone(),
            provider.clone(),
        );
        let adapters = state.operation_adapters.clone();
        let app = crate::api::router(state);

        let first = app
            .clone()
            .oneshot(action_request(&session, Some("service-unresolved-1")))
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::ACCEPTED);
        let first_body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
        let first_json: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
        let blocked_job = first_json["job"]["id"].as_str().unwrap().to_owned();
        let blocked_resource = first_json["resource_id"].as_str().unwrap().to_owned();

        // The provider outcome for that job becomes uncertain.
        sqlx::query(
            "UPDATE jobs SET state = 'needs_attention', error_code = 'provider_unavailable', \
             error_message = 'outcome unresolved' WHERE id = ?",
        )
        .bind(&blocked_job)
        .execute(&db)
        .await
        .unwrap();

        let deferred = app
            .oneshot(action_request(&session, Some("service-unresolved-2")))
            .await
            .unwrap();
        let status = deferred.status();
        let deferred_body = to_bytes(deferred.into_body(), usize::MAX).await.unwrap();
        let deferred_json: serde_json::Value = serde_json::from_slice(&deferred_body).unwrap();
        assert_eq!(
            status,
            StatusCode::ACCEPTED,
            "a human session is admitted on the adopted route; observed {status} with \
             {deferred_json}"
        );
        let deferred_job = deferred_json["job"]["id"].as_str().unwrap().to_owned();
        assert_ne!(deferred_job, blocked_job);
        assert_eq!(deferred_json["job"]["state"], "queued");
        assert_eq!(deferred_json["job"]["ingress"], "http_session");
        assert_eq!(
            deferred_json["resource_id"].as_str().unwrap(),
            blocked_resource,
            "both intents must resolve the same canonical service resource"
        );
        assert_eq!(
            *provider.restart_calls.lock().unwrap(),
            0,
            "acceptance of the deferred intent must not mutate the provider"
        );

        // The key holds exactly one unresolved job and one deferred intent.
        assert_eq!(
            jobs_state(&db, &blocked_job).await,
            "needs_attention",
            "the earlier job stays unresolved"
        );
        assert_eq!(
            jobs_state(&db, &deferred_job).await,
            "queued",
            "the deferred intent stays durable and queued"
        );

        // The deferred restart must not be claimable while the earlier outcome is unresolved.
        assert!(
            crate::operations::worker::claim_next(&db, &adapters, "worker-deferred", 100, 20)
                .await
                .unwrap()
                .is_none(),
            "the deferred session restart must not execute while the earlier job is unresolved"
        );
        assert_eq!(
            jobs_state(&db, &deferred_job).await,
            "queued",
            "the deferred intent must stay queued, not be dropped or failed"
        );
        assert_eq!(*provider.restart_calls.lock().unwrap(), 0);

        // Negative control: `None` above must be attributable to the concurrency key, not to a job
        // that can never run. Once the earlier job is terminal the deferred restart is claimable.
        sqlx::query("UPDATE jobs SET state = 'failed', error_code = 'reconciled' WHERE id = ?")
            .bind(&blocked_job)
            .execute(&db)
            .await
            .unwrap();
        let claimed =
            crate::operations::worker::claim_next(&db, &adapters, "worker-deferred", 200, 20)
                .await
                .unwrap()
                .expect("the deferred restart becomes claimable once the earlier job is terminal");
        assert_eq!(claimed.id, deferred_job);
        assert_eq!(
            *provider.restart_calls.lock().unwrap(),
            0,
            "claiming the deferred job must not execute a provider restart"
        );
    }

    async fn jobs_state(db: &sqlx::SqlitePool, job_id: &str) -> String {
        sqlx::query_scalar::<_, String>("SELECT state FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(db)
            .await
            .unwrap()
    }
}
