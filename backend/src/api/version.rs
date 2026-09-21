use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

pub const API_VERSION: &str = "1";
pub const ACTION_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const JOB_READ_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const COLLECTION_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const APPROVAL_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const RESOURCE_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const EVENT_HISTORY_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const INVENTORY_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const MAX_PAGE_LIMIT: i64 = 500;
const VERSION_HEADER: &str = "x-voidtower-api-version";

#[derive(Debug, Serialize)]
pub struct ResourceListEnvelopeV1<T> {
    pub schema_version: u16,
    pub resources: Vec<T>,
}

#[derive(Debug, Serialize)]
pub struct ResourceReadEnvelopeV1<R, A, C> {
    pub schema_version: u16,
    pub resource: R,
    pub aliases: Vec<A>,
    pub capabilities: Vec<C>,
}

#[derive(Debug, Serialize)]
pub struct ResourceCapabilitiesEnvelopeV1<T> {
    pub schema_version: u16,
    pub resource_id: String,
    pub capabilities: Vec<T>,
}

#[derive(Debug, Serialize)]
pub struct EventHistoryEnvelopeV1<T> {
    pub schema_version: u16,
    pub events: Vec<T>,
    pub next_cursor: i64,
    pub earliest_available: Option<i64>,
    pub latest_available: i64,
}

#[derive(Debug, Serialize)]
pub struct PlanSuccessEnvelopeV1<T> {
    pub schema_version: u16,
    pub resource_id: String,
    pub action: String,
    pub plan: T,
}

impl<T> PlanSuccessEnvelopeV1<T> {
    pub fn new(resource_id: impl Into<String>, action: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: ACTION_ENVELOPE_SCHEMA_VERSION,
            resource_id: resource_id.into(),
            action: action.into(),
            plan: data,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JobSuccessEnvelopeV1<T> {
    pub schema_version: u16,
    pub resource_id: String,
    pub action: String,
    pub job: T,
}

impl<T> JobSuccessEnvelopeV1<T> {
    pub fn new(resource_id: impl Into<String>, action: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: ACTION_ENVELOPE_SCHEMA_VERSION,
            resource_id: resource_id.into(),
            action: action.into(),
            job: data,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JobReadEnvelopeV1<T> {
    pub schema_version: u16,
    pub resource_id: String,
    pub action: String,
    pub job: T,
}

impl<T> JobReadEnvelopeV1<T> {
    pub fn new(resource_id: impl Into<String>, action: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: JOB_READ_ENVELOPE_SCHEMA_VERSION,
            resource_id: resource_id.into(),
            action: action.into(),
            job: data,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct JobListEnvelopeV1<T> {
    pub schema_version: u16,
    pub jobs: Vec<T>,
}

impl<T> JobListEnvelopeV1<T> {
    pub fn new(jobs: Vec<T>) -> Self {
        Self {
            schema_version: COLLECTION_ENVELOPE_SCHEMA_VERSION,
            jobs,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApprovalListEnvelopeV1<T> {
    pub schema_version: u16,
    pub approvals: Vec<T>,
}

impl<T> ApprovalListEnvelopeV1<T> {
    pub fn new(approvals: Vec<T>) -> Self {
        Self {
            schema_version: APPROVAL_ENVELOPE_SCHEMA_VERSION,
            approvals,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApprovalReadEnvelopeV1<T> {
    pub schema_version: u16,
    pub approval: T,
}

impl<T> ApprovalReadEnvelopeV1<T> {
    pub fn new(approval: T) -> Self {
        Self {
            schema_version: APPROVAL_ENVELOPE_SCHEMA_VERSION,
            approval,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InventoryUploadEnvelopeV1<T> {
    pub schema_version: u16,
    pub result: T,
}

impl<T> InventoryUploadEnvelopeV1<T> {
    pub fn new(result: T) -> Self {
        Self {
            schema_version: INVENTORY_ENVELOPE_SCHEMA_VERSION,
            result,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ApiErrorV1 {
    pub code: &'static str,
    pub message: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorEnvelopeV1 {
    pub error: ApiErrorV1,
}

#[derive(Debug, Serialize)]
pub struct VersionNegotiationErrorV1 {
    pub code: &'static str,
    pub message: &'static str,
    pub supported_versions: [&'static str; 1],
}

#[derive(Debug, Serialize)]
pub struct VersionErrorEnvelopeV1 {
    pub error: VersionNegotiationErrorV1,
}

/// Negotiate the version of the HTTP API without requiring legacy clients to
/// opt in. A client may omit the header and receives the current version.
pub async fn negotiate(req: Request, next: Next) -> Response {
    let mut values = req.headers().get_all(VERSION_HEADER).iter();
    let unsupported = match (values.next(), values.next()) {
        (None, None) => false,
        (Some(value), None) => value
            .to_str()
            .map(|version| version != API_VERSION)
            .unwrap_or(true),
        _ => true,
    };

    if unsupported {
        return version_error().into_response();
    }

    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(VERSION_HEADER, HeaderValue::from_static(API_VERSION));
    response
}

fn version_error() -> (StatusCode, axum::Json<VersionErrorEnvelopeV1>) {
    (
        StatusCode::NOT_ACCEPTABLE,
        axum::Json(VersionErrorEnvelopeV1 {
            error: VersionNegotiationErrorV1 {
                code: "unsupported_api_version",
                message: "The requested API version is not supported",
                supported_versions: [API_VERSION],
            },
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

    #[test]
    fn version_error_contract_serializes_without_schema_drift() {
        let body = serde_json::to_value(VersionErrorEnvelopeV1 {
            error: VersionNegotiationErrorV1 {
                code: "unsupported_api_version",
                message: "The requested API version is not supported",
                supported_versions: [API_VERSION],
            },
        })
        .unwrap();

        assert_eq!(
            body,
            serde_json::json!({
                "error": {
                    "code": "unsupported_api_version",
                    "message": "The requested API version is not supported",
                    "supported_versions": ["1"]
                }
            })
        );
    }

    #[test]
    fn canonical_error_contract_serializes_without_schema_drift() {
        let body = serde_json::to_string(&ApiErrorEnvelopeV1 {
            error: ApiErrorV1 {
                code: "job_not_found",
                message: "The requested job does not exist.",
                job_id: None,
            },
        })
        .unwrap();

        assert_eq!(
            body,
            r#"{"error":{"code":"job_not_found","message":"The requested job does not exist."}}"#
        );
    }

    #[test]
    fn inventory_upload_contract_is_versioned_and_nested() {
        let envelope =
            InventoryUploadEnvelopeV1::new(crate::cmdb::contracts::InventorySnapshotResultV1 {
                snapshot_id: "snapshot-1".into(),
                replayed: false,
                linked: 1,
                registered: 0,
                review_required: 0,
                missing: 0,
            });

        assert_eq!(
            serde_json::to_value(envelope).unwrap(),
            serde_json::json!({
                "schema_version": 1,
                "result": {
                    "snapshot_id": "snapshot-1",
                    "replayed": false,
                    "linked": 1,
                    "registered": 0,
                    "review_required": 0,
                    "missing": 0
                }
            })
        );
    }

    #[test]
    fn checked_in_envelope_contract_artifact_matches_source_serialization() {
        let artifact: serde_json::Value = serde_json::from_str(include_str!(
            "../../contracts/api-v1-envelope-contract.json"
        ))
        .unwrap();
        let expected = serde_json::json!({
            "contract": "voidtower.api.envelopes",
            "api_version": API_VERSION,
            "requests": {
                "canonical_action_v1": {
                    "body": {"input": {}},
                    "plan_headers": {
                        "Content-Type": "application/json",
                        "x-voidtower-api-version": "1"
                    },
                    "submit_headers": {
                        "Content-Type": "application/json",
                        "Idempotency-Key": "operation-1",
                        "x-voidtower-api-version": "1"
                    },
                    "idempotency_key": "1-128 ASCII characters matching [A-Za-z0-9][A-Za-z0-9._:-]{0,127}"
                }
            },
            "envelopes": {
                "plan_success_v1": {
                    "schema_version": ACTION_ENVELOPE_SCHEMA_VERSION,
                    "resource_id": "resource-1",
                    "action": "container.start",
                    "plan": {"job_id": "job-1"}
                },
                "job_success_v1": {
                    "schema_version": ACTION_ENVELOPE_SCHEMA_VERSION,
                    "resource_id": "resource-1",
                    "action": "container.start",
                    "job": {"id": "job-1"}
                },
                "job_read_v1": {
                    "schema_version": JOB_READ_ENVELOPE_SCHEMA_VERSION,
                    "resource_id": "resource-1",
                    "action": "container.start",
                    "job": {"id": "job-1"}
                },
                "job_list_v1": {
                    "schema_version": COLLECTION_ENVELOPE_SCHEMA_VERSION,
                    "jobs": [{"id": "job-1"}]
                },
                "approval_list_v1": {
                    "schema_version": APPROVAL_ENVELOPE_SCHEMA_VERSION,
                    "approvals": [{"id": "approval-1"}]
                },
                "approval_read_v1": {
                    "schema_version": APPROVAL_ENVELOPE_SCHEMA_VERSION,
                    "approval": {"id": "approval-1"}
                },
                "error_v1": {
                    "error": {
                        "code": "job_not_found",
                        "message": "The requested job does not exist."
                    }
                },
                "version_error_v1": {
                    "error": {
                        "code": "unsupported_api_version",
                        "message": "The requested API version is not supported",
                        "supported_versions": ["1"]
                    }
                },
                "event_v1": {
                    "sequence": 1,
                    "event_id": "event-1",
                    "schema_version": 1,
                    "event_type": "job.running.v1",
                    "occurred_at": 100,
                    "actor": null,
                    "resource_id": "resource-1",
                    "job_id": "job-1",
                    "approval_id": null,
                    "correlation_id": "correlation-1",
                    "causation_id": null,
                    "payload": {}
                },
                "stream_ready_v1": {"cursor": 0, "high_water": 0},
                "stream_gap_v1": {
                    "reason": "behind_retention",
                    "requested_after": 0,
                    "earliest_available": 1,
                    "latest_available": 1
                },
                "resource_list_v1": {
                    "schema_version": RESOURCE_ENVELOPE_SCHEMA_VERSION,
                    "resources": [{
                        "id": "resource-1",
                        "kind": "container",
                        "display_name": "Example container",
                        "revision": 1
                    }]
                },
                "resource_read_v1": {
                    "schema_version": RESOURCE_ENVELOPE_SCHEMA_VERSION,
                    "resource": {
                        "id": "resource-1",
                        "kind": "container",
                        "display_name": "Example container",
                        "revision": 1
                    },
                    "aliases": [{
                        "resource_id": "resource-1",
                        "namespace": "provider",
                        "scope_key": "default",
                        "value": "container-1"
                    }],
                    "capabilities": []
                },
                "resource_capabilities_v1": {
                    "schema_version": RESOURCE_ENVELOPE_SCHEMA_VERSION,
                    "resource_id": "resource-1",
                    "capabilities": []
                },
                "event_history_v1": {
                    "schema_version": EVENT_HISTORY_ENVELOPE_SCHEMA_VERSION,
                    "events": [],
                    "next_cursor": 0,
                    "earliest_available": null,
                    "latest_available": 0
                },
                "inventory_upload_v1": {
                    "schema_version": INVENTORY_ENVELOPE_SCHEMA_VERSION,
                    "result": {
                        "snapshot_id": "snapshot-1",
                        "replayed": false,
                        "linked": 1,
                        "registered": 0,
                        "review_required": 0,
                        "missing": 0
                    }
                }
            }
        });
        assert_eq!(artifact, expected);
    }

    #[test]
    fn action_success_envelopes_serialize_without_schema_drift() {
        let plan = serde_json::to_string(&PlanSuccessEnvelopeV1::new(
            "resource-1",
            "container.start",
            serde_json::json!({"job_id": "job-1"}),
        ))
        .unwrap();
        let job = serde_json::to_string(&JobSuccessEnvelopeV1::new(
            "resource-1",
            "container.start",
            serde_json::json!({"id": "job-1"}),
        ))
        .unwrap();
        let read = serde_json::to_string(&JobReadEnvelopeV1::new(
            "resource-1",
            "container.start",
            serde_json::json!({"id": "job-1"}),
        ))
        .unwrap();

        assert_eq!(
            plan,
            r#"{"schema_version":1,"resource_id":"resource-1","action":"container.start","plan":{"job_id":"job-1"}}"#
        );
        assert_eq!(
            job,
            r#"{"schema_version":1,"resource_id":"resource-1","action":"container.start","job":{"id":"job-1"}}"#
        );
        assert_eq!(
            read,
            r#"{"schema_version":1,"resource_id":"resource-1","action":"container.start","job":{"id":"job-1"}}"#
        );
    }

    #[tokio::test]
    async fn supported_version_is_echoed_and_unsupported_version_is_rejected() {
        let app = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(negotiate));

        let supported = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .header(VERSION_HEADER, API_VERSION)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(supported.status(), StatusCode::OK);
        assert_eq!(
            supported
                .headers()
                .get(VERSION_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(API_VERSION)
        );

        let unsupported = app
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .header(VERSION_HEADER, "999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unsupported.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(
            unsupported
                .headers()
                .get(VERSION_HEADER)
                .and_then(|value| value.to_str().ok()),
            None
        );
    }

    #[tokio::test]
    async fn router_applies_contract_to_public_health_endpoint() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .header(VERSION_HEADER, API_VERSION)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(VERSION_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(API_VERSION)
        );
    }

    #[tokio::test]
    async fn unsupported_version_has_stable_json_error_envelope() {
        let app = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(negotiate));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .header(VERSION_HEADER, "999")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            body.as_ref(),
            br#"{"error":{"code":"unsupported_api_version","message":"The requested API version is not supported","supported_versions":["1"]}}"#
        );
    }

    #[tokio::test]
    async fn duplicate_versions_fail_closed() {
        let app = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(negotiate));

        let mut request = Request::builder()
            .uri("/probe")
            .body(Body::empty())
            .unwrap();
        request
            .headers_mut()
            .append(VERSION_HEADER, HeaderValue::from_static(API_VERSION));
        request
            .headers_mut()
            .append(VERSION_HEADER, HeaderValue::from_static(API_VERSION));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            body.as_ref(),
            br#"{"error":{"code":"unsupported_api_version","message":"The requested API version is not supported","supported_versions":["1"]}}"#
        );
    }

    #[tokio::test]
    async fn invalid_version_bytes_fail_closed_with_same_contract() {
        let app = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(negotiate));
        let mut request = Request::builder()
            .uri("/probe")
            .body(Body::empty())
            .unwrap();
        request
            .headers_mut()
            .insert(VERSION_HEADER, HeaderValue::from_bytes(b"\xff").unwrap());

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            body.as_ref(),
            br#"{"error":{"code":"unsupported_api_version","message":"The requested API version is not supported","supported_versions":["1"]}}"#
        );
    }

    #[tokio::test]
    async fn omitted_version_defaults_to_current_contract() {
        let app = Router::new()
            .route("/probe", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(negotiate));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/probe")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(VERSION_HEADER)
                .and_then(|value| value.to_str().ok()),
            Some(API_VERSION)
        );
    }
}
