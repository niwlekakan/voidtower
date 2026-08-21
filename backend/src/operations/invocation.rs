use crate::api::mcp::action_registry::{
    self, ActionExecution, ActionIngress, ActionKind as RegistryActionKind, ActionMetadata,
    AiExposure, ApprovalPolicy, BearerPolicy, RecoveryClass, RetryClass, RiskClass, RoleTier,
};
use crate::{
    operations::{
        adapters::{AdapterRegistry, PlanRequest},
        canonical_json,
        contracts::{ActorRef, ActorType, OperationPlanV1, ResourceCapability, ResourceRef},
        jobs::{self, IdempotencyLookup, SubmissionPolicy, SubmitJob},
        resources, unix_now,
    },
    voidwatch::{self, ActionKind, Actor, ActorKind, Resource, Verdict},
};
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialContext {
    Session {
        user_id: String,
        role: String,
    },
    Bearer {
        token_id: String,
        user_id: String,
        role: String,
        scopes: Vec<String>,
    },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InvocationError {
    #[error("unknown canonical action")]
    UnknownAction,
    #[error("the current role does not permit this action")]
    Forbidden,
    #[error("the API token scope does not permit this action")]
    InsufficientScope,
    #[error("the action is not exposed to machine-capable ingress")]
    AiExposureDenied,
    #[error("resource not found")]
    ResourceNotFound,
    #[error("action and resource kinds do not match")]
    ResourceKindMismatch,
    #[error("resource capability is unavailable")]
    CapabilityUnavailable,
    #[error("resource or provider state changed during planning")]
    StaleState,
    #[error("operation planning rejected the request")]
    PlanningRejected,
    #[error("idempotency key belongs to different intent")]
    IdempotencyConflict,
    #[error("idempotency key is invalid")]
    InvalidIdempotencyKey,
    #[error("operation runtime is unavailable")]
    RuntimeUnavailable,
    #[error("an internal operation error occurred")]
    Internal,
}

#[derive(Debug, Clone)]
pub struct PreparedInvocation {
    pub action: &'static ActionMetadata,
    pub resource: ResourceRef,
    pub actor: ActorRef,
    pub ingress: String,
    pub input: Value,
    pub plan: OperationPlanV1,
    pub policy: SubmissionPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyOutcome {
    Allow,
    RequireApproval,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyPreview {
    pub outcome: PolicyOutcome,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanViewV1 {
    pub action: String,
    pub resource: ResourceRef,
    pub input_schema_id: String,
    pub result_schema_id: String,
    pub operation: OperationPlanV1,
    pub policy: PolicyPreview,
}

pub fn authorize_action(
    action: &ActionMetadata,
    credential: &CredentialContext,
) -> Result<(), InvocationError> {
    let required_role = action
        .canonical_session_role
        .ok_or(InvocationError::UnknownAction)?;
    let current_role = match credential {
        CredentialContext::Session { role, .. } | CredentialContext::Bearer { role, .. } => role,
    };
    if !role_allows(current_role, required_role) {
        return Err(InvocationError::Forbidden);
    }

    if let CredentialContext::Bearer { scopes, .. } = credential {
        if action.ai_exposure != AiExposure::Callable {
            return Err(InvocationError::AiExposureDenied);
        }
        match action.canonical_bearer {
            BearerPolicy::Scope(required) if scopes.iter().any(|scope| scope == required) => {}
            BearerPolicy::Scope(_) => return Err(InvocationError::InsufficientScope),
            BearerPolicy::Denied
            | BearerPolicy::Public
            | BearerPolicy::Unscoped
            | BearerPolicy::ActionScoped => {
                return Err(InvocationError::AiExposureDenied);
            }
        }
    }
    Ok(())
}

fn role_allows(role: &str, required: RoleTier) -> bool {
    let actual = match role {
        "owner" => 3,
        "admin" => 2,
        "operator" => 1,
        "viewer" | "member" | "guest" | "demo" => 0,
        _ => return false,
    };
    let required = match required {
        RoleTier::Session => 0,
        RoleTier::Operator => 1,
        RoleTier::Admin => 2,
        RoleTier::Owner => 3,
    };
    actual >= required
}

impl CredentialContext {
    pub fn actor(&self) -> ActorRef {
        match self {
            Self::Session { user_id, .. } => ActorRef {
                actor_type: ActorType::Human,
                id: Some(user_id.clone()),
                source: Some("http_session".into()),
            },
            Self::Bearer { token_id, .. } => ActorRef {
                actor_type: ActorType::ApiToken,
                id: Some(token_id.clone()),
                source: Some("http_bearer".into()),
            },
        }
    }

    pub fn ingress(&self) -> &'static str {
        match self {
            Self::Session { .. } => "http_session",
            Self::Bearer { .. } => "http_bearer",
        }
    }

    pub fn idempotency_scope(&self) -> String {
        match self {
            Self::Session { user_id, .. } => format!("v1:http_session:human:{user_id}"),
            Self::Bearer { token_id, .. } => {
                format!("v1:http_bearer:api_token:{token_id}")
            }
        }
    }
}

impl PreparedInvocation {
    pub fn view(&self) -> PlanViewV1 {
        let (outcome, reason) = match &self.policy {
            SubmissionPolicy::Allow => (PolicyOutcome::Allow, None),
            SubmissionPolicy::RequireApproval { reason, .. } => {
                (PolicyOutcome::RequireApproval, Some(reason.clone()))
            }
            SubmissionPolicy::Deny { reason } => (PolicyOutcome::Deny, Some(reason.clone())),
        };
        PlanViewV1 {
            action: self.action.name.into(),
            resource: self.resource.clone(),
            input_schema_id: self.action.input_schema_id.unwrap_or_default().into(),
            result_schema_id: self.action.result_schema_id.unwrap_or_default().into(),
            operation: self.plan.clone(),
            policy: PolicyPreview { outcome, reason },
        }
    }
}

pub async fn prepare(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    credential: &CredentialContext,
    resource_id: &str,
    action_name: &str,
    input: Value,
) -> Result<PreparedInvocation, InvocationError> {
    let action = action_registry::action(action_name)
        .filter(|action| {
            action.execution == ActionExecution::DurableJob
                && action.ingresses.contains(&ActionIngress::Http)
        })
        .ok_or(InvocationError::UnknownAction)?;
    authorize_action(action, credential)?;
    canonical_json::to_canonical_string(&input).map_err(|_| InvocationError::PlanningRejected)?;

    let resource = resources::get_active(pool, resource_id)
        .await
        .map_err(|_| InvocationError::Internal)?
        .ok_or(InvocationError::ResourceNotFound)?;
    if action.resource_kind != Some(resource.kind.as_str()) {
        return Err(InvocationError::ResourceKindMismatch);
    }
    let capability = available_capability(pool, resource_id, action_name).await?;
    let adapter = adapters
        .for_action(action_name)
        .map_err(|_| InvocationError::RuntimeUnavailable)?;
    let request = PlanRequest {
        action: action_name.into(),
        resource: resource.clone(),
        input: input.clone(),
    };
    let plan = adapter
        .plan(request.clone())
        .await
        .map_err(|_| InvocationError::PlanningRejected)?;
    validate_plan(action, &plan)?;

    let observed_resource = resources::get_active(pool, resource_id)
        .await
        .map_err(|_| InvocationError::Internal)?
        .ok_or(InvocationError::StaleState)?;
    if observed_resource != resource {
        return Err(InvocationError::StaleState);
    }
    let observed_capability = resources::capability(pool, resource_id, action_name)
        .await
        .map_err(|_| InvocationError::Internal)?;
    if observed_capability.as_ref() != Some(&capability)
        || observed_capability
            .as_ref()
            .is_none_or(|capability| capability.availability != "available")
    {
        return Err(InvocationError::StaleState);
    }
    let observed_fingerprint = adapter
        .external_fingerprint(&request)
        .await
        .map_err(|_| InvocationError::StaleState)?;
    if observed_fingerprint != plan.external_fingerprint {
        return Err(InvocationError::StaleState);
    }

    let policy = derive_policy(pool, credential, action, &resource).await;
    Ok(PreparedInvocation {
        action,
        resource,
        actor: credential.actor(),
        ingress: credential.ingress().into(),
        input,
        plan,
        policy,
    })
}

pub async fn submit(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    credential: &CredentialContext,
    resource_id: &str,
    action_name: &str,
    input: Value,
    idempotency_key: &str,
) -> Result<crate::operations::contracts::JobSummaryV1, InvocationError> {
    let action = action_registry::action(action_name)
        .filter(|action| {
            action.execution == ActionExecution::DurableJob
                && action.ingresses.contains(&ActionIngress::Http)
        })
        .ok_or(InvocationError::UnknownAction)?;
    authorize_action(action, credential)?;
    validate_idempotency_key(idempotency_key)?;
    let scope = credential.idempotency_scope();
    let digest = jobs::intent_digest(action_name, resource_id, &input)
        .map_err(|_| InvocationError::PlanningRejected)?;
    match jobs::lookup_idempotency(pool, &scope, idempotency_key, &digest)
        .await
        .map_err(|_| InvocationError::Internal)?
    {
        IdempotencyLookup::Existing(job) => return Ok(*job),
        IdempotencyLookup::Conflict => return Err(InvocationError::IdempotencyConflict),
        IdempotencyLookup::Missing => {}
    }

    let prepared = prepare(pool, adapters, credential, resource_id, action_name, input).await?;
    let retry = prepared
        .action
        .retry
        .ok_or(InvocationError::UnknownAction)?;
    let recovery = prepared
        .action
        .recovery
        .ok_or(InvocationError::UnknownAction)?;
    jobs::submit(
        pool,
        SubmitJob {
            action: prepared.action.name.into(),
            concurrency_key: prepared.resource.id.clone(),
            resource: prepared.resource,
            actor: prepared.actor,
            ingress: prepared.ingress,
            input: prepared.input,
            plan: prepared.plan,
            idempotency_scope: scope,
            idempotency_key: idempotency_key.into(),
            retry_class: retry.class.as_str().into(),
            recovery_class: recovery.as_str().into(),
            policy: prepared.policy,
        },
    )
    .await
    .map_err(|error| match error {
        jobs::SubmitError::IdempotencyConflict => InvocationError::IdempotencyConflict,
        jobs::SubmitError::Database(_)
        | jobs::SubmitError::Internal(_)
        | jobs::SubmitError::Integer(_) => InvocationError::Internal,
    })
}

pub fn validate_idempotency_key(key: &str) -> Result<(), InvocationError> {
    let valid = !key.is_empty()
        && key.len() <= 128
        && key.is_ascii()
        && key.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphanumeric() || (index > 0 && matches!(byte, b'.' | b'_' | b':' | b'-'))
        });
    valid
        .then_some(())
        .ok_or(InvocationError::InvalidIdempotencyKey)
}

async fn available_capability(
    pool: &SqlitePool,
    resource_id: &str,
    action: &str,
) -> Result<ResourceCapability, InvocationError> {
    let capability = resources::capability(pool, resource_id, action)
        .await
        .map_err(|_| InvocationError::Internal)?
        .ok_or(InvocationError::CapabilityUnavailable)?;
    if capability.availability != "available" {
        return Err(InvocationError::CapabilityUnavailable);
    }
    Ok(capability)
}

fn validate_plan(action: &ActionMetadata, plan: &OperationPlanV1) -> Result<(), InvocationError> {
    if plan.schema_version != 1
        || plan.title.is_empty()
        || plan.title.chars().count() > 256
        || plan.external_fingerprint.is_empty()
        || plan.external_fingerprint.chars().count() > 256
        || plan.steps.is_empty()
        || plan.steps.len() > 64
        || plan.changes.len() > 64
        || plan.risk != risk_name(action.risk)
        || plan.changes.iter().any(|change| {
            change.label.is_empty()
                || change.label.chars().count() > 128
                || change.value.chars().count() > 2 * 1024
        })
        || plan
            .preview
            .as_ref()
            .is_some_and(|preview| preview.chars().count() > 16 * 1024)
        || plan.steps.iter().any(|step| {
            step.kind.is_empty()
                || step.kind.chars().count() > 64
                || step.name.is_empty()
                || step.name.chars().count() > 256
        })
    {
        return Err(InvocationError::PlanningRejected);
    }
    let encoded =
        canonical_json::to_canonical_string(plan).map_err(|_| InvocationError::PlanningRejected)?;
    if crate::api::mcp::redact::redact_patterns(&encoded) != encoded {
        return Err(InvocationError::PlanningRejected);
    }
    let retry = action.retry.ok_or(InvocationError::UnknownAction)?;
    let recovery = action.recovery.ok_or(InvocationError::UnknownAction)?;
    if plan.steps.iter().any(|step| {
        step.retry_class != retry_name(retry.class)
            || step.recovery_class != recovery_name(recovery)
    }) {
        return Err(InvocationError::PlanningRejected);
    }
    Ok(())
}

async fn derive_policy(
    pool: &SqlitePool,
    credential: &CredentialContext,
    action: &ActionMetadata,
    resource: &ResourceRef,
) -> SubmissionPolicy {
    let actor_kind = match credential {
        CredentialContext::Session { .. } => ActorKind::User,
        CredentialContext::Bearer { .. } => ActorKind::ApiToken,
    };
    let action_kind = match action.kind {
        RegistryActionKind::Read => ActionKind::Read,
        RegistryActionKind::Mutating => ActionKind::Mutating,
    };
    let verdict = voidwatch::evaluate(
        pool,
        Actor { kind: actor_kind },
        action_kind,
        action.name,
        Resource {
            resource_type: &resource.kind,
            resource_id: &resource.id,
        },
    )
    .await;
    match verdict {
        Verdict::Deny(reason) => SubmissionPolicy::Deny {
            reason: safe_reason(&reason),
        },
        Verdict::RequireApproval(reason) => approval(action, &reason),
        Verdict::AllowRequireSnapshot(reason) => approval(
            action,
            &format!("Snapshot precondition requires human approval: {reason}"),
        ),
        Verdict::Allow if action.approval == ApprovalPolicy::Always => approval(
            action,
            "Action registry requires approval for this irreversible operation",
        ),
        Verdict::Allow => SubmissionPolicy::Allow,
    }
}

fn approval(action: &ActionMetadata, reason: &str) -> SubmissionPolicy {
    SubmissionPolicy::RequireApproval {
        requirement: match action.approval {
            ApprovalPolicy::Always => "always",
            ApprovalPolicy::RiskLadder => "risk_ladder",
            ApprovalPolicy::NotApplicable => "policy",
        }
        .into(),
        reason: safe_reason(reason),
        expires_at: unix_now() + 15 * 60,
    }
}

fn safe_reason(reason: &str) -> String {
    crate::api::mcp::redact::redact_patterns(reason)
        .chars()
        .take(1024)
        .collect()
}

const fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "read",
        RiskClass::Mutate => "mutate",
        RiskClass::Destructive => "destructive",
        RiskClass::Irreversible => "irreversible",
    }
}

const fn retry_name(retry: RetryClass) -> &'static str {
    retry.as_str()
}

const fn recovery_name(recovery: RecoveryClass) -> &'static str {
    recovery.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::mcp::action_registry,
        operations::{
            adapters::{
                AdapterRegistry, OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome,
                StepRequest,
            },
            contracts::{CapabilityAvailability, OperationPlanV1, PlanChange, PlannedStepV1},
            resources::{self, ObserveResource},
        },
    };
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct PlanningAdapter {
        calls: Arc<AtomicUsize>,
    }

    struct CapabilityChangingAdapter {
        pool: sqlx::SqlitePool,
    }

    #[async_trait]
    impl OperationAdapter for CapabilityChangingAdapter {
        fn key(&self) -> &'static str {
            "containers"
        }

        fn actions(&self) -> &[&'static str] {
            &[
                "container.start",
                "container.stop",
                "container.restart",
                "container.remove",
                "container.compose.apply",
            ]
        }

        async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
            resources::set_capability(
                &self.pool,
                &request.resource.id,
                &request.action,
                CapabilityAvailability::Unavailable,
                Some("provider_changed"),
                None,
                "changed-during-plan",
            )
            .await?;
            Ok(OperationPlanV1 {
                schema_version: 1,
                title: "Start container".into(),
                risk: "mutate".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: "provider-state-1".into(),
                steps: vec![PlannedStepV1 {
                    kind: "execute".into(),
                    name: "Start".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                }],
            })
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            Ok("provider-state-1".into())
        }

        async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
            unreachable!()
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            unreachable!()
        }
    }

    #[async_trait]
    impl OperationAdapter for PlanningAdapter {
        fn key(&self) -> &'static str {
            "containers"
        }

        fn actions(&self) -> &[&'static str] {
            &[
                "container.start",
                "container.stop",
                "container.restart",
                "container.remove",
                "container.compose.apply",
            ]
        }

        async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(OperationPlanV1 {
                schema_version: 1,
                title: "Start container".into(),
                risk: "mutate".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: "provider-state-1".into(),
                steps: vec![PlannedStepV1 {
                    kind: "execute".into(),
                    name: format!("{} {}", request.action, request.resource.display_name),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                }],
            })
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            Ok("provider-state-1".into())
        }

        async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
            unreachable!()
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            unreachable!()
        }
    }

    async fn planning_fixture(
        capability: Option<CapabilityAvailability>,
    ) -> (
        sqlx::SqlitePool,
        AdapterRegistry,
        crate::operations::contracts::ResourceRef,
        Arc<AtomicUsize>,
    ) {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: "container",
                display_name: "web",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "web",
            },
            None,
            "setup",
        )
        .await
        .unwrap();
        if let Some(availability) = capability {
            resources::set_capability(
                &pool,
                &resource.id,
                "container.start",
                availability,
                None,
                None,
                "setup-capability",
            )
            .await
            .unwrap();
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let mut adapters = AdapterRegistry::new();
        adapters
            .register(Arc::new(PlanningAdapter {
                calls: calls.clone(),
            }))
            .unwrap();
        (pool, adapters, resource, calls)
    }

    #[test]
    fn canonical_authorization_fails_closed_before_planning() {
        let container = action_registry::action("container.start").unwrap();
        assert!(authorize_action(
            container,
            &CredentialContext::Session {
                user_id: "operator-1".into(),
                role: "operator".into(),
            },
        )
        .is_ok());
        assert_eq!(
            authorize_action(
                container,
                &CredentialContext::Session {
                    user_id: "viewer-1".into(),
                    role: "viewer".into(),
                },
            ),
            Err(InvocationError::Forbidden)
        );
        assert_eq!(
            authorize_action(
                container,
                &CredentialContext::Bearer {
                    token_id: "token-1".into(),
                    user_id: "owner-1".into(),
                    role: "owner".into(),
                    scopes: vec!["containers:read".into()],
                },
            ),
            Err(InvocationError::InsufficientScope)
        );

        let proxy = action_registry::action("proxy.rule.create").unwrap();
        assert!(authorize_action(
            proxy,
            &CredentialContext::Bearer {
                token_id: "token-1".into(),
                user_id: "owner-1".into(),
                role: "owner".into(),
                scopes: vec!["proxy:manage".into()],
            },
        )
        .is_ok());

        let backup_check = action_registry::action("backup.check").unwrap();
        assert_eq!(
            authorize_action(
                backup_check,
                &CredentialContext::Bearer {
                    token_id: "token-1".into(),
                    user_id: "owner-1".into(),
                    role: "owner".into(),
                    scopes: vec!["backups:run".into()],
                },
            ),
            Err(InvocationError::AiExposureDenied)
        );
    }

    #[test]
    fn canonical_role_lattice_is_closed_and_ordered() {
        for role in ["viewer", "member", "guest", "demo"] {
            assert!(role_allows(role, RoleTier::Session), "{role}");
            assert!(!role_allows(role, RoleTier::Operator), "{role}");
        }
        assert!(role_allows("operator", RoleTier::Operator));
        assert!(!role_allows("operator", RoleTier::Admin));
        assert!(role_allows("admin", RoleTier::Admin));
        assert!(!role_allows("admin", RoleTier::Owner));
        assert!(role_allows("owner", RoleTier::Owner));
        assert!(!role_allows("future-role", RoleTier::Session));
    }

    #[test]
    fn all_durable_actions_enforce_the_exhaustive_session_and_bearer_matrix() {
        let durable: Vec<_> = action_registry::ACTIONS
            .iter()
            .filter(|action| action.execution == ActionExecution::DurableJob)
            .collect();
        assert_eq!(durable.len(), 51);
        for action in durable {
            let required = action.canonical_session_role.unwrap();
            let allowed_role = match required {
                RoleTier::Session => "viewer",
                RoleTier::Operator => "operator",
                RoleTier::Admin => "admin",
                RoleTier::Owner => "owner",
            };
            assert!(
                authorize_action(
                    action,
                    &CredentialContext::Session {
                        user_id: "allowed".into(),
                        role: allowed_role.into(),
                    },
                )
                .is_ok(),
                "{} rejected its declared session role",
                action.name
            );
            let denied_role = match required {
                RoleTier::Session => "future-role",
                RoleTier::Operator => "viewer",
                RoleTier::Admin => "operator",
                RoleTier::Owner => "admin",
            };
            assert_eq!(
                authorize_action(
                    action,
                    &CredentialContext::Session {
                        user_id: "denied".into(),
                        role: denied_role.into(),
                    },
                ),
                Err(InvocationError::Forbidden),
                "{} accepted a role below its threshold",
                action.name
            );

            match action.canonical_bearer {
                BearerPolicy::Scope(scope) => {
                    assert!(authorize_action(
                        action,
                        &CredentialContext::Bearer {
                            token_id: "token".into(),
                            user_id: "owner".into(),
                            role: "owner".into(),
                            scopes: vec![scope.into()],
                        },
                    )
                    .is_ok());
                    assert_eq!(
                        authorize_action(
                            action,
                            &CredentialContext::Bearer {
                                token_id: "token".into(),
                                user_id: "owner".into(),
                                role: "owner".into(),
                                scopes: vec!["wrong:scope".into()],
                            },
                        ),
                        Err(InvocationError::InsufficientScope)
                    );
                }
                BearerPolicy::Denied => assert_eq!(
                    authorize_action(
                        action,
                        &CredentialContext::Bearer {
                            token_id: "token".into(),
                            user_id: "owner".into(),
                            role: "owner".into(),
                            scopes: vec!["*".into()],
                        },
                    ),
                    Err(InvocationError::AiExposureDenied)
                ),
                other => panic!("{} has unsafe bearer policy {other:?}", action.name),
            }
        }
    }

    #[test]
    fn idempotency_keys_use_the_bounded_public_grammar() {
        for valid in ["a", "request-1", "client.intent_2:retry"] {
            assert!(validate_idempotency_key(valid).is_ok(), "{valid}");
        }
        for invalid in [
            "",
            "-starts-with-punctuation",
            "contains space",
            "contains/slash",
            "contains\nnewline",
        ] {
            assert_eq!(
                validate_idempotency_key(invalid),
                Err(InvocationError::InvalidIdempotencyKey),
                "{invalid:?}"
            );
        }
        assert_eq!(
            validate_idempotency_key(&"a".repeat(129)),
            Err(InvocationError::InvalidIdempotencyKey)
        );
    }

    #[test]
    fn idempotency_scopes_separate_users_tokens_and_ingress_kinds() {
        let first_session = CredentialContext::Session {
            user_id: "user-1".into(),
            role: "operator".into(),
        };
        let second_session = CredentialContext::Session {
            user_id: "user-2".into(),
            role: "operator".into(),
        };
        let first_token = CredentialContext::Bearer {
            token_id: "token-1".into(),
            user_id: "user-1".into(),
            role: "owner".into(),
            scopes: vec![],
        };
        let second_token = CredentialContext::Bearer {
            token_id: "token-2".into(),
            user_id: "user-1".into(),
            role: "owner".into(),
            scopes: vec![],
        };

        let scopes = [
            first_session.idempotency_scope(),
            second_session.idempotency_scope(),
            first_token.idempotency_scope(),
            second_token.idempotency_scope(),
        ];
        assert_eq!(
            scopes
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            4
        );
        assert_eq!(scopes[0], "v1:http_session:human:user-1");
        assert_eq!(scopes[2], "v1:http_bearer:api_token:token-1");
    }

    #[test]
    fn plan_validation_rejects_secret_shaped_persisted_content() {
        let action = action_registry::action("container.start").unwrap();
        let plan = OperationPlanV1 {
            schema_version: 1,
            title: "Start with token: super-secret-value".into(),
            risk: "mutate".into(),
            changes: vec![],
            preview: None,
            external_fingerprint: "provider-state-1".into(),
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: "Start container".into(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
            }],
        };

        assert_eq!(
            validate_plan(action, &plan),
            Err(InvocationError::PlanningRejected)
        );
    }

    #[test]
    fn plan_validation_enforces_field_and_collection_bounds() {
        let action = action_registry::action("container.start").unwrap();
        let valid = OperationPlanV1 {
            schema_version: 1,
            title: "Start container".into(),
            risk: "mutate".into(),
            changes: vec![PlanChange {
                label: "state".into(),
                value: "running".into(),
            }],
            preview: Some("container will start".into()),
            external_fingerprint: "provider-state-1".into(),
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: "Start container".into(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
            }],
        };
        assert!(validate_plan(action, &valid).is_ok());

        let mut empty_title = valid.clone();
        empty_title.title.clear();
        assert_eq!(
            validate_plan(action, &empty_title),
            Err(InvocationError::PlanningRejected)
        );

        let mut long_title = valid.clone();
        long_title.title = "x".repeat(257);
        assert_eq!(
            validate_plan(action, &long_title),
            Err(InvocationError::PlanningRejected)
        );

        let mut many_changes = valid.clone();
        many_changes.changes = vec![valid.changes[0].clone(); 65];
        assert_eq!(
            validate_plan(action, &many_changes),
            Err(InvocationError::PlanningRejected)
        );

        let mut long_change = valid.clone();
        long_change.changes[0].label = "x".repeat(129);
        assert_eq!(
            validate_plan(action, &long_change),
            Err(InvocationError::PlanningRejected)
        );

        let mut long_preview = valid.clone();
        long_preview.preview = Some("x".repeat(16 * 1024 + 1));
        assert_eq!(
            validate_plan(action, &long_preview),
            Err(InvocationError::PlanningRejected)
        );

        let mut many_steps = valid.clone();
        many_steps.steps = vec![valid.steps[0].clone(); 65];
        assert_eq!(
            validate_plan(action, &many_steps),
            Err(InvocationError::PlanningRejected)
        );

        let mut long_step_name = valid;
        long_step_name.steps[0].name = "x".repeat(257);
        assert_eq!(
            validate_plan(action, &long_step_name),
            Err(InvocationError::PlanningRejected)
        );
    }

    #[tokio::test]
    async fn planning_requires_available_capability_and_persists_no_job() {
        for availability in [
            None,
            Some(CapabilityAvailability::Unknown),
            Some(CapabilityAvailability::Unavailable),
        ] {
            let (pool, adapters, resource, calls) = planning_fixture(availability).await;
            let result = prepare(
                &pool,
                &adapters,
                &CredentialContext::Session {
                    user_id: "operator-1".into(),
                    role: "operator".into(),
                },
                &resource.id,
                "container.start",
                serde_json::json!({}),
            )
            .await;
            assert_eq!(result.unwrap_err(), InvocationError::CapabilityUnavailable);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(jobs, 0);
        }
    }

    #[tokio::test]
    async fn planning_fails_closed_when_the_runtime_adapter_is_unavailable() {
        let (pool, _adapters, resource, calls) =
            planning_fixture(Some(CapabilityAvailability::Available)).await;
        let result = prepare(
            &pool,
            &AdapterRegistry::new(),
            &CredentialContext::Session {
                user_id: "operator-1".into(),
                role: "operator".into(),
            },
            &resource.id,
            "container.start",
            serde_json::json!({}),
        )
        .await;

        assert_eq!(result.unwrap_err(), InvocationError::RuntimeUnavailable);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn planning_returns_the_exact_adapter_plan_without_creating_a_job() {
        let (pool, adapters, resource, calls) =
            planning_fixture(Some(CapabilityAvailability::Available)).await;
        let prepared = prepare(
            &pool,
            &adapters,
            &CredentialContext::Session {
                user_id: "operator-1".into(),
                role: "operator".into(),
            },
            &resource.id,
            "container.start",
            serde_json::json!({}),
        )
        .await
        .unwrap();

        assert_eq!(prepared.plan.title, "Start container");
        assert_eq!(prepared.plan.external_fingerprint, "provider-state-1");
        assert_eq!(prepared.resource, resource);
        assert_eq!(prepared.policy, SubmissionPolicy::Allow);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(jobs, 0);
    }

    #[tokio::test]
    async fn submission_replays_full_intent_before_another_provider_read() {
        let (pool, adapters, resource, calls) =
            planning_fixture(Some(CapabilityAvailability::Available)).await;
        let credential = CredentialContext::Session {
            user_id: "operator-1".into(),
            role: "operator".into(),
        };
        let first = submit(
            &pool,
            &adapters,
            &credential,
            &resource.id,
            "container.start",
            serde_json::json!({"requested": true}),
            "intent-1",
        )
        .await
        .unwrap();
        let replay = submit(
            &pool,
            &adapters,
            &credential,
            &resource.id,
            "container.start",
            serde_json::json!({"requested": true}),
            "intent-1",
        )
        .await
        .unwrap();

        assert_eq!(first.id, replay.id);
        assert_eq!(first.plan, replay.plan);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        let conflict = submit(
            &pool,
            &adapters,
            &credential,
            &resource.id,
            "container.start",
            serde_json::json!({"requested": false}),
            "intent-1",
        )
        .await;
        assert_eq!(conflict.unwrap_err(), InvocationError::IdempotencyConflict);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn policy_mapping_never_weakens_assisted_snapshot_or_always_approval() {
        let (pool, _adapters, resource, _calls) =
            planning_fixture(Some(CapabilityAvailability::Available)).await;
        let credential = CredentialContext::Session {
            user_id: "operator-1".into(),
            role: "operator".into(),
        };
        let container = action_registry::action("container.start").unwrap();

        sqlx::query(
            "INSERT INTO voidwatch_mode_settings (scope, mode, updated_at) \
             VALUES ('global', 'assisted', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            derive_policy(&pool, &credential, container, &resource).await,
            SubmissionPolicy::RequireApproval { .. }
        ));

        sqlx::query("UPDATE voidwatch_mode_settings SET mode = 'observer' WHERE scope = 'global'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            derive_policy(&pool, &credential, container, &resource).await,
            SubmissionPolicy::Deny { .. }
        ));

        sqlx::query("UPDATE voidwatch_mode_settings SET mode = 'trusted' WHERE scope = 'global'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO voidwatch_default_allowlist \
             (id, actor_type, action, resource_type, created_at) \
             VALUES ('allow-1', 'user', 'container.start', 'container', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let snapshot_policy = derive_policy(&pool, &credential, container, &resource).await;
        assert!(matches!(
            snapshot_policy,
            SubmissionPolicy::RequireApproval { ref reason, .. }
                if reason.contains("Snapshot precondition")
        ));

        sqlx::query("DELETE FROM voidwatch_mode_settings")
            .execute(&pool)
            .await
            .unwrap();
        let irreversible = action_registry::action("update.voidtower.apply").unwrap();
        let update_resource = crate::operations::contracts::ResourceRef {
            id: "update-target".into(),
            kind: "update_target".into(),
            display_name: "VoidTower".into(),
            revision: 0,
        };
        let always = derive_policy(&pool, &credential, irreversible, &update_resource).await;
        assert!(matches!(
            always,
            SubmissionPolicy::RequireApproval {
                ref requirement,
                ..
            } if requirement == "always"
        ));

        let before = unix_now();
        let expiry = match approval(container, "approval required") {
            SubmissionPolicy::RequireApproval { expires_at, .. } => expires_at,
            other => panic!("unexpected approval policy {other:?}"),
        };
        let after = unix_now();
        assert!(
            (before + 15 * 60..=after + 15 * 60).contains(&expiry),
            "approval expiry must be exactly fifteen minutes from evaluation"
        );
    }

    #[tokio::test]
    async fn capability_change_during_planning_is_reported_as_stale_state() {
        let (pool, _adapters, resource, _calls) =
            planning_fixture(Some(CapabilityAvailability::Available)).await;
        let mut adapters = AdapterRegistry::new();
        adapters
            .register(Arc::new(CapabilityChangingAdapter { pool: pool.clone() }))
            .unwrap();
        let result = prepare(
            &pool,
            &adapters,
            &CredentialContext::Session {
                user_id: "operator-1".into(),
                role: "operator".into(),
            },
            &resource.id,
            "container.start",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(result.unwrap_err(), InvocationError::StaleState);
    }
}
