use crate::{auth, AppState};
use axum::{
    extract::{Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;

/// Paths a demo account may still hit even though they're not a GET request
/// (e.g. logging out should always work).
const EXEMPT_PATHS: &[&str] = &["/api/auth/logout"];

/// Blocks any non-GET `/api/*` request from a `role = "demo"` account with a
/// friendly, distinguishable error instead of a generic Forbidden — demo
/// accounts read real, live data but can never mutate it.
pub async fn middleware(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if req.method() != Method::GET && !EXEMPT_PATHS.contains(&req.uri().path()) {
        let jar = CookieJar::from_headers(req.headers());
        if let Some(session_id) = jar.get("vt_session").map(|c| c.value().to_string()) {
            if let Ok(Some(user)) = auth::validate_session(&state.db, &session_id).await {
                if user.role == "demo" {
                    return (
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": {
                                "code": "demo_mode",
                                "message": "Demo accounts can't make changes — you're viewing live data read-only."
                            }
                        })),
                    )
                        .into_response();
                }
            }
        }
    }
    next.run(req).await
}
