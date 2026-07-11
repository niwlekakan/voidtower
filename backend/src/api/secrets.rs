use aes_gcm::{aead::{Aead, KeyInit, OsRng}, Aes256Gcm, Nonce};
use aes_gcm::aead::rand_core::RngCore;
use axum::{extract::{Path, State}, http::HeaderMap, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{audit, auth, error::{AppError, Result}, AppState};

pub(crate) fn encrypt(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
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

pub(crate) fn decrypt(key: &[u8; 32], encoded: &str) -> anyhow::Result<String> {
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
    version: i64,
}

#[derive(Deserialize)]
pub struct RotateSecret {
    new_value: Option<String>,
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

pub async fn list(State(state): State<AppState>, jar: CookieJar, headers: HeaderMap) -> Result<Json<serde_json::Value>> {
    auth_user(&state, &jar, false).await?;
    let rows = sqlx::query_as::<_, (String, String, Option<String>, i64, i64, Option<i64>, i64)>(
        "SELECT id, name, description, created_at, updated_at, last_used_at, version FROM secrets ORDER BY name"
    ).fetch_all(&state.db).await.map_err(AppError::Database)?;

    // Item #7B: if the Bearer token has secret_ids restrictions, filter the list
    let allowed = token_secret_ids(&state, &headers).await;

    let secrets: Vec<SecretMeta> = rows.into_iter()
        .filter(|(id, ..)| allowed.as_ref().map(|ids| ids.contains(id)).unwrap_or(true))
        .map(|(id, name, description, created_at, updated_at, last_used_at, version)| {
            SecretMeta { id, name, description, created_at, updated_at, last_used_at, version }
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

pub async fn reveal(State(state): State<AppState>, jar: CookieJar, headers: HeaderMap, Path(id): Path<String>) -> Result<Json<serde_json::Value>> {
    let user = auth_user(&state, &jar, true).await?;

    // Item #7B: enforce scoped token restriction
    let allowed = token_secret_ids(&state, &headers).await;
    if let Some(ids) = &allowed {
        if !ids.contains(&id) {
            audit::log(&state.db, Some(&user.id), "human", "reveal_secret", Some("secret"), Some(&id), "forbidden", None, Some("token not scoped to this secret")).await;
            return Err(AppError::Forbidden);
        }
    }

    let row = sqlx::query_as::<_, (String, String)>("SELECT name, value_enc FROM secrets WHERE id=?")
        .bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?;
    let Some((name, value_enc)) = row else {
        audit::log(&state.db, Some(&user.id), "human", "reveal_secret", Some("secret"), Some(&id), "not_found", None, None).await;
        return Err(AppError::NotFound);
    };
    let value = match decrypt(&state.secrets_key, &value_enc) {
        Ok(v) => v,
        Err(e) => {
            audit::log(&state.db, Some(&user.id), "human", "reveal_secret", Some("secret"), Some(&id), "internal_error", None, Some(&format!("name={name}, decrypt failed"))).await;
            return Err(AppError::Internal(e));
        }
    };
    let now = now_ts();
    let _ = sqlx::query("UPDATE secrets SET last_used_at=? WHERE id=?").bind(now).bind(&id).execute(&state.db).await;
    audit::log(&state.db, Some(&user.id), "human", "reveal_secret", Some("secret"), Some(&id), "success", None, Some(&format!("name={name}"))).await;
    Ok(Json(serde_json::json!({ "value": value })))
}

pub async fn rotate(State(state): State<AppState>, jar: CookieJar, Path(id): Path<String>, Json(body): Json<RotateSecret>) -> Result<Json<serde_json::Value>> {
    let user = auth_user(&state, &jar, true).await?;

    // Fetch name and existing value
    let row = sqlx::query_as::<_, (String, String, i64)>("SELECT name, value_enc, version FROM secrets WHERE id=?")
        .bind(&id).fetch_optional(&state.db).await.map_err(AppError::Database)?;
    let Some((name, existing_enc, current_version)) = row else {
        audit::log(&state.db, Some(&user.id), "human", "secret_rotated", Some("secret"), Some(&id), "not_found", None, None).await;
        return Err(AppError::NotFound);
    };

    // Use provided new_value or generate a random 32-byte hex value
    let new_value = match body.new_value {
        Some(v) if !v.is_empty() => v,
        _ => {
            let mut bytes = [0u8; 32];
            OsRng.fill_bytes(&mut bytes);
            hex::encode(bytes)
        }
    };

    // Verify the existing ciphertext is still valid (key hasn't changed)
    if let Err(e) = decrypt(&state.secrets_key, &existing_enc) {
        audit::log(&state.db, Some(&user.id), "human", "secret_rotated", Some("secret"), Some(&id), "internal_error", None, Some(&format!("name={name}, decrypt verification failed"))).await;
        return Err(AppError::Internal(e));
    }

    let enc = encrypt(&state.secrets_key, &new_value).map_err(AppError::Internal)?;
    let new_version = current_version + 1;
    let now = now_ts();

    sqlx::query("UPDATE secrets SET value_enc=?, version=?, updated_at=? WHERE id=?")
        .bind(&enc).bind(new_version).bind(now).bind(&id)
        .execute(&state.db).await.map_err(AppError::Database)?;

    audit::log(&state.db, Some(&user.id), "human", "secret_rotated", Some("secret"), Some(&id), "success", None, Some(&format!("name={},version={}", name, new_version))).await;
    Ok(Json(serde_json::json!({ "rotated": true, "version": new_version })))
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

/// Item #7B: if the request carries a Bearer token that has `secret_ids` set,
/// return Some(allowed_ids). Returns None when the token is unrestricted or
/// the request is session-authenticated (no bearer token present).
async fn token_secret_ids(state: &AppState, headers: &HeaderMap) -> Option<std::collections::HashSet<String>> {
    use sha2::{Digest, Sha256};
    let raw_token = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str().ok()?
        .strip_prefix("Bearer ")?
        .trim();

    let mut h = Sha256::new();
    h.update(raw_token.as_bytes());
    let token_hash = hex::encode(h.finalize());

    // fetch_optional returns Option<Option<String>> — outer = row found, inner = column value
    let row: Option<Option<String>> = sqlx::query_scalar(
        "SELECT secret_ids FROM api_tokens WHERE token_hash = ?"
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .ok()?;

    // If no row found, or column is NULL, the token has no restriction
    let secret_ids_json = row??.to_string();
    let ids: Vec<String> = serde_json::from_str(&secret_ids_json).ok()?;
    Some(ids.into_iter().collect())
}

fn now_ts() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

#[cfg(test)]
mod tests {
    //! P1-02 (ADR-007): the reveal-audit invariant — every attempt to create,
    //! update, delete, reveal, or rotate a secret produces exactly one
    //! `audit_log` row, regardless of outcome. Drives the real router
    //! end-to-end (`tower::ServiceExt::oneshot`), following
    //! `api/scope_bypass_tests.rs`'s precedent, so these exercise the actual
    //! middleware stack and handler bodies, not just the handler functions
    //! in isolation.
    //!
    //! Scope decisions made while closing this gap (see PR body for the
    //! full write-up):
    //! - `reveal`'s `token_secret_ids` denial and both handlers' not-found
    //!   paths are now audited (ADR-007's Decision names exactly this).
    //! - `auth_user`'s own Unauthorized/Forbidden returns (missing/invalid
    //!   session, wrong role) are deliberately left unaudited: they fire
    //!   before any secret identity is known, are shared by every handler
    //!   in this file, and are not named by any acceptance test — auditing
    //!   them would be a materially larger change than "the smallest diff
    //!   that makes the invariant hold".
    //! - `create`'s pre-ID validation failures (empty name/value) and its
    //!   `encrypt()` failure branch are left unaudited for the same reason:
    //!   no secret exists yet at that point to attach a `resource_id` to.

    use super::encrypt;
    use axum::{
        body::Body,
        extract::connect_info::ConnectInfo,
        http::{header, Request, StatusCode},
    };
    use sha2::{Digest, Sha256};
    use sqlx::SqlitePool;
    use std::net::SocketAddr;
    use tower::ServiceExt;

    async fn setup_db() -> SqlitePool {
        // Full schema (baseline + post-baseline ALTER TABLEs, e.g.
        // `api_tokens.secret_ids`, `secrets.version`, `users.expires_at`) is
        // only assembled by `init_pool`, not the baseline-only
        // `run_migrations` — this suite needs all three. A unique temp file
        // per test avoids cross-test interference.
        let path = std::env::temp_dir().join(format!("vt-p1-02-test-{}.sqlite", uuid::Uuid::new_v4()));
        crate::db::init_pool(&path).await.expect("init test db")
    }

    fn unix_now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    async fn insert_user(db: &SqlitePool, role: &str) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, force_password_change, created_at, updated_at)
             VALUES (?, ?, 'x', ?, 0, ?, ?)",
        )
        .bind(&id)
        .bind(format!("user-{id}"))
        .bind(role)
        .bind(now)
        .bind(now)
        .execute(db)
        .await
        .unwrap();
        id
    }

    async fn insert_session(db: &SqlitePool, user_id: &str) -> String {
        let id = crate::auth::generate_session_token();
        let now = unix_now();
        sqlx::query("INSERT INTO sessions (id, user_id, expires_at, created_at) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(user_id)
            .bind(now + 3600)
            .bind(now)
            .execute(db)
            .await
            .unwrap();
        id
    }

    /// A scoped API token restricted to `secret_ids`, inserted directly
    /// (bypassing the HTTP token-creation endpoint, which is P0-06/other
    /// tasks' territory). Scopes are left `[]` since this exercises
    /// `token_secret_ids`'s in-handler restriction, not `scope_enforce`'s
    /// route-level scope table.
    async fn insert_scoped_token(db: &SqlitePool, user_id: &str, secret_ids: &[&str]) -> String {
        let raw = format!("vt_test_{}", uuid::Uuid::new_v4().simple());
        let mut h = Sha256::new();
        h.update(raw.as_bytes());
        let hash = hex::encode(h.finalize());
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        let secret_ids_json = serde_json::to_string(secret_ids).unwrap();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, expires_at, created_at, secret_ids)
             VALUES (?, ?, 'test-scoped-token', ?, '[]', NULL, ?, ?)",
        )
        .bind(&id)
        .bind(user_id)
        .bind(&hash)
        .bind(now)
        .bind(&secret_ids_json)
        .execute(db)
        .await
        .unwrap();
        raw
    }

    /// Stores a validly encrypted value under the same all-zero key
    /// `test_support::build` wires up.
    async fn insert_secret(db: &SqlitePool) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        let name = format!("test-secret-{id}");
        let value_enc = encrypt(&[0u8; 32], "super-secret-value").unwrap();
        sqlx::query("INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&name)
            .bind(&value_enc)
            .bind(now)
            .bind(now)
            .execute(db)
            .await
            .unwrap();
        id
    }

    /// Stores a `value_enc` that passes `decrypt()`'s length guard (>12
    /// bytes) but fails AES-GCM tag verification, exercising the
    /// decrypt-failure path without touching `decrypt()` itself (ADR-007
    /// withholds that function).
    async fn insert_secret_with_bad_ciphertext(db: &SqlitePool) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        let name = format!("test-secret-bad-{id}");
        let value_enc = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0xAAu8; 40]);
        sqlx::query("INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind(&id)
            .bind(&name)
            .bind(&value_enc)
            .bind(now)
            .bind(now)
            .execute(db)
            .await
            .unwrap();
        id
    }

    async fn audit_rows(db: &SqlitePool, action: &str, resource_id: &str) -> Vec<(String, Option<String>)> {
        sqlx::query_as::<_, (String, Option<String>)>(
            "SELECT outcome, details FROM audit_log WHERE action=? AND resource_id=? ORDER BY timestamp ASC",
        )
        .bind(action)
        .bind(resource_id)
        .fetch_all(db)
        .await
        .unwrap()
    }

    fn with_connect_info(mut req: Request<Body>) -> Request<Body> {
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 0))));
        req
    }

    fn cookie_req(method: &str, uri: &str, session_id: &str, body: Option<serde_json::Value>) -> Request<Body> {
        let b = body.map(|v| v.to_string()).unwrap_or_default();
        with_connect_info(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::COOKIE, format!("vt_session={session_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b))
                .unwrap(),
        )
    }

    /// A request carrying *both* a valid session cookie and an
    /// `Authorization: Bearer` header. `bearer_auth::middleware` only
    /// resolves the Bearer token into a `TokenScopes` session-upgrade when
    /// no session cookie is already present, so this combination reaches
    /// the handler as a normal (no-op-for-scope-enforce) session request —
    /// exactly the case `token_secret_ids`'s own doc comment describes as
    /// "session-authenticated" while still carrying a restricted token, and
    /// the only way to reach its in-handler denial branch over HTTP: a
    /// bearer-only request to `reveal` is already denied further out by
    /// `scope_enforce`'s default-deny (no `ROUTE_SCOPES` entry for reveal),
    /// per `unlisted_route_scope_behavior_is_deliberate_and_tested` in
    /// `api/scope_bypass_tests.rs`.
    fn cookie_and_bearer_req(
        method: &str,
        uri: &str,
        session_id: &str,
        token: &str,
        body: Option<serde_json::Value>,
    ) -> Request<Body> {
        let b = body.map(|v| v.to_string()).unwrap_or_default();
        with_connect_info(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::COOKIE, format!("vt_session={session_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b))
                .unwrap(),
        )
    }

    /// Baseline: confirms current behavior before any change in this task —
    /// `reveal`'s success path already audit-logs unconditionally.
    #[tokio::test]
    async fn successful_reveal_produces_an_audit_row() {
        let db = setup_db().await;
        let admin = insert_user(&db, "admin").await;
        let session = insert_session(&db, &admin).await;
        let secret_id = insert_secret(&db).await;

        let app = crate::api::router(crate::api::mcp::test_support::build(db.clone()));
        let res = app
            .oneshot(cookie_req("GET", &format!("/api/secrets/{secret_id}/reveal"), &session, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let rows = audit_rows(&db, "reveal_secret", &secret_id).await;
        assert_eq!(rows.len(), 1, "expected exactly one audit row, got {rows:?}");
        assert_eq!(rows[0].0, "success");
    }

    /// The core regression guard from ADR-007's Context: before this task,
    /// `reveal` returned `AppError::Internal` from a failed `decrypt()` call
    /// before ever reaching the `audit::log` call, leaving no trail for a
    /// failed reveal attempt against undecryptable ciphertext.
    #[tokio::test]
    async fn failed_reveal_due_to_decrypt_error_still_produces_an_audit_row() {
        let db = setup_db().await;
        let admin = insert_user(&db, "admin").await;
        let session = insert_session(&db, &admin).await;
        let secret_id = insert_secret_with_bad_ciphertext(&db).await;

        let app = crate::api::router(crate::api::mcp::test_support::build(db.clone()));
        let res = app
            .oneshot(cookie_req("GET", &format!("/api/secrets/{secret_id}/reveal"), &session, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let rows = audit_rows(&db, "reveal_secret", &secret_id).await;
        assert_eq!(rows.len(), 1, "expected exactly one audit row, got {rows:?}");
        assert_eq!(rows[0].0, "internal_error");
    }

    /// ADR-007's Decision names this explicitly: the `token_secret_ids`
    /// scope restriction on `reveal` is unauthorized-request handling, not
    /// audit logging, and its denial path must itself be audited. Reaching
    /// that in-handler branch over HTTP requires a request that carries a
    /// restricted token but isn't blocked earlier by `scope_enforce`'s
    /// default-deny — see `cookie_and_bearer_req`'s doc comment.
    #[tokio::test]
    async fn unauthorized_reveal_attempt_produces_an_audit_row_or_is_explicitly_exempted() {
        let db = setup_db().await;
        let admin = insert_user(&db, "admin").await;
        let session = insert_session(&db, &admin).await;
        let secret_id = insert_secret(&db).await;
        let other_secret_id = insert_secret(&db).await;
        // Token is scoped to `other_secret_id` only — not the one being revealed.
        let token = insert_scoped_token(&db, &admin, &[&other_secret_id]).await;

        let app = crate::api::router(crate::api::mcp::test_support::build(db.clone()));
        let res = app
            .oneshot(cookie_and_bearer_req(
                "GET",
                &format!("/api/secrets/{secret_id}/reveal"),
                &session,
                &token,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);

        let rows = audit_rows(&db, "reveal_secret", &secret_id).await;
        assert_eq!(rows.len(), 1, "expected exactly one audit row, got {rows:?}");
        assert_eq!(rows[0].0, "forbidden");
    }

    /// Regression guard against double-logging (a call site duplicated by a
    /// future refactor) or a future change silently dropping one of the
    /// existing unconditional `audit::log` calls.
    #[tokio::test]
    async fn create_update_delete_rotate_each_produce_exactly_one_audit_row_per_call() {
        let db = setup_db().await;
        let admin = insert_user(&db, "admin").await;
        let session = insert_session(&db, &admin).await;

        let app = crate::api::router(crate::api::mcp::test_support::build(db.clone()));

        let create_res = app
            .clone()
            .oneshot(cookie_req(
                "POST",
                "/api/secrets",
                &session,
                Some(serde_json::json!({ "name": "p1-02-test-secret", "value": "v1" })),
            ))
            .await
            .unwrap();
        assert_eq!(create_res.status(), StatusCode::OK);
        let create_body = axum::body::to_bytes(create_res.into_body(), usize::MAX).await.unwrap();
        let create_json: serde_json::Value = serde_json::from_slice(&create_body).unwrap();
        let id = create_json["id"].as_str().unwrap().to_string();
        assert_eq!(audit_rows(&db, "create_secret", &id).await.len(), 1);

        let update_res = app
            .clone()
            .oneshot(cookie_req(
                "PATCH",
                &format!("/api/secrets/{id}"),
                &session,
                Some(serde_json::json!({ "description": "updated" })),
            ))
            .await
            .unwrap();
        assert_eq!(update_res.status(), StatusCode::OK);
        assert_eq!(audit_rows(&db, "update_secret", &id).await.len(), 1);

        let rotate_res = app
            .clone()
            .oneshot(cookie_req(
                "POST",
                &format!("/api/secrets/{id}/rotate"),
                &session,
                Some(serde_json::json!({})),
            ))
            .await
            .unwrap();
        assert_eq!(rotate_res.status(), StatusCode::OK);
        assert_eq!(audit_rows(&db, "secret_rotated", &id).await.len(), 1);

        let delete_res = app
            .oneshot(cookie_req("DELETE", &format!("/api/secrets/{id}"), &session, None))
            .await
            .unwrap();
        assert_eq!(delete_res.status(), StatusCode::OK);
        assert_eq!(audit_rows(&db, "delete_secret", &id).await.len(), 1);
    }

    /// A human reviewing `audit_log` after an incident must be able to tell
    /// "someone tried and failed" apart from "someone succeeded" from the
    /// row itself, not just its absence/presence.
    #[tokio::test]
    async fn audit_rows_for_failed_and_successful_reveals_are_distinguishable() {
        let db = setup_db().await;
        let admin = insert_user(&db, "admin").await;
        let session = insert_session(&db, &admin).await;
        let good_id = insert_secret(&db).await;
        let bad_id = insert_secret_with_bad_ciphertext(&db).await;

        let app = crate::api::router(crate::api::mcp::test_support::build(db.clone()));
        app.clone()
            .oneshot(cookie_req("GET", &format!("/api/secrets/{good_id}/reveal"), &session, None))
            .await
            .unwrap();
        app.oneshot(cookie_req("GET", &format!("/api/secrets/{bad_id}/reveal"), &session, None))
            .await
            .unwrap();

        let good_rows = audit_rows(&db, "reveal_secret", &good_id).await;
        let bad_rows = audit_rows(&db, "reveal_secret", &bad_id).await;
        assert_eq!(good_rows.len(), 1);
        assert_eq!(bad_rows.len(), 1);
        assert_ne!(
            good_rows[0].0, bad_rows[0].0,
            "successful and failed reveal attempts must have distinguishable outcomes"
        );
        assert_eq!(good_rows[0].0, "success");
        assert_eq!(bad_rows[0].0, "internal_error");
    }
}
