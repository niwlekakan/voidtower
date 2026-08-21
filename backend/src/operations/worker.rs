//! Durable worker repository primitives.
//!
//! Concrete domain adapters are not started from `main` yet. These functions establish the
//! fail-closed lease, concurrency, attempt, cancellation, and restart-recovery semantics that the
//! worker loop will use once the runtime adapter registry is complete.

use super::{
    adapters::{AdapterRegistry, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest},
    canonical_json,
    clock::Clock,
    contracts::{ActorRef, ActorType, JobState, PlannedStepV1, ResourceRef},
    events::{self, PendingEvent},
};
use crate::api::mcp::action_registry::{self, RetryClass};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

const MAX_PERSISTED_TEXT_CHARS: usize = 4 * 1024;

#[derive(Debug, Clone)]
pub struct ClaimedJob {
    pub id: String,
    pub action: String,
    pub resource: ResourceRef,
    pub input: Value,
    pub external_fingerprint: String,
    pub lease_expires_at: i64,
}

#[derive(Debug, Clone)]
pub struct ClaimedStep {
    pub id: String,
    pub job_id: String,
    pub position: i64,
    pub step: PlannedStepV1,
    pub attempt: u32,
    pub external_operation_id: Option<String>,
    pub lease_expires_at: i64,
}

#[derive(Debug, FromRow)]
struct CompletionRow {
    action: String,
    resource_id: String,
    job_state: String,
    step_state: String,
    retry_class: String,
}

#[derive(Debug, FromRow)]
struct ClaimRow {
    id: String,
    action: String,
    resource_id: String,
    resource_revision: i64,
    resource_kind: String,
    resource_name: String,
    input_json: String,
    external_fingerprint: String,
}

#[derive(Debug, FromRow)]
struct StepRow {
    id: String,
    job_id: String,
    position: i64,
    kind: String,
    name: String,
    retry_class: String,
    recovery_class: String,
    external_operation_id: Option<String>,
}

pub async fn claim_next(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    worker_id: &str,
    now: i64,
    lease_seconds: i64,
) -> Result<Option<ClaimedJob>> {
    ensure!(!worker_id.trim().is_empty(), "worker id is required");
    ensure!(lease_seconds > 0, "lease duration must be positive");
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let candidate: Option<ClaimRow> = sqlx::query_as(
        "SELECT j.id, j.action, j.resource_id, j.resource_revision, r.kind AS resource_kind, \
                r.display_name AS resource_name, j.input_json, j.external_fingerprint \
         FROM jobs j JOIN resources r ON r.id = j.resource_id \
         WHERE j.state = 'queued' \
           AND EXISTS (SELECT 1 FROM resource_capabilities c \
                       WHERE c.resource_id = j.resource_id AND c.action = j.action \
                         AND c.availability = 'available') \
           AND NOT EXISTS (SELECT 1 FROM jobs active \
                           WHERE active.state IN ('running', 'needs_attention') \
                             AND active.concurrency_key = j.concurrency_key) \
         ORDER BY j.queued_at, j.submitted_at, j.id LIMIT 1",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(candidate) = candidate else {
        transaction.rollback().await?;
        return Ok(None);
    };
    adapters.for_action(&candidate.action)?;
    let lease_expires_at = now
        .checked_add(lease_seconds)
        .context("worker lease expiry overflow")?;
    let updated = sqlx::query(
        "UPDATE jobs SET state = 'running', lease_owner = ?, lease_expires_at = ?, \
         started_at = COALESCE(started_at, ?), updated_at = ? \
         WHERE id = ? AND state = 'queued' \
           AND NOT EXISTS (SELECT 1 FROM jobs active \
                           WHERE active.state IN ('running', 'needs_attention') \
                             AND active.concurrency_key = jobs.concurrency_key)",
    )
    .bind(worker_id)
    .bind(lease_expires_at)
    .bind(now)
    .bind(now)
    .bind(&candidate.id)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(None);
    }
    append_worker_event(
        &mut transaction,
        &candidate.id,
        &candidate.resource_id,
        "job.running.v1",
        serde_json::json!({"previous_state": "queued", "state": "running"}),
    )
    .await?;
    transaction.commit().await?;

    Ok(Some(ClaimedJob {
        id: candidate.id,
        action: candidate.action,
        resource: ResourceRef {
            id: candidate.resource_id,
            kind: candidate.resource_kind,
            display_name: candidate.resource_name,
            revision: candidate.resource_revision,
        },
        input: serde_json::from_str(&candidate.input_json)?,
        external_fingerprint: candidate.external_fingerprint,
        lease_expires_at,
    }))
}

pub async fn claim_step(
    pool: &SqlitePool,
    job_id: &str,
    worker_id: &str,
    now: i64,
    lease_seconds: i64,
) -> Result<Option<ClaimedStep>> {
    ensure!(lease_seconds > 0, "lease duration must be positive");
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let owns_job: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE id = ? AND state = 'running' \
         AND lease_owner = ? AND lease_expires_at > ?",
    )
    .bind(job_id)
    .bind(worker_id)
    .bind(now)
    .fetch_one(&mut *transaction)
    .await?;
    if owns_job != 1 {
        transaction.rollback().await?;
        return Ok(None);
    }
    let step: Option<StepRow> = sqlx::query_as(
        "SELECT id, job_id, position, kind, name, retry_class, recovery_class, \
                external_operation_id \
         FROM job_steps WHERE job_id = ? AND state = 'pending' ORDER BY position LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(step) = step else {
        transaction.rollback().await?;
        return Ok(None);
    };
    let lease_expires_at = now
        .checked_add(lease_seconds)
        .context("step lease expiry overflow")?;
    let attempt: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM job_attempts WHERE step_id = ?",
    )
    .bind(&step.id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE job_steps SET state = 'running', started_at = COALESCE(started_at, ?), \
         updated_at = ? WHERE id = ? AND state = 'pending'",
    )
    .bind(now)
    .bind(now)
    .bind(&step.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO job_attempts \
         (id, job_id, step_id, attempt_number, worker_id, lease_expires_at, started_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(job_id)
    .bind(&step.id)
    .bind(attempt)
    .bind(worker_id)
    .bind(lease_expires_at)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    sqlx::query("UPDATE jobs SET lease_expires_at = ?, updated_at = ? WHERE id = ?")
        .bind(lease_expires_at)
        .bind(now)
        .bind(job_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;

    Ok(Some(ClaimedStep {
        id: step.id,
        job_id: step.job_id,
        position: step.position,
        step: PlannedStepV1 {
            kind: step.kind,
            name: step.name,
            retry_class: step.retry_class,
            recovery_class: step.recovery_class,
        },
        attempt: u32::try_from(attempt)?,
        external_operation_id: step.external_operation_id,
        lease_expires_at,
    }))
}

pub async fn renew_lease(
    pool: &SqlitePool,
    job_id: &str,
    worker_id: &str,
    now: i64,
    lease_seconds: i64,
) -> Result<bool> {
    ensure!(lease_seconds > 0, "lease duration must be positive");
    let expires_at = now
        .checked_add(lease_seconds)
        .context("worker lease expiry overflow")?;
    let mut transaction = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE jobs SET lease_expires_at = ?, updated_at = ? \
         WHERE id = ? AND state IN ('running', 'needs_attention') \
           AND lease_owner = ? AND lease_expires_at > ?",
    )
    .bind(expires_at)
    .bind(now)
    .bind(job_id)
    .bind(worker_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() == 1 {
        sqlx::query(
            "UPDATE job_attempts SET lease_expires_at = ? WHERE job_id = ? \
             AND worker_id = ? AND finished_at IS NULL",
        )
        .bind(expires_at)
        .bind(job_id)
        .bind(worker_id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(updated.rows_affected() == 1)
}

/// Revalidate a claimed job at its safe pre-execution checkpoint, dispatch one immutable step,
/// and persist the attempt, step, job, audit, and event outcome atomically. If the lease expires
/// after the external call, completion deliberately fails and lease recovery reconciles the
/// uncertain attempt instead of replaying it.
pub async fn execute_claimed_step(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    job: &ClaimedJob,
    step: &ClaimedStep,
    worker_id: &str,
    clock: &dyn Clock,
) -> Result<JobState> {
    ensure!(job.id == step.job_id, "claimed step belongs to another job");
    let now = clock.now();
    let adapter = adapters.for_action(&job.action)?;
    let preflight: Option<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT r.revision, r.lifecycle_state, COALESCE(c.availability, 'missing'), \
                j.cancel_requested FROM jobs j \
         JOIN resources r ON r.id = j.resource_id \
         LEFT JOIN resource_capabilities c ON c.resource_id = j.resource_id AND c.action = j.action \
         WHERE j.id = ? AND j.state = 'running' AND j.lease_owner = ? \
           AND j.lease_expires_at > ?",
    )
    .bind(&job.id)
    .bind(worker_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let Some((resource_revision, lifecycle_state, capability, cancel_requested)) = preflight else {
        anyhow::bail!("job lease is not valid for execution");
    };
    if cancel_requested != 0 {
        return complete_step(
            pool,
            step,
            worker_id,
            clock.now(),
            StepOutcome::Cancelled {
                message: "Cancellation accepted at the pre-execution checkpoint".into(),
            },
        )
        .await;
    }
    if resource_revision != job.resource.revision {
        return complete_step(
            pool,
            step,
            worker_id,
            clock.now(),
            StepOutcome::Failed {
                code: "stale_resource_revision".into(),
                message: "Resource changed after this operation was planned".into(),
                retryable: false,
                diagnostic: None,
            },
        )
        .await;
    }
    if lifecycle_state != "active" || capability != "available" {
        return complete_step(
            pool,
            step,
            worker_id,
            clock.now(),
            StepOutcome::Failed {
                code: "capability_unavailable".into(),
                message: "Resource capability is no longer available".into(),
                retryable: false,
                diagnostic: None,
            },
        )
        .await;
    }

    let plan_request = PlanRequest {
        action: job.action.clone(),
        resource: job.resource.clone(),
        input: job.input.clone(),
    };
    let fingerprint = match adapter.external_fingerprint(&plan_request).await {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return complete_step(
                pool,
                step,
                worker_id,
                clock.now(),
                StepOutcome::Failed {
                    code: "preflight_failed".into(),
                    message: safe_text(&format!("Unable to verify provider state: {error}")),
                    retryable: false,
                    diagnostic: None,
                },
            )
            .await;
        }
    };
    if fingerprint != job.external_fingerprint {
        return complete_step(
            pool,
            step,
            worker_id,
            clock.now(),
            StepOutcome::Failed {
                code: "stale_external_state".into(),
                message: "Provider state changed after this operation was planned".into(),
                retryable: false,
                diagnostic: None,
            },
        )
        .await;
    }

    let request = StepRequest {
        job_id: job.id.clone(),
        action: job.action.clone(),
        resource: job.resource.clone(),
        input: job.input.clone(),
        step: step.step.clone(),
        attempt: step.attempt,
        external_operation_id: step.external_operation_id.clone(),
    };
    let outcome =
        adapter
            .execute_step(request)
            .await
            .unwrap_or_else(|error| StepOutcome::Uncertain {
                code: "adapter_execution_uncertain".into(),
                message: safe_text(&format!(
                    "Adapter execution did not report an outcome: {error}"
                )),
                external_operation_id: None,
                diagnostic: None,
            });
    complete_step(pool, step, worker_id, clock.now(), outcome).await
}

/// Finish the current append-only attempt and atomically derive the step and job state. The
/// runtime coordinator renews ownership while the adapter future is in flight.
pub async fn complete_step(
    pool: &SqlitePool,
    step: &ClaimedStep,
    worker_id: &str,
    now: i64,
    outcome: StepOutcome,
) -> Result<JobState> {
    let outcome = sanitize_outcome(outcome)?;
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let row: CompletionRow = sqlx::query_as(
        "SELECT j.action, j.resource_id, j.state AS job_state, s.state AS step_state, \
                s.retry_class \
         FROM jobs j JOIN job_steps s ON s.job_id = j.id \
         JOIN job_attempts a ON a.job_id = j.id AND a.step_id = s.id \
         WHERE j.id = ? AND s.id = ? AND a.attempt_number = ? \
           AND a.finished_at IS NULL AND a.worker_id = ? \
           AND j.lease_owner = ? AND j.lease_expires_at > ?",
    )
    .bind(&step.job_id)
    .bind(&step.id)
    .bind(i64::from(step.attempt))
    .bind(worker_id)
    .bind(worker_id)
    .bind(now)
    .fetch_optional(&mut *transaction)
    .await?
    .context("claimed attempt is no longer owned by this worker")?;
    ensure!(row.job_state == "running", "job is not running");
    ensure!(row.step_state == "running", "step is not running");

    let action = action_registry::action(&row.action)
        .ok_or_else(|| anyhow::anyhow!("unknown operation action: {}", row.action))?;
    let retry = action
        .retry
        .context("durable action has no retry metadata")?;

    let (state, event_type, audit_outcome) = match outcome {
        StepOutcome::Succeeded {
            result,
            external_operation_id,
        } => {
            let result_json = canonical_json::to_canonical_string(&result)?;
            finish_attempt(&mut transaction, step, now, "succeeded", None).await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'succeeded', progress_current = 1, \
                 progress_total = MAX(progress_total, 1), external_operation_id = ?, result_json = ?, \
                 error_code = NULL, error_message = NULL, finished_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(external_operation_id)
            .bind(&result_json)
            .bind(now)
            .bind(now)
            .bind(&step.id)
            .execute(&mut *transaction)
            .await?;
            let succeeded: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM job_steps WHERE job_id = ? AND state = 'succeeded'",
            )
            .bind(&step.job_id)
            .fetch_one(&mut *transaction)
            .await?;
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_steps WHERE job_id = ?")
                .bind(&step.job_id)
                .fetch_one(&mut *transaction)
                .await?;
            let state = if succeeded == total {
                sqlx::query(
                    "UPDATE jobs SET state = 'succeeded', progress_current = ?, result_json = ?, \
                     error_code = NULL, error_message = NULL, finished_at = ?, lease_owner = NULL, \
                     lease_expires_at = NULL, updated_at = ? \
                     WHERE id = ? AND state = 'running'",
                )
                .bind(succeeded)
                .bind(result_json)
                .bind(now)
                .bind(now)
                .bind(&step.job_id)
                .execute(&mut *transaction)
                .await?;
                JobState::Succeeded
            } else {
                sqlx::query(
                    "UPDATE jobs SET progress_current = ?, progress_message = ?, updated_at = ? \
                     WHERE id = ? AND state = 'running'",
                )
                .bind(succeeded)
                .bind(format!("Completed step {} of {total}", step.position + 1))
                .bind(now)
                .bind(&step.job_id)
                .execute(&mut *transaction)
                .await?;
                JobState::Running
            };
            let event_type = if state == JobState::Succeeded {
                "job.succeeded.v1"
            } else {
                "job.progress.v1"
            };
            (state, event_type, "succeeded")
        }
        StepOutcome::Failed {
            code,
            message,
            retryable,
            diagnostic,
        } => {
            let diagnostic_json = diagnostic
                .as_ref()
                .map(canonical_json::to_canonical_string)
                .transpose()?;
            let may_retry = retryable
                && retry.class == RetryClass::Transient
                && row.retry_class == RetryClass::Transient.as_str()
                && step.attempt < u32::from(retry.max_attempts);
            finish_attempt(
                &mut transaction,
                step,
                now,
                if may_retry {
                    "retryable_failure"
                } else {
                    "failed"
                },
                diagnostic_json.as_deref(),
            )
            .await?;
            if may_retry {
                sqlx::query(
                    "UPDATE job_steps SET state = 'pending', error_code = ?, error_message = ?, \
                     finished_at = NULL, updated_at = ? WHERE id = ?",
                )
                .bind(&code)
                .bind(&message)
                .bind(now)
                .bind(&step.id)
                .execute(&mut *transaction)
                .await?;
                sqlx::query(
                    "UPDATE jobs SET state = 'queued', queued_at = ?, lease_owner = NULL, \
                     lease_expires_at = NULL, error_code = ?, error_message = ?, updated_at = ? \
                     WHERE id = ? AND state = 'running'",
                )
                .bind(now)
                .bind(&code)
                .bind(&message)
                .bind(now)
                .bind(&step.job_id)
                .execute(&mut *transaction)
                .await?;
                (
                    JobState::Queued,
                    "job.retry_scheduled.v1",
                    "retry_scheduled",
                )
            } else {
                sqlx::query(
                    "UPDATE job_steps SET state = 'failed', error_code = ?, error_message = ?, \
                     finished_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(&code)
                .bind(&message)
                .bind(now)
                .bind(now)
                .bind(&step.id)
                .execute(&mut *transaction)
                .await?;
                finish_job_with_error(
                    &mut transaction,
                    step,
                    now,
                    JobState::Failed,
                    &code,
                    &message,
                )
                .await?;
                (JobState::Failed, "job.failed.v1", "failed")
            }
        }
        StepOutcome::Cancelled { message } => {
            finish_attempt(&mut transaction, step, now, "cancelled", None).await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'cancelled', error_code = 'cancelled', \
                 error_message = ?, finished_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&message)
            .bind(now)
            .bind(now)
            .bind(&step.id)
            .execute(&mut *transaction)
            .await?;
            finish_job_with_error(
                &mut transaction,
                step,
                now,
                JobState::Cancelled,
                "cancelled",
                &message,
            )
            .await?;
            (JobState::Cancelled, "job.cancelled.v1", "cancelled")
        }
        StepOutcome::Uncertain {
            code,
            message,
            external_operation_id,
            diagnostic,
        } => {
            let diagnostic_json = diagnostic
                .as_ref()
                .map(canonical_json::to_canonical_string)
                .transpose()?;
            finish_attempt(
                &mut transaction,
                step,
                now,
                "uncertain",
                diagnostic_json.as_deref(),
            )
            .await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'needs_attention', external_operation_id = ?, \
                 error_code = ?, error_message = ?, finished_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(external_operation_id)
            .bind(&code)
            .bind(&message)
            .bind(now)
            .bind(now)
            .bind(&step.id)
            .execute(&mut *transaction)
            .await?;
            finish_job_with_error(
                &mut transaction,
                step,
                now,
                JobState::NeedsAttention,
                &code,
                &message,
            )
            .await?;
            (
                JobState::NeedsAttention,
                "job.needs_attention.v1",
                "needs_attention",
            )
        }
    };

    append_worker_event(
        &mut transaction,
        &step.job_id,
        &row.resource_id,
        event_type,
        serde_json::json!({
            "previous_state": "running",
            "state": state,
            "step_id": step.id,
            "step_position": step.position,
            "attempt": step.attempt,
        }),
    )
    .await?;
    let actor = ActorRef {
        actor_type: ActorType::System,
        id: None,
        source: Some("operation_worker".into()),
    };
    insert_worker_audit(
        &mut transaction,
        now,
        &actor,
        &row.action,
        &step.job_id,
        audit_outcome,
    )
    .await?;
    transaction.commit().await?;
    Ok(state)
}

/// Claim one uncertain step for side-effect-free provider reconciliation. Reconciliation keeps
/// the job in `needs_attention`; the lease only serializes verification attempts and never makes
/// the original operation executable again.
pub async fn claim_reconciliation(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    worker_id: &str,
    now: i64,
    lease_seconds: i64,
) -> Result<Option<(ClaimedJob, ClaimedStep)>> {
    ensure!(!worker_id.trim().is_empty(), "worker id is required");
    ensure!(lease_seconds > 0, "lease duration must be positive");
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let candidate: Option<ClaimRow> = sqlx::query_as(
        "SELECT j.id, j.action, j.resource_id, j.resource_revision, r.kind AS resource_kind, \
                r.display_name AS resource_name, j.input_json, j.external_fingerprint \
         FROM jobs j JOIN resources r ON r.id = j.resource_id \
         WHERE j.state = 'needs_attention' \
           AND EXISTS (SELECT 1 FROM job_steps s WHERE s.job_id = j.id \
                       AND s.state = 'needs_attention' AND s.recovery_class = 'reconcile') \
           AND (j.lease_expires_at IS NULL OR j.lease_expires_at <= ?) \
         ORDER BY j.updated_at, j.id LIMIT 1",
    )
    .bind(now)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some(candidate) = candidate else {
        transaction.rollback().await?;
        return Ok(None);
    };
    adapters.for_action(&candidate.action)?;
    let step: StepRow = sqlx::query_as(
        "SELECT id, job_id, position, kind, name, retry_class, recovery_class, \
                external_operation_id FROM job_steps \
         WHERE job_id = ? AND state = 'needs_attention' ORDER BY position LIMIT 1",
    )
    .bind(&candidate.id)
    .fetch_one(&mut *transaction)
    .await?;
    let lease_expires_at = now
        .checked_add(lease_seconds)
        .context("reconciliation lease expiry overflow")?;
    let updated = sqlx::query(
        "UPDATE jobs SET lease_owner = ?, lease_expires_at = ?, updated_at = ? \
         WHERE id = ? AND state = 'needs_attention' \
           AND (lease_expires_at IS NULL OR lease_expires_at <= ?)",
    )
    .bind(worker_id)
    .bind(lease_expires_at)
    .bind(now)
    .bind(&candidate.id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(None);
    }
    sqlx::query(
        "UPDATE job_attempts SET finished_at = ?, outcome = 'reconciliation_lease_expired' \
         WHERE job_id = ? AND finished_at IS NULL",
    )
    .bind(now)
    .bind(&candidate.id)
    .execute(&mut *transaction)
    .await?;
    let attempt: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM job_attempts WHERE step_id = ?",
    )
    .bind(&step.id)
    .fetch_one(&mut *transaction)
    .await?;
    sqlx::query(
        "INSERT INTO job_attempts \
         (id, job_id, step_id, attempt_number, worker_id, lease_expires_at, started_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&candidate.id)
    .bind(&step.id)
    .bind(attempt)
    .bind(worker_id)
    .bind(lease_expires_at)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    append_worker_event(
        &mut transaction,
        &candidate.id,
        &candidate.resource_id,
        "job.reconciliation_started.v1",
        serde_json::json!({
            "state": "needs_attention",
            "step_id": step.id,
            "attempt": attempt,
        }),
    )
    .await?;
    transaction.commit().await?;

    Ok(Some((
        ClaimedJob {
            id: candidate.id,
            action: candidate.action,
            resource: ResourceRef {
                id: candidate.resource_id,
                kind: candidate.resource_kind,
                display_name: candidate.resource_name,
                revision: candidate.resource_revision,
            },
            input: serde_json::from_str(&candidate.input_json)?,
            external_fingerprint: candidate.external_fingerprint,
            lease_expires_at,
        },
        ClaimedStep {
            id: step.id,
            job_id: step.job_id,
            position: step.position,
            step: PlannedStepV1 {
                kind: step.kind,
                name: step.name,
                retry_class: step.retry_class,
                recovery_class: step.recovery_class,
            },
            attempt: u32::try_from(attempt)?,
            external_operation_id: step.external_operation_id,
            lease_expires_at,
        },
    )))
}

pub async fn reconcile_claimed_step(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    job: &ClaimedJob,
    step: &ClaimedStep,
    worker_id: &str,
    clock: &dyn Clock,
) -> Result<JobState> {
    ensure!(job.id == step.job_id, "claimed step belongs to another job");
    let adapter = adapters.for_action(&job.action)?;
    let request = StepRequest {
        job_id: job.id.clone(),
        action: job.action.clone(),
        resource: job.resource.clone(),
        input: job.input.clone(),
        step: step.step.clone(),
        attempt: step.attempt,
        external_operation_id: step.external_operation_id.clone(),
    };
    let outcome =
        adapter
            .reconcile(request)
            .await
            .unwrap_or_else(|error| ReconcileOutcome::StillUncertain {
                message: safe_text(&format!("Provider reconciliation failed: {error}")),
            });
    complete_reconciliation(pool, step, worker_id, clock.now(), outcome).await
}

/// Release an owned job only at a safe checkpoint where no provider step is in flight.
/// Runtime shutdown uses this to stop before the next step without waiting for lease expiry.
pub async fn release_claimed_job(
    pool: &SqlitePool,
    job_id: &str,
    worker_id: &str,
    now: i64,
) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE jobs SET state = 'queued', queued_at = ?, lease_owner = NULL, \
         lease_expires_at = NULL, updated_at = ? \
         WHERE id = ? AND state = 'running' AND lease_owner = ? AND lease_expires_at > ? \
           AND NOT EXISTS (SELECT 1 FROM job_steps \
                           WHERE job_steps.job_id = jobs.id AND state = 'running')",
    )
    .bind(now)
    .bind(now)
    .bind(job_id)
    .bind(worker_id)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    if updated.rows_affected() == 0 {
        transaction.rollback().await?;
        return Ok(false);
    }
    let (action, resource_id): (String, String) =
        sqlx::query_as("SELECT action, resource_id FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_one(&mut *transaction)
            .await?;
    append_worker_event(
        &mut transaction,
        job_id,
        &resource_id,
        "job.recovered.v1",
        serde_json::json!({
            "previous_state": "running",
            "state": "queued",
            "reason": "runtime_shutdown",
        }),
    )
    .await?;
    let actor = ActorRef {
        actor_type: ActorType::System,
        id: None,
        source: Some("operation_runtime".into()),
    };
    insert_worker_audit(
        &mut transaction,
        now,
        &actor,
        &action,
        job_id,
        "released_for_shutdown",
    )
    .await?;
    transaction.commit().await?;
    Ok(true)
}

/// Release a step that was durably claimed but whose provider future has not been started.
/// The runtime calls this only after observing shutdown in the claim-to-execution gap.
pub async fn release_step_before_execution(
    pool: &SqlitePool,
    step: &ClaimedStep,
    worker_id: &str,
    now: i64,
) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let owned: Option<(String, String)> = sqlx::query_as(
        "SELECT j.action, j.resource_id FROM jobs j \
         JOIN job_steps s ON s.job_id = j.id \
         JOIN job_attempts a ON a.job_id = j.id AND a.step_id = s.id \
         WHERE j.id = ? AND s.id = ? AND a.attempt_number = ? \
           AND j.state = 'running' AND s.state = 'running' \
           AND j.lease_owner = ? AND j.lease_expires_at > ? \
           AND a.worker_id = ? AND a.finished_at IS NULL",
    )
    .bind(&step.job_id)
    .bind(&step.id)
    .bind(i64::from(step.attempt))
    .bind(worker_id)
    .bind(now)
    .bind(worker_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((action, resource_id)) = owned else {
        transaction.rollback().await?;
        return Ok(false);
    };
    finish_attempt(
        &mut transaction,
        step,
        now,
        "runtime_shutdown_before_execution",
        None,
    )
    .await?;
    sqlx::query(
        "UPDATE job_steps SET state = 'pending', finished_at = NULL, updated_at = ? \
         WHERE id = ? AND state = 'running'",
    )
    .bind(now)
    .bind(&step.id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE jobs SET state = 'queued', queued_at = ?, lease_owner = NULL, \
         lease_expires_at = NULL, updated_at = ? WHERE id = ? AND state = 'running'",
    )
    .bind(now)
    .bind(now)
    .bind(&step.job_id)
    .execute(&mut *transaction)
    .await?;
    append_worker_event(
        &mut transaction,
        &step.job_id,
        &resource_id,
        "job.recovered.v1",
        serde_json::json!({
            "previous_state": "running",
            "state": "queued",
            "reason": "runtime_shutdown_before_execution",
            "step_id": step.id,
            "attempt": step.attempt,
        }),
    )
    .await?;
    let actor = ActorRef {
        actor_type: ActorType::System,
        id: None,
        source: Some("operation_runtime".into()),
    };
    insert_worker_audit(
        &mut transaction,
        now,
        &actor,
        &action,
        &step.job_id,
        "released_before_execution",
    )
    .await?;
    transaction.commit().await?;
    Ok(true)
}

/// Release a reconciliation attempt claimed immediately before runtime shutdown. No provider
/// verification has started, so the job remains in `needs_attention` for a later cadence.
pub async fn release_reconciliation_before_verification(
    pool: &SqlitePool,
    step: &ClaimedStep,
    worker_id: &str,
    now: i64,
) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let owned: Option<(String, String)> = sqlx::query_as(
        "SELECT j.action, j.resource_id FROM jobs j \
         JOIN job_steps s ON s.job_id = j.id \
         JOIN job_attempts a ON a.job_id = j.id AND a.step_id = s.id \
         WHERE j.id = ? AND s.id = ? AND a.attempt_number = ? \
           AND j.state = 'needs_attention' AND s.state = 'needs_attention' \
           AND j.lease_owner = ? AND j.lease_expires_at > ? \
           AND a.worker_id = ? AND a.finished_at IS NULL",
    )
    .bind(&step.job_id)
    .bind(&step.id)
    .bind(i64::from(step.attempt))
    .bind(worker_id)
    .bind(now)
    .bind(worker_id)
    .fetch_optional(&mut *transaction)
    .await?;
    let Some((action, resource_id)) = owned else {
        transaction.rollback().await?;
        return Ok(false);
    };
    finish_attempt(
        &mut transaction,
        step,
        now,
        "runtime_shutdown_before_reconciliation",
        None,
    )
    .await?;
    sqlx::query(
        "UPDATE jobs SET lease_owner = NULL, lease_expires_at = NULL, updated_at = ? \
         WHERE id = ? AND state = 'needs_attention'",
    )
    .bind(now)
    .bind(&step.job_id)
    .execute(&mut *transaction)
    .await?;
    append_worker_event(
        &mut transaction,
        &step.job_id,
        &resource_id,
        "job.reconciliation_pending.v1",
        serde_json::json!({
            "state": "needs_attention",
            "reason": "runtime_shutdown_before_reconciliation",
            "step_id": step.id,
            "attempt": step.attempt,
        }),
    )
    .await?;
    let actor = ActorRef {
        actor_type: ActorType::System,
        id: None,
        source: Some("operation_runtime".into()),
    };
    insert_worker_audit(
        &mut transaction,
        now,
        &actor,
        &action,
        &step.job_id,
        "reconciliation_released_for_shutdown",
    )
    .await?;
    transaction.commit().await?;
    Ok(true)
}

pub async fn complete_reconciliation(
    pool: &SqlitePool,
    step: &ClaimedStep,
    worker_id: &str,
    now: i64,
    outcome: ReconcileOutcome,
) -> Result<JobState> {
    let outcome = sanitize_reconciliation(outcome);
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let row: CompletionRow = sqlx::query_as(
        "SELECT j.action, j.resource_id, j.state AS job_state, s.state AS step_state, \
                s.retry_class \
         FROM jobs j JOIN job_steps s ON s.job_id = j.id \
         JOIN job_attempts a ON a.job_id = j.id AND a.step_id = s.id \
         WHERE j.id = ? AND s.id = ? AND a.attempt_number = ? \
           AND a.finished_at IS NULL AND a.worker_id = ? \
           AND j.lease_owner = ? AND j.lease_expires_at > ?",
    )
    .bind(&step.job_id)
    .bind(&step.id)
    .bind(i64::from(step.attempt))
    .bind(worker_id)
    .bind(worker_id)
    .bind(now)
    .fetch_optional(&mut *transaction)
    .await?
    .context("reconciliation attempt is no longer owned by this worker")?;
    ensure!(
        row.job_state == "needs_attention",
        "job does not need reconciliation"
    );
    ensure!(
        row.step_state == "needs_attention",
        "step does not need reconciliation"
    );

    let outcome = if matches!(&outcome, ReconcileOutcome::Succeeded { .. }) {
        let incomplete_other_steps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM job_steps WHERE job_id = ? AND id != ? AND state != 'succeeded'",
        )
        .bind(&step.job_id)
        .bind(&step.id)
        .fetch_one(&mut *transaction)
        .await?;
        if incomplete_other_steps == 0 {
            outcome
        } else {
            ReconcileOutcome::Failed {
                code: "incomplete_ordered_plan".into(),
                message: "An intermediate step was reconciled, but later ordered steps did not run; submit a new operation".into(),
            }
        }
    } else {
        outcome
    };

    let (state, event_type, audit_outcome) = match outcome {
        ReconcileOutcome::Succeeded { result } => {
            let result_json = canonical_json::to_canonical_string(&result)?;
            finish_attempt(
                &mut transaction,
                step,
                now,
                "reconciliation_succeeded",
                None,
            )
            .await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'succeeded', progress_current = 1, \
                 progress_total = MAX(progress_total, 1), result_json = ?, error_code = NULL, \
                 error_message = NULL, finished_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&result_json)
            .bind(now)
            .bind(now)
            .bind(&step.id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE jobs SET state = 'succeeded', \
                 progress_current = (SELECT COUNT(*) FROM job_steps \
                                     WHERE job_id = jobs.id AND state = 'succeeded'), \
                 result_json = ?, error_code = NULL, error_message = NULL, finished_at = ?, \
                 lease_owner = NULL, lease_expires_at = NULL, updated_at = ? \
                 WHERE id = ? AND state = 'needs_attention'",
            )
            .bind(result_json)
            .bind(now)
            .bind(now)
            .bind(&step.job_id)
            .execute(&mut *transaction)
            .await?;
            (
                JobState::Succeeded,
                "job.succeeded.v1",
                "reconciliation_succeeded",
            )
        }
        ReconcileOutcome::Failed { code, message } => {
            finish_attempt(&mut transaction, step, now, "reconciliation_failed", None).await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'failed', error_code = ?, error_message = ?, \
                 finished_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&code)
            .bind(&message)
            .bind(now)
            .bind(now)
            .bind(&step.id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'cancelled', error_code = 'dependency_not_run', \
                 error_message = 'A previous step did not complete', finished_at = ?, updated_at = ? \
                 WHERE job_id = ? AND state = 'pending'",
            )
            .bind(now)
            .bind(now)
            .bind(&step.job_id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE jobs SET state = 'failed', error_code = ?, error_message = ?, \
                 finished_at = ?, lease_owner = NULL, lease_expires_at = NULL, updated_at = ? \
                 WHERE id = ? AND state = 'needs_attention'",
            )
            .bind(&code)
            .bind(&message)
            .bind(now)
            .bind(now)
            .bind(&step.job_id)
            .execute(&mut *transaction)
            .await?;
            (JobState::Failed, "job.failed.v1", "reconciliation_failed")
        }
        ReconcileOutcome::StillUncertain { message } => {
            finish_attempt(
                &mut transaction,
                step,
                now,
                "reconciliation_uncertain",
                None,
            )
            .await?;
            sqlx::query("UPDATE job_steps SET error_message = ?, updated_at = ? WHERE id = ?")
                .bind(&message)
                .bind(now)
                .bind(&step.id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(
                "UPDATE jobs SET error_message = ?, lease_owner = NULL, lease_expires_at = NULL, \
                 updated_at = ? WHERE id = ? AND state = 'needs_attention'",
            )
            .bind(&message)
            .bind(now)
            .bind(&step.job_id)
            .execute(&mut *transaction)
            .await?;
            (
                JobState::NeedsAttention,
                "job.reconciliation_pending.v1",
                "reconciliation_uncertain",
            )
        }
    };
    append_worker_event(
        &mut transaction,
        &step.job_id,
        &row.resource_id,
        event_type,
        serde_json::json!({
            "previous_state": "needs_attention",
            "state": state,
            "step_id": step.id,
            "attempt": step.attempt,
        }),
    )
    .await?;
    let actor = ActorRef {
        actor_type: ActorType::System,
        id: None,
        source: Some("operation_reconciler".into()),
    };
    insert_worker_audit(
        &mut transaction,
        now,
        &actor,
        &row.action,
        &step.job_id,
        audit_outcome,
    )
    .await?;
    transaction.commit().await?;
    Ok(state)
}

async fn finish_attempt(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    step: &ClaimedStep,
    now: i64,
    outcome: &str,
    diagnostic_json: Option<&str>,
) -> Result<()> {
    let updated = sqlx::query(
        "UPDATE job_attempts SET finished_at = ?, outcome = ?, diagnostic_json = ? \
         WHERE job_id = ? AND step_id = ? AND attempt_number = ? AND finished_at IS NULL",
    )
    .bind(now)
    .bind(outcome)
    .bind(diagnostic_json)
    .bind(&step.job_id)
    .bind(&step.id)
    .bind(i64::from(step.attempt))
    .execute(&mut **transaction)
    .await?;
    ensure!(
        updated.rows_affected() == 1,
        "attempt was already completed"
    );
    Ok(())
}

async fn finish_job_with_error(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    step: &ClaimedStep,
    now: i64,
    state: JobState,
    code: &str,
    message: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE job_steps SET state = 'cancelled', error_code = 'dependency_not_run', \
         error_message = 'A previous step did not complete', finished_at = ?, updated_at = ? \
         WHERE job_id = ? AND state = 'pending'",
    )
    .bind(now)
    .bind(now)
    .bind(&step.job_id)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "UPDATE jobs SET state = ?, error_code = ?, error_message = ?, finished_at = ?, \
         lease_owner = NULL, lease_expires_at = NULL, updated_at = ? \
         WHERE id = ? AND state = 'running'",
    )
    .bind(state.as_str())
    .bind(code)
    .bind(message)
    .bind(state.is_terminal().then_some(now))
    .bind(now)
    .bind(&step.job_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn sanitize_outcome(outcome: StepOutcome) -> Result<StepOutcome> {
    Ok(match outcome {
        StepOutcome::Succeeded {
            result,
            external_operation_id,
        } => StepOutcome::Succeeded {
            result: safe_value(result),
            external_operation_id: external_operation_id.map(|value| safe_text(&value)),
        },
        StepOutcome::Failed {
            code,
            message,
            retryable,
            diagnostic,
        } => StepOutcome::Failed {
            code: safe_text(&code),
            message: safe_text(&message),
            retryable,
            diagnostic: diagnostic.map(safe_value),
        },
        StepOutcome::Cancelled { message } => StepOutcome::Cancelled {
            message: safe_text(&message),
        },
        StepOutcome::Uncertain {
            code,
            message,
            external_operation_id,
            diagnostic,
        } => StepOutcome::Uncertain {
            code: safe_text(&code),
            message: safe_text(&message),
            external_operation_id: external_operation_id.map(|value| safe_text(&value)),
            diagnostic: diagnostic.map(safe_value),
        },
    })
}

fn sanitize_reconciliation(outcome: ReconcileOutcome) -> ReconcileOutcome {
    match outcome {
        ReconcileOutcome::Succeeded { result } => ReconcileOutcome::Succeeded {
            result: safe_value(result),
        },
        ReconcileOutcome::Failed { code, message } => ReconcileOutcome::Failed {
            code: safe_text(&code),
            message: safe_text(&message),
        },
        ReconcileOutcome::StillUncertain { message } => ReconcileOutcome::StillUncertain {
            message: safe_text(&message),
        },
    }
}

fn safe_value(value: Value) -> Value {
    match value {
        Value::String(value) => Value::String(safe_text(&value)),
        Value::Array(values) => Value::Array(values.into_iter().map(safe_value).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let normalized = key.to_ascii_lowercase();
                    let value = if ["password", "passwd", "token", "secret", "credential"]
                        .iter()
                        .any(|needle| normalized.contains(needle))
                    {
                        Value::String("[REDACTED]".into())
                    } else {
                        safe_value(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        scalar => scalar,
    }
}

fn safe_text(value: &str) -> String {
    let redacted = crate::api::mcp::redact::redact_patterns(value);
    let mut chars = redacted.chars();
    let mut bounded: String = chars.by_ref().take(MAX_PERSISTED_TEXT_CHARS).collect();
    if chars.next().is_some() {
        bounded.push_str("…[truncated]");
    }
    bounded
}

pub async fn request_cancellation(
    pool: &SqlitePool,
    job_id: &str,
    actor: ActorRef,
    now: i64,
) -> Result<JobState> {
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let (state, resource_id): (String, String) =
        sqlx::query_as("SELECT state, resource_id FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(&mut *transaction)
            .await?
            .context("job not found")?;
    let target = match state.as_str() {
        "queued" => {
            sqlx::query(
                "UPDATE jobs SET state = 'cancelled', cancel_requested = 1, finished_at = ?, \
                 updated_at = ? WHERE id = ? AND state = 'queued'",
            )
            .bind(now)
            .bind(now)
            .bind(job_id)
            .execute(&mut *transaction)
            .await?;
            sqlx::query(
                "UPDATE job_steps SET state = 'cancelled', finished_at = ?, updated_at = ? \
                 WHERE job_id = ? AND state = 'pending'",
            )
            .bind(now)
            .bind(now)
            .bind(job_id)
            .execute(&mut *transaction)
            .await?;
            JobState::Cancelled
        }
        "running" => {
            sqlx::query(
                "UPDATE jobs SET cancel_requested = 1, updated_at = ? \
                 WHERE id = ? AND state = 'running'",
            )
            .bind(now)
            .bind(job_id)
            .execute(&mut *transaction)
            .await?;
            JobState::Running
        }
        _ => anyhow::bail!("job cannot be cancelled from state {state}"),
    };
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: if target == JobState::Cancelled {
                "job.cancelled.v1".into()
            } else {
                "job.cancellation_requested.v1".into()
            },
            actor: Some(actor.clone()),
            resource_id: Some(resource_id),
            job_id: Some(job_id.into()),
            approval_id: None,
            correlation_id: job_id.into(),
            causation_id: None,
            payload: serde_json::json!({"previous_state": state, "state": target}),
        },
    )
    .await?;
    insert_worker_audit(
        &mut transaction,
        now,
        &actor,
        "job.cancel",
        job_id,
        if target == JobState::Cancelled {
            "cancelled"
        } else {
            "cancellation_pending"
        },
    )
    .await?;
    transaction.commit().await?;
    Ok(target)
}

/// Recover jobs whose worker lease expired. A job with no started step is safe to requeue. A
/// started retryable step is requeued for a new attempt; a non-retryable/uncertain step is moved to
/// `needs_attention` and is never replayed blindly.
pub async fn recover_expired(pool: &SqlitePool, now: i64) -> Result<u64> {
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let jobs: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, resource_id FROM jobs WHERE state = 'running' \
         AND lease_expires_at IS NOT NULL AND lease_expires_at <= ? ORDER BY id",
    )
    .bind(now)
    .fetch_all(&mut *transaction)
    .await?;
    let mut recovered = 0u64;
    for (job_id, resource_id) in jobs {
        let running_step: Option<(String, String)> = sqlx::query_as(
            "SELECT id, retry_class FROM job_steps WHERE job_id = ? AND state = 'running' \
             ORDER BY position LIMIT 1",
        )
        .bind(&job_id)
        .fetch_optional(&mut *transaction)
        .await?;
        let (target, event_type) = match running_step {
            None => ("queued", "job.recovered.v1"),
            Some((step_id, retry_class)) if retry_class != "never" => {
                sqlx::query("UPDATE job_steps SET state = 'pending', updated_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(&step_id)
                    .execute(&mut *transaction)
                    .await?;
                ("queued", "job.recovered.v1")
            }
            Some((step_id, _)) => {
                sqlx::query(
                    "UPDATE job_steps SET state = 'needs_attention', error_code = 'lease_expired', \
                     error_message = 'Worker lease expired after external execution began', \
                     finished_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(&step_id)
                .execute(&mut *transaction)
                .await?;
                ("needs_attention", "job.needs_attention.v1")
            }
        };
        sqlx::query(
            "UPDATE job_attempts SET finished_at = ?, outcome = 'lease_expired' \
             WHERE job_id = ? AND finished_at IS NULL",
        )
        .bind(now)
        .bind(&job_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE jobs SET state = ?, queued_at = CASE WHEN ? = 'queued' THEN ? ELSE queued_at END, \
             lease_owner = NULL, lease_expires_at = NULL, \
             error_code = CASE WHEN ? = 'needs_attention' THEN 'lease_expired' ELSE error_code END, \
             error_message = CASE WHEN ? = 'needs_attention' \
                 THEN 'Worker lease expired with an uncertain external outcome' ELSE error_message END, \
             updated_at = ? WHERE id = ? AND state = 'running'",
        )
        .bind(target)
        .bind(target)
        .bind(now)
        .bind(target)
        .bind(target)
        .bind(now)
        .bind(&job_id)
        .execute(&mut *transaction)
        .await?;
        append_worker_event(
            &mut transaction,
            &job_id,
            &resource_id,
            event_type,
            serde_json::json!({"previous_state": "running", "state": target}),
        )
        .await?;
        recovered += 1;
    }
    transaction.commit().await?;
    Ok(recovered)
}

/// SQLite deferred transactions that read before their first write can deadlock each other while
/// upgrading. A no-row write obtains the reserved writer slot before transition reads begin.
async fn acquire_write_intent(transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<()> {
    sqlx::query("UPDATE jobs SET updated_at = updated_at WHERE 0")
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn append_worker_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    job_id: &str,
    resource_id: &str,
    event_type: &str,
    payload: Value,
) -> Result<()> {
    events::append(
        transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(ActorRef {
                actor_type: ActorType::System,
                id: None,
                source: Some("operation_worker".into()),
            }),
            resource_id: Some(resource_id.into()),
            job_id: Some(job_id.into()),
            approval_id: None,
            correlation_id: job_id.into(),
            causation_id: None,
            payload,
        },
    )
    .await?;
    Ok(())
}

async fn insert_worker_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    now: i64,
    actor: &ActorRef,
    action: &str,
    job_id: &str,
    outcome: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO audit_log \
         (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, request_id, details, source) \
         VALUES (?, ?, ?, ?, ?, 'job', ?, ?, ?, NULL, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(now)
    .bind(
        (actor.actor_type == ActorType::Human)
            .then_some(actor.id.as_deref())
            .flatten(),
    )
    .bind(actor.actor_type.as_str())
    .bind(action)
    .bind(job_id)
    .bind(outcome)
    .bind(job_id)
    .bind(actor.source.as_deref())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        adapters::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest},
        contracts::{CapabilityAvailability, OperationPlanV1, PlannedStepV1},
        jobs::{self, SubmissionPolicy, SubmitJob},
        resources::{self, ObserveResource},
    };
    use anyhow::bail;
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    };

    struct FixedClock(i64);

    impl Clock for FixedClock {
        fn now(&self) -> i64 {
            self.0
        }
    }

    struct AtomicClock(AtomicI64);

    impl AtomicClock {
        fn new(now: i64) -> Self {
            Self(AtomicI64::new(now))
        }

        fn set(&self, now: i64) {
            self.0.store(now, Ordering::SeqCst);
        }
    }

    impl Clock for AtomicClock {
        fn now(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    struct FakeContainerAdapter;

    #[async_trait]
    impl OperationAdapter for FakeContainerAdapter {
        fn key(&self) -> &'static str {
            "containers"
        }

        fn actions(&self) -> &[&'static str] {
            &["container.start"]
        }

        async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
            bail!("not used")
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            Ok("state-1".into())
        }

        async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
            bail!("not used")
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            bail!("not used")
        }
    }

    async fn setup() -> (SqlitePool, AdapterRegistry, ResourceRef) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: "container",
                display_name: "container",
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
        resources::set_capability(
            &pool,
            &resource.id,
            "container.start",
            CapabilityAvailability::Available,
            None,
            None,
            "setup",
        )
        .await
        .unwrap();
        let mut adapters = AdapterRegistry::new();
        adapters.register(Arc::new(FakeContainerAdapter)).unwrap();
        (pool, adapters, resource)
    }

    async fn submit_job(
        pool: &SqlitePool,
        resource: &ResourceRef,
        idempotency_key: &str,
        retry_class: &str,
    ) -> String {
        jobs::submit(
            pool,
            SubmitJob {
                action: "container.start".into(),
                resource: resource.clone(),
                actor: ActorRef {
                    actor_type: ActorType::Human,
                    id: Some("owner".into()),
                    source: Some("test".into()),
                },
                ingress: "http".into(),
                input: serde_json::json!({}),
                plan: OperationPlanV1 {
                    schema_version: 1,
                    title: "Start container".into(),
                    risk: "mutate".into(),
                    changes: vec![],
                    preview: None,
                    external_fingerprint: "state-1".into(),
                    steps: vec![PlannedStepV1 {
                        kind: "execute".into(),
                        name: "start".into(),
                        retry_class: retry_class.into(),
                        recovery_class: "reconcile".into(),
                    }],
                },
                idempotency_scope: "test:owner".into(),
                idempotency_key: idempotency_key.into(),
                concurrency_key: resource.id.clone(),
                retry_class: retry_class.into(),
                recovery_class: "reconcile".into(),
                policy: SubmissionPolicy::Allow,
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn claim_job_and_step(
        pool: &SqlitePool,
        adapters: &AdapterRegistry,
        resource: &ResourceRef,
        idempotency_key: &str,
    ) -> (ClaimedJob, ClaimedStep) {
        let job_id = submit_job(pool, resource, idempotency_key, "never").await;
        let job = claim_next(pool, adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(job.id, job_id);
        let step = claim_step(pool, &job.id, "worker-a", 101, 20)
            .await
            .unwrap()
            .unwrap();
        (job, step)
    }

    #[tokio::test]
    async fn lease_claim_serializes_resource_and_records_attempt() {
        let (pool, adapters, resource) = setup().await;
        let first = submit_job(&pool, &resource, "first", "never").await;
        let second = submit_job(&pool, &resource, "second", "never").await;

        let claimed = claim_next(&pool, &adapters, "worker-a", 100, 10)
            .await
            .unwrap()
            .unwrap();
        assert!([first, second].contains(&claimed.id));
        assert!(claim_next(&pool, &adapters, "worker-b", 100, 10)
            .await
            .unwrap()
            .is_none());
        let step = claim_step(&pool, &claimed.id, "worker-a", 101, 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(step.attempt, 1);
        assert!(renew_lease(&pool, &claimed.id, "worker-a", 102, 10)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn successful_completion_is_atomic_bounded_and_redacted() {
        let (pool, adapters, resource) = setup().await;
        let (job, step) = claim_job_and_step(&pool, &adapters, &resource, "complete-success").await;
        let secret = "known-super-secret-value";
        assert_eq!(
            complete_step(
                &pool,
                &step,
                "worker-a",
                102,
                StepOutcome::Succeeded {
                    result: serde_json::json!({
                        "password": secret,
                        "message": format!("api_key={secret}"),
                        "bounded": "x".repeat(MAX_PERSISTED_TEXT_CHARS + 100),
                        "safe": "completed",
                    }),
                    external_operation_id: None,
                },
            )
            .await
            .unwrap(),
            JobState::Succeeded
        );

        let summary = jobs::get(&pool, &job.id).await.unwrap().unwrap();
        assert_eq!(summary.state, JobState::Succeeded);
        let persisted = serde_json::to_string(&summary.result).unwrap();
        assert!(!persisted.contains(secret));
        assert!(persisted.contains("completed"));
        assert!(persisted.contains("[truncated]"));
        let attempt: (String, i64) =
            sqlx::query_as("SELECT outcome, finished_at FROM job_attempts WHERE job_id = ?")
                .bind(&job.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attempt, ("succeeded".into(), 102));
        let final_event: String = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE job_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(&job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(final_event, "job.succeeded.v1");
        let audit: (String, String) = sqlx::query_as(
            "SELECT action, outcome FROM audit_log WHERE request_id = ? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(&job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit, ("container.start".into(), "succeeded".into()));
    }

    #[tokio::test]
    async fn uncertain_completion_needs_attention_and_cannot_be_replayed() {
        let (pool, adapters, resource) = setup().await;
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "complete-uncertain").await;
        assert_eq!(
            complete_step(
                &pool,
                &step,
                "worker-a",
                102,
                StepOutcome::Uncertain {
                    code: "provider_timeout".into(),
                    message: "Provider outcome could not be verified".into(),
                    external_operation_id: Some("task-7".into()),
                    diagnostic: Some(serde_json::json!({"token": "must-not-persist"})),
                },
            )
            .await
            .unwrap(),
            JobState::NeedsAttention
        );
        assert_eq!(
            jobs::get(&pool, &job.id).await.unwrap().unwrap().state,
            JobState::NeedsAttention
        );
        let step_row: (String, Option<String>) =
            sqlx::query_as("SELECT state, external_operation_id FROM job_steps WHERE id = ?")
                .bind(&step.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(step_row, ("needs_attention".into(), Some("task-7".into())));
        let diagnostic: String =
            sqlx::query_scalar("SELECT diagnostic_json FROM job_attempts WHERE job_id = ?")
                .bind(&job.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!diagnostic.contains("must-not-persist"));
        let blocked = submit_job(&pool, &resource, "blocked-by-uncertain", "never").await;
        assert!(claim_next(&pool, &adapters, "worker-b", 103, 20)
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            jobs::get(&pool, &blocked).await.unwrap().unwrap().state,
            JobState::Queued
        );
    }

    #[tokio::test]
    async fn reconciliation_records_a_new_attempt_without_replaying_execution() {
        struct SuccessfulReconciler;

        #[async_trait]
        impl OperationAdapter for SuccessfulReconciler {
            fn key(&self) -> &'static str {
                "containers"
            }

            fn actions(&self) -> &[&'static str] {
                &["container.start"]
            }

            async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
                bail!("not used")
            }

            async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
                bail!("reconciliation does not run execution preflight")
            }

            async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
                panic!("reconciliation must never replay execution")
            }

            async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
                assert_eq!(request.external_operation_id.as_deref(), Some("task-9"));
                Ok(ReconcileOutcome::Succeeded {
                    result: serde_json::json!({
                        "verified": true,
                        "access_token": "must-not-persist",
                    }),
                })
            }
        }

        let (pool, adapters, resource) = setup().await;
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "reconcile-success").await;
        complete_step(
            &pool,
            &step,
            "worker-a",
            102,
            StepOutcome::Uncertain {
                code: "provider_timeout".into(),
                message: "Provider outcome could not be verified".into(),
                external_operation_id: Some("task-9".into()),
                diagnostic: None,
            },
        )
        .await
        .unwrap();

        let mut reconcilers = AdapterRegistry::new();
        reconcilers
            .register(Arc::new(SuccessfulReconciler))
            .unwrap();
        let (claimed_job, claimed_step) =
            claim_reconciliation(&pool, &reconcilers, "reconciler-a", 103, 20)
                .await
                .unwrap()
                .unwrap();
        assert_eq!(claimed_step.attempt, 2);
        assert_eq!(
            reconcile_claimed_step(
                &pool,
                &reconcilers,
                &claimed_job,
                &claimed_step,
                "reconciler-a",
                &FixedClock(104),
            )
            .await
            .unwrap(),
            JobState::Succeeded
        );
        let summary = jobs::get(&pool, &job.id).await.unwrap().unwrap();
        assert_eq!(summary.state, JobState::Succeeded);
        assert_eq!(summary.progress_current, 1);
        let persisted = serde_json::to_string(&summary.result).unwrap();
        assert!(persisted.contains("verified"));
        assert!(!persisted.contains("must-not-persist"));
        let outcomes: Vec<String> = sqlx::query_scalar(
            "SELECT outcome FROM job_attempts WHERE job_id = ? ORDER BY attempt_number",
        )
        .bind(&job.id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(outcomes, vec!["uncertain", "reconciliation_succeeded"]);
    }

    #[tokio::test]
    async fn reconciliation_cannot_claim_success_when_later_ordered_steps_did_not_run() {
        let (pool, adapters, resource) = setup().await;
        let job_id = submit_job(&pool, &resource, "reconcile-intermediate", "never").await;
        sqlx::query(
            "INSERT INTO job_steps \
             (id, job_id, position, kind, name, state, retry_class, recovery_class, updated_at) \
             VALUES (?, ?, 1, 'execute', 'later step', 'pending', 'never', 'reconcile', 99)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&job_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE jobs SET progress_total = 2 WHERE id = ?")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();

        let job = claim_next(&pool, &adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        let first = claim_step(&pool, &job.id, "worker-a", 101, 20)
            .await
            .unwrap()
            .unwrap();
        complete_step(
            &pool,
            &first,
            "worker-a",
            102,
            StepOutcome::Uncertain {
                code: "provider_timeout".into(),
                message: "Intermediate outcome could not be verified".into(),
                external_operation_id: Some("task-intermediate".into()),
                diagnostic: None,
            },
        )
        .await
        .unwrap();

        let (_, reconciliation) = claim_reconciliation(&pool, &adapters, "reconciler-a", 103, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            complete_reconciliation(
                &pool,
                &reconciliation,
                "reconciler-a",
                104,
                ReconcileOutcome::Succeeded {
                    result: serde_json::json!({"verified": true}),
                },
            )
            .await
            .unwrap(),
            JobState::Failed
        );
        let summary = jobs::get(&pool, &job_id).await.unwrap().unwrap();
        assert_eq!(summary.state, JobState::Failed);
        assert_eq!(summary.error.unwrap().code, "incomplete_ordered_plan");
    }

    #[tokio::test]
    async fn shutdown_can_release_reconciliation_before_provider_verification() {
        let (pool, adapters, resource) = setup().await;
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "release-reconciliation").await;
        complete_step(
            &pool,
            &step,
            "worker-a",
            102,
            StepOutcome::Uncertain {
                code: "provider_timeout".into(),
                message: "Provider outcome could not be verified".into(),
                external_operation_id: Some("task-release".into()),
                diagnostic: None,
            },
        )
        .await
        .unwrap();
        let (_, reconciliation) = claim_reconciliation(&pool, &adapters, "reconciler-a", 103, 20)
            .await
            .unwrap()
            .unwrap();

        assert!(release_reconciliation_before_verification(
            &pool,
            &reconciliation,
            "reconciler-a",
            104,
        )
        .await
        .unwrap());
        let summary = jobs::get(&pool, &job.id).await.unwrap().unwrap();
        assert_eq!(summary.state, JobState::NeedsAttention);
        let lease_owner: Option<String> =
            sqlx::query_scalar("SELECT lease_owner FROM jobs WHERE id = ?")
                .bind(&job.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(lease_owner.is_none());
        let outcome: String = sqlx::query_scalar(
            "SELECT outcome FROM job_attempts WHERE job_id = ? ORDER BY attempt_number DESC LIMIT 1",
        )
        .bind(&job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(outcome, "runtime_shutdown_before_reconciliation");
    }

    #[tokio::test]
    async fn execution_rejects_stale_external_fingerprint_before_adapter_call() {
        struct StaleAdapter;

        #[async_trait]
        impl OperationAdapter for StaleAdapter {
            fn key(&self) -> &'static str {
                "containers"
            }

            fn actions(&self) -> &[&'static str] {
                &["container.start"]
            }

            async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
                bail!("not used")
            }

            async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
                Ok("changed-state".into())
            }

            async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
                panic!("stale execution must stop before the provider call")
            }

            async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
                bail!("not used")
            }
        }

        let (pool, _, resource) = setup().await;
        let mut adapters = AdapterRegistry::new();
        adapters.register(Arc::new(StaleAdapter)).unwrap();
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "stale-fingerprint").await;
        assert_eq!(
            execute_claimed_step(&pool, &adapters, &job, &step, "worker-a", &FixedClock(102),)
                .await
                .unwrap(),
            JobState::Failed
        );
        let summary = jobs::get(&pool, &job.id).await.unwrap().unwrap();
        assert_eq!(summary.error.unwrap().code, "stale_external_state");
    }

    #[tokio::test]
    async fn execution_rejects_an_unavailable_capability_on_an_active_resource() {
        let (pool, adapters, resource) = setup().await;
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "unavailable-capability").await;
        sqlx::query(
            "UPDATE resource_capabilities SET availability = 'unavailable' \
             WHERE resource_id = ? AND action = 'container.start'",
        )
        .bind(&resource.id)
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(
            execute_claimed_step(&pool, &adapters, &job, &step, "worker-a", &FixedClock(102),)
                .await
                .unwrap(),
            JobState::Failed
        );
        let summary = jobs::get(&pool, &job.id).await.unwrap().unwrap();
        assert_eq!(summary.error.unwrap().code, "capability_unavailable");
    }

    #[tokio::test]
    async fn expired_non_retryable_attempt_needs_attention() {
        let (pool, adapters, resource) = setup().await;
        let job_id = submit_job(&pool, &resource, "uncertain", "never").await;
        claim_next(&pool, &adapters, "worker-a", 100, 10)
            .await
            .unwrap()
            .unwrap();
        claim_step(&pool, &job_id, "worker-a", 101, 10)
            .await
            .unwrap()
            .unwrap();
        sqlx::query("UPDATE jobs SET lease_expires_at = 99 WHERE id = ?")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(recover_expired(&pool, 100).await.unwrap(), 1);
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::NeedsAttention
        );
        let step_state: String = sqlx::query_scalar("SELECT state FROM job_steps WHERE job_id = ?")
            .bind(&job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(step_state, "needs_attention");
    }

    #[tokio::test]
    async fn expired_retryable_attempt_is_requeued_with_history() {
        let (pool, adapters, resource) = setup().await;
        let job_id = submit_job(&pool, &resource, "retryable", "transient").await;
        claim_next(&pool, &adapters, "worker-a", 100, 10)
            .await
            .unwrap()
            .unwrap();
        claim_step(&pool, &job_id, "worker-a", 101, 10)
            .await
            .unwrap()
            .unwrap();
        sqlx::query("UPDATE jobs SET lease_expires_at = 99 WHERE id = ?")
            .bind(&job_id)
            .execute(&pool)
            .await
            .unwrap();
        recover_expired(&pool, 100).await.unwrap();
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::Queued
        );
        let attempt_outcome: String =
            sqlx::query_scalar("SELECT outcome FROM job_attempts WHERE job_id = ?")
                .bind(&job_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(attempt_outcome, "lease_expired");
    }

    #[tokio::test]
    async fn cancellation_is_terminal_before_claim_and_at_the_next_safe_checkpoint() {
        let (pool, adapters, resource) = setup().await;
        let queued = submit_job(&pool, &resource, "queued-cancel", "never").await;
        let actor = ActorRef {
            actor_type: ActorType::Human,
            id: Some("owner".into()),
            source: Some("test".into()),
        };
        assert_eq!(
            request_cancellation(&pool, &queued, actor.clone(), 100)
                .await
                .unwrap(),
            JobState::Cancelled
        );

        let running = submit_job(&pool, &resource, "running-cancel", "never").await;
        claim_next(&pool, &adapters, "worker-a", 101, 10)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            request_cancellation(&pool, &running, actor, 102)
                .await
                .unwrap(),
            JobState::Running
        );
        let claimed_job = jobs::get(&pool, &running).await.unwrap().unwrap();
        assert_eq!(claimed_job.state, JobState::Running);
        let step = claim_step(&pool, &running, "worker-a", 103, 10)
            .await
            .unwrap()
            .unwrap();
        let job = ClaimedJob {
            id: running.clone(),
            action: "container.start".into(),
            resource,
            input: serde_json::json!({}),
            external_fingerprint: "state-1".into(),
            lease_expires_at: 113,
        };
        assert_eq!(
            execute_claimed_step(&pool, &adapters, &job, &step, "worker-a", &FixedClock(104),)
                .await
                .unwrap(),
            JobState::Cancelled
        );
        assert_eq!(
            jobs::get(&pool, &running).await.unwrap().unwrap().state,
            JobState::Cancelled
        );
    }

    #[tokio::test]
    async fn completion_uses_time_sampled_after_provider_execution() {
        struct TimeAdvancingAdapter {
            clock: Arc<AtomicClock>,
        }

        #[async_trait]
        impl OperationAdapter for TimeAdvancingAdapter {
            fn key(&self) -> &'static str {
                "containers"
            }

            fn actions(&self) -> &[&'static str] {
                &["container.start"]
            }

            async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
                bail!("not used")
            }

            async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
                Ok("state-1".into())
            }

            async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
                self.clock.set(109);
                Ok(StepOutcome::Succeeded {
                    result: serde_json::json!({"completed": true}),
                    external_operation_id: None,
                })
            }

            async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
                bail!("not used")
            }
        }

        let (pool, _, resource) = setup().await;
        let clock = Arc::new(AtomicClock::new(102));
        let mut adapters = AdapterRegistry::new();
        adapters
            .register(Arc::new(TimeAdvancingAdapter {
                clock: clock.clone(),
            }))
            .unwrap();
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "fresh-completion-time").await;

        assert_eq!(
            execute_claimed_step(&pool, &adapters, &job, &step, "worker-a", clock.as_ref())
                .await
                .unwrap(),
            JobState::Succeeded
        );
        let finished_at: i64 =
            sqlx::query_scalar("SELECT finished_at FROM job_attempts WHERE job_id = ?")
                .bind(&job.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(finished_at, 109);
    }

    #[tokio::test]
    async fn untyped_adapter_errors_are_bounded_and_pattern_redacted() {
        struct LeakyAdapter;

        #[async_trait]
        impl OperationAdapter for LeakyAdapter {
            fn key(&self) -> &'static str {
                "containers"
            }

            fn actions(&self) -> &[&'static str] {
                &["container.start"]
            }

            async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
                bail!("not used")
            }

            async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
                Ok("state-1".into())
            }

            async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
                bail!(
                    "api_key=provider-secret {}",
                    "x".repeat(MAX_PERSISTED_TEXT_CHARS + 100)
                )
            }

            async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
                bail!("not used")
            }
        }

        let (pool, _, resource) = setup().await;
        let mut adapters = AdapterRegistry::new();
        adapters.register(Arc::new(LeakyAdapter)).unwrap();
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "redacted-adapter-error").await;
        assert_eq!(
            execute_claimed_step(&pool, &adapters, &job, &step, "worker-a", &FixedClock(102),)
                .await
                .unwrap(),
            JobState::NeedsAttention
        );
        let error = jobs::get(&pool, &job.id)
            .await
            .unwrap()
            .unwrap()
            .error
            .unwrap();
        assert!(!error.message.contains("provider-secret"));
        assert!(error.message.contains("[REDACTED]"));
        assert!(error.message.chars().count() <= MAX_PERSISTED_TEXT_CHARS + 20);
    }

    #[tokio::test]
    async fn shutdown_release_requires_a_safe_checkpoint() {
        let (pool, adapters, resource) = setup().await;
        let job_id = submit_job(&pool, &resource, "safe-release", "never").await;
        claim_next(&pool, &adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();

        assert!(release_claimed_job(&pool, &job_id, "worker-a", 101)
            .await
            .unwrap());
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::Queued
        );
        let event: String = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE job_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(&job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event, "job.recovered.v1");

        let claimed = claim_next(&pool, &adapters, "worker-b", 102, 20)
            .await
            .unwrap()
            .unwrap();
        let _step = claim_step(&pool, &claimed.id, "worker-b", 103, 20)
            .await
            .unwrap()
            .unwrap();
        assert!(!release_claimed_job(&pool, &job_id, "worker-b", 104)
            .await
            .unwrap());
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::Running
        );

        assert!(
            release_step_before_execution(&pool, &_step, "worker-b", 105)
                .await
                .unwrap()
        );
        assert_eq!(
            jobs::get(&pool, &job_id).await.unwrap().unwrap().state,
            JobState::Queued
        );
        let attempt_outcome: String = sqlx::query_scalar(
            "SELECT outcome FROM job_attempts WHERE job_id = ? ORDER BY attempt_number DESC LIMIT 1",
        )
        .bind(&job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(attempt_outcome, "runtime_shutdown_before_execution");
    }
}
