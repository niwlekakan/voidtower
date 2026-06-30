use crate::{
    ai::{AiProvider, AiRequest, ProviderConfig, build_provider},
    error::{AppError, Result},
};
use sqlx::SqlitePool;
use std::sync::Arc;

/// Central AI orchestrator — builds the provider list from DB at call time so
/// config changes take effect immediately without restart.
pub struct AiOrchestrator {
    db: SqlitePool,
}

impl AiOrchestrator {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
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
            let api_key = if let Some(key_ref) = &cfg.api_key_ref {
                // Read the raw (encrypted or plain) secret value — for MVP we store
                // the key reference in the settings table to avoid secrets table
                // complexity; actual secrets-table encryption can be added later.
                sqlx::query_scalar::<_, String>(
                    "SELECT value FROM settings WHERE key = ?",
                )
                .bind(key_ref)
                .fetch_optional(&self.db)
                .await
                .ok()
                .flatten()
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
    pub async fn stream(
        &self,
        req: &AiRequest,
    ) -> Result<(String, reqwest::Response)> {
        let (configs, providers) = self.load_providers().await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        let provider = crate::ai::router::select(&providers, req, &configs)
            .ok_or_else(|| AppError::BadRequest("No AI providers configured".into()))?;

        let provider_id = provider.id().to_string();
        let resp = provider.stream(req).await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        Ok((provider_id, resp))
    }

    /// Return (provider_id, text) via a non-streaming call.
    pub async fn complete(&self, req: &AiRequest) -> Result<(String, String)> {
        let (configs, providers) = self.load_providers().await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

        let provider = crate::ai::router::select(&providers, req, &configs)
            .ok_or_else(|| AppError::BadRequest("No AI providers configured".into()))?;

        let provider_id = provider.id().to_string();
        let text = provider.complete(req).await
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

        let api_key = if let Some(key_ref) = &cfg.api_key_ref {
            sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?")
                .bind(key_ref)
                .fetch_optional(&self.db)
                .await
                .ok()
                .flatten()
        } else {
            None
        };

        let provider = build_provider(&cfg, api_key)
            .ok_or_else(|| format!("Cannot build provider of kind '{}'", cfg.kind))?;

        provider.health_check().await
    }
}
