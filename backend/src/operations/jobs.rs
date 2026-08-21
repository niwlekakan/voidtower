use super::{
    canonical_json,
    contracts::{
        ActorRef, ActorType, JobState, JobSummaryV1, OperationErrorV1, OperationPlanV1, ResourceRef,
    },
    events::{self, PendingEvent},
    state, unix_now,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error("idempotency key already belongs to a different request")]
    IdempotencyConflict,
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error(transparent)]
    Integer(#[from] std::num::TryFromIntError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionPolicy {
    Allow,
    RequireApproval {
        requirement: String,
        reason: String,
        expires_at: i64,
    },
    Deny {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct SubmitJob {
    pub action: String,
    pub resource: ResourceRef,
    pub actor: ActorRef,
    pub ingress: String,
    pub input: Value,
    pub plan: OperationPlanV1,
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub concurrency_key: String,
    pub retry_class: String,
    pub recovery_class: String,
    pub policy: SubmissionPolicy,
}

pub fn intent_digest(action: &str, resource_id: &str, input: &Value) -> Result<String> {
    canonical_json::digest(&serde_json::json!({
        "schema_version": 1,
        "action": action,
        "resource_id": resource_id,
        "input": input,
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub enum IdempotencyLookup {
    Missing,
    Existing(Box<JobSummaryV1>),
    Conflict,
}

pub async fn lookup_idempotency(
    pool: &SqlitePool,
    scope: &str,
    key: &str,
    request_digest: &str,
) -> Result<IdempotencyLookup> {
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT id, request_digest FROM jobs WHERE idempotency_scope = ? AND idempotency_key = ?",
    )
    .bind(scope)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    let Some((job_id, existing_digest)) = existing else {
        return Ok(IdempotencyLookup::Missing);
    };
    if existing_digest != request_digest {
        return Ok(IdempotencyLookup::Conflict);
    }
    Ok(IdempotencyLookup::Existing(Box::new(
        get(pool, &job_id)
            .await?
            .context("idempotent job disappeared")?,
    )))
}

#[derive(Debug, sqlx::FromRow)]
struct JobRow {
    id: String,
    action: String,
    resource_id: String,
    resource_revision: i64,
    resource_kind: String,
    resource_name: String,
    actor_type: String,
    actor_id: Option<String>,
    actor_source: Option<String>,
    ingress: String,
    state: String,
    progress_current: i64,
    progress_total: i64,
    progress_message: Option<String>,
    plan_json: String,
    approval_id: Option<String>,
    result_json: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    submitted_at: i64,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    updated_at: i64,
}

pub async fn submit(
    pool: &SqlitePool,
    request: SubmitJob,
) -> std::result::Result<JobSummaryV1, SubmitError> {
    validate_submission(&request)?;
    let input_json = canonical_json::to_canonical_string(&request.input)?;
    let request_digest = intent_digest(&request.action, &request.resource.id, &request.input)?;
    let plan_json = canonical_json::to_canonical_string(&request.plan)?;
    let plan_digest = canonical_json::digest(&request.plan)?;
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    // Acquire SQLite write intent before the idempotency read. Concurrent first submissions can
    // then serialize on the unique scope/key boundary instead of racing a deferred transaction
    // upgrade after both observed an empty key.
    sqlx::query("UPDATE jobs SET updated_at = updated_at WHERE 0")
        .execute(&mut *transaction)
        .await?;

    if let Some((existing_id, existing_digest)) = sqlx::query_as::<_, (String, String)>(
        "SELECT id, request_digest FROM jobs WHERE idempotency_scope = ? AND idempotency_key = ?",
    )
    .bind(&request.idempotency_scope)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *transaction)
    .await?
    {
        if existing_digest != request_digest {
            return Err(SubmitError::IdempotencyConflict);
        }
        transaction.rollback().await?;
        return get(pool, &existing_id)
            .await?
            .context("idempotent job disappeared")
            .map_err(SubmitError::from);
    }

    let (state, queued_at, policy_reason, approval) = match &request.policy {
        SubmissionPolicy::Allow => (JobState::Queued, Some(now), None, None),
        SubmissionPolicy::RequireApproval {
            requirement,
            reason,
            expires_at,
        } => (
            JobState::AwaitingApproval,
            None,
            Some(reason.as_str()),
            Some((requirement.as_str(), reason.as_str(), *expires_at)),
        ),
        SubmissionPolicy::Deny { reason } => {
            (JobState::Rejected, None, Some(reason.as_str()), None)
        }
    };
    let job_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO jobs \
         (id, action, resource_id, resource_revision, actor_type, actor_id, actor_source, ingress, \
          input_json, request_digest, plan_json, plan_digest, external_fingerprint, state, \
          progress_current, progress_total, idempotency_scope, idempotency_key, concurrency_key, \
          retry_class, recovery_class, cancel_requested, submitted_at, queued_at, finished_at, updated_at, \
          error_code, error_message) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind(&request.action)
    .bind(&request.resource.id)
    .bind(request.resource.revision)
    .bind(request.actor.actor_type.as_str())
    .bind(request.actor.id.as_deref())
    .bind(request.actor.source.as_deref())
    .bind(&request.ingress)
    .bind(input_json)
    .bind(&request_digest)
    .bind(plan_json)
    .bind(&plan_digest)
    .bind(&request.plan.external_fingerprint)
    .bind(state.as_str())
    .bind(i64::try_from(request.plan.steps.len())?)
    .bind(&request.idempotency_scope)
    .bind(&request.idempotency_key)
    .bind(&request.concurrency_key)
    .bind(&request.retry_class)
    .bind(&request.recovery_class)
    .bind(now)
    .bind(queued_at)
    .bind(state.is_terminal().then_some(now))
    .bind(now)
    .bind(policy_reason.map(|_| "policy_denied"))
    .bind(policy_reason)
    .execute(&mut *transaction)
    .await?;

    for (position, step) in request.plan.steps.iter().enumerate() {
        sqlx::query(
            "INSERT INTO job_steps \
             (id, job_id, position, kind, name, state, retry_class, recovery_class, updated_at) \
             VALUES (?, ?, ?, ?, ?, 'pending', ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&job_id)
        .bind(i64::try_from(position)?)
        .bind(&step.kind)
        .bind(&step.name)
        .bind(&step.retry_class)
        .bind(&step.recovery_class)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
    }

    let approval_id = if let Some((requirement, reason, expires_at)) = approval {
        let approval_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO approvals \
             (id, job_id, requirement, reason, status, expires_at, request_digest, plan_digest, \
              resource_revision, external_fingerprint, requested_at, updated_at) \
             VALUES (?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&approval_id)
        .bind(&job_id)
        .bind(requirement)
        .bind(reason)
        .bind(expires_at)
        .bind(&request_digest)
        .bind(&plan_digest)
        .bind(request.resource.revision)
        .bind(&request.plan.external_fingerprint)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        Some(approval_id)
    } else {
        None
    };

    append_job_event(
        &mut transaction,
        &request.actor,
        &request.resource.id,
        &job_id,
        approval_id.as_deref(),
        "job.submitted.v1",
        serde_json::json!({"action": request.action, "state": state}),
    )
    .await?;
    append_job_event(
        &mut transaction,
        &request.actor,
        &request.resource.id,
        &job_id,
        approval_id.as_deref(),
        match state {
            JobState::AwaitingApproval => "job.awaiting_approval.v1",
            JobState::Queued => "job.queued.v1",
            JobState::Rejected => "job.rejected.v1",
            _ => unreachable!(),
        },
        serde_json::json!({"action": request.action, "state": state}),
    )
    .await?;
    if let Some(approval_id) = approval_id.as_deref() {
        append_job_event(
            &mut transaction,
            &request.actor,
            &request.resource.id,
            &job_id,
            Some(approval_id),
            "approval.requested.v1",
            serde_json::json!({
                "action": request.action,
                "job_state": state,
                "requirement": match &request.policy {
                    SubmissionPolicy::RequireApproval { requirement, .. } => requirement,
                    _ => unreachable!(),
                },
            }),
        )
        .await?;
    }
    insert_audit(
        &mut transaction,
        &request.actor,
        &request.action,
        &request.resource,
        &job_id,
        match state {
            JobState::AwaitingApproval => "pending_approval",
            JobState::Queued => "queued",
            JobState::Rejected => "blocked",
            _ => unreachable!(),
        },
        policy_reason,
    )
    .await?;
    transaction.commit().await?;
    get(pool, &job_id)
        .await?
        .context("created job disappeared")
        .map_err(SubmitError::from)
}

pub async fn get(pool: &SqlitePool, job_id: &str) -> Result<Option<JobSummaryV1>> {
    let row = fetch_row(pool, job_id).await?;
    row.map(row_to_summary).transpose()
}

pub async fn list(pool: &SqlitePool, limit: i64) -> Result<Vec<JobSummaryV1>> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM jobs ORDER BY submitted_at DESC, id DESC LIMIT ?")
            .bind(limit.clamp(1, 200))
            .fetch_all(pool)
            .await?;
    let mut jobs = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(job) = get(pool, &id).await? {
            jobs.push(job);
        }
    }
    Ok(jobs)
}

pub async fn transition(
    pool: &SqlitePool,
    job_id: &str,
    target: JobState,
    actor: ActorRef,
    result: Option<&Value>,
    error: Option<(&str, &str)>,
) -> Result<JobSummaryV1> {
    let mut transaction = pool.begin().await?;
    let current: (String, String) =
        sqlx::query_as("SELECT state, resource_id FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(&mut *transaction)
            .await?
            .context("job not found")?;
    let current_state = parse_job_state(&current.0)?;
    if !state::can_transition(current_state, target) {
        bail!(
            "invalid job transition {} -> {}",
            current_state.as_str(),
            target.as_str()
        );
    }
    let now = unix_now();
    let result_json = result
        .map(canonical_json::to_canonical_string)
        .transpose()?;
    let terminal = target.is_terminal().then_some(now);
    let started = (target == JobState::Running).then_some(now);
    sqlx::query(
        "UPDATE jobs SET state = ?, result_json = COALESCE(?, result_json), error_code = ?, \
         error_message = ?, started_at = COALESCE(started_at, ?), finished_at = ?, \
         lease_owner = CASE WHEN ? = 'running' THEN lease_owner ELSE NULL END, \
         lease_expires_at = CASE WHEN ? = 'running' THEN lease_expires_at ELSE NULL END, updated_at = ? \
         WHERE id = ?",
    )
    .bind(target.as_str())
    .bind(result_json)
    .bind(error.map(|value| value.0))
    .bind(error.map(|value| value.1))
    .bind(started)
    .bind(terminal)
    .bind(target.as_str())
    .bind(target.as_str())
    .bind(now)
    .bind(job_id)
    .execute(&mut *transaction)
    .await?;
    append_job_event(
        &mut transaction,
        &actor,
        &current.1,
        job_id,
        None,
        &format!("job.{}.v1", target.as_str()),
        serde_json::json!({"previous_state": current_state, "state": target}),
    )
    .await?;
    transaction.commit().await?;
    get(pool, job_id).await?.context("job disappeared")
}

async fn fetch_row(pool: &SqlitePool, job_id: &str) -> Result<Option<JobRow>> {
    Ok(sqlx::query_as(
        "SELECT j.id, j.action, j.resource_id, j.resource_revision, r.kind AS resource_kind, \
                r.display_name AS resource_name, j.actor_type, j.actor_id, j.actor_source, j.ingress, \
                j.state, j.progress_current, j.progress_total, j.progress_message, j.plan_json, \
                a.id AS approval_id, j.result_json, j.error_code, j.error_message, j.submitted_at, \
                j.started_at, j.finished_at, j.updated_at \
         FROM jobs j JOIN resources r ON r.id = j.resource_id \
         LEFT JOIN approvals a ON a.job_id = j.id WHERE j.id = ?",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?)
}

fn row_to_summary(row: JobRow) -> Result<JobSummaryV1> {
    let job_id = row.id.clone();
    Ok(JobSummaryV1 {
        id: row.id,
        action: row.action,
        resource: ResourceRef {
            id: row.resource_id,
            kind: row.resource_kind,
            display_name: row.resource_name,
            revision: row.resource_revision,
        },
        actor: ActorRef {
            actor_type: parse_actor_type(&row.actor_type),
            id: row.actor_id,
            source: row.actor_source,
        },
        ingress: row.ingress,
        state: parse_job_state(&row.state)?,
        progress_current: row.progress_current,
        progress_total: row.progress_total,
        progress_message: row.progress_message,
        plan: serde_json::from_str(&row.plan_json)?,
        approval_id: row.approval_id,
        result: row
            .result_json
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
        error: row.error_code.map(|code| OperationErrorV1 {
            code,
            message: row.error_message.unwrap_or_default(),
            retryable: false,
            job_id: Some(job_id),
        }),
        submitted_at: row.submitted_at,
        started_at: row.started_at,
        finished_at: row.finished_at,
        updated_at: row.updated_at,
    })
}

fn validate_submission(request: &SubmitJob) -> Result<()> {
    if request.idempotency_key.trim().is_empty() {
        bail!("idempotency key is required");
    }
    if request.resource.revision < 0 {
        bail!("resource revision cannot be negative");
    }
    if request.plan.schema_version != 1 {
        bail!("unsupported plan schema version");
    }
    if request.plan.external_fingerprint.is_empty() {
        bail!("external fingerprint is required");
    }
    Ok(())
}

fn parse_job_state(value: &str) -> Result<JobState> {
    Ok(match value {
        "awaiting_approval" => JobState::AwaitingApproval,
        "queued" => JobState::Queued,
        "running" => JobState::Running,
        "succeeded" => JobState::Succeeded,
        "failed" => JobState::Failed,
        "cancelled" => JobState::Cancelled,
        "needs_attention" => JobState::NeedsAttention,
        "rejected" => JobState::Rejected,
        "expired" => JobState::Expired,
        _ => bail!("unknown persisted job state {value}"),
    })
}

fn parse_actor_type(value: &str) -> ActorType {
    match value {
        "human" => ActorType::Human,
        "api_token" => ActorType::ApiToken,
        "automation" => ActorType::Automation,
        "plugin" => ActorType::Plugin,
        "node" => ActorType::Node,
        "ai" => ActorType::Ai,
        _ => ActorType::System,
    }
}

#[allow(clippy::too_many_arguments)]
async fn append_job_event(
    transaction: &mut Transaction<'_, Sqlite>,
    actor: &ActorRef,
    resource_id: &str,
    job_id: &str,
    approval_id: Option<&str>,
    event_type: &str,
    payload: Value,
) -> Result<()> {
    events::append(
        transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(actor.clone()),
            resource_id: Some(resource_id.into()),
            job_id: Some(job_id.into()),
            approval_id: approval_id.map(str::to_owned),
            correlation_id: job_id.into(),
            causation_id: None,
            payload,
        },
    )
    .await?;
    Ok(())
}

async fn insert_audit(
    transaction: &mut Transaction<'_, Sqlite>,
    actor: &ActorRef,
    action: &str,
    resource: &ResourceRef,
    job_id: &str,
    outcome: &str,
    details: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO audit_log \
         (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, request_id, details, source) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(unix_now())
    .bind(
        (actor.actor_type == ActorType::Human)
            .then_some(actor.id.as_deref())
            .flatten(),
    )
    .bind(actor.actor_type.as_str())
    .bind(action)
    .bind(&resource.kind)
    .bind(&resource.id)
    .bind(outcome)
    .bind(job_id)
    .bind(details)
    .bind(actor.source.as_deref())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{contracts::PlannedStepV1, resources};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> (SqlitePool, ResourceRef) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let resource = resources::observe(
            &pool,
            resources::ObserveResource {
                kind: "container",
                display_name: "test",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "one",
            },
            None,
            "setup",
        )
        .await
        .unwrap();
        (pool, resource)
    }

    fn request(resource: ResourceRef, input: Value, policy: SubmissionPolicy) -> SubmitJob {
        SubmitJob {
            action: "container.remove".into(),
            concurrency_key: resource.id.clone(),
            resource,
            actor: ActorRef {
                actor_type: ActorType::Human,
                id: Some("user-1".into()),
                source: Some("web".into()),
            },
            ingress: "web".into(),
            input,
            plan: OperationPlanV1 {
                schema_version: 1,
                title: "Remove container".into(),
                risk: "high".into(),
                changes: vec![],
                preview: None,
                external_fingerprint: "state-1".into(),
                steps: vec![PlannedStepV1 {
                    kind: "execute".into(),
                    name: "Remove".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                }],
            },
            idempotency_scope: "human:user-1".into(),
            idempotency_key: "request-1".into(),
            retry_class: "never".into(),
            recovery_class: "reconcile".into(),
            policy,
        }
    }

    #[tokio::test]
    async fn identical_idempotent_submission_returns_original_job() {
        let (pool, resource) = setup().await;
        let first = submit(
            &pool,
            request(
                resource.clone(),
                serde_json::json!({"force": true}),
                SubmissionPolicy::Allow,
            ),
        )
        .await
        .unwrap();
        let second = submit(
            &pool,
            request(
                resource,
                serde_json::json!({"force": true}),
                SubmissionPolicy::Allow,
            ),
        )
        .await
        .unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.state, JobState::Queued);
    }

    #[tokio::test]
    async fn reused_idempotency_key_with_different_input_conflicts() {
        let (pool, resource) = setup().await;
        submit(
            &pool,
            request(
                resource.clone(),
                serde_json::json!({"force": true}),
                SubmissionPolicy::Allow,
            ),
        )
        .await
        .unwrap();
        let error = submit(
            &pool,
            request(
                resource,
                serde_json::json!({"force": false}),
                SubmissionPolicy::Allow,
            ),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, SubmitError::IdempotencyConflict));
    }

    #[tokio::test]
    async fn reused_idempotency_key_cannot_alias_another_action_or_resource() {
        let (pool, resource) = setup().await;
        submit(
            &pool,
            request(
                resource.clone(),
                serde_json::json!({"force": true}),
                SubmissionPolicy::Allow,
            ),
        )
        .await
        .unwrap();

        let mut different_action = request(
            resource.clone(),
            serde_json::json!({"force": true}),
            SubmissionPolicy::Allow,
        );
        different_action.action = "container.restart".into();
        assert!(matches!(
            submit(&pool, different_action).await.unwrap_err(),
            SubmitError::IdempotencyConflict
        ));

        let second_resource = resources::observe(
            &pool,
            resources::ObserveResource {
                kind: "container",
                display_name: "other",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "two",
            },
            None,
            "setup-two",
        )
        .await
        .unwrap();
        assert!(matches!(
            submit(
                &pool,
                request(
                    second_resource,
                    serde_json::json!({"force": true}),
                    SubmissionPolicy::Allow,
                ),
            )
            .await
            .unwrap_err(),
            SubmitError::IdempotencyConflict
        ));
    }

    #[tokio::test]
    async fn concurrent_identical_submissions_create_one_durable_job() {
        let path = std::env::temp_dir().join(format!(
            "voidtower-idempotency-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::init_pool(&path).await.unwrap();
        let resource = resources::observe(
            &pool,
            resources::ObserveResource {
                kind: "container",
                display_name: "concurrent",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "concurrent",
            },
            None,
            "setup",
        )
        .await
        .unwrap();
        let left = request(
            resource.clone(),
            serde_json::json!({"force": true}),
            SubmissionPolicy::Allow,
        );
        let right = request(
            resource,
            serde_json::json!({"force": true}),
            SubmissionPolicy::Allow,
        );

        let (left, right) = tokio::join!(submit(&pool, left), submit(&pool, right));
        let left = left.unwrap();
        let right = right.unwrap();
        assert_eq!(left.id, right.id);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM jobs WHERE idempotency_scope = 'human:user-1' \
             AND idempotency_key = 'request-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
        pool.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn concurrent_conflicting_submissions_create_one_job_and_one_conflict() {
        let path = std::env::temp_dir().join(format!(
            "voidtower-idempotency-conflict-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let pool = crate::db::init_pool(&path).await.unwrap();
        let resource = resources::observe(
            &pool,
            resources::ObserveResource {
                kind: "container",
                display_name: "conflicting",
                node_id: None,
                provider: Some("docker"),
                namespace: "test.container",
                scope_key: "local",
                alias: "conflicting",
            },
            None,
            "setup",
        )
        .await
        .unwrap();
        let left = request(
            resource.clone(),
            serde_json::json!({"force": true}),
            SubmissionPolicy::Allow,
        );
        let right = request(
            resource,
            serde_json::json!({"force": false}),
            SubmissionPolicy::Allow,
        );

        let (left, right) = tokio::join!(submit(&pool, left), submit(&pool, right));
        let mut created = 0;
        let mut conflicts = 0;
        for result in [left, right] {
            match result {
                Ok(_) => created += 1,
                Err(SubmitError::IdempotencyConflict) => conflicts += 1,
                Err(error) => panic!("unexpected submission error: {error}"),
            }
        }
        assert_eq!((created, conflicts), (1, 1));
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM jobs WHERE idempotency_scope = 'human:user-1' \
             AND idempotency_key = 'request-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1);
        pool.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn approval_policy_creates_immutable_waiting_job() {
        let (pool, resource) = setup().await;
        let job = submit(
            &pool,
            request(
                resource,
                serde_json::json!({"force": true}),
                SubmissionPolicy::RequireApproval {
                    requirement: "risk_ladder".into(),
                    reason: "Assisted mode".into(),
                    expires_at: unix_now() + 900,
                },
            ),
        )
        .await
        .unwrap();
        assert_eq!(job.state, JobState::AwaitingApproval);
        assert!(job.approval_id.is_some());
        let events = events::list_after(&pool, 0, 20).await.unwrap();
        let event_types: Vec<&str> = events
            .iter()
            .filter(|event| event.job_id.as_deref() == Some(&job.id))
            .map(|event| event.event_type.as_str())
            .collect();
        assert_eq!(
            event_types,
            [
                "job.submitted.v1",
                "job.awaiting_approval.v1",
                "approval.requested.v1",
            ]
        );
    }

    #[tokio::test]
    async fn uncertain_job_cannot_be_requeued() {
        let (pool, resource) = setup().await;
        let job = submit(
            &pool,
            request(resource, serde_json::json!({}), SubmissionPolicy::Allow),
        )
        .await
        .unwrap();
        let actor = job.actor.clone();
        transition(&pool, &job.id, JobState::Running, actor.clone(), None, None)
            .await
            .unwrap();
        transition(
            &pool,
            &job.id,
            JobState::NeedsAttention,
            actor.clone(),
            None,
            Some(("uncertain", "provider outcome is unknown")),
        )
        .await
        .unwrap();
        assert!(
            transition(&pool, &job.id, JobState::Queued, actor, None, None)
                .await
                .is_err()
        );
    }
}
