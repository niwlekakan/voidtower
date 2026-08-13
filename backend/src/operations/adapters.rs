//! Runtime adapter contract and dispatch registry.
//!
//! The compile-time action registry owns security and execution metadata. This registry contains
//! executable implementations only; it cannot redefine action risk, approval, schemas, retry, or
//! recovery. Startup may call `validate_complete` once all six concrete adapters are installed.

use super::contracts::{OperationPlanV1, PlannedStepV1, ResourceRef};
use crate::api::mcp::action_registry::{self, ActionExecution, ACTIONS};
use anyhow::{bail, ensure, Result};
use async_trait::async_trait;
use serde_json::Value;
use sqlx::SqlitePool;
use std::{collections::HashMap, fmt, sync::Arc};

pub mod containers;

#[derive(Debug, Clone)]
pub struct PlanRequest {
    pub action: String,
    pub resource: ResourceRef,
    pub input: Value,
}

#[derive(Debug, Clone)]
pub struct StepRequest {
    pub job_id: String,
    pub action: String,
    pub resource: ResourceRef,
    pub input: Value,
    pub step: PlannedStepV1,
    pub attempt: u32,
    /// Provider task/operation identity persisted by an uncertain execution attempt. This is
    /// absent during initial execution and present when the reconciler can verify provider state.
    pub external_operation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    Succeeded {
        result: Value,
        external_operation_id: Option<String>,
    },
    Failed {
        code: String,
        message: String,
        retryable: bool,
        diagnostic: Option<Value>,
    },
    Cancelled {
        message: String,
    },
    Uncertain {
        code: String,
        message: String,
        external_operation_id: Option<String>,
        diagnostic: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReconcileOutcome {
    Succeeded { result: Value },
    Failed { code: String, message: String },
    StillUncertain { message: String },
}

#[async_trait]
pub trait OperationAdapter: Send + Sync {
    fn key(&self) -> &'static str;

    /// Canonical action names implemented by this adapter. Every name must exist in the security
    /// registry and declare this adapter key.
    fn actions(&self) -> &[&'static str];

    /// Planning must be side-effect free. It returns a redacted immutable plan and its external
    /// fingerprint; raw credentials and provider output must never appear in the result.
    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1>;

    /// Re-read provider state immediately before execution or approval revalidation.
    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String>;

    /// Execute one immutable ordered step. Implementations must bound and redact all persisted
    /// result, error, and diagnostic values before returning.
    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome>;

    /// Verify an uncertain external attempt without replaying it.
    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome>;
}

#[derive(Default)]
pub struct AdapterRegistry {
    adapters: HashMap<&'static str, Arc<dyn OperationAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runtime implementations completed so far. This registry is safe for fail-closed planning
    /// and approval revalidation, but workers must not start until `validate_complete` succeeds.
    pub fn staged(pool: SqlitePool) -> Result<Self> {
        let mut registry = Self::new();
        registry.register(Arc::new(containers::ContainerAdapter::new(pool)))?;
        Ok(registry)
    }

    pub fn register(&mut self, adapter: Arc<dyn OperationAdapter>) -> Result<()> {
        let key = adapter.key();
        ensure!(!key.is_empty(), "operation adapter key is empty");
        ensure!(
            !self.adapters.contains_key(key),
            "duplicate runtime operation adapter: {key}"
        );
        self.adapters.insert(key, adapter);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<Arc<dyn OperationAdapter>> {
        self.adapters.get(key).cloned()
    }

    pub fn for_action(&self, action_name: &str) -> Result<Arc<dyn OperationAdapter>> {
        let action = action_registry::action(action_name)
            .ok_or_else(|| anyhow::anyhow!("unknown operation action: {action_name}"))?;
        ensure!(
            action.execution == ActionExecution::DurableJob,
            "action {action_name} is not a durable job"
        );
        let key = action
            .adapter_key
            .ok_or_else(|| anyhow::anyhow!("durable action {action_name} has no adapter key"))?;
        let adapter = self
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("no runtime adapter registered for {key}"))?;
        ensure!(
            adapter.actions().contains(&action_name),
            "adapter {key} does not implement action {action_name}"
        );
        Ok(adapter)
    }

    pub fn validate_complete(&self) -> Result<()> {
        let declared_keys: std::collections::HashSet<&str> = super::registry::ADAPTERS
            .iter()
            .map(|adapter| adapter.key)
            .collect();
        for key in self.adapters.keys() {
            ensure!(
                declared_keys.contains(key),
                "runtime adapter {key} is absent from the adapter metadata inventory"
            );
        }
        for declared in super::registry::ADAPTERS {
            ensure!(
                self.adapters.contains_key(declared.key),
                "declared adapter {} has no runtime implementation",
                declared.key
            );
        }

        let mut implemented = HashMap::<&str, &str>::new();
        for adapter in self.adapters.values() {
            ensure!(
                !adapter.actions().is_empty(),
                "runtime adapter {} implements no actions",
                adapter.key()
            );
            for action_name in adapter.actions() {
                let action = action_registry::action(action_name).ok_or_else(|| {
                    anyhow::anyhow!(
                        "runtime adapter {} exposes unknown action {}",
                        adapter.key(),
                        action_name
                    )
                })?;
                ensure!(
                    action.execution == ActionExecution::DurableJob,
                    "runtime adapter {} exposes non-durable action {}",
                    adapter.key(),
                    action_name
                );
                ensure!(
                    action.adapter_key == Some(adapter.key()),
                    "runtime adapter {} conflicts with action {} adapter metadata",
                    adapter.key(),
                    action_name
                );
                if let Some(previous) = implemented.insert(action.name, adapter.key()) {
                    bail!(
                        "action {} is implemented by both {} and {}",
                        action.name,
                        previous,
                        adapter.key()
                    );
                }
            }
        }

        for action in ACTIONS
            .iter()
            .filter(|action| action.execution == ActionExecution::DurableJob)
        {
            ensure!(
                implemented.contains_key(action.name),
                "durable action {} has no runtime implementation",
                action.name
            );
        }
        Ok(())
    }
}

impl fmt::Debug for AdapterRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut keys: Vec<_> = self.adapters.keys().copied().collect();
        keys.sort_unstable();
        formatter
            .debug_struct("AdapterRegistry")
            .field("adapter_keys", &keys)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ContractAdapter {
        key: &'static str,
        actions: Vec<&'static str>,
    }

    #[async_trait]
    impl OperationAdapter for ContractAdapter {
        fn key(&self) -> &'static str {
            self.key
        }

        fn actions(&self) -> &[&'static str] {
            &self.actions
        }

        async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
            bail!("contract-only test adapter cannot plan")
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            bail!("contract-only test adapter has no provider")
        }

        async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
            bail!("contract-only test adapter cannot execute")
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            bail!("contract-only test adapter cannot reconcile")
        }
    }

    fn complete_registry() -> AdapterRegistry {
        let mut registry = AdapterRegistry::new();
        for metadata in super::super::registry::ADAPTERS {
            let actions = ACTIONS
                .iter()
                .filter(|action| {
                    action.execution == ActionExecution::DurableJob
                        && action.adapter_key == Some(metadata.key)
                })
                .map(|action| action.name)
                .collect();
            registry
                .register(Arc::new(ContractAdapter {
                    key: metadata.key,
                    actions,
                }))
                .unwrap();
        }
        registry
    }

    #[test]
    fn complete_runtime_registry_converges_with_action_metadata() {
        complete_registry().validate_complete().unwrap();
    }

    #[test]
    fn missing_adapter_or_action_fails_closed() {
        let mut registry = complete_registry();
        registry.adapters.remove("updates");
        assert!(registry.validate_complete().is_err());

        let mut registry = complete_registry();
        let updates = registry.adapters.get_mut("updates").unwrap();
        *updates = Arc::new(ContractAdapter {
            key: "updates",
            actions: vec!["update.voidtower.check"],
        });
        assert!(registry.validate_complete().is_err());
    }

    #[test]
    fn dispatch_rejects_unknown_and_direct_actions() {
        let registry = complete_registry();
        assert!(registry.for_action("does.not.exist").is_err());
        assert!(registry.for_action("get_node_metrics").is_err());
        assert_eq!(
            registry.for_action("container.start").unwrap().key(),
            "containers"
        );
    }
}
