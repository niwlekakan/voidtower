//! Voidwatch risk lookup backed by the authoritative action registry.
//!
//! HTTP route risks and known AI/automation action risks are declared once in
//! the shared action registry. Unknown action names remain maximally risky.

pub use crate::api::mcp::action_registry::RiskClass;

pub fn for_action(action_name: &str) -> RiskClass {
    crate::api::mcp::action_registry::action(action_name)
        .map(|metadata| metadata.risk)
        .unwrap_or(RiskClass::Irreversible)
}

/// Resource types whose mutations Trusted mode must snapshot before applying.
pub const SNAPSHOT_CAPABLE_RESOURCE_TYPES: &[&str] = &["vm", "container", "app"];

pub fn requires_snapshot_before_apply(resource_type: &str) -> bool {
    SNAPSHOT_CAPABLE_RESOURCE_TYPES.contains(&resource_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mcp::action_registry;

    #[test]
    fn known_ai_ingress_actions_are_explicitly_classified() {
        for action_name in ["restart", "start", "stop", "automation.run"] {
            let metadata = action_registry::action(action_name)
                .unwrap_or_else(|| panic!("known action {action_name:?} lacks metadata"));
            assert_eq!(for_action(action_name), metadata.risk);
            assert_ne!(metadata.risk, RiskClass::Irreversible);
        }
    }

    #[test]
    fn read_only_mcp_actions_preserve_legacy_risk_lookup() {
        use action_registry::{ActionIngress, ActionKind, ACTIONS};

        let read_only_mcp_actions: Vec<_> = ACTIONS
            .iter()
            .filter(|metadata| {
                metadata.kind == ActionKind::Read
                    && metadata.ingresses.contains(&ActionIngress::Mcp)
            })
            .collect();
        assert!(
            !read_only_mcp_actions.is_empty(),
            "the legacy-risk regression must cover the MCP inventory"
        );
        for metadata in read_only_mcp_actions {
            assert_eq!(for_action(metadata.name), RiskClass::Irreversible);
        }
    }

    #[test]
    fn voidwatch_mode_set_has_explicit_irreversible_metadata() {
        let metadata = action_registry::action("voidwatch.mode.set")
            .expect("voidwatch.mode.set must be explicitly registered");
        assert_eq!(metadata.risk, RiskClass::Irreversible);
        assert_eq!(for_action("voidwatch.mode.set"), RiskClass::Irreversible);
    }

    #[test]
    fn for_action_fails_safe_for_unknown_actions() {
        assert_eq!(
            for_action("some_future_unclassified_action"),
            RiskClass::Irreversible
        );
    }
}
