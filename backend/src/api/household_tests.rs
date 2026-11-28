#![cfg(test)]

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

use crate::api::mcp::test_support;

async fn user_session(db: &sqlx::SqlitePool, id: &str, username: &str, role: &str) -> String {
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
         VALUES (?, ?, 'x', ?, 0, 0)",
    )
    .bind(id)
    .bind(username)
    .bind(role)
    .execute(db)
    .await
    .unwrap();
    crate::auth::create_session(db, id, None, None)
        .await
        .unwrap()
        .id
}

fn session_request(method: Method, uri: &str, session: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("vt_session={session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn raw_session_request(
    method: Method,
    uri: &str,
    session: &str,
    body: impl Into<Body>,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("vt_session={session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.into())
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn insert_resource(db: &sqlx::SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO resources \
         (id, kind, display_name, lifecycle_state, revision, created_at, updated_at) \
         VALUES (?, 'service', ?, 'active', 0, 0, 0)",
    )
    .bind(id)
    .bind(id)
    .execute(db)
    .await
    .unwrap();
}

#[tokio::test]
async fn household_membership_grants_and_revocation_are_enforced_at_real_routes() {
    let db = test_support::setup_db().await;
    let owner = user_session(&db, "owner-1", "owner-one", "owner").await;
    let member = user_session(&db, "member-1", "member-one", "member").await;
    let outsider = user_session(&db, "outsider-1", "outsider-one", "member").await;
    let app = crate::api::router(test_support::build(db.clone()));

    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            "/api/households",
            &owner,
            serde_json::json!({"name":"Home"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let household = json_body(response).await;
    let household_id = household["household"]["id"].as_str().unwrap().to_owned();

    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/members"),
            &owner,
            serde_json::json!({"user_id":"member-1","role":"member"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    insert_resource(&db, "resource-a").await;
    insert_resource(&db, "resource-b").await;
    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/grants"),
            &owner,
            serde_json::json!({
                "user_id":"member-1",
                "resource_id":"resource-a",
                "permission":"read"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let raw_token = "vt_household_member_token";
    let mut token_hash = Sha256::new();
    token_hash.update(raw_token.as_bytes());
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
         VALUES ('member-token-1', 'member-1', 'member', ?, '[\"metrics:read\"]', 0)",
    )
    .bind(hex::encode(token_hash.finalize()))
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO jobs
         (id, action, resource_id, resource_revision, actor_type, actor_id, actor_source,
          ingress, input_json, request_digest, plan_json, plan_digest, external_fingerprint,
          state, idempotency_scope, idempotency_key, concurrency_key, retry_class,
          recovery_class, submitted_at, updated_at)
         VALUES ('member-job-1', 'test.action', 'resource-a', 0, 'human', 'member-1',
                 'http_session', 'http_session', '{}', 'request', '{}', 'plan', 'fingerprint',
                 'queued', 'scope', 'key', 'resource-a', 'never', 'reconcile', 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    let allowed = app
        .clone()
        .oneshot(session_request(
            Method::GET,
            "/api/resources/resource-a",
            &member,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);

    let grant_revoked = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!(
                    "/api/households/{household_id}/grants/resource-a/member-1"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(grant_revoked.status(), StatusCode::OK);

    let denied_after_grant_revocation = app
        .clone()
        .oneshot(session_request(
            Method::GET,
            "/api/resources/resource-a",
            &member,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(
        denied_after_grant_revocation.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        json_body(denied_after_grant_revocation).await["error"]["code"],
        "resource_grant_required"
    );

    let denied = app
        .clone()
        .oneshot(session_request(
            Method::GET,
            "/api/resources/resource-b",
            &member,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(denied).await["error"]["code"],
        "resource_grant_required"
    );

    let cross_household = app
        .clone()
        .oneshot(session_request(
            Method::GET,
            &format!("/api/households/{household_id}/members"),
            &outsider,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(cross_household.status(), StatusCode::FORBIDDEN);

    let revoked = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/households/{household_id}/members/member-1"))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::OK);

    let expired_session = app
        .oneshot(session_request(
            Method::GET,
            "/api/resources/resource-a",
            &member,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(expired_session.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM api_tokens WHERE user_id = 'member-1'")
            .fetch_one(&db)
            .await
            .unwrap(),
        0
    );
    let (job_state, error_code): (String, Option<String>) =
        sqlx::query_as("SELECT state, error_code FROM jobs WHERE id = 'member-job-1'")
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(job_state, "cancelled");
    assert_eq!(error_code.as_deref(), Some("household_membership_revoked"));
}

#[tokio::test]
async fn household_operate_grant_authorizes_only_the_declared_grant_scoped_action() {
    let db = test_support::setup_db().await;
    let owner = user_session(&db, "owner-h5", "owner-h5", "owner").await;
    let member = user_session(&db, "member-h5", "member-h5", "member").await;
    let operator = user_session(&db, "operator-h5", "operator-h5", "operator").await;
    let app = crate::api::router(test_support::build(db.clone()));

    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            "/api/households",
            &owner,
            serde_json::json!({"name":"Home"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let household_id = json_body(response).await["household"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/members"),
            &owner,
            serde_json::json!({"user_id":"member-h5","role":"member"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // A canonical automation resource with an available capability, exactly the shape the
    // compatibility run route publishes before a granted actor can plan against it.
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, description, command, schedule, enabled,
             timeout_secs, last_run_at, last_status, last_exit_code, created_at, updated_at)
         VALUES ('auto-1', 'Evening scene', NULL, 'true', NULL, 1, 60, NULL, NULL, NULL, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resources
         (id, kind, display_name, lifecycle_state, revision, created_at, updated_at)
         VALUES ('resource-auto', 'automation_job', 'Evening scene', 'active', 0, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_aliases
         (resource_id, namespace, scope_key, value, created_at, last_seen_at)
         VALUES ('resource-auto', 'automation.job', 'local', 'auto-1', 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_capabilities
         (resource_id, action, availability, reason_code, detail, schema_version, observed_at)
         VALUES ('resource-auto', 'automation.run', 'available', NULL, NULL, 1, 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO resources
         (id, kind, display_name, lifecycle_state, revision, created_at, updated_at)
         VALUES ('resource-container', 'container', 'media', 'active', 0, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_capabilities
         (resource_id, action, availability, reason_code, detail, schema_version, observed_at)
         VALUES ('resource-container', 'container.restart', 'available', NULL, NULL, 1, 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    async fn plan_as(
        app: axum::Router,
        session: &str,
        resource: &str,
        action: &str,
    ) -> axum::response::Response {
        app.oneshot(session_request(
            Method::POST,
            &format!("/api/resources/{resource}/actions/{action}/plan"),
            session,
            serde_json::json!({"input":{}}),
        ))
        .await
        .unwrap()
    }

    async fn submit_as(
        app: axum::Router,
        session: &str,
        idempotency_key: &str,
    ) -> axum::response::Response {
        app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/resources/resource-auto/actions/automation.run")
                .header(header::COOKIE, format!("vt_session={session}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", idempotency_key)
                .body(Body::from(serde_json::json!({"input":{}}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
    }

    // No grant yet: the grant-scoped action is refused before planning, with the household code.
    let refused = plan_as(app.clone(), &member, "resource-auto", "automation.run").await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(refused).await["error"]["code"],
        "resource_grant_required"
    );

    // A read grant is not enough for a mutating action.
    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/grants"),
            &owner,
            serde_json::json!({
                "user_id":"member-h5",
                "resource_id":"resource-auto",
                "permission":"read"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let read_only = plan_as(app.clone(), &member, "resource-auto", "automation.run").await;
    assert_eq!(read_only.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(read_only).await["error"]["code"],
        "resource_grant_required"
    );

    // An operate grant authorizes exactly the declared grant-scoped action.
    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/grants"),
            &owner,
            serde_json::json!({
                "user_id":"member-h5",
                "resource_id":"resource-auto",
                "permission":"operate"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let granted = plan_as(app.clone(), &member, "resource-auto", "automation.run").await;
    assert_eq!(granted.status(), StatusCode::OK);
    let granted_body = json_body(granted).await;
    assert_eq!(granted_body["action"], "automation.run");
    assert_eq!(granted_body["resource_id"], "resource-auto");
    assert_eq!(granted_body["schema_version"], 1);

    // The household grant change itself is on the audit trail with canonical identifiers only.
    let (grant_action, grant_actor, grant_resource, grant_details): (
        String,
        String,
        String,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT action, user_id, resource_id, details FROM audit_log
         WHERE action = 'household.grant' ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(grant_action, "household.grant");
    assert_eq!(grant_actor, "owner-h5");
    assert_eq!(grant_resource, "resource-auto");
    assert_eq!(
        grant_details.as_deref(),
        Some("member=member-h5;permission=operate")
    );

    // A member reads only its own grant rows, which is what lets the web page gate the
    // grant-scoped action on the permission it actually holds.
    let mine = app
        .clone()
        .oneshot(session_request(
            Method::GET,
            &format!("/api/households/{household_id}/grants/me"),
            &member,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(mine.status(), StatusCode::OK);
    let mine_body = json_body(mine).await;
    assert_eq!(mine_body["grants"].as_array().unwrap().len(), 1);
    assert_eq!(mine_body["grants"][0]["permission"], "operate");
    assert_eq!(mine_body["grants"][0]["user_id"], "member-h5");

    let outsider = user_session(&db, "outsider-h5", "outsider-h5", "member").await;
    let outsider_read = app
        .clone()
        .oneshot(session_request(
            Method::GET,
            &format!("/api/households/{household_id}/grants/me"),
            &outsider,
            Value::Null,
        ))
        .await
        .unwrap();
    assert_eq!(outsider_read.status(), StatusCode::FORBIDDEN);

    // The grant never widens an action the registry did not mark grant-scoped.
    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/grants"),
            &owner,
            serde_json::json!({
                "user_id":"member-h5",
                "resource_id":"resource-container",
                "permission":"operate"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let widened = plan_as(
        app.clone(),
        &member,
        "resource-container",
        "container.restart",
    )
    .await;
    assert_eq!(widened.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(widened).await["error"]["code"], "forbidden");

    // Submit keeps the same authority and is idempotent for the granted actor.
    let submitted = submit_as(app.clone(), &member, "h5-granted-run-1").await;
    assert_eq!(submitted.status(), StatusCode::ACCEPTED);
    let job_id = json_body(submitted).await["job"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let replayed = submit_as(app.clone(), &member, "h5-granted-run-1").await;
    assert_eq!(replayed.status(), StatusCode::ACCEPTED);
    assert_eq!(json_body(replayed).await["job"]["id"], job_id.as_str());
    let (job_actor, job_action): (String, String) =
        sqlx::query_as("SELECT actor_id, action FROM jobs WHERE id = ?")
            .bind(&job_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(job_actor, "member-h5");
    assert_eq!(job_action, "automation.run");

    // An operator keeps the existing boundary and needs no household grant.
    let operator_plan = plan_as(app.clone(), &operator, "resource-auto", "automation.run").await;
    assert_eq!(operator_plan.status(), StatusCode::OK);

    // Revocation closes the grant-scoped action again.
    let revoked = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!(
                    "/api/households/{household_id}/grants/resource-auto/member-h5"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::OK);
    let (revoked_job_state, revoked_job_code): (String, Option<String>) =
        sqlx::query_as("SELECT state, error_code FROM jobs WHERE id = ?")
            .bind(&job_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(revoked_job_state, "cancelled");
    assert_eq!(revoked_job_code.as_deref(), Some("household_grant_revoked"));

    let after_revoke = plan_as(app.clone(), &member, "resource-auto", "automation.run").await;
    assert_eq!(after_revoke.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(after_revoke).await["error"]["code"],
        "resource_grant_required"
    );
    let revoked_audit: (String, String, Option<String>) = sqlx::query_as(
        "SELECT action, resource_id, details FROM audit_log
         WHERE action = 'household.grant.revoke' ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(revoked_audit.0, "household.grant.revoke");
    assert_eq!(revoked_audit.1, "resource-auto");
    assert_eq!(revoked_audit.2.as_deref(), Some("member=member-h5"));
}

#[tokio::test]
async fn household_grant_validation_is_bounded_and_does_not_leak_database_details() {
    let db = test_support::setup_db().await;
    let owner = user_session(&db, "owner-2", "owner-two", "owner").await;
    let app = crate::api::router(test_support::build(db));
    let response = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            "/api/households",
            &owner,
            serde_json::json!({"name":"Home"}),
        ))
        .await
        .unwrap();
    let household_id = json_body(response).await["household"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let invalid = app
        .oneshot(session_request(
            Method::POST,
            &format!("/api/households/{household_id}/grants"),
            &owner,
            serde_json::json!({
                "user_id":"missing",
                "resource_id":"missing",
                "permission":"root"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let body = json_body(invalid).await;
    assert_eq!(body["error"]["code"], "bad_request");
    assert!(!body.to_string().contains("SQL"));
}

#[tokio::test]
async fn household_mutations_authenticate_before_body_parsing_and_bound_json() {
    let db = test_support::setup_db().await;
    let owner = user_session(&db, "owner-3", "owner-three", "owner").await;
    let app = crate::api::router(test_support::build(db));

    for path in [
        "/api/households",
        "/api/households/missing-household/members",
        "/api/households/missing-household/grants",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }

    let oversized_body = format!(r#"{{"name":"{}"}}"#, "x".repeat(64 * 1024));
    for path in [
        "/api/households",
        "/api/households/missing-household/members",
        "/api/households/missing-household/grants",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(path)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(oversized_body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
    }

    let created = app
        .clone()
        .oneshot(session_request(
            Method::POST,
            "/api/households",
            &owner,
            serde_json::json!({"name":"Home"}),
        ))
        .await
        .unwrap();
    let household_id = json_body(created).await["household"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    for path in [
        "/api/households",
        &format!("/api/households/{household_id}/members"),
        &format!("/api/households/{household_id}/grants"),
    ] {
        let response = app
            .clone()
            .oneshot(raw_session_request(Method::POST, path, &owner, "{"))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{path}"
        );
    }

    let oversized = app
        .oneshot(raw_session_request(
            Method::POST,
            "/api/households",
            &owner,
            format!(r#"{{"name":"{}"}}"#, "x".repeat(64 * 1024)),
        ))
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
