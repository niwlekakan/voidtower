use crate::{auth, policy::ApiTokenActor, AppState};
use axum::{
    extract::State,
    http::{header, HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

/// Marker extension carrying the *declared scopes* of the Bearer token that
/// authenticated this request, inserted alongside `ApiTokenActor` so
/// `auth::scope_enforce::middleware` (which runs right after this one, see
/// `api/mod.rs`'s layer ordering) can check them against the route being hit.
#[derive(Clone)]
pub struct TokenScopes(pub Vec<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedApiToken {
    pub token_id: String,
    pub user_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CachedTokenSession {
    pub session_id: String,
    pub expires_at: i64,
    pub token: AuthenticatedApiToken,
}

pub async fn middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Only act when there is no existing session cookie
    if !has_session_cookie(req.headers()) {
        if let Some(cached) = resolve_session(&state, req.headers()).await {
            let cookie = format!("vt_session={}", cached.session_id);
            if let Ok(val) = axum::http::HeaderValue::from_str(&cookie) {
                req.headers_mut().insert(header::COOKIE, val);
            }
            // Mark request so policy engine knows this came via API token
            req.extensions_mut().insert(ApiTokenActor);
            req.extensions_mut()
                .insert(TokenScopes(cached.token.scopes.clone()));
            req.extensions_mut().insert(cached.token);
        }
    }
    next.run(req).await
}

fn has_session_cookie(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .any(|v| v.to_str().unwrap_or("").contains("vt_session="))
}

async fn resolve_session(state: &AppState, headers: &HeaderMap) -> Option<CachedTokenSession> {
    let raw_token = bearer_token(headers)?;

    // Hash for cache lookup
    let mut h = Sha256::new();
    h.update(raw_token.as_bytes());
    let token_hash = hex::encode(h.finalize());

    let now = unix_now();

    // Return cached session if still valid
    {
        let cache = state.token_sessions.read().await;
        if let Some(cached) = cache.get(&token_hash) {
            if cached.expires_at > now {
                return Some(cached.clone());
            }
        }
    }

    // Validate token against DB
    let identity = auth::validate_api_token_identity(&state.db, raw_token)
        .await
        .ok()?;
    let scopes = auth::token_scopes(&state.db, raw_token)
        .await
        .unwrap_or_default();

    // Create a 1-hour session
    let (session_id, expires_at) = auth::create_temp_session(&state.db, &identity.user_id)
        .await
        .ok()?;
    let cached = CachedTokenSession {
        session_id,
        expires_at,
        token: AuthenticatedApiToken {
            token_id: identity.token_id,
            user_id: identity.user_id,
            scopes,
        },
    };

    // Cache it
    state
        .token_sessions
        .write()
        .await
        .insert(token_hash, cached.clone());

    Some(cached)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Extension,
        http::{header, Request},
        routing::get,
        Json, Router,
    };
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    async fn probe(Extension(token): Extension<AuthenticatedApiToken>) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "token_id": token.token_id,
            "user_id": token.user_id,
            "scopes": token.scopes,
        }))
    }

    #[tokio::test]
    async fn bearer_context_preserves_stable_token_identity_on_cache_hits() {
        let db = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
             VALUES ('user-1', 'owner', 'x', 'owner', 0, 0)",
        )
        .execute(&db)
        .await
        .unwrap();
        let raw = "vt_test_stable_identity";
        let mut hash = Sha256::new();
        hash.update(raw.as_bytes());
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at) \
             VALUES ('token-1', 'user-1', 'test', ?, '[\"proxy:manage\"]', 0)",
        )
        .bind(hex::encode(hash.finalize()))
        .execute(&db)
        .await
        .unwrap();

        let state = crate::api::mcp::test_support::build(db);
        let app = Router::new()
            .route("/probe", get(probe))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                middleware,
            ))
            .with_state(state);

        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/probe")
                        .header(header::AUTHORIZATION, format!("Bearer {raw}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["token_id"], "token-1");
            assert_eq!(json["user_id"], "user-1");
            assert_eq!(json["scopes"], serde_json::json!(["proxy:manage"]));
        }
    }
}
