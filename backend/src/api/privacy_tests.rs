#![cfg(test)]

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use crate::{api::mcp::test_support, api::privacy::MAX_BODY_BYTES};

async fn user_session(db: &sqlx::SqlitePool, id: &str, username: &str, role: &str) -> String {
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at)\
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

fn json_request(method: Method, uri: &str, session: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, format!("vt_session={session}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn household_fixture() -> (
    sqlx::SqlitePool,
    axum::Router,
    String,
    String,
    String,
    String,
) {
    let db = test_support::setup_db().await;
    let owner = user_session(&db, "privacy-owner", "privacy-owner", "owner").await;
    let member = user_session(&db, "privacy-member", "privacy-member", "member").await;
    let outsider = user_session(&db, "privacy-outsider", "privacy-outsider", "member").await;
    let app = crate::api::router(test_support::build(db.clone()));

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/households",
            &owner,
            serde_json::json!({"name":"Privacy Home"}),
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
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/members"),
            &owner,
            serde_json::json!({"user_id":"privacy-member","role":"member"}),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query(
        "INSERT INTO resources \
         (id, kind, display_name, lifecycle_state, revision, created_at, updated_at) \
         VALUES ('privacy-resource', 'service', 'Privacy resource', 'active', 0, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/grants"),
            &owner,
            serde_json::json!({
                "user_id":"privacy-member",
                "resource_id":"privacy-resource",
                "permission":"read"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    (db, app, household_id, owner, member, outsider)
}

#[tokio::test]
async fn privacy_export_requires_grant_consent_and_explicit_cloud_opt_in() {
    let (_db, app, household_id, owner, member, outsider) = household_fixture().await;

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"local",
                "minimization":"metadata",
                "idempotency_key":"export-before-consent"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "consent_required"
    );

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &member,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let _consent = json_body(response).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/households/{household_id}/consents"))
                .header(header::COOKIE, format!("vt_session={member}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/households/{household_id}/consents"))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        json_body(response).await["consents"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"local",
                "minimization":"metadata",
                "idempotency_key":"export-local"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let local = json_body(response).await;
    assert_eq!(local["request"]["state"], "queued");
    assert_eq!(local["request"]["destination"], "local");

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"cloud",
                "minimization":"metadata",
                "idempotency_key":"export-cloud-without-opt-in"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "cloud_export_disabled"
    );

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &member,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"cloud",
                "ai_cloud_export":true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cloud_consent = json_body(response).await;
    let cloud_consent_id = cloud_consent["consent"]["id"].as_str().unwrap().to_owned();

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"cloud",
                "minimization":"metadata",
                "idempotency_key":"export-cloud-opted-in"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!(
                    "/api/households/{household_id}/consents/{cloud_consent_id}"
                ))
                .header(header::COOKIE, format!("vt_session={member}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"cloud",
                "minimization":"metadata",
                "idempotency_key":"export-cloud-after-withdrawal"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "cloud_export_disabled"
    );

    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &outsider,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"local",
                "minimization":"metadata",
                "idempotency_key":"outsider"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(response).await["error"]["code"], "forbidden");
    let _ = owner;
}

#[tokio::test]
async fn privacy_requests_are_idempotent_and_deletion_status_is_durable() {
    let (db, app, household_id, owner, _member, _outsider) = household_fixture().await;

    let body = serde_json::json!({
        "kind":"deletion",
        "user_id":"privacy-member",
        "purpose":"privacy_deletion",
        "domain":"identity",
        "destination":"local",
        "minimization":"records",
        "idempotency_key":"delete-member"
    });
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            body.clone(),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let first = json_body(response).await;
    let request_id = first["request"]["id"].as_str().unwrap().to_owned();

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let replay = json_body(response).await;
    assert_eq!(replay["request"]["id"], request_id);
    assert_eq!(replay["replayed"], true);

    sqlx::query(
        "UPDATE privacy_requests SET state = 'failed', failure_code = 'retention_blocked',\
         failure_message = 'Deletion requires operator review', updated_at = 2 WHERE id = ?",
    )
    .bind(&request_id)
    .execute(&db)
    .await
    .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/privacy-requests/{request_id}"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let status = json_body(response).await;
    assert_eq!(status["request"]["state"], "failed");
    assert_eq!(status["request"]["failure"]["code"], "retention_blocked");
    assert_eq!(
        status["request"]["failure"]["message"],
        "Privacy operation failed and requires operator review."
    );
}

#[tokio::test]
async fn privacy_request_status_requires_active_subject_membership() {
    let (db, app, household_id, owner, _member, _outsider) = household_fixture().await;
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "destination":"local",
                "minimization":"records",
                "idempotency_key":"status-after-revocation"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let request_id = json_body(response).await["request"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    sqlx::query(
        "DELETE FROM resource_grants WHERE household_id = ? AND user_id = 'privacy-member'",
    )
    .bind(&household_id)
    .execute(&db)
    .await
    .unwrap();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/privacy-requests/{request_id}"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    sqlx::query(
        "UPDATE household_memberships
         SET status = 'revoked', revoked_at = 100, updated_at = 100
         WHERE household_id = ? AND user_id = 'privacy-member'",
    )
    .bind(&household_id)
    .execute(&db)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/privacy-requests/{request_id}"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn processing_privacy_request_cancellation_is_durable_and_worker_visible() {
    let (db, app, household_id, owner, _member, _outsider) = household_fixture().await;
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"configuration",
                "destination":"local",
                "minimization":"records",
                "idempotency_key":"cancel-processing"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let request_id = json_body(response).await["request"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "UPDATE privacy_requests SET state = 'processing', lease_owner = 'privacy-worker',
         lease_expires_at = 9999999999 WHERE id = ?",
    )
    .bind(&request_id)
    .execute(&db)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/households/{household_id}/privacy-requests/{request_id}/cancel"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["cancel_requested"], true);
    let (state, cancel_requested): (String, i64) =
        sqlx::query_as("SELECT state, cancel_requested FROM privacy_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(state, "processing");
    assert_eq!(cancel_requested, 1);
}

#[tokio::test]
async fn privacy_contract_rejects_unknown_fields_and_invalid_expiry_without_database_details() {
    let (_db, app, household_id, _owner, member, _outsider) = household_fixture().await;
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &member,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "unexpected":"must fail"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = json_body(response).await;
    assert_eq!(body["error"]["code"], "bad_request");
    assert!(!body.to_string().contains("SQL"));

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &member,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "expires_at":0
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"]["code"], "bad_request");

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &member,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "destination":null
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"identity",
                "minimization":"records",
                "idempotency_key":":invalid-leading-character"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn privacy_contract_rejects_changed_intent_expired_retention_non_json_and_event_identity_leaks(
) {
    let (db, app, household_id, owner, member, _outsider) = household_fixture().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/households/{household_id}/consents"))
                .header(header::COOKIE, format!("vt_session={member}"))
                .body(Body::from(
                    serde_json::json!({
                        "user_id":"privacy-member",
                        "purpose":"privacy_export",
                        "domain":"inventory"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &member,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let consent_id = json_body(response).await["consent"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query("UPDATE household_consents SET created_at = 0, retention_until = 0 WHERE id = ?")
        .bind(&consent_id)
        .execute(&db)
        .await
        .unwrap();

    let export_response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &member,
            serde_json::json!({
                "kind":"export",
                "user_id":"privacy-member",
                "purpose":"privacy_export",
                "domain":"inventory",
                "resource_id":"privacy-resource",
                "minimization":"metadata",
                "idempotency_key":"retention-expired"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(export_response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        json_body(export_response).await["error"]["code"],
        "consent_required"
    );

    let deletion = serde_json::json!({
        "kind":"deletion",
        "user_id":"privacy-member",
        "purpose":"privacy_deletion",
        "domain":"identity",
        "minimization":"records",
        "idempotency_key":"same-intent-key"
    });
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            deletion,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let changed_intent = serde_json::json!({
        "kind":"deletion",
        "user_id":"privacy-member",
        "purpose":"privacy_deletion",
        "domain":"telemetry",
        "minimization":"records",
        "idempotency_key":"same-intent-key"
    });
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            changed_intent,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(response).await["error"]["code"], "conflict");

    let events: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT event_type, payload_json, resource_id, actor_id FROM events
         WHERE event_type IN ('consent.granted.v1', 'privacy.requested.v1')
         ORDER BY sequence",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    assert!(!events.is_empty());
    for (_event_type, payload, resource_id, actor_id) in events {
        assert!(!payload.contains("privacy-owner"));
        assert!(!payload.contains("privacy-member"));
        assert!(!payload.contains(&household_id));
        assert!(!payload.contains("privacy-resource"));
        assert!(resource_id.is_none());
        assert!(actor_id.is_none());
    }
}

#[tokio::test]
async fn resource_scoped_deletion_is_rejected_outside_inventory() {
    let (_db, app, household_id, owner, _member, _outsider) = household_fixture().await;
    let response = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"configuration",
                "resource_id":"privacy-resource",
                "minimization":"records",
                "idempotency_key":"scoped-configuration-delete"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn retry_rejects_a_request_that_exhausted_the_worker_budget() {
    let (db, app, household_id, owner, _member, _outsider) = household_fixture().await;
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"identity",
                "minimization":"records",
                "idempotency_key":"retry-exhausted"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let request_id = json_body(response).await["request"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "UPDATE privacy_requests
         SET state = 'failed', failure_code = 'authorization_revoked', attempt_count = 3
         WHERE id = ?",
    )
    .bind(&request_id)
    .execute(&db)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!(
                    "/api/households/{household_id}/privacy-requests/{request_id}/retry"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(json_body(response).await["error"]["code"], "conflict");
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT state FROM privacy_requests WHERE id = ?")
            .bind(&request_id)
            .fetch_one(&db)
            .await
            .unwrap(),
        "failed"
    );
}

#[tokio::test]
async fn privacy_contract_reports_versioned_consent_pagination_and_accepted_requests() {
    let (_db, app, household_id, owner, _member, _outsider) = household_fixture().await;

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"identity",
                "destination":"local",
                "minimization":"records",
                "idempotency_key":"versioned-request"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let request = json_body(response).await;
    assert_eq!(request["schema_version"], 1);
    assert_eq!(request["request"]["schema_version"], 1);
    assert_eq!(request["replayed"], false);

    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/consents"),
            &owner,
            serde_json::json!({
                "user_id":"privacy-member",
                "purpose":"support",
                "domain":"identity"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/households/{household_id}/consents?limit=1"))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let list = json_body(response).await;
    assert_eq!(list["schema_version"], 1);
    assert_eq!(list["consents"].as_array().unwrap().len(), 1);
    assert_eq!(list["has_more"], false);
    assert_eq!(list["next_cursor"], serde_json::Value::Null);
}

#[tokio::test]
async fn consent_query_validation_preserves_authentication_first_behavior() {
    let (_db, app, household_id, owner, _member, _outsider) = household_fixture().await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/consents?limit=not-a-number"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/households/{household_id}/privacy-requests"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(vec![b'x'; MAX_BODY_BYTES + 1]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/households/{household_id}/privacy-requests"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::from(vec![b'x'; MAX_BODY_BYTES + 1]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/consents?limit=not-a-number"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/consents?cursor=1000000000"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn privacy_status_redacts_unknown_failure_codes() {
    let (db, app, household_id, owner, _member, _outsider) = household_fixture().await;
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &format!("/api/households/{household_id}/privacy-requests"),
            &owner,
            serde_json::json!({
                "kind":"deletion",
                "user_id":"privacy-member",
                "purpose":"privacy_deletion",
                "domain":"identity",
                "minimization":"records",
                "idempotency_key":"failure-redaction"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let request_id = json_body(response).await["request"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    sqlx::query(
        "UPDATE privacy_requests SET state = 'failed', failure_code = 'provider_secret=redact-me' WHERE id = ?",
    )
    .bind(&request_id)
    .execute(&db)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/households/{household_id}/privacy-requests/{request_id}"
                ))
                .header(header::COOKIE, format!("vt_session={owner}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(
        body["request"]["failure"]["code"],
        "privacy_operation_failed"
    );
    assert!(!body.to_string().contains("provider_secret=redact-me"));
}
