use crate::ai::{AiProvider, AiRequest};
use std::sync::Arc;

/// Select the best available provider for a request.
/// Priority rules (in order):
///   1. Explicit `provider_id` in context → use that provider if available
///   2. Lowest `priority` number among enabled providers
///   3. If no providers configured → None
pub fn select(
    providers: &[Arc<dyn AiProvider>],
    req: &AiRequest,
    configs: &[crate::ai::ProviderConfig],
) -> Option<Arc<dyn AiProvider>> {
    // Explicit override via context.provider_id
    if let Some(id) = req
        .context
        .as_ref()
        .and_then(|c| c.get("provider_id"))
        .and_then(|v| v.as_str())
    {
        // An explicit provider request is fail-closed: never silently route
        // the request to a different provider when the requested one is
        // unavailable.
        return providers.iter().find(|p| p.id() == id).map(Arc::clone);
    }

    // Sort by priority (ascending) and pick the first reachable one
    let mut ordered: Vec<&Arc<dyn AiProvider>> = providers.iter().collect();
    ordered.sort_by_key(|p| {
        configs
            .iter()
            .find(|c| c.id == p.id())
            .map(|c| c.priority)
            .unwrap_or(999)
    });
    ordered.into_iter().next().map(Arc::clone)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubProvider(&'static str);

    #[async_trait::async_trait]
    impl AiProvider for StubProvider {
        fn id(&self) -> &str {
            self.0
        }

        fn display_name(&self) -> &str {
            self.0
        }

        fn capabilities(&self) -> crate::ai::AiCapabilities {
            crate::ai::AiCapabilities {
                reasoning: 0,
                coding: 0,
                tool_use: false,
                vision: false,
                local: true,
                streaming: false,
            }
        }

        async fn complete(&self, _req: &AiRequest) -> std::result::Result<String, String> {
            unimplemented!()
        }

        async fn stream(&self, _req: &AiRequest) -> std::result::Result<reqwest::Response, String> {
            unimplemented!()
        }

        async fn health_check(&self) -> std::result::Result<(), String> {
            unimplemented!()
        }
    }

    #[test]
    fn explicit_unavailable_provider_does_not_fall_back() {
        let provider: Arc<dyn AiProvider> = Arc::new(StubProvider("available"));
        let mut request = AiRequest::new(Vec::new());
        request.context = Some(serde_json::json!({ "provider_id": "missing" }));

        assert!(select(&[provider], &request, &[]).is_none());
    }
}
