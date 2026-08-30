#![cfg(test)]

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::time::Duration;
use tower::ServiceExt;

use crate::operations::events::PendingEvent;

use super::mcp::test_support;

fn request(uri: &str, session: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("vt_session={session}"));
    }
    builder.body(Body::empty()).unwrap()
}

async fn append_event(db: &SqlitePool, number: i64) -> i64 {
    let mut transaction = db.begin().await.unwrap();
    let event = crate::operations::events::append(
        &mut transaction,
        PendingEvent {
            event_type: "test.stream.v1".into(),
            actor: None,
            resource_id: None,
            job_id: None,
            approval_id: None,
            correlation_id: "stream-test".into(),
            causation_id: None,
            payload: serde_json::json!({"number": number}),
        },
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    event.sequence
}

async fn token(db: &SqlitePool, scopes: &[&str]) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
         VALUES (?, ?, 'x', 'owner', 0, 0)",
    )
    .bind(&user_id)
    .bind(format!("stream-token-{user_id}"))
    .execute(db)
    .await
    .unwrap();

    let raw = format!("vt_stream_{}", uuid::Uuid::new_v4().simple());
    let mut hash = Sha256::new();
    hash.update(raw.as_bytes());
    sqlx::query(
        "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at) \
         VALUES (?, ?, 'stream-test', ?, ?, 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(hex::encode(hash.finalize()))
    .bind(serde_json::to_string(scopes).unwrap())
    .execute(db)
    .await
    .unwrap();
    raw
}

async fn first_chunk(response: axum::response::Response) -> String {
    let mut body = response.into_body().into_data_stream();
    let chunk = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("stream must emit promptly")
        .expect("stream must have a frame")
        .expect("stream frame must be readable");
    String::from_utf8(chunk.to_vec()).unwrap()
}

async fn through_durable_event(response: axum::response::Response) -> String {
    let mut body = response.into_body().into_data_stream();
    let mut output = String::new();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !output.contains("event: durable_event") {
            let chunk = body
                .next()
                .await
                .expect("stream ended before durable event")
                .expect("stream frame must be readable");
            output.push_str(std::str::from_utf8(&chunk).unwrap());
        }
    })
    .await
    .expect("durable event must arrive promptly");
    output
}

#[tokio::test]
async fn durable_stream_uses_exact_operator_session_allowlist() {
    let db = test_support::setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["owner", "admin", "operator"] {
        let session = test_support::user_with_role_session(&db, role).await;
        for path in ["/api/events/stream", "/api/integrations/events"] {
            let response = app
                .clone()
                .oneshot(request(path, Some(&session)))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{role} {path}");
            assert!(first_chunk(response).await.contains("event: stream.ready"));
        }
    }

    for role in ["viewer", "guest", "demo", "member", "future-role"] {
        let session = test_support::user_with_role_session(&db, role).await;
        let response = app
            .clone()
            .oneshot(request("/api/events/stream", Some(&session)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{role}");
    }
    assert_eq!(
        app.oneshot(request("/api/events/stream", None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn token_scope_and_emergency_disable_apply_to_both_durable_aliases() {
    let db = test_support::setup_db().await;
    let allowed = token(&db, &["alerts:read"]).await;
    let denied = token(&db, &["metrics:read"]).await;
    let app = crate::api::router(test_support::build(db.clone()));

    for path in ["/api/events/stream", "/api/integrations/events"] {
        let response = app
            .clone()
            .oneshot(request(&format!("{path}?token={allowed}"), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert!(first_chunk(response).await.contains("event: stream.ready"));

        assert_eq!(
            app.clone()
                .oneshot(request(&format!("{path}?token={denied}"), None))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED,
            "{path} must reject a missing scope"
        );
    }

    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES \
         ('odysseus.emergency_disabled', 'true', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let recovery_session = test_support::user_with_role_session(&db, "operator").await;
    for path in ["/api/events/stream", "/api/integrations/events"] {
        assert_eq!(
            app.clone()
                .oneshot(request(&format!("{path}?token={allowed}"), None))
                .await
                .unwrap()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{path} must honor emergency disable"
        );
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(header::AUTHORIZATION, format!("Bearer {allowed}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{path} Bearer middleware must not bypass emergency disable"
        );
        assert_eq!(
            app.clone()
                .oneshot(request(path, Some(&recovery_session)))
                .await
                .unwrap()
                .status(),
            StatusCode::OK,
            "{path} must preserve local session recovery"
        );
    }
    assert_eq!(
        app.clone()
            .oneshot(request(
                "/api/integrations/events/legacy",
                Some(&recovery_session),
            ))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.oneshot(
            Request::builder()
                .uri("/api/integrations/events/legacy")
                .header(header::AUTHORIZATION, format!("Bearer {allowed}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn no_cursor_is_live_only_and_explicit_cursor_replays_identically_on_aliases() {
    let db = test_support::setup_db().await;
    let sequence = append_event(&db, 1).await;
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db));

    let live = app
        .clone()
        .oneshot(request("/api/events/stream", Some(&session)))
        .await
        .unwrap();
    let live = first_chunk(live).await;
    assert!(live.contains(&format!(r#""cursor":{sequence}"#)));
    assert!(!live.contains("event: durable_event"));

    let canonical = app
        .clone()
        .oneshot(request("/api/events/stream?after=0", Some(&session)))
        .await
        .unwrap();
    let alias = app
        .oneshot(request("/api/integrations/events?after=0", Some(&session)))
        .await
        .unwrap();
    let canonical = through_durable_event(canonical).await;
    let alias = through_durable_event(alias).await;
    for output in [&canonical, &alias] {
        assert!(output.contains("event: stream.ready"));
        assert!(output.contains(&format!("id: {sequence}")));
        assert!(output.contains("event: durable_event"));
        assert!(output.contains(r#""event_type":"test.stream.v1""#));
        assert!(output.contains(r#""number":1"#));
    }
    assert_eq!(canonical, alias);
}

#[tokio::test]
async fn last_event_id_is_monotonic_and_gap_frames_close_without_durable_delivery() {
    let db = test_support::setup_db().await;
    append_event(&db, 1).await;
    let second = append_event(&db, 2).await;
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/events/stream?after=0")
                .header(header::COOKIE, format!("vt_session={session}"))
                .header("last-event-id", "1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let output = through_durable_event(response).await;
    assert!(output.contains(r#""cursor":1"#));
    assert!(output.contains(&format!("id: {second}")));
    assert!(!output.contains("id: 1\n"));

    let response = app
        .oneshot(request("/api/events/stream?after=999999", Some(&session)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let output = std::str::from_utf8(&body).unwrap();
    assert!(output.contains("event: stream.gap"));
    assert!(output.contains(r#""reason":"future_cursor""#));
    assert!(!output.contains("event: stream.ready"));
    assert!(!output.contains("event: durable_event"));
}
