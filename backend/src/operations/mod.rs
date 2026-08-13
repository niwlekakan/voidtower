//! Shared operation kernel. Some public contracts are intentionally staged before
//! their six domain adapters; keep them compiled and tested throughout that rollout.
#![allow(dead_code)]

pub mod adapters;
pub mod approvals;
pub mod canonical_json;
pub mod contracts;
pub mod events;
pub mod jobs;
pub mod registry;
pub mod resources;
pub mod state;
pub mod worker;

pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
