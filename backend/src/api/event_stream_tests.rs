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

use crate::operations::events::{PendingEvent, MAX_EVENT_FRAME_BYTES};

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
        while !output
            .split("event: durable_event")
            .nth(1)
            .is_some_and(|frame| frame.contains("\n\n"))
        {
            let chunk = body
                .next()
                .await
                .expect("stream ended before durable event")
                .expect("stream frame must be readable");
            output.push_str(std::str::from_utf8(&chunk).unwrap());
        }
    })
    .await
    .expect("complete durable event frame must arrive promptly");
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
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header(header::AUTHORIZATION, format!("Bearer {allowed}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert!(first_chunk(response).await.contains("event: stream.ready"));

        assert_eq!(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(header::AUTHORIZATION, format!("Bearer {denied}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN,
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
async fn durable_aliases_reject_query_string_api_tokens() {
    let db = test_support::setup_db().await;
    let allowed = token(&db, &["alerts:read"]).await;
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db));

    for path in ["/api/events/stream", "/api/integrations/events"] {
        let response = app
            .clone()
            .oneshot(request(&format!("{path}?token={allowed}"), Some(&session)))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
    }

    let response = app
        .oneshot(request(
            &format!("/api/integrations/events/legacy?token={allowed}"),
            Some(&session),
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn oversized_persisted_event_is_not_delivered_as_a_durable_frame() {
    let db = test_support::setup_db().await;
    let payload = serde_json::json!({
        "blob": "x".repeat(MAX_EVENT_FRAME_BYTES + 1),
    });
    sqlx::query(
        "INSERT INTO events (event_id, schema_version, event_type, occurred_at, correlation_id, payload_json)\
         VALUES ('oversized-event', 1, 'test.oversized.v1', 0, 'oversized-test', ?)",
    )
    .bind(serde_json::to_string(&payload).unwrap())
    .execute(&db)
    .await
    .unwrap();
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db));
    let response = app
        .oneshot(request("/api/events/stream?after=0", Some(&session)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let mut body = response.into_body().into_data_stream();
    let ready = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("stream must emit ready frame")
        .expect("stream must emit a frame")
        .expect("ready frame must be readable");
    assert!(String::from_utf8(ready.to_vec())
        .unwrap()
        .contains("event: stream.ready"));
    let gap = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("oversized event must emit a recovery gap")
        .expect("oversized event must emit a gap frame")
        .expect("gap frame must be readable");
    let gap = String::from_utf8(gap.to_vec()).unwrap();
    assert!(gap.contains("event: stream.gap"));
    assert!(gap.contains(r#""reason":"discontinuity""#));
    let next = tokio::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("oversized event must close after its recovery gap");
    assert!(next.is_none());
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
    assert_eq!(live.headers().get("x-voidtower-api-version").unwrap(), "1");
    assert_eq!(
        live.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
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
async fn real_router_serializes_source_owned_event_frames_exactly() {
    let db = test_support::setup_db().await;
    let sequence = append_event(&db, 7).await;
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db));

    let response = app
        .clone()
        .oneshot(request("/api/events/stream?after=0", Some(&session)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    assert_eq!(
        response.headers().get("x-voidtower-api-version").unwrap(),
        "1"
    );
    let body = through_durable_event(response).await;
    assert!(body.contains("event: stream.ready\ndata: {\"cursor\":0,\"high_water\":1}\n\n"));
    assert!(body.contains(&format!("id: {sequence}\nevent: durable_event\ndata: ")));
    let durable_data = body
        .split("event: durable_event")
        .nth(1)
        .and_then(|frame| frame.split("data: ").nth(1))
        .and_then(|data| data.split("\n\n").next())
        .expect("durable event data frame");
    let envelope: serde_json::Value = serde_json::from_str(durable_data).unwrap();
    assert_eq!(envelope["sequence"], sequence);
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["event_type"], "test.stream.v1");
    assert_eq!(envelope["correlation_id"], "stream-test");
    assert_eq!(envelope["payload"], serde_json::json!({"number": 7}));
    assert_eq!(
        envelope.as_object().unwrap().keys().collect::<Vec<_>>(),
        [
            "actor",
            "approval_id",
            "causation_id",
            "correlation_id",
            "event_id",
            "event_type",
            "job_id",
            "occurred_at",
            "payload",
            "resource_id",
            "schema_version",
            "sequence",
        ]
        .into_iter()
        .collect::<Vec<_>>()
    );

    let gap = app
        .oneshot(request("/api/events/stream?after=999999", Some(&session)))
        .await
        .unwrap();
    assert_eq!(gap.status(), StatusCode::OK);
    assert_eq!(
        gap.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
    let body = axum::body::to_bytes(gap.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        body.as_ref(),
        br#"event: stream.gap
data: {"reason":"future_cursor","requested_after":999999,"earliest_available":1,"latest_available":1}

"#
    );
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
