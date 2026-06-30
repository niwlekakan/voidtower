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
    if let Some(id) = req.context
        .as_ref()
        .and_then(|c| c.get("provider_id"))
        .and_then(|v| v.as_str())
    {
        if let Some(p) = providers.iter().find(|p| p.id() == id) {
            return Some(Arc::clone(p));
        }
    }

    // Sort by priority (ascending) and pick the first reachable one
    let mut ordered: Vec<&Arc<dyn AiProvider>> = providers.iter().collect();
    ordered.sort_by_key(|p| {
        configs.iter()
            .find(|c| c.id == p.id())
            .map(|c| c.priority)
            .unwrap_or(999)
    });

    ordered.into_iter().next().map(Arc::clone)
}
