use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use serde_json::json;

const NAV_DEFAULT_KEY: &str = "nav_config_default";

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

async fn require_owner(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let user = require_user(state, jar).await?;
    if user.role != "owner" {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn settings_get(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
}

async fn settings_set(state: &AppState, key: &str, value: &str) -> Result<()> {
    sqlx::query("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key)
        .bind(value)
        .bind(unix_now())
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn settings_delete(state: &AppState, key: &str) -> Result<()> {
    sqlx::query("DELETE FROM settings WHERE key = ?")
        .bind(key)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct NavConfigBody {
    pub items: serde_json::Value,
    pub nav_groups: serde_json::Value,
}

#[derive(Debug, sqlx::FromRow)]
struct NavConfigRow {
    items: String,
    nav_groups: String,
}

fn parse_value(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or(serde_json::Value::Null)
}

pub async fn get_nav_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let row = sqlx::query_as::<_, NavConfigRow>(
        "SELECT items, nav_groups FROM user_nav_config WHERE user_id = ?",
    )
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(match row {
        Some(r) => json!({
            "items": parse_value(&r.items),
            "nav_groups": parse_value(&r.nav_groups),
            "source": "user",
        }),
        None => json!({ "items": null, "nav_groups": null, "source": "none" }),
    }))
}

pub async fn save_nav_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<NavConfigBody>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let items = serde_json::to_string(&body.items).map_err(|e| AppError::Internal(e.into()))?;
    let nav_groups =
        serde_json::to_string(&body.nav_groups).map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query(
        "INSERT INTO user_nav_config (user_id, items, nav_groups, updated_at) VALUES (?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET items = excluded.items, nav_groups = excluded.nav_groups, updated_at = excluded.updated_at",
    )
    .bind(&user.id)
    .bind(items)
    .bind(nav_groups)
    .bind(unix_now())
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_nav_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    sqlx::query("DELETE FROM user_nav_config WHERE user_id = ?")
        .bind(&user.id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn get_nav_default(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    Ok(Json(match settings_get(&state, NAV_DEFAULT_KEY).await {
        Some(v) => parse_value(&v),
        None => serde_json::Value::Null,
    }))
}

pub async fn set_nav_default(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<NavConfigBody>,
) -> Result<Json<serde_json::Value>> {
    require_owner(&state, &jar).await?;
    let payload = json!({ "items": body.items, "nav_groups": body.nav_groups });
    settings_set(&state, NAV_DEFAULT_KEY, &payload.to_string()).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_nav_default(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_owner(&state, &jar).await?;
    settings_delete(&state, NAV_DEFAULT_KEY).await?;
    Ok(Json(json!({ "ok": true })))
}
