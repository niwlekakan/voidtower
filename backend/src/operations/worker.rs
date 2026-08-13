//! Durable worker repository primitives.
//!
//! Concrete domain adapters are not started from `main` yet. These functions establish the
//! fail-closed lease, concurrency, attempt, cancellation, and restart-recovery semantics that the
//! worker loop will use once the runtime adapter registry is complete.

use super::{
    adapters::AdapterRegistry,
    contracts::{ActorRef, ActorType, JobState, PlannedStepV1, ResourceRef},
    events::{self, PendingEvent},
};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

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
    pub lease_expires_at: i64,
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
    let candidate: Option<ClaimRow> = sqlx::query_as(
        "SELECT j.id, j.action, j.resource_id, j.resource_revision, r.kind AS resource_kind, \
                r.display_name AS resource_name, j.input_json, j.external_fingerprint \
         FROM jobs j JOIN resources r ON r.id = j.resource_id \
         WHERE j.state = 'queued' \
           AND EXISTS (SELECT 1 FROM resource_capabilities c \
                       WHERE c.resource_id = j.resource_id AND c.action = j.action \
                         AND c.availability = 'available') \
           AND NOT EXISTS (SELECT 1 FROM jobs active \
                           WHERE active.state = 'running' \
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
                           WHERE active.state = 'running' \
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
    let owns_job: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM jobs WHERE id = ? AND state = 'running' \
         AND lease_owner = ? AND lease_expires_at > ? AND cancel_requested = 0",
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
        "SELECT id, job_id, position, kind, name, retry_class, recovery_class \
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
         WHERE id = ? AND state = 'running' AND lease_owner = ? AND lease_expires_at > ?",
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

pub async fn request_cancellation(
    pool: &SqlitePool,
    job_id: &str,
    actor: ActorRef,
    now: i64,
) -> Result<JobState> {
    let mut transaction = pool.begin().await?;
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
    use std::sync::Arc;

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
    async fn cancellation_is_terminal_before_claim_and_pending_while_running() {
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
        assert!(claim_step(&pool, &running, "worker-a", 103, 10)
            .await
            .unwrap()
            .is_none());
    }
}
