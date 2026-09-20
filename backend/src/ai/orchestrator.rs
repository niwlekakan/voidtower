use crate::{
    ai::{build_provider, AiProvider, AiRequest, ProviderConfig},
    api::secrets,
    error::{AppError, Result},
};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Central AI orchestrator — builds the provider list from DB at call time so
/// config changes take effect immediately without restart.
pub struct AiOrchestrator {
    db: SqlitePool,
    secrets_key: Arc<[u8; 32]>,
}

impl AiOrchestrator {
    pub fn new(db: SqlitePool, secrets_key: Arc<[u8; 32]>) -> Self {
        Self { db, secrets_key }
    }

    /// Load all enabled providers from DB and resolve their API keys from the
    /// secrets table. Returns (configs, live provider instances).
    pub async fn load_providers(
        &self,
    ) -> std::result::Result<(Vec<ProviderConfig>, Vec<Arc<dyn AiProvider>>), String> {
        let configs = sqlx::query_as::<_, ProviderConfig>(
            "SELECT id, kind, name, enabled, base_url, api_key_ref, model, priority, \
             created_at, updated_at FROM ai_providers WHERE enabled = 1 ORDER BY priority ASC",
        )
        .fetch_all(&self.db)
        .await
        .map_err(|e| format!("DB error loading providers: {e}"))?;

        let mut providers: Vec<Arc<dyn AiProvider>> = Vec::new();
        for cfg in &configs {
            let api_key = if let Some(secret_id) = &cfg.api_key_ref {
                secrets::resolve(&self.db, &self.secrets_key, secret_id, "ai_provider")
                    .await
                    .ok()
            } else {
                None
            };
            if let Some(p) = build_provider(cfg, api_key) {
                providers.push(p);
            }
        }

        Ok((configs, providers))
    }

    /// Stream a chat request through the best available provider.
    /// Returns the raw `reqwest::Response` so the caller can pipe it back to
    /// the browser unchanged (SSE / NDJSON).
    pub async fn stream(&self, req: &AiRequest) -> Result<(String, reqwest::Response)> {
        let (configs, providers) = self
            .load_providers()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        let provider = crate::ai::router::select(&providers, req, &configs).ok_or_else(|| {
            if configs.is_empty() {
                AppError::BadRequest("No AI providers configured".into())
            } else {
                AppError::Internal(anyhow::anyhow!("Configured AI providers are unavailable"))
            }
        })?;

        let provider_id = provider.id().to_string();
        let resp = provider
            .stream(req)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        Ok((provider_id, resp))
    }

    /// Return (provider_id, text) via a non-streaming call.
    pub async fn complete(&self, req: &AiRequest) -> Result<(String, String)> {
        let (configs, providers) = self
            .load_providers()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        let provider = crate::ai::router::select(&providers, req, &configs).ok_or_else(|| {
            if configs.is_empty() {
                AppError::BadRequest("No AI providers configured".into())
            } else {
                AppError::Internal(anyhow::anyhow!("Configured AI providers are unavailable"))
            }
        })?;

        let provider_id = provider.id().to_string();
        let text = provider
            .complete(req)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        Ok((provider_id, text))
    }

    /// Health-check a specific provider by id.
    pub async fn health_check(&self, provider_id: &str) -> std::result::Result<(), String> {
        let cfg = sqlx::query_as::<_, ProviderConfig>(
            "SELECT id, kind, name, enabled, base_url, api_key_ref, model, priority, \
             created_at, updated_at FROM ai_providers WHERE id = ?",
        )
        .bind(provider_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| format!("DB: {e}"))?
        .ok_or_else(|| "Provider not found".to_string())?;

        let api_key = if let Some(secret_id) = &cfg.api_key_ref {
            Some(
                secrets::resolve(&self.db, &self.secrets_key, secret_id, "ai_provider")
                    .await
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };

        let provider = build_provider(&cfg, api_key)
            .ok_or_else(|| format!("Cannot build provider of kind '{}'", cfg.kind))?;

        provider.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unavailable_provider_secret_errors_are_redacted_and_fail_closed() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::run_migrations(&db).await.unwrap();
        let key = Arc::new([7_u8; 32]);
        let secret_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, disabled, created_at, updated_at) \
             VALUES (?, 'disabled-provider-key', 'not-ciphertext', 1, 1, 1)",
        )
        .bind(&secret_id)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ai_providers \
             (id, kind, name, enabled, api_key_ref, priority, created_at, updated_at) \
             VALUES ('provider-2', 'openai', 'Disabled provider', 1, ?, 1, 1, 1)",
        )
        .bind(&secret_id)
        .execute(&db)
        .await
        .unwrap();

        let orchestrator = AiOrchestrator::new(db, key);
        let error = orchestrator.health_check("provider-2").await.unwrap_err();

        assert_eq!(error, "secret unavailable");
        assert!(!error.contains("not-ciphertext"));
    }

    #[tokio::test]
    async fn configured_unavailable_provider_is_not_treated_as_no_configuration() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::run_migrations(&db).await.unwrap();
        let key = Arc::new([7_u8; 32]);
        let secret_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, disabled, created_at, updated_at) \
             VALUES (?, 'disabled-provider-key-2', 'not-ciphertext', 1, 1, 1)",
        )
        .bind(&secret_id)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ai_providers \
             (id, kind, name, enabled, api_key_ref, priority, created_at, updated_at) \
             VALUES ('provider-3', 'openai', 'Disabled provider', 1, ?, 1, 1, 1)",
        )
        .bind(secret_id)
        .execute(&db)
        .await
        .unwrap();

        let orchestrator = AiOrchestrator::new(db, key);
        let error = match orchestrator.stream(&AiRequest::new(Vec::new())).await {
            Ok(_) => panic!("unavailable provider unexpectedly streamed"),
            Err(error) => error,
        };
        assert!(matches!(error, AppError::Internal(_)));
    }

    #[tokio::test]
    async fn load_providers_resolves_encrypted_secret_references() {
        let db = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        crate::db::run_migrations(&db).await.unwrap();
        let key = [7_u8; 32];
        let secret = crate::api::secrets::encrypt(&key, "provider-secret-value").unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) \
             VALUES ('secret-1', 'provider-key', ?, 1, 1)",
        )
        .bind(secret)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO ai_providers \
             (id, kind, name, enabled, api_key_ref, priority, created_at, updated_at) \
             VALUES ('provider-1', 'openai', 'Test provider', 1, 'secret-1', 1, 1, 1)",
        )
        .execute(&db)
        .await
        .unwrap();

        let orchestrator = AiOrchestrator::new(db.clone(), Arc::new(key));
        let (configs, providers) = orchestrator.load_providers().await.unwrap();

        assert_eq!(configs.len(), 1);
        assert_eq!(providers.len(), 1);
        let last_used_at: Option<i64> =
            sqlx::query_scalar("SELECT last_used_at FROM secrets WHERE id = 'secret-1'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(last_used_at.is_some());
        let plaintext_settings: Option<String> =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'secret-1'")
                .fetch_optional(&db)
                .await
                .unwrap();
        assert!(plaintext_settings.is_none());
    }
}
