#![cfg(test)]
//! Session-role test projection of the authoritative route metadata registry.
//!
//! S0-03 moved the editable role ledger to the shared action registry. This module keeps the
//! established real-router probe vocabulary while deriving every entry from that registry.

use crate::api::mcp::action_registry::{SessionPolicy, ROUTES};

pub(crate) use crate::api::mcp::action_registry::RoleTier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Public,
    Session(RoleTier),
    NonSessionCredential,
}

impl Role {
    pub(crate) fn requires_unauthenticated_401(self) -> bool {
        matches!(self, Self::Session(_))
    }

    pub(crate) fn wrong_role_tier(self) -> Option<RoleTier> {
        match self {
            Self::Session(tier) => Some(tier),
            Self::Public | Self::NonSessionCredential => None,
        }
    }
}

pub(crate) fn session_role_matrix() -> impl Iterator<Item = (&'static str, &'static str, Role)> {
    ROUTES.iter().map(|metadata| {
        let role = match metadata.session {
            SessionPolicy::Public => Role::Public,
            SessionPolicy::Required(tier) => Role::Session(tier),
            SessionPolicy::HandlerManaged => Role::NonSessionCredential,
        };
        (metadata.method.as_str(), metadata.path, role)
    })
}

/// WebSocket-upgrade routes need a real handshake before their handler-level role checks execute,
/// so the generated plain-HTTP probes exclude them. This is transport test metadata, not security
/// policy; the routes remain fully classified in `action_registry::ROUTES`.
pub(crate) const WS_UPGRADE_ROUTES: &[(&str, &str)] = &[
    ("GET", "/api/agents/ws"),
    ("GET", "/api/metrics/ws"),
    ("GET", "/api/containers/:id/logs/stream"),
    ("GET", "/api/containers/:id/exec"),
    ("GET", "/api/terminal/ws"),
    ("GET", "/api/terminal/ssh/ws"),
];
