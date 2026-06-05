use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use data_encoding::BASE32;
use serde::{Deserialize, Serialize};
use totp_lite::{totp_custom, Sha1, DEFAULT_STEP};

pub fn verify_totp(secret_b32: &str, code: &str) -> bool {
    let Ok(secret) = BASE32.decode(secret_b32.as_bytes()) else { return false; };
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Accept ±1 window (30 s tolerance for clock skew)
    [time.saturating_sub(30), time, time + 30]
        .iter()
        .any(|&t| totp_custom::<Sha1>(DEFAULT_STEP, 6, &secret, t) == code)
}

fn generate_secret() -> String {
    use rand::Rng;
    let bytes: [u8; 20] = rand::thread_rng().gen();
    BASE32.encode(&bytes)
}

fn totp_uri(username: &str, secret: &str) -> String {
    format!(
        "otpauth://totp/VoidTower:{username}?secret={secret}&issuer=VoidTower&algorithm=SHA1&digits=6&period=30"
    )
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)
}

#[derive(Serialize)]
pub struct TotpSetupResponse {
    pub secret: String,
    pub uri: String,
}

/// Generate a new TOTP secret and store it (not yet enabled). Returns the URI for QR display.
pub async fn setup(State(state): State<AppState>, jar: CookieJar) -> Result<Json<TotpSetupResponse>> {
    let user = require_user(&state, &jar).await?;
    let secret = generate_secret();
    let uri = totp_uri(&user.username, &secret);
    // Store secret but leave totp_enabled = false until verified
    auth::set_totp(&state.db, &user.id, Some(&secret), false)
        .await.map_err(AppError::Internal)?;
    Ok(Json(TotpSetupResponse { secret, uri }))
}

#[derive(Deserialize)]
pub struct CodeReq {
    pub code: String,
}

/// Verify the code against the pending secret and enable TOTP.
pub async fn enable(
    State(state): State<AppState>, jar: CookieJar, Json(req): Json<CodeReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let secret = user.totp_secret.as_deref()
        .ok_or_else(|| AppError::BadRequest("Call /setup first".into()))?;
    if !verify_totp(secret, &req.code) {
        return Err(AppError::BadRequest("Invalid TOTP code".into()));
    }
    auth::set_totp(&state.db, &user.id, Some(secret), true)
        .await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Verify the current code then disable TOTP and wipe the secret.
pub async fn disable(
    State(state): State<AppState>, jar: CookieJar, Json(req): Json<CodeReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if !user.totp_enabled {
        return Err(AppError::BadRequest("TOTP is not enabled".into()));
    }
    let secret = user.totp_secret.as_deref().unwrap_or("");
    if !verify_totp(secret, &req.code) {
        return Err(AppError::BadRequest("Invalid TOTP code".into()));
    }
    auth::set_totp(&state.db, &user.id, None, false)
        .await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
