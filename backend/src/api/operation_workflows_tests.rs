#![cfg(test)]

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use sqlx::SqlitePool;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tower::ServiceExt;

use crate::operations::{
    clock::SystemClock,
    contracts::{ActorRef, ActorType, OperationPlanV1, PlannedStepV1},
    jobs::{self, SubmissionPolicy, SubmitJob},
    resources::{self, ObserveResource},
    runtime, unix_now, worker,
};

use super::mcp::test_support;

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct CanonicalJobContract {
    action: String,
    resource_id: String,
    input_json: String,
    plan_json: String,
    plan_digest: String,
    external_fingerprint: String,
    actor_id: Option<String>,
    actor_source: Option<String>,
    idempotency_scope: String,
    idempotency_key: String,
    retry_class: String,
    recovery_class: String,
    resource_revision: i64,
    request_digest: String,
    concurrency_key: String,
    actor_type: String,
    ingress: String,
}

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

fn webhook_request(secret: &str, body: &str, idempotency_key: Option<&str>) -> Request<Body> {
    webhook_request_with_timestamp_nonce(
        secret,
        body,
        idempotency_key,
        unix_now(),
        &uuid::Uuid::new_v4().to_string(),
    )
}

fn webhook_request_with_timestamp_nonce(
    secret: &str,
    body: &str,
    idempotency_key: Option<&str>,
    timestamp: i64,
    nonce: &str,
) -> Request<Body> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(format!("{timestamp}.{nonce}.{body}").as_bytes());
    let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/api/integrations/webhooks")
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-VoidTower-Timestamp", timestamp.to_string())
        .header("X-VoidTower-Nonce", nonce)
        .header("X-VoidTower-Signature", signature);
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

async fn configure_webhook_secret(db: &SqlitePool, secret: &str) {
    let id = uuid::Uuid::new_v4().to_string();
    let encrypted = crate::api::secrets::encrypt(&[0u8; 32], secret).unwrap();
    sqlx::query(
        "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at)
         VALUES (?, 'test-odysseus-webhook', 'test webhook credential', ?, 0, 0)",
    )
    .bind(&id)
    .bind(encrypted)
    .execute(db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES
         ('odysseus.enabled', 'true', 0), (?, ?, 0)",
    )
    .bind(crate::api::secrets::ODYSSEUS_WEBHOOK_SECRET_REF_SETTING)
    .bind(id)
    .execute(db)
    .await
    .unwrap();
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
async fn uncertain_job_resolution_requires_operator_session() {
    let db = test_support::setup_db().await;
    let app = crate::api::router(test_support::build(db.clone()));
    let body = r#"{"resolution":"confirm_succeeded","reason":"Verified in system journal"}"#;
    let anonymous = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/jobs/missing/resolve-uncertain",
            None,
            body,
        ))
        .await
        .unwrap();
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

    let viewer = test_support::user_with_role_session(&db, "viewer").await;
    let denied = app
        .oneshot(request(
            Method::POST,
            "/api/jobs/missing/resolve-uncertain",
            Some(&viewer),
            body,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
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
        assert_eq!(list["schema_version"], 1);
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
        assert_eq!(list["schema_version"], 1);
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

    let approval_detail = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("/api/approvals/{selected_approval}"),
            Some(&test_support::user_with_role_session(&db, "admin").await),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(approval_detail.status(), StatusCode::OK);
    let approval_detail_body = json(approval_detail).await;
    assert_eq!(approval_detail_body["schema_version"], 1);
    assert_eq!(approval_detail_body["approval"]["id"], selected_approval);

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
    let rejected_body = json(rejected).await;
    assert_eq!(rejected_body["schema_version"], 1);
    assert_eq!(rejected_body["resource_id"], selected.resource.id);
    assert_eq!(rejected_body["action"], "container.restart");
    assert_eq!(rejected_body["job"]["id"], selected.id);

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
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/approvals/{selected_approval}/reject"),
            Some(&admin),
            r#"{"comment":null}"#,
        ))
        .await
        .unwrap();
    assert_eq!(repeat.status(), StatusCode::CONFLICT);
    let repeat_body = json(repeat).await;
    assert_eq!(repeat_body["error"]["code"], "approval_conflict");
    assert_eq!(
        repeat_body["error"]["message"],
        "The approval can no longer be decided."
    );
    assert!(!repeat_body
        .to_string()
        .contains("approval is no longer pending"));

    let oversized_comment = app
        .clone()
        .oneshot(request(
            Method::POST,
            &format!("/api/approvals/{untouched_approval}/reject"),
            Some(&admin),
            &format!(r#"{{"comment":"{}"}}"#, "x".repeat(501)),
        ))
        .await
        .unwrap();
    assert_eq!(oversized_comment.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json(oversized_comment).await["error"]["code"],
        "bad_request"
    );

    let missing = app
        .oneshot(request(
            Method::POST,
            "/api/approvals/missing-approval/reject",
            Some(&admin),
            r#"{"comment":null}"#,
        ))
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_eq!(json(missing).await["error"]["code"], "not_found");
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
    assert_eq!(first_body["schema_version"], 1);
    assert_eq!(
        first_body["resource_id"],
        first_body["job"]["resource"]["id"]
    );
    assert_eq!(first_body["action"], "automation.run");
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
    assert_eq!(
        run_count, 0,
        "the HTTP seam must enqueue, not execute inline"
    );
}

#[tokio::test]
async fn automation_reads_are_operator_only_and_limits_are_bounded() {
    let db = test_support::setup_db().await;
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at) VALUES ('private-automation', 'Private automation', 'cat /sensitive/path', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO automation_runs (id, job_id, started_at, finished_at, status, output) VALUES ('private-run', 'private-automation', 0, 1, 'success', 'sensitive output')",
    )
    .execute(&db)
    .await
    .unwrap();
    let app = crate::api::router(test_support::build(db.clone()));

    for role in ["viewer", "guest", "member"] {
        let session = test_support::user_with_role_session(&db, role).await;
        let list = app
            .clone()
            .oneshot(request(Method::GET, "/api/automation", Some(&session), ""))
            .await
            .unwrap();
        assert_eq!(
            list.status(),
            StatusCode::FORBIDDEN,
            "{role} must not read commands"
        );
        let runs = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/automation/private-automation/runs?limit=999",
                Some(&session),
                "",
            ))
            .await
            .unwrap();
        assert_eq!(
            runs.status(),
            StatusCode::FORBIDDEN,
            "{role} must not read output"
        );
    }

    let operator = test_support::user_with_role_session(&db, "operator").await;
    let list = app
        .clone()
        .oneshot(request(Method::GET, "/api/automation", Some(&operator), ""))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(
        json(list).await["jobs"][0]["command"],
        "cat /sensitive/path"
    );

    let invalid_limit = app
        .oneshot(request(
            Method::GET,
            "/api/automation/private-automation/runs?limit=999",
            Some(&operator),
            "",
        ))
        .await
        .unwrap();
    assert_eq!(invalid_limit.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn automation_webhook_uses_canonical_durable_job_replay_and_dry_run_contract() {
    let db = test_support::setup_db().await;
    configure_webhook_secret(&db, "fixture-secret").await;
    sqlx::query(
        "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at) VALUES ('webhook-allow', 'automation', 'automation.run', 'automation_job', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at) VALUES ('webhook-automation', 'Webhook automation', 'printf webhook', 1, 30, 0, 0), ('webhook-other', 'Other automation', 'printf other', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let app = crate::api::router(test_support::build(db.clone()));

    let dry_run = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-automation","dry_run":true}"#,
            Some("webhook-dry-run"),
        ))
        .await
        .unwrap();
    assert_eq!(dry_run.status(), StatusCode::OK);
    let dry_body = json(dry_run).await;
    assert_eq!(dry_body["dry_run"], true);
    assert!(dry_body["plan"].is_object());
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(job_count, 0, "dry-run must not create a durable job");

    let first = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-automation"}"#,
            Some("webhook-replay"),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = json(first).await;
    assert_eq!(first_body["job"]["action"], "automation.run");
    assert_eq!(first_body["job"]["actor"]["actor_type"], "automation");
    assert_eq!(first_body["job"]["ingress"], "webhook");

    let repeat = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-automation"}"#,
            Some("webhook-replay"),
        ))
        .await
        .unwrap();
    assert_eq!(repeat.status(), StatusCode::ACCEPTED);
    assert_eq!(json(repeat).await["job"]["id"], first_body["job"]["id"]);

    let conflict = app
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-other"}"#,
            Some("webhook-replay"),
        ))
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn automation_http_and_webhook_share_canonical_plan_semantics() {
    let db = test_support::setup_db().await;
    configure_webhook_secret(&db, "fixture-secret").await;
    sqlx::query(
        "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at) VALUES ('equivalence-allow', 'automation', 'automation.run', 'automation_job', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at) VALUES ('equivalence-automation', 'Equivalent automation', 'printf equivalent', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db.clone()));
    let key = "cross-ingress-equivalence";

    let http = app
        .clone()
        .oneshot(automation_run_request(
            &session,
            "equivalence-automation",
            key,
        ))
        .await
        .unwrap();
    assert_eq!(http.status(), StatusCode::ACCEPTED);
    let http_body = json(http).await;

    let webhook = app
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"equivalence-automation"}"#,
            Some(key),
        ))
        .await
        .unwrap();
    assert_eq!(webhook.status(), StatusCode::ACCEPTED);
    let webhook_body = json(webhook).await;

    let http_job_id = http_body["job"]["id"].as_str().unwrap();
    let webhook_job_id = webhook_body["job"]["id"].as_str().unwrap();
    assert_ne!(http_job_id, webhook_job_id);

    let http_job: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT action, resource_id, input_json, plan_json, retry_class, recovery_class,
                    actor_type, actor_id, actor_source, ingress
             FROM jobs WHERE id = ?",
    )
    .bind(http_job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    let webhook_job: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT action, resource_id, input_json, plan_json, retry_class, recovery_class,
                    actor_type, actor_id, actor_source, ingress
             FROM jobs WHERE id = ?",
    )
    .bind(webhook_job_id)
    .fetch_one(&db)
    .await
    .unwrap();

    assert_eq!(http_job.0, "automation.run");
    assert_eq!(webhook_job.0, http_job.0);
    assert_eq!(
        webhook_job.1, http_job.1,
        "both ingresses must adopt one canonical resource"
    );
    assert_eq!(webhook_job.2, http_job.2);
    assert_eq!(
        webhook_job.3, http_job.3,
        "equivalent intent must persist one immutable plan"
    );
    assert_eq!(webhook_job.4, http_job.4);
    assert_eq!(webhook_job.5, http_job.5);
    assert_eq!(http_job.6, "human");
    assert_eq!(http_job.9, "http_session");
    assert_eq!(webhook_job.6, "automation");
    assert_eq!(webhook_job.7, "odysseus");
    assert_eq!(webhook_job.8, "odysseus_webhook");
    assert_eq!(webhook_job.9, "webhook");

    let (http_scope, webhook_scope): (String, String) = sqlx::query_as(
        "SELECT
            (SELECT idempotency_scope FROM jobs WHERE id = ?),
            (SELECT idempotency_scope FROM jobs WHERE id = ?)",
    )
    .bind(http_job_id)
    .bind(webhook_job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_ne!(http_scope, webhook_scope);
    assert_eq!(webhook_scope, "v1:webhook:automation:odysseus");

    for job_id in [http_job_id, webhook_job_id] {
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log
             WHERE request_id = ? AND action = 'automation.run' AND outcome = 'queued'",
        )
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            audit_count, 1,
            "canonical submission must retain audit evidence"
        );

        let event_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events
             WHERE job_id = ? AND event_type = 'job.submitted.v1' AND resource_id = ?
               AND correlation_id = ?",
        )
        .bind(job_id)
        .bind(&http_job.1)
        .bind(job_id)
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(
            event_count, 1,
            "canonical submission must retain event evidence"
        );
    }
}

#[tokio::test]
async fn automation_webhook_rejects_ambiguous_and_unknown_intents() {
    let db = test_support::setup_db().await;
    configure_webhook_secret(&db, "fixture-secret").await;
    let app = crate::api::router(test_support::build(db));

    let oversized_body = "x".repeat(64 * 1024 + 1);
    let oversized_unauthenticated = app
        .clone()
        .oneshot(webhook_request("wrong-secret", &oversized_body, None))
        .await
        .unwrap();
    assert_eq!(
        oversized_unauthenticated.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "signed authentication requires a bounded raw-body read before verification"
    );
    let oversized_authenticated = app
        .clone()
        .oneshot(webhook_request("fixture-secret", &oversized_body, None))
        .await
        .unwrap();
    assert_eq!(
        oversized_authenticated.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        json(oversized_authenticated).await["error"]["code"],
        "payload_too_large"
    );

    let unauthenticated_malformed = app
        .clone()
        .oneshot(webhook_request("wrong-secret", "{", None))
        .await
        .unwrap();
    assert_eq!(
        unauthenticated_malformed.status(),
        StatusCode::UNAUTHORIZED,
        "secret verification must precede JSON parsing"
    );

    let unsupported_media_request = {
        let mut request = webhook_request("fixture-secret", r#"{"automation_id":"one"}"#, None);
        request.headers_mut().insert(
            header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("text/plain"),
        );
        request
    };
    let unsupported_media = app
        .clone()
        .oneshot(unsupported_media_request)
        .await
        .unwrap();
    assert_eq!(
        unsupported_media.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );

    let malformed_authenticated = app
        .clone()
        .oneshot(webhook_request("fixture-secret", "{", None))
        .await
        .unwrap();
    assert_eq!(
        malformed_authenticated.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    for body in [
        r#"{}"#,
        r#"{"automation_id":"one","action":"container.start","resource_id":"web"}"#,
        r#"{"automation_id":"one","unexpected":true}"#,
    ] {
        let response = app
            .clone()
            .oneshot(webhook_request("fixture-secret", body, None))
            .await
            .unwrap();
        assert!(
            matches!(
                response.status(),
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "unexpected status for {body}: {}",
            response.status()
        );
    }
}

#[tokio::test]
async fn signed_webhook_auth_rejects_bearer_tampering_and_clock_skew() {
    let db = test_support::setup_db().await;
    configure_webhook_secret(&db, "fixture-secret").await;
    let app = crate::api::router(test_support::build(db));
    let body = r#"{"automation_id":"one"}"#;

    let bearer = Request::builder()
        .method(Method::POST)
        .uri("/api/integrations/webhooks")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, "Bearer fixture-secret")
        .body(Body::from(body))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(bearer).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let stale = webhook_request_with_timestamp_nonce(
        "fixture-secret",
        body,
        None,
        unix_now() - 301,
        "stale-nonce",
    );
    assert_eq!(
        app.clone().oneshot(stale).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let future = webhook_request_with_timestamp_nonce(
        "fixture-secret",
        body,
        None,
        unix_now() + 301,
        "future-nonce",
    );
    assert_eq!(
        app.clone().oneshot(future).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );

    let valid = webhook_request_with_timestamp_nonce(
        "fixture-secret",
        body,
        None,
        unix_now(),
        "tamper-nonce",
    );
    let (parts, _) = valid.into_parts();
    let tampered = Request::from_parts(parts, Body::from(r#"{\"automation_id\":\"two\"}"#));
    let tampered_response = app.clone().oneshot(tampered).await.unwrap();
    assert_eq!(tampered_response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        json(tampered_response).await["error"]["code"],
        "webhook_authentication_failed"
    );
}

#[tokio::test]
async fn signed_webhook_nonce_is_persisted_and_replay_is_rejected() {
    let db = test_support::setup_db().await;
    configure_webhook_secret(&db, "fixture-secret").await;
    sqlx::query(
        "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at) VALUES ('webhook-replay-allow', 'automation', 'automation.run', 'automation_job', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at) VALUES ('replay-automation', 'Replay automation', 'printf replay', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let app = crate::api::router(test_support::build(db.clone()));
    let body = r#"{"automation_id":"replay-automation"}"#;
    let first = app
        .clone()
        .oneshot(webhook_request_with_timestamp_nonce(
            "fixture-secret",
            body,
            Some("replay-idempotency"),
            unix_now(),
            "duplicate-delivery",
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let repeat = app
        .oneshot(webhook_request_with_timestamp_nonce(
            "fixture-secret",
            body,
            Some("replay-idempotency"),
            unix_now(),
            "duplicate-delivery",
        ))
        .await
        .unwrap();
    assert_eq!(repeat.status(), StatusCode::CONFLICT);
    assert_eq!(json(repeat).await["error"]["code"], "webhook_replay");
    let receipt_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhook_replay_receipts")
        .fetch_one(&db)
        .await
        .unwrap();
    let job_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(receipt_count, 1);
    assert_eq!(job_count, 1, "a replay must not create another durable job");
}

/// Durable residue a signed delivery can leave behind: `(jobs, approvals, audit_log,
/// automation_runs, webhook_replay_receipts)`. The replay receipt is counted separately because the
/// signed-delivery ledger is written before admission on purpose.
async fn webhook_admission_residue(db: &SqlitePool) -> (i64, i64, i64, i64, i64) {
    let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
        .fetch_one(db)
        .await
        .unwrap();
    let approvals: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM approvals")
        .fetch_one(db)
        .await
        .unwrap();
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(db)
        .await
        .unwrap();
    let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM automation_runs")
        .fetch_one(db)
        .await
        .unwrap();
    let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhook_replay_receipts")
        .fetch_one(db)
        .await
        .unwrap();
    (jobs, approvals, audits, runs, receipts)
}

/// The real automation adapter behind a planning counter. Delegates every operation so the test
/// exercises production planning while observing whether the admission guard ran first.
struct CountingAutomationAdapter {
    inner: crate::operations::adapters::automation::AutomationAdapter,
    plan_calls: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl crate::operations::adapters::OperationAdapter for CountingAutomationAdapter {
    fn key(&self) -> &'static str {
        self.inner.key()
    }

    fn actions(&self) -> &[&'static str] {
        self.inner.actions()
    }

    async fn plan(
        &self,
        request: crate::operations::adapters::PlanRequest,
    ) -> anyhow::Result<crate::operations::contracts::OperationPlanV1> {
        self.plan_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.plan(request).await
    }

    async fn external_fingerprint(
        &self,
        request: &crate::operations::adapters::PlanRequest,
    ) -> anyhow::Result<String> {
        self.inner.external_fingerprint(request).await
    }

    async fn execute_step(
        &self,
        request: crate::operations::adapters::StepRequest,
    ) -> anyhow::Result<crate::operations::adapters::StepOutcome> {
        self.inner.execute_step(request).await
    }

    async fn reconcile(
        &self,
        request: crate::operations::adapters::StepRequest,
    ) -> anyhow::Result<crate::operations::adapters::ReconcileOutcome> {
        self.inner.reconcile(request).await
    }
}

/// M1-03 — the signed webhook is machine ingress and must obey the same admission rule as the
/// canonical bearer, built-in MCP, and Studio ingresses.
///
/// While a resource's `concurrency_key` holds a `needs_attention` job, a new signed `automation_id`
/// delivery is refused with `409 unresolved_operation` before planning and before any job, approval,
/// audit row, or automation run. This branch resolves its resource from the database only, so it
/// proves no provider call precedes the refusal; the `container.*` branch, which performs the
/// read-only Docker container list first, is not covered by this test. The refusal is
/// resource-scoped; the same-idempotency-key replay still returns the blocked job so an ambiguous
/// sender recovers by query; the human operator and an unrelated resource stay admitted; and the
/// refusal clears when the uncertain job is reconciled.
#[tokio::test]
async fn signed_webhook_refuses_new_intent_while_its_resource_operation_is_unresolved() {
    let db = test_support::setup_db().await;
    configure_webhook_secret(&db, "fixture-secret").await;
    sqlx::query(
        "INSERT INTO voidwatch_default_allowlist (id, actor_type, action, resource_type, created_at) \
         VALUES ('webhook-guard-allow', 'automation', 'automation.run', 'automation_job', 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at) VALUES \
         ('webhook-guard-a', 'Guard automation', 'printf guard-a', 1, 30, 0, 0), \
         ('webhook-guard-b', 'Unrelated automation', 'printf guard-b', 1, 30, 0, 0), \
         ('webhook-guard-c', 'Operator automation', 'printf guard-c', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    // The real automation adapter behind a plan counter, so "refused before planning" is an
    // observed call count rather than an inference from the absence of a job.
    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(CountingAutomationAdapter {
            inner: crate::operations::adapters::automation::AutomationAdapter::new(db.clone()),
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);
    let app = crate::api::router(state);

    // A signed delivery for an unblocked resource is accepted and lands the canonical durable job.
    let first = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-guard-a"}"#,
            Some("webhook-guard-first"),
        ))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = json(first).await;
    assert_eq!(first_body["job"]["action"], "automation.run");
    assert_eq!(first_body["job"]["ingress"], "webhook");
    let blocked_job_id = first_body["job"]["id"].as_str().unwrap().to_string();
    assert_eq!(plan_calls.load(Ordering::SeqCst), 1);

    // The provider outcome for that job becomes uncertain.
    sqlx::query(
        "UPDATE jobs SET state = 'needs_attention', error_code = 'provider_unavailable', \
         error_message = 'outcome unresolved' WHERE id = ?",
    )
    .bind(&blocked_job_id)
    .execute(&db)
    .await
    .unwrap();
    let before = webhook_admission_residue(&db).await;

    let refused = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-guard-a"}"#,
            Some("webhook-guard-new"),
        ))
        .await
        .unwrap();
    let refused_status = refused.status();
    let refused_body = json(refused).await;
    assert_eq!(
        refused_status,
        StatusCode::CONFLICT,
        "a new signed intent must be refused: {refused_body}"
    );
    assert_eq!(refused_body["error"]["code"], "unresolved_operation");
    assert!(
        !refused_body.to_string().contains(&blocked_job_id),
        "the refusal must not disclose the blocking job id: {refused_body}"
    );
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "the refusal must happen before planning"
    );

    let refused_residue = webhook_admission_residue(&db).await;
    assert_eq!(
        (
            refused_residue.0,
            refused_residue.1,
            refused_residue.2,
            refused_residue.3
        ),
        (before.0, before.1, before.2, before.3),
        "a refused webhook must not persist a job, approval, audit record, or automation run"
    );
    assert_eq!(
        refused_residue.4,
        before.4 + 1,
        "the signed delivery's nonce is burned in the anti-replay ledger before admission"
    );

    // Recovery by query: the sender's original key still returns the blocked job unchanged.
    let replay = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-guard-a"}"#,
            Some("webhook-guard-first"),
        ))
        .await
        .unwrap();
    assert_eq!(replay.status(), StatusCode::ACCEPTED);
    let replay_body = json(replay).await;
    assert_eq!(replay_body["job"]["id"], blocked_job_id.as_str());
    assert_eq!(replay_body["job"]["state"], "needs_attention");
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "a same-key replay must not re-plan the intent"
    );

    // The refusal is scoped to the affected resource, not a global machine lockout.
    let unrelated = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-guard-b"}"#,
            Some("webhook-guard-unrelated"),
        ))
        .await
        .unwrap();
    assert_eq!(unrelated.status(), StatusCode::ACCEPTED);
    assert_eq!(json(unrelated).await["job"]["state"], "queued");

    // A human operator session stays admitted on purpose: the refusal is machine-scoped.
    let session = test_support::user_with_role_session(&db, "operator").await;
    let human = app
        .clone()
        .oneshot(automation_run_request(
            &session,
            "webhook-guard-c",
            "webhook-guard-human",
        ))
        .await
        .unwrap();
    assert_eq!(
        human.status(),
        StatusCode::ACCEPTED,
        "a human session must still be able to submit while machine ingress is refused"
    );

    // A `dry_run` delivery remains a plan preview: it accepts no intent and writes no job, so it is
    // deliberately outside the admission guard.
    let jobs_before_preview = webhook_admission_residue(&db).await.0;
    let preview = app
        .clone()
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-guard-a","dry_run":true}"#,
            Some("webhook-guard-dry"),
        ))
        .await
        .unwrap();
    assert_eq!(preview.status(), StatusCode::OK);
    let preview_body = json(preview).await;
    assert_eq!(preview_body["dry_run"], true);
    assert!(preview_body["plan"].is_object());
    assert_eq!(
        webhook_admission_residue(&db).await.0,
        jobs_before_preview,
        "a preview must not create a durable job"
    );

    // Reconciling the uncertain job clears the refusal for later equivalent intent.
    sqlx::query("UPDATE jobs SET state = 'failed' WHERE id = ?")
        .bind(&blocked_job_id)
        .execute(&db)
        .await
        .unwrap();
    let cleared = app
        .oneshot(webhook_request(
            "fixture-secret",
            r#"{"automation_id":"webhook-guard-a"}"#,
            Some("webhook-guard-after"),
        ))
        .await
        .unwrap();
    assert_eq!(cleared.status(), StatusCode::ACCEPTED);
    let cleared_body = json(cleared).await;
    assert_eq!(cleared_body["job"]["action"], "automation.run");
    assert_eq!(cleared_body["job"]["state"], "queued");
}

#[tokio::test]
async fn automation_write_contract_rejects_unknown_and_invalid_fields() {
    let db = test_support::setup_db().await;
    let session = test_support::user_with_role_session(&db, "operator").await;
    let app = crate::api::router(test_support::build(db.clone()));

    let oversized_body = "x".repeat(64 * 1024 + 1);
    let oversized = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/automation")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, format!("vt_session={session}"))
                .body(Body::from(oversized_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(json(oversized).await["error"]["code"], "payload_too_large");

    let unsupported_media = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/automation")
                .header(header::CONTENT_TYPE, "text/plain")
                .header(header::COOKIE, format!("vt_session={session}"))
                .body(Body::from(r#"{"name":"job","command":"printf ok"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        unsupported_media.status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );

    for body in [
        r#"{"name":"job","command":"printf ok","timeout_secs":0}"#,
        r#"{"name":"job","command":"printf ok","schedule":"every tuesday"}"#,
        r#"{"name":"job","command":"printf ok","unexpected":true}"#,
    ] {
        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/automation",
                Some(&session),
                body,
            ))
            .await
            .unwrap();
        assert!(
            matches!(
                response.status(),
                StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY
            ),
            "unexpected status for {body}: {}",
            response.status()
        );
    }

    let created = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/automation",
            Some(&session),
            r#"{"name":"valid job","command":"printf ok","schedule":"@hourly","timeout_secs":30}"#,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let id = json(created).await["id"].as_str().unwrap().to_owned();

    let invalid_update = app
        .clone()
        .oneshot(request(
            Method::PATCH,
            &format!("/api/automation/{id}"),
            Some(&session),
            r#"{"timeout_secs":3601}"#,
        ))
        .await
        .unwrap();
    assert_eq!(invalid_update.status(), StatusCode::BAD_REQUEST);

    let missing_update = app
        .oneshot(request(
            Method::PATCH,
            "/api/automation/missing",
            Some(&session),
            r#"{"enabled":false}"#,
        ))
        .await
        .unwrap();
    assert_eq!(missing_update.status(), StatusCode::NOT_FOUND);
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

    let (job_id, resource_id, actor_type, actor_id, actor_source, ingress, scope, key): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = sqlx::query_as(
        "SELECT id, resource_id, actor_type, actor_id, actor_source, ingress,
                idempotency_scope, idempotency_key
         FROM jobs WHERE action = 'automation.run'",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(actor_type, "system");
    assert_eq!(actor_id, "automation_scheduler");
    assert_eq!(actor_source, "scheduler");
    assert_eq!(ingress, "scheduler");
    assert_eq!(scope, "v1:scheduler:system:automation_scheduler");
    assert!(key.starts_with("schedule-scheduled-1-"));
    assert_ne!(resource_id, "scheduled-1");
    let resource_kind: String = sqlx::query_scalar("SELECT kind FROM resources WHERE id = ?")
        .bind(&resource_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(resource_kind, "automation_job");

    let audit_evidence: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE request_id = ? AND action = 'automation.run' AND resource_id = ?
           AND actor_type = 'system' AND source = 'scheduler' AND outcome = 'queued'",
    )
    .bind(&job_id)
    .bind(&resource_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(audit_evidence, 1);
    let event_evidence: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events
         WHERE job_id = ? AND event_type = 'job.submitted.v1'
           AND resource_id = ? AND actor_type = 'system'
           AND actor_id = 'automation_scheduler' AND actor_source = 'scheduler'
           AND correlation_id = ?",
    )
    .bind(&job_id)
    .bind(&resource_id)
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(event_evidence, 1);
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM automation_runs")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(
        run_count, 0,
        "the scheduler must enqueue, not execute inline"
    );
}

/// M1-03 — the in-process automation scheduler is machine ingress and must obey the same
/// admission rule as the canonical bearer, built-in MCP, Studio, and signed-webhook ingresses.
///
/// While a resource's `concurrency_key` holds a `needs_attention` job, a tick that would submit a
/// *new* slot is refused inside canonical submission: the tick resolves the resource and refreshes
/// its capability projection first, then writes no job, approval, audit record, or automation run,
/// and the adapter is never asked to plan. The scheduler has no HTTP response, so the refusal is a
/// bounded warning and the next tick retries. The refusal is resource-scoped, an unrelated
/// automation still schedules, and a reconciled outcome admits the deferred slot.
///
/// The scheduler derives its idempotency key from the wall-clock slot, so the unresolved job is
/// re-keyed to the previous slot after the first tick: that models the production case of an
/// earlier slot whose run is still unresolved, and makes the next tick a genuinely new intent
/// instead of a same-slot replay (which the sibling test already covers).
#[tokio::test]
async fn automation_scheduler_refuses_new_slot_while_its_resource_operation_is_unresolved() {
    let db = test_support::setup_db().await;
    sqlx::query(
        "INSERT INTO automation_jobs \
         (id, name, command, schedule, enabled, timeout_secs, created_at, updated_at) \
         VALUES ('scheduler-guard-a', 'Blocked automation', 'printf blocked', '@minutely', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();

    // The real automation adapter behind a plan counter, so \"refused before planning\" is an
    // observed call count rather than an inference from the absence of a job.
    let plan_calls = Arc::new(AtomicUsize::new(0));
    let mut state = test_support::build(db.clone());
    let mut adapters = crate::operations::adapters::AdapterRegistry::new();
    adapters
        .register(Arc::new(CountingAutomationAdapter {
            inner: crate::operations::adapters::automation::AutomationAdapter::new(db.clone()),
            plan_calls: plan_calls.clone(),
        }))
        .unwrap();
    state.operation_adapters = Arc::new(adapters);

    // The first tick resolves the automation resource and queues its current slot.
    super::automation::run_scheduled_jobs(&state).await;
    assert_eq!(plan_calls.load(Ordering::SeqCst), 1);

    let (blocked_job_id, resource_id, current_key): (String, String, String) = sqlx::query_as(
        "SELECT j.id, j.resource_id, j.idempotency_key FROM jobs j \
         WHERE j.action = 'automation.run' AND j.resource_id = (\
           SELECT resource_id FROM resource_aliases \
           WHERE namespace = 'automation.job' AND scope_key = 'local' AND value = 'scheduler-guard-a')",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    let (prefix, slot) = current_key.rsplit_once('-').unwrap();
    let prior_key = format!("{prefix}-{}", slot.parse::<i64>().unwrap() - 1);

    // The previous slot's run ends with an unresolved provider outcome: it still holds this
    // resource's concurrency key.
    sqlx::query(
        "UPDATE jobs SET state = 'needs_attention', error_code = 'provider_unavailable', \
         error_message = 'outcome unresolved', idempotency_key = ? WHERE id = ?",
    )
    .bind(&prior_key)
    .bind(&blocked_job_id)
    .execute(&db)
    .await
    .unwrap();

    // The residue tuple also proves the scheduler burns no webhook replay receipt.
    let before = webhook_admission_residue(&db).await;
    super::automation::run_scheduled_jobs(&state).await;
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        1,
        "a refused scheduled submission must not reach planning"
    );
    assert_eq!(
        webhook_admission_residue(&db).await,
        before,
        "a refused tick must not write a job, approval, audit record, automation run, or receipt"
    );

    // The refusal is stable while the outcome stays unresolved: repeating the tick in the same
    // slot neither accumulates deferred duplicates nor re-plans.
    super::automation::run_scheduled_jobs(&state).await;
    assert_eq!(plan_calls.load(Ordering::SeqCst), 1);
    assert_eq!(webhook_admission_residue(&db).await, before);

    // The refusal is scoped to the affected resource, not a global machine lockout: an automation
    // introduced after the first tick still schedules its slot.
    sqlx::query(
        "INSERT INTO automation_jobs \
         (id, name, command, schedule, enabled, timeout_secs, created_at, updated_at) \
         VALUES ('scheduler-guard-b', 'Unrelated automation', 'printf unrelated', '@minutely', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    super::automation::run_scheduled_jobs(&state).await;
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        2,
        "an unrelated automation must still be planned while another resource is unresolved"
    );
    let unrelated_jobs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE action = 'automation.run' AND resource_id = (\
           SELECT resource_id FROM resource_aliases \
           WHERE namespace = 'automation.job' AND scope_key = 'local' AND value = 'scheduler-guard-b')",
    )
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(unrelated_jobs, 1);

    // Reconciling the uncertain outcome to a terminal state re-admits the deferred slot.
    sqlx::query(
        "UPDATE jobs SET state = 'failed', error_code = 'provider_rejected', \
         error_message = 'reconciled' WHERE id = ?",
    )
    .bind(&blocked_job_id)
    .execute(&db)
    .await
    .unwrap();
    super::automation::run_scheduled_jobs(&state).await;
    assert_eq!(
        plan_calls.load(Ordering::SeqCst),
        3,
        "a resource without an unresolved outcome must admit the deferred slot"
    );
    let admitted_key: String = sqlx::query_scalar(
        "SELECT idempotency_key FROM jobs WHERE action = 'automation.run' \
         AND resource_id = ? AND state = 'queued' ORDER BY rowid DESC LIMIT 1",
    )
    .bind(&resource_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(
        admitted_key.starts_with("schedule-scheduler-guard-a-"),
        "the deferred intent keeps the scheduler idempotency key: {admitted_key}"
    );
    assert_ne!(
        admitted_key, prior_key,
        "the deferred intent is a new slot, not the unresolved job"
    );
    let (ingress, actor_id): (String, String) = sqlx::query_as(
        "SELECT ingress, actor_id FROM jobs WHERE idempotency_key = ? AND resource_id = ?",
    )
    .bind(&admitted_key)
    .bind(&resource_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(ingress, "scheduler");
    assert_eq!(actor_id, "automation_scheduler");
}

#[tokio::test]
async fn automation_scheduler_recovers_expired_run_without_replaying() {
    let db = test_support::setup_db().await;
    sqlx::query(
        "INSERT INTO automation_jobs \
         (id, name, command, schedule, enabled, timeout_secs, created_at, updated_at) \
         VALUES ('recovery-scheduled-1', 'Recovery automation', 'printf should-not-run', '@minutely', 1, 30, 0, 0)",
    )
    .execute(&db)
    .await
    .unwrap();
    let state = test_support::build(db.clone());

    super::automation::run_scheduled_jobs(&state).await;
    let job_id: String = sqlx::query_scalar(
        "SELECT id FROM jobs WHERE action = 'automation.run' ORDER BY submitted_at DESC LIMIT 1",
    )
    .fetch_one(&db)
    .await
    .unwrap();

    let worker_id = "scheduler-recovery-worker";
    let claimed = worker::claim_next(&db, &state.operation_adapters, worker_id, 100, 30)
        .await
        .unwrap()
        .expect("scheduler job should be claimable");
    let _step = worker::claim_step(&db, &job_id, worker_id, 100, 30)
        .await
        .unwrap()
        .expect("scheduler job should have an executable step");
    assert_eq!(claimed.id, job_id);

    // Model a process that created its durable run record before its lease expired. The recovery
    // path must observe this existing record rather than invoke the shell command a second time.
    sqlx::query(
        "INSERT INTO automation_runs (id, job_id, started_at, status, output) \
         VALUES (?, 'recovery-scheduled-1', 100, 'running', '')",
    )
    .bind(&job_id)
    .execute(&db)
    .await
    .unwrap();
    sqlx::query("UPDATE jobs SET lease_expires_at = 0 WHERE id = ?")
        .bind(&job_id)
        .execute(&db)
        .await
        .unwrap();

    let canonical_before_recovery: CanonicalJobContract = sqlx::query_as(
        "SELECT action, resource_id, input_json, plan_json, plan_digest,
                    external_fingerprint, actor_id, actor_source,
                    idempotency_scope, idempotency_key, retry_class, recovery_class,
                    resource_revision, request_digest, concurrency_key, actor_type, ingress
             FROM jobs WHERE id = ?",
    )
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();

    // Startup recovery is the deterministic equivalent of a worker/process restart for this
    // local integration seam.
    let prepared = runtime::prepare_with_clock(
        db.clone(),
        state.operation_adapters.clone(),
        crate::config::OperationRuntimeConfig::default(),
        std::sync::Arc::new(SystemClock),
    )
    .await
    .unwrap();
    drop(prepared);

    let recovered_state: String = sqlx::query_scalar("SELECT state FROM jobs WHERE id = ?")
        .bind(&job_id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(recovered_state, "needs_attention");

    sqlx::query(
        "UPDATE automation_runs SET finished_at = 101, status = 'success', exit_code = 0, \
         output = 'observed externally' WHERE id = ?",
    )
    .bind(&job_id)
    .execute(&db)
    .await
    .unwrap();

    let (reconciliation_job, reconciliation_step) = worker::claim_reconciliation(
        &db,
        &state.operation_adapters,
        "scheduler-recovery-reconciler",
        unix_now(),
        30,
    )
    .await
    .unwrap()
    .expect("expired scheduler job should be reconciled");
    assert_eq!(
        worker::reconcile_claimed_step(
            &db,
            &state.operation_adapters,
            &reconciliation_job,
            &reconciliation_step,
            "scheduler-recovery-reconciler",
            &SystemClock,
        )
        .await
        .unwrap(),
        crate::operations::contracts::JobState::Succeeded
    );

    let (run_count, run_output): (i64, String) =
        sqlx::query_as("SELECT COUNT(*), MAX(output) FROM automation_runs WHERE id = ?")
            .bind(&job_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(run_count, 1, "reconciliation must not create a second run");
    assert_eq!(run_output, "observed externally");

    let canonical_after_recovery: CanonicalJobContract = sqlx::query_as(
        "SELECT action, resource_id, input_json, plan_json, plan_digest,
                    external_fingerprint, actor_id, actor_source,
                    idempotency_scope, idempotency_key, retry_class, recovery_class,
                    resource_revision, request_digest, concurrency_key, actor_type, ingress
             FROM jobs WHERE id = ?",
    )
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        canonical_after_recovery, canonical_before_recovery,
        "recovery must preserve the immutable canonical job contract"
    );

    let event_types: Vec<String> =
        sqlx::query_scalar("SELECT event_type FROM events WHERE job_id = ? ORDER BY sequence")
            .bind(&job_id)
            .fetch_all(&db)
            .await
            .unwrap();
    assert!(
        event_types.contains(&"job.needs_attention.v1".into()),
        "events: {event_types:?}"
    );
    assert!(event_types.contains(&"job.reconciliation_started.v1".into()));
    assert!(event_types.contains(&"job.succeeded.v1".into()));

    let recovery_audit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE request_id = ? AND action = 'automation.run' \
         AND outcome = 'needs_attention' AND source = 'operation_worker'",
    )
    .bind(&job_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(recovery_audit, 1);
}
