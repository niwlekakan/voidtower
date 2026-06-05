use aes_gcm::{aead::{Aead, KeyInit, OsRng}, Aes256Gcm, Nonce};
use aes_gcm::aead::rand_core::RngCore;
use axum::{extract::{Path, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{audit, auth, error::{AppError, Result}, AppState};

fn encrypt(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| anyhow::anyhow!("encryption failed"))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ciphertext);
    Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &blob))
}

fn decrypt(key: &[u8; 32], encoded: &str) -> anyhow::Result<String> {
    let blob = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| anyhow::anyhow!("base64 decode failed"))?;
    anyhow::ensure!(blob.len() > 12, "ciphertext too short");
    let (nonce_bytes, ciphertext) = blob.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    let plaintext = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed"))?;
    String::from_utf8(plaintext).map_err(Into::into)
}

#[derive(Serialize)]
pub struct SecretMeta {
    id: String,
    name: String,
    description: Option<String>,
    created_at: i64,
    updated_at: i64,
    last_used_at: Option<i64>,
}

#[derive(Deserialize)]
pub struct CreateSecret {
    name: String,
    description: Option<String>,
    value: String,
}

#[derive(Deserialize)]
pub struct UpdateSecret {
    name: Option<String>,
    description: Option<String>,
    value: Option<String>,
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    auth_user(&state, &jar, false).await?;
    let rows = sqlx::query_as::<_, (String, String, Option<String>, i64, i64, Option<i64>)>(
        "SELECT id, name, description, created_at, updated_at, last_used_at FROM secrets ORDER BY name"
    ).fetch_all(&state.db).await.map_err(AppError::Database)?;
    let secrets: Vec<SecretMeta> = rows.into_iter().map(|(id, name, description, created_at, updated_at, last_used_at)| {
        SecretMeta { id, name, description, created_at, updated_at, last_used_at }
    }).collect();
    Ok(Json(serde_json::json!({ "secrets": secrets })))
}

pub async fn create(State(state): State<AppState>, jar: CookieJar, Json(body): Json<CreateSecret>) -> Result<Json<serde_json::Value>> {
    let user = auth_user(&state, &jar, true).await?;
    if body.name.trim().is_empty() { return Err(AppError::BadRequest("name required".into())); }
    if body.value.is_empty() { return Err(AppError::BadRequest("value required".into())); }
    let enc = encrypt(&state.secrets_key, &body.value)
        .map_err(AppError::Internal)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_ts();
    sqlx::query("INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(&body.name).bind(&body.description).bind(&enc).bind(now).bind(now)
        .execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), "human", "create_secret", Some("secret"), Some(&id), "success", None, None).await;
    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn update(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<UpdateSecret>) -> Result<Json<serde_json::Value>> {
    let user = auth_user(&state, &jar, true).await?;
    let now = now_ts();
    if let Some(v) = &body.value {
        let enc = encrypt(&state.secrets_key, v).map_err(AppError::Internal)?;
        sqlx::query("UPDATE secrets SET value_enc=?, updated_at=? WHERE id=?")
            .bind(&enc).bind(now).bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    }
    if let Some(n) = &body.name {
        sqlx::query("UPDATE secrets SET name=?, updated_at=? WHERE id=?")
            .bind(n).bind(now).bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    }
    if body.description.is_some() {
        sqlx::query("UPDATE secrets SET description=?, updated_at=? WHERE id=?")
            .bind(&body.description).bind(now).bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    }
    audit::log(&state.db, Some(&user.id), "human", "update_secret", Some("secret"), Some(&id), "success", None, None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = auth_user(&state, &jar, true).await?;
    sqlx::query("DELETE FROM secrets WHERE id=?").bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    audit::log(&state.db, Some(&user.id), "human", "delete_secret", Some("secret"), Some(&id), "success", None, None).await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn reveal(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = auth_user(&state, &jar, true).await?;
    let row = sqlx::query_as::<_, (String, String)>("SELECT name, value_enc FROM secrets WHERE id=?")
        .bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?
        .ok_or(AppError::NotFound)?;
    let value = decrypt(&state.secrets_key, &row.1).map_err(AppError::Internal)?;
    let now = now_ts();
    let _ = sqlx::query("UPDATE secrets SET last_used_at=? WHERE id=?").bind(now).bind(&id).execute(&state.db).await;
    audit::log(&state.db, Some(&user.id), "human", "reveal_secret", Some("secret"), Some(&id), "success", None, Some(&format!("name={}", row.0))).await;
    Ok(Json(serde_json::json!({ "value": value })))
}

async fn auth_user(state: &AppState, jar: &CookieJar, require_admin: bool) -> Result<auth::User> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id).await
        .map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if require_admin && user.role != "owner" && user.role != "admin" {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

fn now_ts() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}
