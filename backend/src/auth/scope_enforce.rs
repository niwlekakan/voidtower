//! Bearer-token scope enforcement at the mandatory post-authentication choke point.
//!
//! S0-03 moved every route's bearer decision into `action_registry::ROUTES`. Human session
//! requests still bypass this middleware, while token-originated requests require an explicit
//! public, unscoped, or matching-scope policy. Missing metadata and explicitly denied routes fail
//! closed.

use crate::{
    api::{
        bearer_auth::TokenScopes,
        mcp::action_registry::{self, BearerPolicy},
    },
    AppState,
};
use axum::{
    extract::{MatchedPath, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BearerDecision {
    Allow,
    InsufficientScope,
}

fn decision_for(method: &str, matched_path: &str, token_scopes: &[String]) -> BearerDecision {
    let Some(metadata) = action_registry::route(method, matched_path) else {
        return BearerDecision::InsufficientScope;
    };

    match metadata.bearer {
        BearerPolicy::Public | BearerPolicy::Unscoped => BearerDecision::Allow,
        BearerPolicy::Scope(scope) if token_scopes.iter().any(|candidate| candidate == scope) => {
            BearerDecision::Allow
        }
        BearerPolicy::Scope(_) | BearerPolicy::Denied => BearerDecision::InsufficientScope,
    }
}

pub async fn middleware(State(_state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(TokenScopes(token_scopes)) = req.extensions().get::<TokenScopes>().cloned() else {
        return next.run(req).await;
    };

    let method = req.method().as_str();
    let Some(matched_path) = req
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
    else {
        return deny();
    };

    match decision_for(method, matched_path, &token_scopes) {
        BearerDecision::Allow => next.run(req).await,
        BearerDecision::InsufficientScope => deny(),
    }
}

fn deny() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": {
                "code": "insufficient_scope",
                "message": "This API token's scopes do not permit this action."
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::{decision_for, deny, BearerDecision};
    use crate::api::mcp::action_registry::{self, BearerPolicy};

    #[test]
    fn model_job_status_routes_are_explicitly_bearer_denied() {
        for path in [
            "/api/models/download/:id",
            "/api/models/ollama/create/:id",
            "/api/models/ollama/pull/:id",
        ] {
            let metadata = action_registry::route("GET", path)
                .unwrap_or_else(|| panic!("GET {path} must have route metadata"));
            assert_eq!(
                metadata.bearer,
                BearerPolicy::Denied,
                "GET {path} must remain denied to bearer tokens"
            );
        }
    }

    #[tokio::test]
    async fn unknown_route_metadata_fails_with_structured_insufficient_scope() {
        assert_eq!(
            decision_for("GET", "/api/future-unregistered-route", &[]),
            BearerDecision::InsufficientScope
        );

        let response = deny();
        assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("read insufficient-scope response body");
        let json: serde_json::Value =
            serde_json::from_slice(&body).expect("parse insufficient-scope response body");
        assert_eq!(json["error"]["code"], "insufficient_scope");
    }
}
