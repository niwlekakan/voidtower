use crate::{auth, AppState};
use axum::{
    extract::State,
    http::{header, HeaderMap, Request},
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

pub async fn middleware(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Only act when there is no existing session cookie
    if !has_session_cookie(req.headers()) {
        if let Some(session_id) = resolve_session(&state, req.headers()).await {
            let cookie = format!("vt_session={session_id}");
            if let Ok(val) = axum::http::HeaderValue::from_str(&cookie) {
                req.headers_mut().insert(header::COOKIE, val);
            }
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

async fn resolve_session(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let raw_token = bearer_token(headers)?;

    // Hash for cache lookup
    let mut h = Sha256::new();
    h.update(raw_token.as_bytes());
    let token_hash = hex::encode(h.finalize());

    let now = unix_now();

    // Return cached session if still valid
    {
        let cache = state.token_sessions.read().await;
        if let Some((sid, exp)) = cache.get(&token_hash) {
            if *exp > now {
                return Some(sid.clone());
            }
        }
    }

    // Validate token against DB
    let user_id = auth::validate_api_token_any(&state.db, raw_token).await.ok()?;

    // Create a 1-hour session
    let (sid, exp) = auth::create_temp_session(&state.db, &user_id).await.ok()?;

    // Cache it
    state.token_sessions.write().await.insert(token_hash, (sid.clone(), exp));

    Some(sid)
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
