use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;

pub const API_VERSION: &str = "1";
const VERSION_HEADER: &str = "x-voidtower-api-version";

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
    response.headers_mut().insert(
        VERSION_HEADER,
        HeaderValue::from_static(API_VERSION),
    );
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
    use axum::{
        body::Body,
        http::Request,
        routing::get,
        Router,
    };
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
        request.headers_mut().append(
            VERSION_HEADER,
            HeaderValue::from_static(API_VERSION),
        );
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
        request.headers_mut().insert(
            VERSION_HEADER,
            HeaderValue::from_bytes(b"\xff").unwrap(),
        );

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
            .oneshot(Request::builder().uri("/probe").body(Body::empty()).unwrap())
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
