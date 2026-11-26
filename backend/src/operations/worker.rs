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
    schemas,
};
use crate::api::mcp::action_registry::{self, RetryClass};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use sqlx::{AssertSqlSafe, FromRow, SqlitePool};

const MAX_PERSISTED_TEXT_CHARS: usize = 4096;
const REDACTION_LOOKAHEAD_CHARS: usize = 256;

/// Job states that release a `concurrency_key`. `JobState::is_terminal` is the authority; this list
/// exists only because the claim predicate and the pre-execution checkpoint are SQL, and
/// `terminal_job_state_sql_matches_the_job_state_contract` keeps the two in step.
const TERMINAL_JOB_STATES: &str = "'succeeded', 'failed', 'cancelled', 'rejected', 'expired'";

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum CancellationError {
    #[error("job not found")]
    NotFound,
    #[error("job cannot be cancelled in its current state")]
    InvalidState,
    #[error("cancellation persistence failed")]
    Internal,
}

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
    // A `concurrency_key` admits one unresolved operation at a time, and "unresolved" is every
    // state that is not terminal, not only `running`/`needs_attention`. A `queued` submission from
    // the internal `backup.restore_test` scheduler must wait behind an earlier job on the same key
    // instead of becoming claimable the moment that job stops running.
    // Precedence, not mere existence, decides the head of the key: a plain NOT EXISTS would make
    // two queued siblings block each other forever, while the oldest unresolved job is exactly the
    // one that has to run first. Precedence is submission order and must never depend on
    // queue-entry time: `queued_at` is re-stamped by approval resumption
    // (`backend/src/operations/approvals.rs`), retryable requeue, runtime release and lease
    // recovery, so ordering by it lets a later sibling overtake an earlier, operator-approved
    // intent. `submitted_at` is written once at submission and is never rewritten.
    // `submitted_at` has one-second resolution, so two intents accepted in the same second tie;
    // the tie-break is `rowid`, the insertion order, never the random job UUID (which would let a
    // later submission jump an unresolved earlier one).
    let candidate: Option<ClaimRow> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT j.id, j.action, j.resource_id, j.resource_revision, r.kind AS resource_kind, \
                r.display_name AS resource_name, j.input_json, j.external_fingerprint \
         FROM jobs j JOIN resources r ON r.id = j.resource_id \
         WHERE j.state = 'queued' \
           AND EXISTS (SELECT 1 FROM resource_capabilities c \
                       WHERE c.resource_id = j.resource_id AND c.action = j.action \
                         AND c.availability = 'available') \
           AND NOT EXISTS (SELECT 1 FROM jobs prior \
                           WHERE prior.concurrency_key = j.concurrency_key \
                             AND prior.id <> j.id \
                             AND prior.state NOT IN ({TERMINAL_JOB_STATES}) \
                             AND (prior.submitted_at, prior.rowid) \
                               < (j.submitted_at, j.rowid)) \
         ORDER BY j.submitted_at, j.rowid LIMIT 1"
    )))
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
    // The same precedence predicate decides the write, so a sibling that lands between the
    // candidate read and this update cannot be jumped over: the claim takes the head of the key
    // or nothing.
    let updated = sqlx::query(AssertSqlSafe(format!(
        "UPDATE jobs SET state = 'running', lease_owner = ?, lease_expires_at = ?, \
         started_at = COALESCE(started_at, ?), updated_at = ? \
         WHERE id = ? AND state = 'queued' \
           AND NOT EXISTS (SELECT 1 FROM jobs prior \
                           WHERE prior.concurrency_key = jobs.concurrency_key \
                             AND prior.id <> jobs.id \
                             AND prior.state NOT IN ({TERMINAL_JOB_STATES}) \
                             AND (prior.submitted_at, prior.rowid) \
                               < (jobs.submitted_at, jobs.rowid))"
    )))
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
    // The checkpoint re-verifies the claim's precedence invariant, not only the observed resource
    // revision. `resources.revision` tracks observed state, and container/service mutations do not
    // bump it, so a revision-only guard waves a deferred sibling through; the same
    // earlier-unresolved-job test that gates the claim is the honest check here. Ordering is
    // immutable submission order, matching the claim predicate, because `queued_at` is re-stamped
    // by approval resumption, retry requeue, runtime release and lease recovery. The tie-break is
    // `rowid` (insertion order), because two intents submitted in the same second share
    // `submitted_at`.
    let preflight: Option<(i64, String, String, i64, i64)> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT r.revision, r.lifecycle_state, COALESCE(c.availability, 'missing'), \
                j.cancel_requested, \
                (SELECT COUNT(*) FROM jobs prior \
                 WHERE prior.concurrency_key = j.concurrency_key \
                   AND prior.id <> j.id \
                   AND prior.state NOT IN ({TERMINAL_JOB_STATES}) \
                   AND (prior.submitted_at, prior.rowid) \
                     < (j.submitted_at, j.rowid)) \
         FROM jobs j \
         JOIN resources r ON r.id = j.resource_id \
         LEFT JOIN resource_capabilities c ON c.resource_id = j.resource_id AND c.action = j.action \
         WHERE j.id = ? AND j.state = 'running' AND j.lease_owner = ? \
           AND j.lease_expires_at > ?"
    )))
    .bind(&job.id)
    .bind(worker_id)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    let Some((
        resource_revision,
        lifecycle_state,
        capability,
        cancel_requested,
        unresolved_predecessors,
    )) = preflight
    else {
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
    if unresolved_predecessors > 0 {
        return complete_step(
            pool,
            step,
            worker_id,
            clock.now(),
            StepOutcome::Failed {
                code: "unresolved_prior_operation".into(),
                message: "An earlier operation on this resource is still unresolved".into(),
                retryable: false,
                diagnostic: None,
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
    if let StepOutcome::Succeeded { result, .. } = &outcome {
        schemas::validate_result(action, result)
            .map_err(|reason| anyhow::anyhow!("action result rejected: {reason}"))?;
    }
    let outcome = sanitize_outcome(outcome)?;

    let (state, event_type, audit_outcome) = match outcome {
        StepOutcome::Succeeded {
            result,
            external_operation_id,
        } => {
            schemas::validate_result(action, &result)
                .map_err(|reason| anyhow::anyhow!("action result rejected: {reason}"))?;
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

    let action =
        action_registry::action(&row.action).context("reconciliation action is not registered")?;
    if let ReconcileOutcome::Succeeded { result } = &outcome {
        schemas::validate_result(action, result)
            .map_err(|reason| anyhow::anyhow!("reconciliation result rejected: {reason}"))?;
    }
    let outcome = sanitize_reconciliation(outcome)?;

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
            schemas::validate_result(action, &result)
                .map_err(|reason| anyhow::anyhow!("reconciliation result rejected: {reason}"))?;
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
    let code = safe_error_code(code);
    let message = safe_error_text(message);
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
            result: safe_value(result)?,
            external_operation_id: external_operation_id.map(|value| safe_text(&value)),
        },
        StepOutcome::Failed {
            code,
            message,
            retryable,
            diagnostic,
        } => StepOutcome::Failed {
            code: safe_error_code(&code),
            message: safe_error_text(&message),
            retryable,
            diagnostic: diagnostic.map(safe_value).transpose()?,
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
            code: safe_error_code(&code),
            message: safe_error_text(&message),
            external_operation_id: external_operation_id.map(|value| safe_text(&value)),
            diagnostic: diagnostic.map(safe_value).transpose()?,
        },
    })
}

fn sanitize_reconciliation(outcome: ReconcileOutcome) -> Result<ReconcileOutcome> {
    match outcome {
        ReconcileOutcome::Succeeded { result } => Ok(ReconcileOutcome::Succeeded {
            result: safe_value(result)?,
        }),
        ReconcileOutcome::Failed { code, message } => Ok(ReconcileOutcome::Failed {
            code: safe_error_code(&code),
            message: safe_error_text(&message),
        }),
        ReconcileOutcome::StillUncertain { message } => Ok(ReconcileOutcome::StillUncertain {
            message: safe_error_text(&message),
        }),
    }
}

fn safe_value(value: Value) -> Result<Value> {
    safe_value_at_depth(value, 0)
}

fn safe_value_at_depth(value: Value, depth: usize) -> Result<Value> {
    anyhow::ensure!(
        depth <= schemas::MAX_ACTION_INPUT_DEPTH,
        "action result exceeds maximum nesting depth"
    );
    match value {
        Value::String(value) => Ok(Value::String(safe_text(&value))),
        Value::Array(values) => {
            anyhow::ensure!(
                values.len() <= schemas::MAX_ACTION_INPUT_ITEMS,
                "action result contains too many array items"
            );
            values
                .into_iter()
                .map(|value| safe_value_at_depth(value, depth + 1))
                .collect::<Result<Vec<_>>>()
                .map(Value::Array)
        }
        Value::Object(values) => {
            anyhow::ensure!(
                values.len() <= schemas::MAX_ACTION_INPUT_ITEMS,
                "action result contains too many object properties"
            );
            values
                .into_iter()
                .map(|(key, value)| -> Result<_> {
                    anyhow::ensure!(
                        key.chars().count() <= schemas::MAX_ACTION_INPUT_STRING_CHARS,
                        "action result object key is too long"
                    );
                    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
                    let value = if [
                        "password",
                        "passwd",
                        "token",
                        "secret",
                        "credential",
                        "apikey",
                        "accesskey",
                        "privatekey",
                        "authtoken",
                        "secretkey",
                        "clientsecret",
                        "refreshtoken",
                        "pwd",
                    ]
                    .iter()
                    .any(|needle| normalized.contains(needle))
                    {
                        Value::String("[REDACTED]".into())
                    } else {
                        safe_value_at_depth(value, depth + 1)?
                    };
                    Ok((key, value))
                })
                .collect::<Result<serde_json::Map<_, _>>>()
                .map(Value::Object)
        }
        scalar => Ok(scalar),
    }
}

fn safe_text(value: &str) -> String {
    let bounded_input: String = value
        .chars()
        .take(MAX_PERSISTED_TEXT_CHARS + REDACTION_LOOKAHEAD_CHARS)
        .collect();
    let redacted = crate::api::mcp::redact::redact_patterns(&bounded_input);
    let mut chars = redacted.chars();
    let mut bounded: String = chars.by_ref().take(MAX_PERSISTED_TEXT_CHARS).collect();
    if chars.next().is_some() {
        bounded.push_str("…[truncated]");
    }
    bounded
}

fn safe_error_code(value: &str) -> String {
    safe_text(value).chars().take(128).collect()
}

fn safe_error_text(value: &str) -> String {
    safe_text(value).chars().take(1024).collect()
}

pub async fn request_cancellation(
    pool: &SqlitePool,
    job_id: &str,
    actor: ActorRef,
    now: i64,
) -> std::result::Result<JobState, CancellationError> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| CancellationError::Internal)?;
    acquire_write_intent(&mut transaction)
        .await
        .map_err(|_| CancellationError::Internal)?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT state, resource_id FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| CancellationError::Internal)?;
    let (state, resource_id) = row.ok_or(CancellationError::NotFound)?;
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
            .await
            .map_err(|_| CancellationError::Internal)?;
            sqlx::query(
                "UPDATE job_steps SET state = 'cancelled', finished_at = ?, updated_at = ? \
                 WHERE job_id = ? AND state = 'pending'",
            )
            .bind(now)
            .bind(now)
            .bind(job_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| CancellationError::Internal)?;
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
            .await
            .map_err(|_| CancellationError::Internal)?;
            JobState::Running
        }
        _ => return Err(CancellationError::InvalidState),
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
    .await
    .map_err(|_| CancellationError::Internal)?;
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
    .await
    .map_err(|_| CancellationError::Internal)?;
    transaction
        .commit()
        .await
        .map_err(|_| CancellationError::Internal)?;
    Ok(target)
}

/// Recover jobs whose worker lease expired. A job with no started step is safe to requeue. A
/// started retryable step is requeued for a new attempt; a non-retryable/uncertain step is moved to
/// `needs_attention` and is never replayed blindly. Every durable mutating action is declared
/// `RetryClass::Never` with exactly one attempt — the mutating constructors hardcode both and the
/// explicit mutating entries pass them literally — so a dispatched mutation takes the parking
/// branch. Keep that property when adding a mutating action: a retryable mutating step would be
/// re-dispatched here after the provider call may already have applied.
pub async fn recover_expired(pool: &SqlitePool, now: i64) -> Result<u64> {
    let mut transaction = pool.begin().await?;
    acquire_write_intent(&mut transaction).await?;
    let jobs: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, action, resource_id FROM jobs WHERE state = 'running' \
         AND lease_expires_at IS NOT NULL AND lease_expires_at <= ? ORDER BY id",
    )
    .bind(now)
    .fetch_all(&mut *transaction)
    .await?;
    let mut recovered = 0u64;
    for (job_id, action, resource_id) in jobs {
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
        let actor = ActorRef {
            actor_type: ActorType::System,
            id: None,
            source: Some("operation_worker".into()),
        };
        insert_worker_audit(
            &mut transaction,
            now,
            &actor,
            &action,
            &job_id,
            if target == "needs_attention" {
                "needs_attention"
            } else {
                "recovered"
            },
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
        approvals,
        contracts::{CapabilityAvailability, OperationPlanV1, PlannedStepV1},
        jobs::{self, SubmissionPolicy, SubmitJob},
        resources::{self, ObserveResource},
    };
    use anyhow::bail;
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::{
        atomic::{AtomicI64, AtomicUsize, Ordering},
        Arc,
    };

    #[test]
    fn successful_results_redact_credential_named_fields_before_persistence() {
        let sanitized = safe_value(serde_json::json!({
            "api_key": "api-value",
            "access_key": "access-value",
            "private_key": "private-value",
            "auth_token": "auth-value",
            "secret_key": "secret-value",
            "client_secret": "client-value",
            "refresh_token": "refresh-value",
            "pwd": "pwd-value",
            "apiKey": "camel-api-value",
            "access-key": "hyphen-access-value",
            "privateKey": "camel-private-value",
        }))
        .expect("sanitized value");
        let object = sanitized.as_object().expect("sanitized object");
        for key in [
            "api_key",
            "access_key",
            "private_key",
            "auth_token",
            "secret_key",
            "client_secret",
            "refresh_token",
            "pwd",
            "apiKey",
            "access-key",
            "privateKey",
        ] {
            assert_eq!(object[key], serde_json::json!("[REDACTED]"));
        }
    }

    #[test]
    fn result_sanitization_rejects_unbounded_shape_before_recursive_redaction() {
        let oversized_items = serde_json::Value::Array(
            (0..=schemas::MAX_ACTION_INPUT_ITEMS)
                .map(|item| serde_json::json!(item))
                .collect(),
        );
        assert!(safe_value(oversized_items).is_err());

        let mut nested = serde_json::json!(null);
        for _ in 0..=schemas::MAX_ACTION_INPUT_DEPTH {
            nested = serde_json::json!([nested]);
        }
        assert!(safe_value(nested).is_err());
    }

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
        submit_job_with_ingress(pool, resource, idempotency_key, retry_class, "http").await
    }

    async fn submit_job_requiring_approval(
        pool: &SqlitePool,
        resource: &ResourceRef,
        idempotency_key: &str,
        expires_at: i64,
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
                        retry_class: "never".into(),
                        recovery_class: "reconcile".into(),
                    }],
                },
                idempotency_scope: "test:owner".into(),
                idempotency_key: idempotency_key.into(),
                concurrency_key: resource.id.clone(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
                policy: SubmissionPolicy::RequireApproval {
                    requirement: "risk_ladder".into(),
                    reason: "Assisted mode".into(),
                    expires_at,
                },
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn submit_job_with_ingress(
        pool: &SqlitePool,
        resource: &ResourceRef,
        idempotency_key: &str,
        retry_class: &str,
        ingress: &str,
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
                ingress: ingress.into(),
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
                        "bounded": "completed",
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
    async fn reconciliation_rejects_an_oversized_result_before_persistence() {
        let (pool, adapters, resource) = setup().await;
        let (job, step) =
            claim_job_and_step(&pool, &adapters, &resource, "reconcile-bounded").await;
        complete_step(
            &pool,
            &step,
            "worker-a",
            102,
            StepOutcome::Uncertain {
                code: "provider_timeout".into(),
                message: "Provider outcome could not be verified".into(),
                external_operation_id: Some("task-bounded".into()),
                diagnostic: None,
            },
        )
        .await
        .unwrap();
        let (_, reconciliation) = claim_reconciliation(&pool, &adapters, "reconciler-a", 103, 20)
            .await
            .unwrap()
            .unwrap();
        let oversized = (0..schemas::MAX_ACTION_INPUT_ITEMS)
            .map(|_| Value::String("x".repeat(1024)))
            .collect();
        assert!(complete_reconciliation(
            &pool,
            &reconciliation,
            "reconciler-a",
            104,
            ReconcileOutcome::Succeeded {
                result: Value::Array(oversized),
            },
        )
        .await
        .is_err());
        let summary = jobs::get(&pool, &job.id).await.unwrap().unwrap();
        assert_eq!(summary.state, JobState::NeedsAttention);
        let persisted: Option<String> =
            sqlx::query_scalar("SELECT result_json FROM job_steps WHERE id = ?")
                .bind(&reconciliation.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(persisted.is_none());
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
        let queued_event: String = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE job_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(&queued)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(queued_event, "job.cancelled.v1");
        let queued_audit: String = sqlx::query_scalar(
            "SELECT outcome FROM audit_log WHERE action = 'job.cancel' AND resource_id = ? \
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(&queued)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(queued_audit, "cancelled");

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
        let running_event: String = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE job_id = ? ORDER BY sequence DESC LIMIT 1",
        )
        .bind(&running)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(running_event, "job.cancellation_requested.v1");
        let running_audit: String = sqlx::query_scalar(
            "SELECT outcome FROM audit_log WHERE action = 'job.cancel' AND resource_id = ? \
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(&running)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(running_audit, "cancellation_pending");
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

    #[test]
    fn persisted_error_sanitizers_redact_boundary_values_and_bound_codes() {
        let mut boundary = "x".repeat(MAX_PERSISTED_TEXT_CHARS - 8);
        boundary.push_str("api_");
        boundary.push_str("key=boundary-value");
        let sanitized = safe_text(&boundary);
        assert!(!sanitized.contains("boundary-value"));
        assert!(
            sanitized.chars().count() <= MAX_PERSISTED_TEXT_CHARS + "…[truncated]".chars().count()
        );

        let code = safe_error_code(&"x".repeat(256));
        assert_eq!(code.chars().count(), 128);
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
    #[test]
    fn terminal_job_state_sql_matches_the_job_state_contract() {
        let listed: std::collections::BTreeSet<&str> = TERMINAL_JOB_STATES
            .split(',')
            .map(|state| state.trim().trim_matches('\''))
            .collect();
        let expected: std::collections::BTreeSet<&str> = [
            JobState::AwaitingApproval,
            JobState::Queued,
            JobState::Running,
            JobState::Succeeded,
            JobState::Failed,
            JobState::Cancelled,
            JobState::NeedsAttention,
            JobState::Rejected,
            JobState::Expired,
        ]
        .into_iter()
        .filter(|state| state.is_terminal())
        .map(JobState::as_str)
        .collect();
        assert_eq!(
            listed, expected,
            "the SQL terminal-state list must match JobState::is_terminal"
        );
    }

    #[tokio::test]
    async fn queued_ungated_submission_waits_for_an_unresolved_sibling_on_the_same_key() {
        let (pool, adapters, resource) = setup().await;
        // An approval-pending job is unresolved but not active: it holds the key without being
        // claimable, which is the state that used to let an ungated sibling through.
        let predecessor =
            submit_job_requiring_approval(&pool, &resource, "predecessor", 10_000).await;
        // A webhook/CLI/scheduler submission is not gated at submission time: it lands `queued`.
        let deferred =
            submit_job_with_ingress(&pool, &resource, "deferred-webhook", "never", "webhook").await;
        // Both submissions can share one timestamp second, so pin precedence explicitly.
        sqlx::query(
            "UPDATE jobs SET submitted_at = CASE WHEN id = ? THEN 1 ELSE 2 END WHERE id IN (?, ?)",
        )
        .bind(&predecessor)
        .bind(&predecessor)
        .bind(&deferred)
        .execute(&pool)
        .await
        .unwrap();

        assert!(
            claim_next(&pool, &adapters, "worker-a", 100, 20)
                .await
                .unwrap()
                .is_none(),
            "an ungated submission must not be claimed while an earlier job on its key is unresolved"
        );
        assert_eq!(
            jobs::get(&pool, &deferred).await.unwrap().unwrap().state,
            JobState::Queued,
            "the deferred submission stays durable and queued, not dropped"
        );

        // The approval decision makes the predecessor terminal; only then does the key release.
        sqlx::query(
            "UPDATE approvals SET status = 'rejected' WHERE job_id = ? AND status = 'pending'",
        )
        .bind(&predecessor)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE jobs SET state = 'rejected', error_code = 'approval_rejected' WHERE id = ?",
        )
        .bind(&predecessor)
        .execute(&pool)
        .await
        .unwrap();

        let claimed = claim_next(&pool, &adapters, "worker-a", 101, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            claimed.id, deferred,
            "the deferred submission becomes claimable once its predecessor is terminal"
        );
    }

    #[tokio::test]
    async fn same_second_submissions_keep_submission_order_on_a_concurrency_key() {
        let (pool, adapters, resource) = setup().await;
        // Two intents submitted in the same second share `submitted_at` and `queued_at` (unix
        // seconds). Insert them so the *later* one sorts first by id: only the ordering rule can
        // decide precedence, so a random-UUID tie-break must not let a later submission jump an
        // unresolved earlier one.
        insert_queued_job_row(&pool, &resource, "zzzz-earlier", "needs_attention", 100).await;
        insert_queued_job_row(&pool, &resource, "aaaa-later", "queued", 100).await;

        assert!(
            claim_next(&pool, &adapters, "worker-a", 100, 20)
                .await
                .unwrap()
                .is_none(),
            "a later same-second submission must not be claimed while the earlier one is unresolved"
        );

        // The key releases the later intent only once the earlier job is terminal.
        sqlx::query("UPDATE jobs SET state = 'succeeded' WHERE id = 'zzzz-earlier'")
            .execute(&pool)
            .await
            .unwrap();
        let claimed = claim_next(&pool, &adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, "aaaa-later");
    }

    /// M1-02 residual — precedence must be immutable submission order. Approving an earlier intent
    /// re-stamps `queued_at` (`approvals::decide`), so a key ordered by queue-entry time lets a
    /// later sibling overtake the approved earlier intent. `submitted_at` is written once at
    /// submission and is never rewritten afterwards.
    #[tokio::test]
    async fn approval_resumption_keeps_submission_order_on_a_concurrency_key() {
        let (pool, adapters, resource) = setup().await;
        let predecessor =
            submit_job_requiring_approval(&pool, &resource, "predecessor", i64::MAX / 2).await;
        let deferred =
            submit_job_with_ingress(&pool, &resource, "deferred-webhook", "never", "webhook").await;
        sqlx::query(
            "UPDATE jobs SET submitted_at = CASE WHEN id = ? THEN 1 ELSE 2 END, \
             queued_at = CASE WHEN id = ? THEN 10 ELSE 20 END WHERE id IN (?, ?)",
        )
        .bind(&predecessor)
        .bind(&predecessor)
        .bind(&predecessor)
        .bind(&deferred)
        .execute(&pool)
        .await
        .unwrap();

        let approval_id: String = sqlx::query_scalar("SELECT id FROM approvals WHERE job_id = ?")
            .bind(&predecessor)
            .fetch_one(&pool)
            .await
            .unwrap();
        approvals::approve(
            &pool,
            &adapters,
            &approval_id,
            ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner".into()),
                source: Some("test".into()),
            },
            None,
        )
        .await
        .unwrap();

        let predecessor_queued_at: i64 =
            sqlx::query_scalar("SELECT queued_at FROM jobs WHERE id = ?")
                .bind(&predecessor)
                .fetch_one(&pool)
                .await
                .unwrap();
        let deferred_queued_at: i64 = sqlx::query_scalar("SELECT queued_at FROM jobs WHERE id = ?")
            .bind(&deferred)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            predecessor_queued_at > deferred_queued_at,
            "approval resumption must have re-stamped the earlier intent later than its sibling \
             for this regression to be meaningful ({predecessor_queued_at} vs {deferred_queued_at})"
        );

        let claimed = claim_next(&pool, &adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            claimed.id, predecessor,
            "an operator-approved earlier submission must stay ahead of a later sibling"
        );
    }

    /// M1-02 residual — a lease-expired retryable step is requeued with a fresh `queued_at`. That
    /// must not move the job behind a later submission: the recovered job keeps its submission
    /// position, so it stays the head of its key and continues to hold it while its provider
    /// outcome may still be in flight.
    #[tokio::test]
    async fn lease_recovery_requeue_keeps_precedence_over_a_later_sibling() {
        let (pool, adapters, resource) = setup().await;
        let retryable = submit_job(&pool, &resource, "retryable", "transient").await;
        let sibling = submit_job(&pool, &resource, "sibling", "never").await;
        sqlx::query(
            "UPDATE jobs SET submitted_at = CASE WHEN id = ? THEN 1 ELSE 2 END, \
             queued_at = CASE WHEN id = ? THEN 10 ELSE 20 END WHERE id IN (?, ?)",
        )
        .bind(&retryable)
        .bind(&retryable)
        .bind(&retryable)
        .bind(&sibling)
        .execute(&pool)
        .await
        .unwrap();

        let claimed = claim_next(&pool, &adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, retryable);
        let _step = claim_step(&pool, &retryable, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();

        // The lease expires while the retryable step is in flight; recovery requeues it and
        // re-stamps `queued_at` to the recovery time.
        recover_expired(&pool, 130).await.unwrap();
        let requeued_at: i64 = sqlx::query_scalar("SELECT queued_at FROM jobs WHERE id = ?")
            .bind(&retryable)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(
            requeued_at > 20,
            "lease recovery must re-stamp the requeued job's queue-entry time"
        );

        let claimed = claim_next(&pool, &adapters, "worker-b", 200, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            claimed.id, retryable,
            "a lease-recovered job keeps its submission position and stays the head of its key"
        );
        assert_eq!(
            jobs::get(&pool, &sibling).await.unwrap().unwrap().state,
            JobState::Queued,
            "the later sibling must not run while the recovered job is unresolved"
        );
    }

    async fn insert_queued_job_row(
        pool: &SqlitePool,
        resource: &ResourceRef,
        id: &str,
        state: &str,
        at: i64,
    ) {
        sqlx::query(
            "INSERT INTO jobs (id, action, resource_id, resource_revision, actor_type, ingress, \
             input_json, request_digest, plan_json, plan_digest, external_fingerprint, state, \
             idempotency_scope, idempotency_key, concurrency_key, retry_class, recovery_class, \
             submitted_at, queued_at, updated_at) \
             VALUES (?, 'container.start', ?, ?, 'human', 'webhook', '{}', 'request-digest', \
                     '{}', 'plan-digest', 'state-1', ?, 'test:owner', ?, ?, 'never', 'reconcile', \
                     ?, ?, ?)",
        )
        .bind(id)
        .bind(&resource.id)
        .bind(resource.revision)
        .bind(state)
        .bind(format!("key-{id}"))
        .bind(&resource.id)
        .bind(at)
        .bind(at)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn pre_execution_checkpoint_refuses_a_deferred_mutation_without_a_revision_bump() {
        struct RecordingAdapter {
            executions: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl OperationAdapter for RecordingAdapter {
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
                self.executions.fetch_add(1, Ordering::SeqCst);
                Ok(StepOutcome::Succeeded {
                    result: serde_json::json!({"message": "started"}),
                    external_operation_id: None,
                })
            }

            async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
                bail!("not used")
            }
        }

        let (pool, _fake, resource) = setup().await;
        let executions = Arc::new(AtomicUsize::new(0));
        let mut adapters = AdapterRegistry::new();
        adapters
            .register(Arc::new(RecordingAdapter {
                executions: executions.clone(),
            }))
            .unwrap();

        let predecessor = submit_job(&pool, &resource, "predecessor", "never").await;
        let deferred =
            submit_job_with_ingress(&pool, &resource, "deferred-webhook", "never", "webhook").await;
        // The earlier submission carries a *later* queue-entry time, which is what approval
        // resumption, retry requeue and lease recovery all do in production. Precedence must still
        // follow the immutable submission order.
        sqlx::query(
            "UPDATE jobs SET submitted_at = CASE WHEN id = ? THEN 1 ELSE 2 END, \
             queued_at = CASE WHEN id = ? THEN 20 ELSE 10 END WHERE id IN (?, ?)",
        )
        .bind(&predecessor)
        .bind(&predecessor)
        .bind(&predecessor)
        .bind(&deferred)
        .execute(&pool)
        .await
        .unwrap();
        let (predecessor_queued_at, deferred_queued_at): (i64, i64) = sqlx::query_as(
            "SELECT MAX(CASE WHEN id = ? THEN queued_at END), MAX(CASE WHEN id = ? THEN queued_at END) \
             FROM jobs WHERE id IN (?, ?)",
        )
        .bind(&predecessor)
        .bind(&deferred)
        .bind(&predecessor)
        .bind(&deferred)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            predecessor_queued_at > deferred_queued_at,
            "the earlier submission must carry the later queue-entry time for this regression to \
             be meaningful"
        );

        let claimed = claim_next(&pool, &adapters, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, predecessor);
        let step = claim_step(&pool, &predecessor, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        complete_step(
            &pool,
            &step,
            "worker-a",
            101,
            StepOutcome::Uncertain {
                code: "provider_timeout".into(),
                message: "Provider outcome could not be verified".into(),
                external_operation_id: None,
                diagnostic: None,
            },
        )
        .await
        .unwrap();

        // Model the claim selection this defect allowed: the deferred sibling reaches `running`
        // while the earlier job on the key is still unresolved. Its resource revision is unchanged,
        // so only a sibling-state check at the checkpoint can refuse it.
        sqlx::query(
            "UPDATE jobs SET state = 'running', lease_owner = 'worker-a', lease_expires_at = 200, \
             started_at = 100 WHERE id = ? AND state = 'queued'",
        )
        .bind(&deferred)
        .execute(&pool)
        .await
        .unwrap();
        let deferred_step = claim_step(&pool, &deferred, "worker-a", 100, 20)
            .await
            .unwrap()
            .unwrap();
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM resources WHERE id = ?")
            .bind(&resource.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let deferred_job = ClaimedJob {
            id: deferred.clone(),
            action: "container.start".into(),
            resource: resource.clone(),
            input: serde_json::json!({}),
            external_fingerprint: "state-1".into(),
            lease_expires_at: 200,
        };
        assert_eq!(
            revision, deferred_job.resource.revision,
            "the guard must fire although the observed resource revision never moved"
        );

        assert_eq!(
            execute_claimed_step(
                &pool,
                &adapters,
                &deferred_job,
                &deferred_step,
                "worker-a",
                &AtomicClock::new(101),
            )
            .await
            .unwrap(),
            JobState::Failed
        );
        assert_eq!(
            executions.load(Ordering::SeqCst),
            0,
            "the checkpoint must refuse before any provider call"
        );
        let summary = jobs::get(&pool, &deferred).await.unwrap().unwrap();
        let error = summary.error.expect("refusal error");
        assert_eq!(error.code, "unresolved_prior_operation");
        assert!(!error.retryable);
    }
}
