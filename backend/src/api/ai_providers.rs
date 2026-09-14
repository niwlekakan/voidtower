use crate::{
    ai::ProviderConfig,
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{extract::{Path, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, Transaction};

// ── List ─────────────────────────────────────────────────────────────────────

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<ProviderConfig>>> {
    require_admin(&state, &jar).await?;
    let rows = sqlx::query_as::<_, ProviderConfig>(
        "SELECT id, kind, name, enabled, base_url, api_key_ref, model, priority, \
         created_at, updated_at FROM ai_providers ORDER BY priority ASC, created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(rows))
}

// ── Create ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateProviderReq {
    pub kind: String,
    pub name: String,
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    /// Canonical `secrets.id`; never a settings key.
    pub api_key_ref: Option<String>,
    /// If provided, encrypt immediately into the referenced/new secret.
    pub api_key_value: Option<String>,
    pub model: Option<String>,
    pub priority: Option<i64>,
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateProviderReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    validate_kind(&req.kind)?;
    validate_provider_fields(&req.kind, &req.name, req.base_url.as_deref(), req.model.as_deref(), req.priority.unwrap_or(50))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let enabled = req.enabled.unwrap_or(true);
    let priority = req.priority.unwrap_or(50);
    let mut tx = state.db.begin().await.map_err(AppError::Database)?;
    let secret_id = persist_secret_reference(
        &mut tx,
        &state.secrets_key,
        &id,
        &req.name,
        req.api_key_ref.as_deref(),
        req.api_key_value.as_deref(),
        now,
    )
    .await?;

    sqlx::query(
        "INSERT INTO ai_providers(id, kind, name, enabled, base_url, api_key_ref, model, \
         priority, created_at, updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&req.kind)
    .bind(&req.name)
    .bind(enabled)
    .bind(&req.base_url)
    .bind(&secret_id)
    .bind(&req.model)
    .bind(priority)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;
    tx.commit().await.map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

// ── Update ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateProviderReq {
    pub name: Option<String>,
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    pub api_key_ref: Option<String>,
    pub api_key_value: Option<String>,
    pub model: Option<String>,
    pub priority: Option<i64>,
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<UpdateProviderReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let mut tx = state.db.begin().await.map_err(AppError::Database)?;
    let current: Option<(String, String, Option<String>, Option<String>, i64, Option<String>)> = sqlx::query_as(
        "SELECT kind, name, base_url, model, priority, api_key_ref FROM ai_providers WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?;
    let (current_kind, current_name, current_base_url, current_model, current_priority, current_ref) = current.ok_or(AppError::NotFound)?;
    validate_provider_fields(
        &current_kind,
        req.name.as_deref().unwrap_or(&current_name),
        req.base_url.as_deref().or(current_base_url.as_deref()),
        req.model.as_deref().or(current_model.as_deref()),
        req.priority.unwrap_or(current_priority),
    )?;
    let now = unix_now();

    if let Some(name) = &req.name {
        sqlx::query("UPDATE ai_providers SET name=?, updated_at=? WHERE id=?")
            .bind(name).bind(now).bind(&id)
            .execute(&mut *tx).await.map_err(AppError::Database)?;
    }
    if let Some(enabled) = req.enabled {
        sqlx::query("UPDATE ai_providers SET enabled=?, updated_at=? WHERE id=?")
            .bind(enabled).bind(now).bind(&id)
            .execute(&mut *tx).await.map_err(AppError::Database)?;
    }
    if let Some(base_url) = &req.base_url {
        sqlx::query("UPDATE ai_providers SET base_url=?, updated_at=? WHERE id=?")
            .bind(base_url).bind(now).bind(&id)
            .execute(&mut *tx).await.map_err(AppError::Database)?;
    }
    if let Some(model) = &req.model {
        sqlx::query("UPDATE ai_providers SET model=?, updated_at=? WHERE id=?")
            .bind(model).bind(now).bind(&id)
            .execute(&mut *tx).await.map_err(AppError::Database)?;
    }
    if let Some(priority) = req.priority {
        sqlx::query("UPDATE ai_providers SET priority=?, updated_at=? WHERE id=?")
            .bind(priority).bind(now).bind(&id)
            .execute(&mut *tx).await.map_err(AppError::Database)?;
    }
    if req.api_key_ref.is_some() || req.api_key_value.is_some() {
        let secret_id = persist_secret_reference(
            &mut tx,
            &state.secrets_key,
            &id,
            req.name.as_deref().unwrap_or("AI provider"),
            req.api_key_ref.as_deref().or(current_ref.as_deref()),
            req.api_key_value.as_deref(),
            now,
        )
        .await?;
        sqlx::query("UPDATE ai_providers SET api_key_ref=?, updated_at=? WHERE id=?")
            .bind(&secret_id)
            .bind(now)
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;
    }

    tx.commit().await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Delete ───────────────────────────────────────────────────────────────────

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let rows = sqlx::query("DELETE FROM ai_providers WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?
        .rows_affected();
    if rows == 0 { return Err(AppError::NotFound); }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Health check ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResult {
    pub id: String,
    pub ok: bool,
    pub error: Option<String>,
}

pub async fn health(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<HealthResult>> {
    require_admin(&state, &jar).await?;
    let orchestrator = crate::ai::AiOrchestrator::new(state.db.clone(), state.secrets_key.clone());
    match orchestrator.health_check(&id).await {
        Ok(()) => Ok(Json(HealthResult { id, ok: true, error: None })),
        Err(e) => Ok(Json(HealthResult { id, ok: false, error: Some(redact_health_error(&e)) })),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn persist_secret_reference(
    tx: &mut Transaction<'_, Sqlite>,
    secrets_key: &[u8; 32],
    provider_id: &str,
    provider_name: &str,
    requested_ref: Option<&str>,
    value: Option<&str>,
    now: i64,
) -> Result<Option<String>> {
    if let Some(value) = value {
        if value.is_empty() {
            return Err(AppError::BadRequest("api key value required".into()));
        }
        if value.len() > crate::api::secrets::MAX_SECRET_VALUE_BYTES {
            return Err(AppError::BadRequest("api key value exceeds size limit".into()));
        }
    }

    let secret_id = match (requested_ref, value) {
        (Some(secret_id), Some(value)) => {
            validate_secret_id(secret_id)?;
            let encrypted = crate::api::secrets::encrypt(secrets_key, value)
                .map_err(AppError::Internal)?;
            let changed = sqlx::query(
                "UPDATE secrets SET value_enc=?, version=version+1, updated_at=? WHERE id=?",
            )
            .bind(encrypted)
            .bind(now)
            .bind(secret_id)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?
            .rows_affected();
            if changed == 0 {
                return Err(AppError::BadRequest("api key secret not found".into()));
            }
            Some(secret_id.to_string())
        }
        (Some(secret_id), None) => {
            validate_secret_id(secret_id)?;
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM secrets WHERE id=?")
                .bind(secret_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(AppError::Database)?;
            if exists.is_none() {
                return Err(AppError::BadRequest("api key secret not found".into()));
            }
            Some(secret_id.to_string())
        }
        (None, Some(value)) => {
            let secret_id = uuid::Uuid::new_v4().to_string();
            let encrypted = crate::api::secrets::encrypt(secrets_key, value)
                .map_err(AppError::Internal)?;
            let name = format!("ai-provider-{provider_id}");
            sqlx::query(
                "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&secret_id)
            .bind(name)
            .bind(format!("Encrypted credential for {provider_name}"))
            .bind(encrypted)
            .bind(now)
            .bind(now)
            .execute(&mut **tx)
            .await
            .map_err(AppError::Database)?;
            Some(secret_id)
        }
        (None, None) => None,
    };
    Ok(secret_id)
}

fn validate_secret_id(secret_id: &str) -> Result<()> {
    uuid::Uuid::parse_str(secret_id)
        .map(|_| ())
        .map_err(|_| AppError::BadRequest("api key reference must be a secret id".into()))
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

fn validate_provider_fields(kind: &str, name: &str, base_url: Option<&str>, model: Option<&str>, priority: i64) -> Result<()> { if name.trim().is_empty() || name.len() > 200 { return Err(AppError::BadRequest("provider name is invalid".into())); } if priority < 0 { return Err(AppError::BadRequest("provider priority must be non-negative".into())); } if let Some(value) = model { if value.trim().is_empty() || value.len() > 200 { return Err(AppError::BadRequest("provider model is invalid".into())); } } if let Some(raw) = base_url { if raw.len() > 500 { return Err(AppError::BadRequest("provider base URL is too long".into())); } let url = reqwest::Url::parse(raw).map_err(|_| AppError::BadRequest("provider base URL is invalid".into()))?; if !matches!(url.scheme(), "http" | "https") || !url.username().is_empty() || url.password().is_some() { return Err(AppError::BadRequest("provider base URL must be an HTTP(S) URL without credentials".into())); } let host = url.host_str().unwrap_or_default().trim_end_matches('.'); if kind != "local" && (host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") || host.parse::<std::net::IpAddr>().is_ok()) { return Err(AppError::BadRequest("provider base URL cannot target a local or private address".into())); } } Ok(()) } fn redact_health_error(_error: &str) -> String { "provider health check failed".into() } fn validate_kind(kind: &str) -> Result<()> {
    match kind {
        "odysseus" | "openai" | "anthropic" | "local" => Ok(()),
        _ => Err(AppError::BadRequest(format!(
            "Unknown provider kind '{}'. Valid: odysseus, openai, anthropic, local", kind
        ))),
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{header, Request, StatusCode},
    };
    use tower::ServiceExt;

    #[test]
    fn provider_validation_rejects_unsafe_endpoint_and_invalid_metadata() {
        assert!(super::validate_provider_fields("openai", " ", None, None, 1).is_err());
        assert!(super::validate_provider_fields("openai", "provider", Some("file:///etc/passwd"), None, 1).is_err());
        assert!(super::validate_provider_fields("openai", "provider", Some("http://127.0.0.1:8080"), None, 1).is_err());
        assert!(super::validate_provider_fields("local", "provider", Some("http://127.0.0.1:11434"), Some("model"), 0).is_ok());
        assert!(super::validate_provider_fields("openai", "provider", Some("https://api.example.test"), Some("model"), 0).is_ok());
    }

    #[test]
    fn provider_health_failure_is_redacted() {
        assert_eq!(super::redact_health_error("database password at http://10.0.0.1"), "provider health check failed");
    }

    #[tokio::test]
    async fn create_encrypts_api_key_and_returns_only_secret_reference() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(db.clone());
        let session = crate::api::mcp::test_support::user_with_session(&db).await;
        let secret_value = "provider-secret-value";
        let app = crate::api::router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/ai/providers")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "kind": "openai",
                            "name": "Encrypted test provider",
                            "enabled": true,
                            "api_key_value": secret_value,
                            "priority": 1
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let provider_id = payload["id"].as_str().unwrap().to_owned();
        let (secret_id, value_enc): (String, String) = sqlx::query_as(
            "SELECT api_key_ref, (SELECT value_enc FROM secrets WHERE id = api_key_ref) \
             FROM ai_providers WHERE id = ?",
        )
        .bind(provider_id)
        .fetch_one(&db)
        .await
        .unwrap();

        assert!(uuid::Uuid::parse_str(&secret_id).is_ok());
        assert_ne!(value_enc, secret_value);
        assert_eq!(crate::api::secrets::decrypt(&state.secrets_key, &value_enc).unwrap(), secret_value);
        let plaintext: Option<String> = sqlx::query_scalar(
            "SELECT value FROM settings WHERE value = ?",
        )
        .bind(secret_value)
        .fetch_optional(&db)
        .await
        .unwrap();
        assert!(plaintext.is_none());
    }
}
