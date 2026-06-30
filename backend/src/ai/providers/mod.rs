pub mod odysseus;
pub mod openai;
pub mod anthropic;
pub mod local;

use crate::ai::{AiProvider, ProviderConfig};
use std::sync::Arc;

/// Construct a boxed provider from its persisted config.
pub fn build_provider(
    cfg: &ProviderConfig,
    api_key: Option<String>,
) -> Option<Arc<dyn AiProvider>> {
    match cfg.kind.as_str() {
        "odysseus" => {
            let base_url = cfg.base_url.clone()?;
            Some(Arc::new(odysseus::OdysseusProvider::new(cfg.id.clone(), cfg.name.clone(), base_url)))
        }
        "openai" => {
            let key = api_key?;
            let base_url = cfg.base_url.clone()
                .unwrap_or_else(|| "https://api.openai.com".to_string());
            let model = cfg.model.clone()
                .unwrap_or_else(|| "gpt-4o".to_string());
            Some(Arc::new(openai::OpenAiProvider::new(cfg.id.clone(), cfg.name.clone(), base_url, key, model)))
        }
        "anthropic" => {
            let key = api_key?;
            let model = cfg.model.clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            Some(Arc::new(anthropic::AnthropicProvider::new(cfg.id.clone(), cfg.name.clone(), key, model)))
        }
        "local" => {
            let base_url = cfg.base_url.clone()
                .unwrap_or_else(|| "http://localhost:11434".to_string());
            let model = cfg.model.clone()
                .unwrap_or_else(|| "default".to_string());
            Some(Arc::new(local::LocalLlmProvider::new(cfg.id.clone(), cfg.name.clone(), base_url, model)))
        }
        _ => None,
    }
}
