use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLifecycle {
    Active,
    Unavailable,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResourceRef {
    pub id: String,
    pub kind: String,
    pub display_name: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResourceAlias {
    pub resource_id: String,
    pub namespace: String,
    pub scope_key: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityAvailability {
    Available,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ResourceCapability {
    pub resource_id: String,
    pub action: String,
    pub availability: String,
    pub reason_code: Option<String>,
    pub detail: Option<String>,
    pub schema_version: i64,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    Human,
    ApiToken,
    Automation,
    Plugin,
    Node,
    Ai,
    System,
}

impl ActorType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::ApiToken => "api_token",
            Self::Automation => "automation",
            Self::Plugin => "plugin",
            Self::Node => "node",
            Self::Ai => "ai",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRef {
    pub actor_type: ActorType,
    pub id: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    AwaitingApproval,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    NeedsAttention,
    Rejected,
    Expired,
}

impl JobState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AwaitingApproval => "awaiting_approval",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::NeedsAttention => "needs_attention",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Rejected | Self::Expired
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Expired,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanChange {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationPlanV1 {
    pub schema_version: u16,
    pub title: String,
    pub risk: String,
    pub changes: Vec<PlanChange>,
    pub preview: Option<String>,
    pub external_fingerprint: String,
    pub steps: Vec<PlannedStepV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedStepV1 {
    pub kind: String,
    pub name: String,
    pub retry_class: String,
    pub recovery_class: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelopeV1 {
    pub sequence: i64,
    pub event_id: String,
    pub schema_version: u16,
    pub event_type: String,
    pub occurred_at: i64,
    pub actor: Option<ActorRef>,
    pub resource_id: Option<String>,
    pub job_id: Option<String>,
    pub approval_id: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationErrorV1 {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub job_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobSummaryV1 {
    pub id: String,
    pub action: String,
    pub resource: ResourceRef,
    pub actor: ActorRef,
    pub ingress: String,
    pub state: JobState,
    pub progress_current: i64,
    pub progress_total: i64,
    pub progress_message: Option<String>,
    pub plan: OperationPlanV1,
    pub approval_id: Option<String>,
    pub result: Option<Value>,
    pub error: Option<OperationErrorV1>,
    pub submitted_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ApprovalViewV1 {
    pub id: String,
    pub job_id: String,
    pub requirement: String,
    pub reason: String,
    pub status: String,
    pub expires_at: i64,
    pub decided_by: Option<String>,
    pub decision_comment: Option<String>,
    pub requested_at: i64,
    pub decided_at: Option<i64>,
    pub updated_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_state_contract_uses_stable_snake_case_names() {
        assert_eq!(
            serde_json::to_string(&JobState::AwaitingApproval).unwrap(),
            "\"awaiting_approval\""
        );
        assert_eq!(JobState::NeedsAttention.as_str(), "needs_attention");
    }

    #[test]
    fn uncertain_outcome_is_not_terminal_until_reconciled() {
        assert!(!JobState::NeedsAttention.is_terminal());
        assert!(JobState::Failed.is_terminal());
    }
}
