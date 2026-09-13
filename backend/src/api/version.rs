use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde_json::json;

pub const API_VERSION: &str = "1";
const VERSION_HEADER: &str = "x-voidtower-api-version";

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

fn version_error() -> (StatusCode, axum::Json<serde_json::Value>) {
    (
        StatusCode::NOT_ACCEPTABLE,
        axum::Json(json!({
            "error": {
                "code": "unsupported_api_version",
                "message": "The requested API version is not supported",
                "supported_versions": [API_VERSION]
            }
        })),
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
