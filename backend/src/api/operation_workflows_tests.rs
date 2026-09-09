#![cfg(test)]

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use serde_json::Value;
use sqlx::SqlitePool;
use tower::ServiceExt;

use crate::operations::{
    contracts::{ActorRef, ActorType, OperationPlanV1, PlannedStepV1},
    jobs::{self, SubmissionPolicy, SubmitJob},
    resources::{self, ObserveResource},
    unix_now,
};

use super::mcp::test_support;

fn request(method: Method, uri: &str, session: Option<&str>, body: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(session) = session {
        builder = builder.header(header::COOKIE, format!("vt_session={session}"));
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

async fn json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn automation_run_request(
    session: &str,
    automation_id: &str,
    idempotency_key: &str,
) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(format!("/api/automation/{automation_id}/run"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, format!("vt_session={session}"))
        .header("Idempotency-Key", idempotency_key)
        .body(Body::empty())
        .unwrap()
}

async fn submit_job(
    db: &SqlitePool,
    suffix: &str,
    approval: bool,
) -> crate::operations::contracts::JobSummaryV1 {
    let resource = resources::observe(
        db,
        ObserveResource {
            kind: "container",
            display_name: &format!("web-{suffix}"),
            node_id: None,
            provider: Some("docker"),
            namespace: "test.jobs",
            scope_key: "local",
            alias: suffix,
        },
        None,
        "test",
    )
    .await
    .unwrap();
    jobs::submit(
        db,
        SubmitJob {
            action: "container.restart".into(),
            resource,
            actor: ActorRef {
                actor_type: ActorType::Human,
                id: Some("submitter".into()),
                source: Some("test".into()),
            },
            ingress: "http".into(),
            input: serde_json::json!({"container": suffix}),
            plan: OperationPlanV1 {
                schema_version: 1,
                title: format!("Restart web-{suffix}"),
                risk: "mutate".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: format!("fingerprint-{suffix}"),
                steps: vec![PlannedStepV1 {
                    kind: "container.restart".into(),
                    name: "Restart container".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                }],
            },
            idempotency_scope: "test".into(),
            idempotency_key: format!("job-{suffix}"),
            concurrency_key: format!("container-{suffix}"),
            retry_class: "never".into(),
            recovery_class: "reconcile".into(),
            policy: if approval {
                SubmissionPolicy::RequireApproval {
                    requirement: "Administrator approval".into(),
                    reason: "Provider state changes".into(),
                    expires_at: unix_now() + 900,
                }
            } else {
                SubmissionPolicy::Allow
            },
        },
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn real_router_jobs_use_exact_operator_allowlist_and_complete_shape() {
    let db = test_support::setup_db().await;
    let job = submit_job(&db, "jobs", false).await;
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["owner", "admin", "operator"] {
        let session = test_support::user_with_role_session(&db, role).await;
        let list = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/jobs?limit=50",
                Some(&session),
                "",
            ))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK, "{role} must list jobs");
        let list = json(list).await;
        assert_eq!(list["jobs"][0]["id"], job.id);
        assert_eq!(list["jobs"][0]["resource"]["display_name"], "web-jobs");
        assert_eq!(list["jobs"][0]["actor"]["actor_type"], "human");
        assert_eq!(list["jobs"][0]["plan"]["schema_version"], 1);
        assert!(list["jobs"][0]["submitted_at"].is_number());

        let detail = app
            .clone()
            .oneshot(request(
                Method::GET,
                &format!("/api/jobs/{}", job.id),
                Some(&session),
                "",
            ))
            .await
            .unwrap();
        assert_eq!(detail.status(), StatusCode::OK, "{role} must inspect jobs");
        assert_eq!(
            json(detail).await["job"]["plan"]["title"],
            "Restart web-jobs"
        );
    }

    for role in ["viewer", "guest", "demo", "member", "future-role"] {
        let session = test_support::user_with_role_session(&db, role).await;
        let response = app
            .clone()
            .oneshot(request(Method::GET, "/api/jobs", Some(&session), ""))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{role} must not list jobs"
        );
    }
    let response = app
        .oneshot(request(Method::GET, "/api/jobs", None, ""))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn real_router_approvals_use_admin_allowlist_and_reject_exact_record() {
    let db = test_support::setup_db().await;
    let selected = submit_job(&db, "selected", true).await;
    let untouched = submit_job(&db, "untouched", true).await;
    let selected_approval = selected.approval_id.clone().unwrap();
    let untouched_approval = untouched.approval_id.clone().unwrap();
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["owner", "admin"] {
        let session = test_support::user_with_role_session(&db, role).await;
        let list = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/approvals?status=pending",
                Some(&session),
                "",
            ))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK, "{role} must list approvals");
        let list = json(list).await;
        assert_eq!(list["approvals"].as_array().unwrap().len(), 2);
        assert!(list["approvals"][0]["expires_at"].is_number());
        assert!(list["approvals"][0]["job_id"].is_string());
    }

    for role in [
        "operator",
        "viewer",
        "guest",
        "demo",
        "member",
        "future-role",
    ] {
        let session = test_support::user_with_role_session(&db, role).await;
        let response = app
            .clone()
            .oneshot(request(Method::GET, "/api/approvals", Some(&session), ""))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{role} must not list approvals"
        );
    }

    let admin = test_support::user_with_role_session(&db, "admin").await;
    let rejected = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/approvals/{selected_approval}/reject"),
            Some(&admin),
            r#"{"comment":"reviewed exact record"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::OK);
    assert_eq!(json(rejected).await["job"]["id"], selected.id);

    let selected_status: String = sqlx::query_scalar("SELECT status FROM approvals WHERE id = ?")
        .bind(&selected_approval)
        .fetch_one(&db)
        .await
        .unwrap();
    let untouched_status: String = sqlx::query_scalar("SELECT status FROM approvals WHERE id = ?")
        .bind(&untouched_approval)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(selected_status, "rejected");
    assert_eq!(untouched_status, "pending");
    assert_eq!(
        jobs::get(&db, &untouched.id)
            .await
            .unwrap()
            .unwrap()
            .state
            .as_str(),
        "awaiting_approval"
    );

    let repeat = app
        .oneshot(request(
            Method::POST,
            &format!("/api/approvals/{selected_approval}/reject"),
            Some(&admin),
            r#"{"comment":null}"#,
        ))
        .await
        .unwrap();
    assert_eq!(repeat.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn automation_run_uses_canonical_job_and_replays_by_idempotency_key() {
    let db = test_support::setup_db().await;
    sqlx::query(
        "INSERT INTO automation_jobs \
         (id, name, command, enabled, timeout_secs, created_at, updated_at) \
         VALUES ('automation-1', 'Safe automation', 'printf canonical', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db.clone()));

    let first = app
        .clone()
        .oneshot(automation_run_request(
            &session,
            "automation-1",
            "automation-run-1",
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = json(first).await;
    assert_eq!(first_body["job"]["action"], "automation.run");
    assert_eq!(first_body["job"]["resource"]["kind"], "automation_job");
    assert_eq!(first_body["job"]["actor"]["actor_type"], "human");

    let repeat = app
        .oneshot(automation_run_request(
            &session,
            "automation-1",
            "automation-run-1",
        ))
        .await
        .unwrap();
    assert_eq!(repeat.status(), StatusCode::ACCEPTED);
    let repeat_body = json(repeat).await;
    assert_eq!(repeat_body["job"]["id"], first_body["job"]["id"]);

    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM automation_runs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(run_count, 0, "the HTTP seam must enqueue, not execute inline");
}

#[tokio::test]
async fn automation_scheduler_submits_canonical_job_and_replays_current_slot() {
    let db = test_support::setup_db().await;
    sqlx::query(
        "INSERT INTO automation_jobs \
         (id, name, command, schedule, enabled, timeout_secs, created_at, updated_at) \
         VALUES ('scheduled-1', 'Scheduled automation', 'printf scheduled', '@minutely', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let state = test_support::build(db.clone());

    super::automation::run_scheduled_jobs(&state).await;
    super::automation::run_scheduled_jobs(&state).await;

    let queued: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs j JOIN resources r ON r.id = j.resource_id \
         WHERE j.action = 'automation.run' AND r.kind = 'automation_job'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(queued, 1, "the scheduler must replay the current slot");
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM automation_runs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(run_count, 0, "the scheduler must enqueue, not execute inline");
}
