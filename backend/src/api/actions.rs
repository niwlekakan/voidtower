use axum::{
    extract::{rejection::JsonRejection, Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    auth,
    operations::{
        contracts::JobState,
        invocation::{self, CredentialContext, InvocationError},
    },
    AppState,
};

use super::bearer_auth::AuthenticatedApiToken;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRequest {
    input: Value,
}

#[derive(Debug)]
pub struct CanonicalApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    job_id: Option<String>,
}

impl CanonicalApiError {
    fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            job_id: None,
        }
    }

    fn policy_denied(job_id: String) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "policy_denied",
            message: "The operation was denied by policy.",
            job_id: Some(job_id),
        }
    }

    pub(crate) fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "The current credential does not permit this action.",
        )
    }

    pub(crate) fn job_not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "job_not_found",
            "The requested job does not exist.",
        )
    }

    pub(crate) fn invalid_job_state() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "invalid_job_state",
            "The job cannot be cancelled in its current state.",
        )
    }

    pub(crate) fn internal() -> Self {
        internal()
    }
}

impl IntoResponse for CanonicalApiError {
    fn into_response(self) -> Response {
        let mut error = serde_json::json!({
            "code": self.code,
            "message": self.message,
        });
        if let Some(job_id) = self.job_id {
            error["job_id"] = Value::String(job_id);
        }
        (self.status, Json(serde_json::json!({"error": error}))).into_response()
    }
}

impl From<InvocationError> for CanonicalApiError {
    fn from(error: InvocationError) -> Self {
        let (status, code, message) = match error {
            InvocationError::UnknownAction => (
                StatusCode::NOT_FOUND,
                "unknown_action",
                "The requested durable action does not exist.",
            ),
            InvocationError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "The current role does not permit this action.",
            ),
            InvocationError::InsufficientScope => (
                StatusCode::FORBIDDEN,
                "insufficient_scope",
                "This API token's scopes do not permit this action.",
            ),
            InvocationError::AiExposureDenied => (
                StatusCode::FORBIDDEN,
                "ai_exposure_denied",
                "This action is not exposed to machine-capable ingress.",
            ),
            InvocationError::ResourceNotFound => (
                StatusCode::NOT_FOUND,
                "resource_not_found",
                "The requested resource does not exist.",
            ),
            InvocationError::ResourceKindMismatch => (
                StatusCode::CONFLICT,
                "resource_kind_mismatch",
                "The action does not apply to this resource kind.",
            ),
            InvocationError::CapabilityUnavailable => (
                StatusCode::CONFLICT,
                "capability_unavailable",
                "The requested capability is not currently available.",
            ),
            InvocationError::StaleState => (
                StatusCode::CONFLICT,
                "stale_state",
                "Resource or provider state changed during planning.",
            ),
            InvocationError::PlanningRejected => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "planning_rejected",
                "The operation could not be planned safely.",
            ),
            InvocationError::IdempotencyConflict => (
                StatusCode::CONFLICT,
                "idempotency_conflict",
                "The idempotency key belongs to different intent.",
            ),
            InvocationError::InvalidIdempotencyKey => (
                StatusCode::BAD_REQUEST,
                "invalid_idempotency_key",
                "A valid Idempotency-Key header is required.",
            ),
            InvocationError::RuntimeUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "operation_runtime_unavailable",
                "The durable operation runtime is unavailable.",
            ),
            InvocationError::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal operation error occurred.",
            ),
        };
        Self::new(status, code, message)
    }
}

pub(crate) async fn credential(
    state: &AppState,
    jar: &CookieJar,
    token: Option<AuthenticatedApiToken>,
) -> Result<CredentialContext, CanonicalApiError> {
    if let Some(token) = token {
        let user = auth::find_user_by_id(&state.db, &token.user_id)
            .await
            .map_err(|_| internal())?
            .ok_or_else(unauthorized)?;
        return Ok(CredentialContext::Bearer {
            token_id: token.token_id,
            user_id: token.user_id,
            role: user.role,
            scopes: token.scopes,
        });
    }
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or_else(unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(|_| internal())?
        .ok_or_else(unauthorized)?;
    Ok(CredentialContext::Session {
        user_id: user.id,
        role: user.role,
    })
}

fn unauthorized() -> CanonicalApiError {
    CanonicalApiError::new(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "Authentication is required.",
    )
}

fn internal() -> CanonicalApiError {
    CanonicalApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "An internal operation error occurred.",
    )
}

fn body(
    request: Result<Json<ActionRequest>, JsonRejection>,
) -> Result<ActionRequest, CanonicalApiError> {
    request.map(|Json(request)| request).map_err(|_| {
        CanonicalApiError::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "The request body is invalid.",
        )
    })
}

pub async fn plan(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path((resource_id, action)): Path<(String, String)>,
    request: Result<Json<ActionRequest>, JsonRejection>,
) -> Result<Json<Value>, CanonicalApiError> {
    let credential = credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    let request = body(request)?;
    let prepared = invocation::prepare(
        &state.db,
        &state.operation_adapters,
        &credential,
        &resource_id,
        &action,
        request.input,
    )
    .await?;
    Ok(Json(serde_json::json!({"plan": prepared.view()})))
}

pub async fn submit(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path((resource_id, action)): Path<(String, String)>,
    headers: HeaderMap,
    request: Result<Json<ActionRequest>, JsonRejection>,
) -> Result<Response, CanonicalApiError> {
    let credential = credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    let request = body(request)?;
    let key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
        .ok_or(InvocationError::InvalidIdempotencyKey)?;
    let job = invocation::submit(
        &state.db,
        &state.operation_adapters,
        &credential,
        &resource_id,
        &action,
        request.input,
        key,
    )
    .await?;
    if job.state == JobState::Rejected {
        return Err(CanonicalApiError::policy_denied(job.id));
    }
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"job": job}))).into_response())
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use anyhow::Result;
    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    use crate::operations::{
        adapters::{
            AdapterRegistry, OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome,
            StepRequest,
        },
        contracts::{CapabilityAvailability, OperationPlanV1, PlannedStepV1},
        resources::{self, ObserveResource},
    };

    struct HttpAdapter {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl OperationAdapter for HttpAdapter {
        fn key(&self) -> &'static str {
            "containers"
        }

        fn actions(&self) -> &[&'static str] {
            &[
                "container.start",
                "container.stop",
                "container.restart",
                "container.remove",
                "container.compose.apply",
            ]
        }

        async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(OperationPlanV1 {
                schema_version: 1,
                title: "Start the web container".into(),
                risk: "mutate".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: "container-running-false".into(),
                steps: vec![PlannedStepV1 {
                    kind: "execute".into(),
                    name: "Start container".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                }],
            })
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            Ok("container-running-false".into())
        }

        async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
            unreachable!()
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            unreachable!()
        }
    }

    fn request(
        uri: &str,
        session: &str,
        body: &str,
        idempotency_key: Option<&str>,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::COOKIE, format!("vt_session={session}"));
        if let Some(key) = idempotency_key {
            builder = builder.header("Idempotency-Key", key);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    fn bearer_request(
        uri: &str,
        token: &str,
        body: &str,
        idempotency_key: Option<&str>,
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        if let Some(key) = idempotency_key {
            builder = builder.header("Idempotency-Key", key);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
    }

    async fn insert_token(
        db: &sqlx::SqlitePool,
        id: &str,
        user_id: &str,
        raw: &str,
        scopes: &[&str],
    ) {
        let mut hash = Sha256::new();
        hash.update(raw.as_bytes());
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at) \
             VALUES (?, ?, 'test', ?, ?, 0)",
        )
        .bind(id)
        .bind(user_id)
        .bind(hex::encode(hash.finalize()))
        .bind(serde_json::to_string(scopes).unwrap())
        .execute(db)
        .await
        .unwrap();
    }

    async fn json(response: axum::response::Response) -> serde_json::Value {
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn canonical_plan_route_returns_a_stable_unauthorized_envelope() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(db));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/resources/missing/actions/container.start/plan")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"input":{}}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn real_router_plans_and_submits_exact_immutable_job_with_replay() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&db).await;
        let resource = resources::observe(
            &db,
            ObserveResource {
                kind: "container",
                display_name: "web",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "web",
            },
            None,
            "setup",
        )
        .await
        .unwrap();
        resources::set_capability(
            &db,
            &resource.id,
            "container.start",
            CapabilityAvailability::Available,
            None,
            None,
            "setup-capability",
        )
        .await
        .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = AdapterRegistry::new();
        registry
            .register(Arc::new(HttpAdapter {
                calls: calls.clone(),
            }))
            .unwrap();
        let mut state = crate::api::mcp::test_support::build(db.clone());
        state.operation_adapters = Arc::new(registry);
        let app = crate::api::router(state);
        let base = format!("/api/resources/{}/actions/container.start", resource.id);

        let planned = app
            .clone()
            .oneshot(request(
                &format!("{base}/plan"),
                &session,
                r#"{"input":{"requested":true}}"#,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(planned.status(), StatusCode::OK);
        let planned = json(planned).await;
        assert_eq!(planned["plan"]["action"], "container.start");
        assert_eq!(
            planned["plan"]["operation"]["title"],
            "Start the web container"
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);

        let submitted = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"requested":true}}"#,
                Some("intent-1"),
            ))
            .await
            .unwrap();
        assert_eq!(submitted.status(), StatusCode::ACCEPTED);
        let submitted = json(submitted).await;
        let job_id = submitted["job"]["id"].as_str().unwrap().to_owned();
        assert_eq!(submitted["job"]["actor"]["source"], "http_session");
        assert_eq!(submitted["job"]["plan"], planned["plan"]["operation"]);

        let replay = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"requested":true}}"#,
                Some("intent-1"),
            ))
            .await
            .unwrap();
        assert_eq!(replay.status(), StatusCode::ACCEPTED);
        assert_eq!(json(replay).await["job"]["id"], job_id);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "one advisory and one submit plan"
        );

        let cancelled = app
            .clone()
            .oneshot(request(
                &format!("/api/jobs/{job_id}/cancel"),
                &session,
                "",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(cancelled.status(), StatusCode::ACCEPTED);
        assert_eq!(json(cancelled).await["job"]["state"], "cancelled");

        let terminal_replay = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"requested":true}}"#,
                Some("intent-1"),
            ))
            .await
            .unwrap();
        assert_eq!(terminal_replay.status(), StatusCode::ACCEPTED);
        let terminal_replay = json(terminal_replay).await;
        assert_eq!(terminal_replay["job"]["id"], job_id);
        assert_eq!(terminal_replay["job"]["state"], "cancelled");

        let repeated_cancel = app
            .clone()
            .oneshot(request(
                &format!("/api/jobs/{job_id}/cancel"),
                &session,
                "",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(repeated_cancel.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(repeated_cancel).await["error"]["code"],
            "invalid_job_state"
        );

        let missing_cancel = app
            .clone()
            .oneshot(request("/api/jobs/missing/cancel", &session, "", None))
            .await
            .unwrap();
        assert_eq!(missing_cancel.status(), StatusCode::NOT_FOUND);
        assert_eq!(json(missing_cancel).await["error"]["code"], "job_not_found");

        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) \
             VALUES ('global', 'assisted', 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let approval_gated = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"approval":"required"}}"#,
                Some("approval-intent"),
            ))
            .await
            .unwrap();
        assert_eq!(approval_gated.status(), StatusCode::ACCEPTED);
        let approval_gated = json(approval_gated).await;
        assert_eq!(approval_gated["job"]["state"], "awaiting_approval");
        let approval_job_id = approval_gated["job"]["id"].as_str().unwrap();
        let approval_replay = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"approval":"required"}}"#,
                Some("approval-intent"),
            ))
            .await
            .unwrap();
        assert_eq!(approval_replay.status(), StatusCode::ACCEPTED);
        let approval_replay = json(approval_replay).await;
        assert_eq!(approval_replay["job"]["id"], approval_job_id);
        assert_eq!(approval_replay["job"]["state"], "awaiting_approval");
        let approval_cancel = app
            .clone()
            .oneshot(request(
                &format!("/api/jobs/{approval_job_id}/cancel"),
                &session,
                "",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(approval_cancel.status(), StatusCode::CONFLICT);
        assert_eq!(
            json(approval_cancel).await["error"]["code"],
            "invalid_job_state"
        );
        sqlx::query("DELETE FROM voidwatch_mode_settings")
            .execute(&db)
            .await
            .unwrap();

        let forbidden_metadata = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{},"approval":"allow"}"#,
                Some("intent-2"),
            ))
            .await
            .unwrap();
        assert_eq!(forbidden_metadata.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json(forbidden_metadata).await["error"]["code"],
            "invalid_request"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 3);

        insert_token(
            &db,
            "token-allowed",
            "u1",
            "vt_allowed",
            &["containers:restart"],
        )
        .await;
        insert_token(
            &db,
            "token-wrong-scope",
            "u1",
            "vt_wrong_scope",
            &["containers:read"],
        )
        .await;
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist \
             (id, actor_type, action, resource_type, created_at) \
             VALUES ('canonical-bearer-allow', 'api_token', 'container.start', 'container', 0)",
        )
        .execute(&db)
        .await
        .unwrap();

        let bearer_submitted = app
            .clone()
            .oneshot(bearer_request(
                &base,
                "vt_allowed",
                r#"{"input":{"requested":"bearer"}}"#,
                Some("bearer-intent-1"),
            ))
            .await
            .unwrap();
        assert_eq!(bearer_submitted.status(), StatusCode::ACCEPTED);
        let bearer_submitted = json(bearer_submitted).await;
        let bearer_job_id = bearer_submitted["job"]["id"].as_str().unwrap().to_owned();
        assert_eq!(bearer_submitted["job"]["actor"]["actor_type"], "api_token");
        assert_eq!(bearer_submitted["job"]["actor"]["id"], "token-allowed");

        let wrong_scope = app
            .clone()
            .oneshot(bearer_request(
                &base,
                "vt_wrong_scope",
                r#"{"input":{}}"#,
                Some("bearer-intent-2"),
            ))
            .await
            .unwrap();
        assert_eq!(wrong_scope.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            json(wrong_scope).await["error"]["code"],
            "insufficient_scope"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 4);

        insert_token(
            &db,
            "token-other",
            "u1",
            "vt_other",
            &["containers:restart"],
        )
        .await;
        let wrong_token_cancel = app
            .clone()
            .oneshot(bearer_request(
                &format!("/api/jobs/{bearer_job_id}/cancel"),
                "vt_other",
                "",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(wrong_token_cancel.status(), StatusCode::FORBIDDEN);
        assert_eq!(json(wrong_token_cancel).await["error"]["code"], "forbidden");

        let bearer_cancelled = app
            .clone()
            .oneshot(bearer_request(
                &format!("/api/jobs/{bearer_job_id}/cancel"),
                "vt_allowed",
                "",
                None,
            ))
            .await
            .unwrap();
        assert_eq!(bearer_cancelled.status(), StatusCode::ACCEPTED);
        assert_eq!(json(bearer_cancelled).await["job"]["state"], "cancelled");

        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) \
             VALUES ('global', 'observer', 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let denied = app
            .clone()
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"policy":"denied"}}"#,
                Some("denied-intent"),
            ))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let denied = json(denied).await;
        assert_eq!(denied["error"]["code"], "policy_denied");
        let denied_job_id = denied["error"]["job_id"].as_str().unwrap().to_owned();
        let denied_state: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = ?")
            .bind(&denied_job_id)
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(denied_state, "rejected");

        let denied_replay = app
            .oneshot(request(
                &base,
                &session,
                r#"{"input":{"policy":"denied"}}"#,
                Some("denied-intent"),
            ))
            .await
            .unwrap();
        assert_eq!(denied_replay.status(), StatusCode::FORBIDDEN);
        assert_eq!(json(denied_replay).await["error"]["job_id"], denied_job_id);
        assert_eq!(calls.load(Ordering::SeqCst), 5);
    }
}
