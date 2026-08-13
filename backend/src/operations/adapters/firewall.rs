//! Durable UFW firewall adapter.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::mcp::action_registry::{self, RiskClass},
    networking::firewall::{
        self as firewall_service, FirewallMutation, FirewallSnapshot, MutationResult,
    },
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

const ACTIONS: &[&str] = &[
    "firewall.rule.add",
    "firewall.rule.delete",
    "firewall.enable",
    "firewall.disable",
    "firewall.reload",
    "firewall.reset",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AddRuleInput {
    action: String,
    port: Option<String>,
    proto: Option<String>,
    from: Option<String>,
    direction: Option<String>,
    comment: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteRuleInput {
    num: u32,
}

#[async_trait]
pub trait FirewallProvider: Send + Sync {
    async fn snapshot(&self) -> Result<FirewallSnapshot>;
    async fn execute(&self, mutation: FirewallMutation) -> Result<MutationResult>;
}

#[derive(Default)]
pub struct UfwFirewallProvider;

#[async_trait]
impl FirewallProvider for UfwFirewallProvider {
    async fn snapshot(&self) -> Result<FirewallSnapshot> {
        firewall_service::snapshot().await
    }

    async fn execute(&self, mutation: FirewallMutation) -> Result<MutationResult> {
        firewall_service::execute(mutation).await
    }
}

pub struct FirewallAdapter {
    provider: Arc<dyn FirewallProvider>,
}

impl FirewallAdapter {
    pub fn new() -> Self {
        Self {
            provider: Arc::new(UfwFirewallProvider),
        }
    }

    #[cfg(test)]
    fn with_provider(provider: Arc<dyn FirewallProvider>) -> Self {
        Self { provider }
    }

    async fn snapshot(&self, request: &PlanRequest) -> Result<FirewallSnapshot> {
        validate_target(&request.action, &request.resource.kind)?;
        self.provider.snapshot().await
    }
}

impl Default for FirewallAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl OperationAdapter for FirewallAdapter {
    fn key(&self) -> &'static str {
        "firewall"
    }

    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        let (_, changes) = parse_mutation(&request.action, &request.input)?;
        let metadata = action_registry::action(&request.action)
            .context("firewall action is absent from the action registry")?;
        let snapshot = self.snapshot(&request).await?;
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: plan_title(&request.action)?.into(),
            risk: risk_name(metadata.risk).into(),
            changes,
            preview: None,
            external_fingerprint: canonical_json::digest(&snapshot)?,
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: request.action,
                retry_class: metadata
                    .retry
                    .context("firewall action has no retry metadata")?
                    .class
                    .as_str()
                    .into(),
                recovery_class: metadata
                    .recovery
                    .context("firewall action has no recovery metadata")?
                    .as_str()
                    .into(),
            }],
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        canonical_json::digest(&self.snapshot(request).await?)
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        validate_target(&request.action, &request.resource.kind)?;
        ensure!(
            request.step.kind == "execute",
            "unsupported firewall step kind"
        );
        ensure!(
            request.step.name == request.action,
            "firewall step/action mismatch"
        );
        let (mutation, _) = parse_mutation(&request.action, &request.input)?;
        match self.provider.execute(mutation).await {
            Ok(result) => Ok(StepOutcome::Succeeded {
                result: serde_json::json!({
                    "action": request.action,
                    "message": result.message,
                }),
                external_operation_id: None,
            }),
            Err(error) => Ok(StepOutcome::Uncertain {
                code: "firewall_execution_uncertain".into(),
                message: crate::api::mcp::redact::redact_patterns(&format!(
                    "UFW did not report a conclusive outcome: {error}"
                )),
                external_operation_id: None,
                diagnostic: None,
            }),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        validate_target(&request.action, &request.resource.kind)?;
        let snapshot = self.provider.snapshot().await?;
        let succeeded = match request.action.as_str() {
            "firewall.enable" => snapshot.enabled,
            "firewall.disable" => !snapshot.enabled,
            "firewall.reset" => !snapshot.enabled && snapshot.rules.is_empty(),
            "firewall.rule.add" | "firewall.rule.delete" | "firewall.reload" => {
                return Ok(ReconcileOutcome::StillUncertain {
                    message: "Current UFW state cannot prove this operation completed".into(),
                });
            }
            _ => bail!("unsupported firewall action"),
        };
        if succeeded {
            Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({"action": request.action, "verified": true}),
            })
        } else {
            Ok(ReconcileOutcome::Failed {
                code: "firewall_state_mismatch".into(),
                message: "Observed UFW state does not match the requested outcome".into(),
            })
        }
    }
}

fn validate_target(action: &str, resource_kind: &str) -> Result<()> {
    ensure!(ACTIONS.contains(&action), "unsupported firewall action");
    let expected = action_registry::action(action)
        .context("firewall action is absent from the action registry")?
        .resource_kind
        .context("firewall action has no resource kind")?;
    ensure!(
        resource_kind == expected,
        "firewall action {action} requires resource kind {expected}, not {resource_kind}"
    );
    Ok(())
}

fn parse_mutation(action: &str, input: &Value) -> Result<(FirewallMutation, Vec<PlanChange>)> {
    match action {
        "firewall.rule.add" => {
            let input: AddRuleInput = serde_json::from_value(input.clone())?;
            let arguments = add_rule_arguments(&input)?;
            let command =
                crate::api::mcp::redact::redact_patterns(&format!("ufw {}", arguments.join(" ")));
            Ok((
                FirewallMutation::AddRule(arguments),
                vec![
                    PlanChange {
                        label: "Action".into(),
                        value: "Add firewall rule".into(),
                    },
                    PlanChange {
                        label: "Command".into(),
                        value: command,
                    },
                ],
            ))
        }
        "firewall.rule.delete" => {
            let input: DeleteRuleInput = serde_json::from_value(input.clone())?;
            ensure!(input.num > 0, "firewall rule number must be positive");
            Ok((
                FirewallMutation::DeleteRule(input.num),
                vec![PlanChange {
                    label: "Rule".into(),
                    value: format!("Delete UFW rule {}", input.num),
                }],
            ))
        }
        "firewall.enable" => unit_mutation(input, FirewallMutation::Enable, "Enable UFW"),
        "firewall.disable" => unit_mutation(input, FirewallMutation::Disable, "Disable UFW"),
        "firewall.reload" => unit_mutation(input, FirewallMutation::Reload, "Reload UFW"),
        "firewall.reset" => unit_mutation(input, FirewallMutation::Reset, "Reset all UFW rules"),
        _ => bail!("unsupported firewall action"),
    }
}

fn unit_mutation(
    input: &Value,
    mutation: FirewallMutation,
    description: &str,
) -> Result<(FirewallMutation, Vec<PlanChange>)> {
    ensure!(
        input.as_object().is_some_and(serde_json::Map::is_empty),
        "firewall action input must be an empty object"
    );
    Ok((
        mutation,
        vec![PlanChange {
            label: "Action".into(),
            value: description.into(),
        }],
    ))
}

fn add_rule_arguments(input: &AddRuleInput) -> Result<Vec<String>> {
    let action = input.action.to_ascii_lowercase();
    ensure!(
        matches!(action.as_str(), "allow" | "deny" | "limit"),
        "firewall rule action must be allow, deny, or limit"
    );
    let mut arguments = vec![action];
    if let Some(direction) = input.direction.as_deref() {
        ensure!(
            matches!(direction, "in" | "out"),
            "direction must be in or out"
        );
        arguments.push(direction.into());
    }
    let from = input.from.as_deref().map(str::trim).filter(|value| {
        !value.eq_ignore_ascii_case("anywhere") && !value.eq_ignore_ascii_case("any")
    });
    if let Some(from) = from {
        validate_address(from)?;
        arguments.extend(["from".into(), from.into()]);
    }
    if let Some(port) = input.port.as_deref() {
        validate_port(port)?;
        let protocol = input.proto.as_deref().unwrap_or("any");
        ensure!(
            matches!(protocol, "any" | "tcp" | "udp"),
            "protocol must be any, tcp, or udp"
        );
        let specification = if protocol == "any" {
            port.into()
        } else {
            format!("{port}/{protocol}")
        };
        if from.is_some() {
            arguments.extend(["to".into(), "any".into(), "port".into()]);
        }
        arguments.push(specification);
    }
    if let Some(comment) = input
        .comment
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        ensure!(
            !comment.contains(['\n', '\r']),
            "firewall comment contains a control character"
        );
        arguments.extend(["comment".into(), comment.into()]);
    }
    firewall_service::validate_add_rule_arguments(&arguments)?;
    Ok(arguments)
}

fn validate_address(address: &str) -> Result<()> {
    if let Some((ip, prefix)) = address.split_once('/') {
        let parsed: std::net::IpAddr = ip.parse().context("invalid firewall source address")?;
        let prefix: u8 = prefix.parse().context("invalid firewall CIDR prefix")?;
        let maximum = if parsed.is_ipv4() { 32 } else { 128 };
        ensure!(prefix <= maximum, "firewall CIDR prefix is out of range");
    } else {
        address
            .parse::<std::net::IpAddr>()
            .context("invalid firewall source address")?;
    }
    Ok(())
}

fn validate_port(port: &str) -> Result<()> {
    ensure!(!port.is_empty(), "firewall port is empty");
    ensure!(
        port.chars()
            .all(|character| character.is_ascii_digit() || matches!(character, ',' | ':')),
        "firewall port must contain digits, comma, or colon only"
    );
    Ok(())
}

fn plan_title(action: &str) -> Result<&'static str> {
    Ok(match action {
        "firewall.rule.add" => "Add firewall rule",
        "firewall.rule.delete" => "Delete firewall rule",
        "firewall.enable" => "Enable firewall",
        "firewall.disable" => "Disable firewall",
        "firewall.reload" => "Reload firewall",
        "firewall.reset" => "Reset firewall",
        _ => bail!("unsupported firewall action"),
    })
}

fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "read",
        RiskClass::Mutate => "mutate",
        RiskClass::Destructive => "destructive",
        RiskClass::Irreversible => "irreversible",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::contracts::ResourceRef;
    use std::sync::Mutex;

    struct FakeProvider {
        snapshot: Mutex<FirewallSnapshot>,
        snapshot_calls: Mutex<u32>,
        mutations: Mutex<Vec<FirewallMutation>>,
        fail_execution: bool,
    }

    #[async_trait]
    impl FirewallProvider for FakeProvider {
        async fn snapshot(&self) -> Result<FirewallSnapshot> {
            *self.snapshot_calls.lock().unwrap() += 1;
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn execute(&self, mutation: FirewallMutation) -> Result<MutationResult> {
            self.mutations.lock().unwrap().push(mutation);
            if self.fail_execution {
                bail!("provider timeout")
            }
            Ok(MutationResult {
                message: "updated".into(),
            })
        }
    }

    fn resource(kind: &str) -> ResourceRef {
        ResourceRef {
            id: "firewall-resource".into(),
            kind: kind.into(),
            display_name: "Local Firewall".into(),
            revision: 0,
        }
    }

    fn provider(enabled: bool) -> Arc<FakeProvider> {
        Arc::new(FakeProvider {
            snapshot: Mutex::new(FirewallSnapshot {
                backend: "ufw",
                enabled,
                rules: vec!["22/tcp ALLOW IN Anywhere".into()],
            }),
            snapshot_calls: Mutex::new(0),
            mutations: Mutex::new(vec![]),
            fail_execution: false,
        })
    }

    #[tokio::test]
    async fn add_rule_plan_validates_input_and_executes_structured_arguments() {
        let provider = provider(true);
        let adapter = FirewallAdapter::with_provider(provider.clone());
        let request = PlanRequest {
            action: "firewall.rule.add".into(),
            resource: resource("firewall"),
            input: serde_json::json!({
                "action": "allow",
                "port": "443",
                "proto": "tcp",
                "from": "10.0.0.0/24",
                "direction": "in",
                "comment": "api_key=known-secret-value",
            }),
        };
        let plan = adapter.plan(request.clone()).await.unwrap();
        assert_eq!(plan.steps[0].name, "firewall.rule.add");
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("known-secret-value"));
        assert_eq!(
            plan.external_fingerprint,
            adapter.external_fingerprint(&request).await.unwrap()
        );
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job".into(),
                action: request.action,
                resource: request.resource,
                input: request.input,
                step: plan.steps[0].clone(),
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Succeeded { .. }));
        assert_eq!(
            provider.mutations.lock().unwrap().as_slice(),
            &[FirewallMutation::AddRule(vec![
                "allow".into(),
                "in".into(),
                "from".into(),
                "10.0.0.0/24".into(),
                "to".into(),
                "any".into(),
                "port".into(),
                "443/tcp".into(),
                "comment".into(),
                "api_key=known-secret-value".into(),
            ])]
        );
    }

    #[tokio::test]
    async fn enable_reconciliation_is_verifiable_but_rule_change_is_not() {
        let provider = provider(true);
        let adapter = FirewallAdapter::with_provider(provider);
        let step = PlannedStepV1 {
            kind: "execute".into(),
            name: "firewall.enable".into(),
            retry_class: "never".into(),
            recovery_class: "reconcile".into(),
        };
        let enabled = adapter
            .reconcile(StepRequest {
                job_id: "job".into(),
                action: "firewall.enable".into(),
                resource: resource("firewall"),
                input: serde_json::json!({}),
                step: step.clone(),
                attempt: 2,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(enabled, ReconcileOutcome::Succeeded { .. }));
        let rule = adapter
            .reconcile(StepRequest {
                job_id: "job".into(),
                action: "firewall.rule.delete".into(),
                resource: resource("firewall_rule"),
                input: serde_json::json!({"num": 1}),
                step,
                attempt: 2,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(rule, ReconcileOutcome::StillUncertain { .. }));
    }

    #[test]
    fn invalid_source_and_port_fail_before_provider_execution() {
        let invalid_address = serde_json::json!({
            "action": "allow",
            "from": "host; reboot",
        });
        assert!(parse_mutation("firewall.rule.add", &invalid_address).is_err());
        let invalid_port = serde_json::json!({
            "action": "allow",
            "port": "22; reboot",
        });
        assert!(parse_mutation("firewall.rule.add", &invalid_port).is_err());
    }

    #[tokio::test]
    async fn invalid_plan_input_does_not_inspect_provider_state() {
        let provider = provider(true);
        let adapter = FirewallAdapter::with_provider(provider.clone());
        let result = adapter
            .plan(PlanRequest {
                action: "firewall.rule.add".into(),
                resource: resource("firewall"),
                input: serde_json::json!({"action": "allow", "port": "22; reboot"}),
            })
            .await;
        assert!(result.is_err());
        assert_eq!(*provider.snapshot_calls.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn provider_execution_error_is_classified_uncertain() {
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(FirewallSnapshot {
                backend: "ufw",
                enabled: true,
                rules: vec![],
            }),
            snapshot_calls: Mutex::new(0),
            mutations: Mutex::new(vec![]),
            fail_execution: true,
        });
        let adapter = FirewallAdapter::with_provider(provider);
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job".into(),
                action: "firewall.disable".into(),
                resource: resource("firewall"),
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "firewall.disable".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Uncertain { .. }));
    }
}
