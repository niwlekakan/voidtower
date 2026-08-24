//! Shared bridge from compatibility HTTP handlers to the canonical durable invocation service.
//!
//! Compatibility routes keep accepting their established resource identifiers and request
//! bodies, but they must not execute providers. A handler resolves and observes the canonical
//! resource, records the capability it just observed, and delegates planning or submission here.

use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;

use crate::{
    error::AppError,
    operations::{
        contracts::{JobState, ResourceRef},
        invocation::{self, CredentialContext, InvocationError, PreparedInvocation},
        resources::{self, ObserveResource},
    },
    AppState,
};

use super::actions::CanonicalApiError;
use super::mcp::action_registry;

#[derive(Debug)]
pub(crate) enum CompatibilityError {
    Canonical(CanonicalApiError),
    Legacy(AppError),
}

impl From<CanonicalApiError> for CompatibilityError {
    fn from(error: CanonicalApiError) -> Self {
        Self::Canonical(error)
    }
}

impl From<InvocationError> for CompatibilityError {
    fn from(error: InvocationError) -> Self {
        Self::Canonical(error.into())
    }
}

impl From<AppError> for CompatibilityError {
    fn from(error: AppError) -> Self {
        Self::Legacy(error)
    }
}

impl From<crate::operations::backup_adoption::BackupAdoptionError> for CompatibilityError {
    fn from(error: crate::operations::backup_adoption::BackupAdoptionError) -> Self {
        use crate::operations::backup_adoption::BackupAdoptionError;
        match error {
            BackupAdoptionError::Invocation(error) => Self::Canonical(error.into()),
            BackupAdoptionError::ConfigNotFound => Self::Legacy(AppError::NotFound),
            BackupAdoptionError::ResticUnavailable => Self::Legacy(AppError::FeatureUnavailable(
                "restic is not installed".into(),
            )),
            BackupAdoptionError::Internal(error) => Self::Legacy(AppError::Internal(error)),
        }
    }
}

impl From<crate::operations::update_adoption::UpdateAdoptionError> for CompatibilityError {
    fn from(error: crate::operations::update_adoption::UpdateAdoptionError) -> Self {
        use crate::operations::update_adoption::UpdateAdoptionError;
        match error {
            UpdateAdoptionError::Invocation(error) => Self::Canonical(error.into()),
            UpdateAdoptionError::Unavailable(message) => {
                Self::Legacy(AppError::FeatureUnavailable(message))
            }
            UpdateAdoptionError::Internal(error) => Self::Legacy(AppError::Internal(error)),
        }
    }
}

impl IntoResponse for CompatibilityError {
    fn into_response(self) -> Response {
        match self {
            Self::Canonical(error) => error.into_response(),
            Self::Legacy(error) => error.into_response(),
        }
    }
}

pub(crate) type CompatibilityResult<T> = std::result::Result<T, CompatibilityError>;

pub(crate) struct CompatibilityResource<'a> {
    pub kind: &'a str,
    pub display_name: &'a str,
    pub node_id: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub namespace: &'a str,
    pub scope_key: &'a str,
    pub alias: &'a str,
}

pub(crate) fn authorize(credential: &CredentialContext, action: &str) -> CompatibilityResult<()> {
    let metadata = action_registry::action(action).ok_or(InvocationError::UnknownAction)?;
    invocation::authorize_action(metadata, credential)?;
    Ok(())
}

pub(crate) async fn observe_available(
    state: &AppState,
    credential: &CredentialContext,
    observed: CompatibilityResource<'_>,
    actions: &[&str],
) -> CompatibilityResult<ResourceRef> {
    let correlation_id = uuid::Uuid::new_v4().to_string();
    let resource = resources::observe(
        &state.db,
        ObserveResource {
            kind: observed.kind,
            display_name: observed.display_name,
            node_id: observed.node_id,
            provider: observed.provider,
            namespace: observed.namespace,
            scope_key: observed.scope_key,
            alias: observed.alias,
        },
        Some(credential.actor()),
        &correlation_id,
    )
    .await
    .map_err(|error| CompatibilityError::Legacy(AppError::Internal(error)))?;

    for action in actions {
        resources::set_capability(
            &state.db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            &correlation_id,
        )
        .await
        .map_err(|error| CompatibilityError::Legacy(AppError::Internal(error)))?;
    }
    Ok(resource)
}

pub(crate) async fn resolve_available(
    state: &AppState,
    credential: &CredentialContext,
    expected_kind: &str,
    namespace: &str,
    scope_key: &str,
    alias: &str,
    actions: &[&str],
) -> CompatibilityResult<ResourceRef> {
    for action in actions {
        authorize(credential, action)?;
    }
    let resource = resources::resolve_alias(&state.db, namespace, scope_key, alias)
        .await
        .map_err(|error| CompatibilityError::Legacy(AppError::Internal(error)))?
        .ok_or(InvocationError::ResourceNotFound)?;
    if resource.kind != expected_kind {
        return Err(InvocationError::ResourceKindMismatch.into());
    }
    let correlation_id = uuid::Uuid::new_v4().to_string();
    for action in actions {
        resources::set_capability(
            &state.db,
            &resource.id,
            action,
            crate::operations::contracts::CapabilityAvailability::Available,
            None,
            None,
            &correlation_id,
        )
        .await
        .map_err(|error| CompatibilityError::Legacy(AppError::Internal(error)))?;
    }
    Ok(resource)
}

pub(crate) async fn prepare(
    state: &AppState,
    credential: &CredentialContext,
    resource_id: &str,
    action: &str,
    input: Value,
) -> CompatibilityResult<PreparedInvocation> {
    invocation::prepare(
        &state.db,
        &state.operation_adapters,
        credential,
        resource_id,
        action,
        input,
    )
    .await
    .map_err(Into::into)
}

pub(crate) async fn submit(
    state: &AppState,
    credential: &CredentialContext,
    resource_id: &str,
    action: &str,
    input: Value,
    headers: &HeaderMap,
) -> CompatibilityResult<Response> {
    let idempotency_key = idempotency_key(headers)?;
    submit_with_key(
        state,
        credential,
        resource_id,
        action,
        input,
        &idempotency_key,
    )
    .await
}

pub(crate) fn idempotency_key(headers: &HeaderMap) -> CompatibilityResult<String> {
    let key = match headers.get("Idempotency-Key") {
        Some(value) => value
            .to_str()
            .map(str::to_owned)
            .map_err(|_| InvocationError::InvalidIdempotencyKey)?,
        None => format!("compat-{}", uuid::Uuid::new_v4()),
    };
    invocation::validate_idempotency_key(&key)?;
    Ok(key)
}

pub(crate) async fn submit_with_key(
    state: &AppState,
    credential: &CredentialContext,
    resource_id: &str,
    action: &str,
    input: Value,
    idempotency_key: &str,
) -> CompatibilityResult<Response> {
    let job = invocation::submit(
        &state.db,
        &state.operation_adapters,
        credential,
        resource_id,
        action,
        input,
        idempotency_key,
    )
    .await?;
    if job.state == JobState::Rejected {
        return Err(CanonicalApiError::policy_denied(job.id).into());
    }
    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"job": job}))).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn compatibility_keys_preserve_valid_callers_and_generate_valid_legacy_keys() {
        let mut explicit = HeaderMap::new();
        explicit.insert("Idempotency-Key", HeaderValue::from_static("caller-key:1"));
        assert_eq!(idempotency_key(&explicit).unwrap(), "caller-key:1");

        let generated = idempotency_key(&HeaderMap::new()).unwrap();
        assert!(generated.starts_with("compat-"));
        assert!(invocation::validate_idempotency_key(&generated).is_ok());
    }

    #[test]
    fn compatibility_authorization_fails_before_observation_for_low_privilege_roles() {
        let viewer = CredentialContext::Session {
            user_id: "viewer".into(),
            role: "viewer".into(),
        };
        assert!(authorize(&viewer, "container.start").is_err());
    }

    #[tokio::test]
    async fn compatibility_observation_records_the_canonical_alias_and_capability() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(db);
        let credential = CredentialContext::Session {
            user_id: "operator".into(),
            role: "operator".into(),
        };
        let resource = observe_available(
            &state,
            &credential,
            CompatibilityResource {
                kind: "container",
                display_name: "web",
                node_id: None,
                provider: Some("docker"),
                namespace: "docker.container",
                scope_key: "local-engine",
                alias: "full-container-id",
            },
            &["container.restart"],
        )
        .await
        .unwrap();

        let resolved = resources::resolve_alias(
            &state.db,
            "docker.container",
            "local-engine",
            "full-container-id",
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(resolved.id, resource.id);
        let capability = resources::capability(&state.db, &resource.id, "container.restart")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(capability.availability, "available");
    }

    #[tokio::test]
    async fn compatibility_resolution_uses_the_seeded_alias_and_marks_capabilities() {
        let db = crate::api::mcp::test_support::setup_db().await;
        let state = crate::api::mcp::test_support::build(db);
        let credential = CredentialContext::Session {
            user_id: "admin".into(),
            role: "admin".into(),
        };
        let seeded = resources::observe(
            &state.db,
            ObserveResource {
                kind: "firewall",
                display_name: "Local Firewall",
                node_id: None,
                provider: Some("local"),
                namespace: "voidtower.singleton",
                scope_key: "local",
                alias: "firewall",
            },
            None,
            "seed",
        )
        .await
        .unwrap();

        let resolved = resolve_available(
            &state,
            &credential,
            "firewall",
            "voidtower.singleton",
            "local",
            "firewall",
            &["firewall.enable"],
        )
        .await
        .unwrap();

        assert_eq!(resolved.id, seeded.id);
        let capability = resources::capability(&state.db, &resolved.id, "firewall.enable")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(capability.availability, "available");
    }
}
