use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not found")]
    NotFound,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Forbidden")]
    Forbidden,
    #[error("Policy denied: {0}")]
    PolicyDenied(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Payload too large")]
    PayloadTooLarge,
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Feature unavailable: {0}")]
    FeatureUnavailable(String),
    #[error("Too many requests")]
    TooManyRequests,
    #[error("TOTP code required")]
    TotpRequired,
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            AppError::PolicyDenied(m) => (StatusCode::FORBIDDEN, "policy_denied", m.clone()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            AppError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "payload_too_large",
                "Request body exceeds the allowed size".to_string(),
            ),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            AppError::FeatureUnavailable(m) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "feature_unavailable",
                m.clone(),
            ),
            AppError::TooManyRequests => (
                StatusCode::TOO_MANY_REQUESTS,
                "too_many_requests",
                "Too many failed login attempts. Try again later.".to_string(),
            ),
            AppError::TotpRequired => (
                StatusCode::FORBIDDEN,
                "totp_required",
                "TOTP code required".to_string(),
            ),
            AppError::Database(e) => {
                tracing::error!("Database error: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "A database error occurred".to_string(),
                )
            }
            AppError::Internal(e) => {
                tracing::error!("Internal error: {e:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                )
            }
        };

        (
            status,
            Json(json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    #[tokio::test]
    async fn internal_failures_return_bounded_redacted_envelopes() {
        for error in [
            AppError::Database(sqlx::Error::Protocol("secret SQL details".into())),
            AppError::Internal(anyhow::anyhow!("provider token and SQL details")),
        ] {
            let response = error.into_response();
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            let text = String::from_utf8(body.to_vec()).unwrap();
            assert!(!text.contains("secret SQL details"));
            assert!(!text.contains("provider token"));
            assert!(text.contains("internal_error") || text.contains("database_error"));
            assert!(text.len() < 256);
        }
    }
}
