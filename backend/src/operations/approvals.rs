use super::{
    adapters::{AdapterRegistry, PlanRequest},
    contracts::{ActorRef, ActorType, ApprovalViewV1, ResourceRef},
    events::{self, PendingEvent},
    jobs, unix_now,
};
use anyhow::{bail, Context, Result};
use sqlx::SqlitePool;

#[derive(Debug, sqlx::FromRow)]
struct ApprovalPreflight {
    action: String,
    input_json: String,
    status: String,
    job_state: String,
    expires_at: i64,
    resource_id: String,
    resource_kind: String,
    resource_name: String,
    resource_revision: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct DecisionRow {
    job_id: String,
    approval_status: String,
    expires_at: i64,
    approved_resource_revision: i64,
    job_state: String,
    current_resource_revision: i64,
    resource_id: String,
    immutable_mismatch: i64,
    external_fingerprint: String,
}

pub async fn list(
    pool: &SqlitePool,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<ApprovalViewV1>> {
    let status = status.unwrap_or("pending");
    Ok(sqlx::query_as(
        "SELECT id, job_id, requirement, reason, status, expires_at, decided_by, \
                decision_comment, requested_at, decided_at, updated_at \
         FROM approvals WHERE status = ? ORDER BY requested_at DESC LIMIT ?",
    )
    .bind(status)
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, approval_id: &str) -> Result<Option<ApprovalViewV1>> {
    Ok(sqlx::query_as(
        "SELECT id, job_id, requirement, reason, status, expires_at, decided_by, \
                decision_comment, requested_at, decided_at, updated_at \
         FROM approvals WHERE id = ?",
    )
    .bind(approval_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn approve(
    pool: &SqlitePool,
    adapters: &AdapterRegistry,
    approval_id: &str,
    actor: ActorRef,
    comment: Option<&str>,
) -> Result<super::contracts::JobSummaryV1> {
    validate_human_actor(&actor)?;
    let now = unix_now();
    let preflight: ApprovalPreflight = sqlx::query_as(
        "SELECT j.action, j.input_json, a.status, j.state AS job_state, a.expires_at, \
                r.id AS resource_id, r.kind AS resource_kind, r.display_name AS resource_name, \
                r.revision AS resource_revision \
         FROM approvals a JOIN jobs j ON j.id = a.job_id \
         JOIN resources r ON r.id = j.resource_id WHERE a.id = ?",
    )
    .bind(approval_id)
    .fetch_optional(pool)
    .await?
    .context("approval not found")?;
    if preflight.status != "pending" || preflight.job_state != "awaiting_approval" {
        bail!("approval is no longer pending");
    }

    let observed_fingerprint = if preflight.expires_at <= now {
        None
    } else {
        let request = PlanRequest {
            action: preflight.action.clone(),
            resource: ResourceRef {
                id: preflight.resource_id,
                kind: preflight.resource_kind,
                display_name: preflight.resource_name,
                revision: preflight.resource_revision,
            },
            input: serde_json::from_str(&preflight.input_json)?,
        };
        Some(
            adapters
                .for_action(&preflight.action)?
                .external_fingerprint(&request)
                .await
                .context("approval external-state revalidation failed")?,
        )
    };
    decide(
        pool,
        approval_id,
        actor,
        comment,
        true,
        observed_fingerprint.as_deref(),
    )
    .await
}

pub async fn reject(
    pool: &SqlitePool,
    approval_id: &str,
    actor: ActorRef,
    comment: Option<&str>,
) -> Result<super::contracts::JobSummaryV1> {
    validate_human_actor(&actor)?;
    decide(pool, approval_id, actor, comment, false, None).await
}

pub async fn expire_pending(pool: &SqlitePool, now: i64) -> Result<u64> {
    let mut transaction = pool.begin().await?;
    let expired: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT a.id, a.job_id, j.resource_id FROM approvals a \
         JOIN jobs j ON j.id = a.job_id \
         WHERE a.status = 'pending' AND a.expires_at <= ? AND j.state = 'awaiting_approval' \
         ORDER BY a.expires_at, a.id",
    )
    .bind(now)
    .fetch_all(&mut *transaction)
    .await?;
    let actor = ActorRef {
        actor_type: ActorType::System,
        id: None,
        source: Some("approval_expiry".into()),
    };
    let mut count = 0u64;

    for (approval_id, job_id, resource_id) in expired {
        let updated = sqlx::query(
            "UPDATE approvals SET status = 'expired', decided_at = ?, updated_at = ? \
             WHERE id = ? AND status = 'pending' AND expires_at <= ?",
        )
        .bind(now)
        .bind(now)
        .bind(&approval_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() == 0 {
            continue;
        }
        sqlx::query(
            "UPDATE jobs SET state = 'expired', finished_at = ?, updated_at = ? \
             WHERE id = ? AND state = 'awaiting_approval'",
        )
        .bind(now)
        .bind(now)
        .bind(&job_id)
        .execute(&mut *transaction)
        .await?;
        for (event_type, payload) in [
            (
                "approval.expired.v1",
                serde_json::json!({"status": "expired", "job_state": "expired"}),
            ),
            (
                "job.expired.v1",
                serde_json::json!({
                    "previous_state": "awaiting_approval",
                    "state": "expired",
                }),
            ),
        ] {
            events::append(
                &mut transaction,
                PendingEvent {
                    event_type: event_type.into(),
                    actor: Some(actor.clone()),
                    resource_id: Some(resource_id.clone()),
                    job_id: Some(job_id.clone()),
                    approval_id: Some(approval_id.clone()),
                    correlation_id: job_id.clone(),
                    causation_id: None,
                    payload,
                },
            )
            .await?;
        }
        sqlx::query(
            "INSERT INTO audit_log \
             (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, request_id, details, source) \
             VALUES (?, ?, NULL, 'system', 'approval.expire', 'approval', ?, 'expired', ?, NULL, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(now)
        .bind(&approval_id)
        .bind(&job_id)
        .bind(actor.source.as_deref())
        .execute(&mut *transaction)
        .await?;
        count += 1;
    }

    transaction.commit().await?;
    Ok(count)
}

async fn decide(
    pool: &SqlitePool,
    approval_id: &str,
    actor: ActorRef,
    comment: Option<&str>,
    approved: bool,
    observed_external_fingerprint: Option<&str>,
) -> Result<super::contracts::JobSummaryV1> {
    validate_human_actor(&actor)?;
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    let row: DecisionRow = sqlx::query_as(
        "SELECT a.job_id, a.status AS approval_status, a.expires_at, \
                a.resource_revision AS approved_resource_revision, j.state AS job_state, \
                r.revision AS current_resource_revision, j.resource_id, \
                CASE WHEN a.request_digest != j.request_digest \
                    OR a.plan_digest != j.plan_digest \
                    OR a.external_fingerprint != j.external_fingerprint THEN 1 ELSE 0 END \
                    AS immutable_mismatch, a.external_fingerprint \
         FROM approvals a JOIN jobs j ON j.id = a.job_id \
         JOIN resources r ON r.id = j.resource_id WHERE a.id = ?",
    )
    .bind(approval_id)
    .fetch_optional(&mut *transaction)
    .await?
    .context("approval not found")?;
    if row.approval_status != "pending" || row.job_state != "awaiting_approval" {
        bail!("approval is no longer pending");
    }

    let stale = row.expires_at <= now
        || row.approved_resource_revision != row.current_resource_revision
        || row.immutable_mismatch != 0
        || (approved
            && observed_external_fingerprint
                .is_some_and(|value| value != row.external_fingerprint));
    let (approval_status, job_state, event_type) = if stale {
        ("stale", "expired", "approval.stale.v1")
    } else if approved {
        ("approved", "queued", "approval.approved.v1")
    } else {
        ("rejected", "rejected", "approval.rejected.v1")
    };
    sqlx::query(
        "UPDATE approvals SET status = ?, decided_by = ?, decision_comment = ?, \
         decided_at = ?, updated_at = ? WHERE id = ? AND status = 'pending'",
    )
    .bind(approval_status)
    .bind(actor.id.as_deref())
    .bind(comment)
    .bind(now)
    .bind(now)
    .bind(approval_id)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        "UPDATE jobs SET state = ?, queued_at = CASE WHEN ? = 'queued' THEN ? ELSE queued_at END, \
         finished_at = CASE WHEN ? IN ('rejected', 'expired') THEN ? ELSE finished_at END, updated_at = ? \
         WHERE id = ? AND state = 'awaiting_approval'",
    )
    .bind(job_state)
    .bind(job_state)
    .bind(now)
    .bind(job_state)
    .bind(now)
    .bind(now)
    .bind(&row.job_id)
    .execute(&mut *transaction)
    .await?;
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(actor.clone()),
            resource_id: Some(row.resource_id.clone()),
            job_id: Some(row.job_id.clone()),
            approval_id: Some(approval_id.into()),
            correlation_id: row.job_id.clone(),
            causation_id: None,
            payload: serde_json::json!({"status": approval_status, "job_state": job_state}),
        },
    )
    .await?;
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: format!("job.{job_state}.v1"),
            actor: Some(actor.clone()),
            resource_id: Some(row.resource_id.clone()),
            job_id: Some(row.job_id.clone()),
            approval_id: Some(approval_id.into()),
            correlation_id: row.job_id.clone(),
            causation_id: None,
            payload: serde_json::json!({
                "previous_state": "awaiting_approval",
                "state": job_state,
            }),
        },
    )
    .await?;
    sqlx::query(
        "INSERT INTO audit_log \
         (id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, request_id, details, source) \
         VALUES (?, ?, ?, ?, ?, 'approval', ?, ?, ?, NULL, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(now)
    .bind(actor.id.as_deref())
    .bind(actor.actor_type.as_str())
    .bind(if approved {
        "approval.approve"
    } else {
        "approval.reject"
    })
    .bind(approval_id)
    .bind(approval_status)
    .bind(&row.job_id)
    .bind(actor.source.as_deref())
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    jobs::get(pool, &row.job_id)
        .await?
        .context("approved job disappeared")
}

fn validate_human_actor(actor: &ActorRef) -> Result<()> {
    if actor.actor_type != ActorType::Human || actor.id.is_none() {
        bail!("only an authenticated human can decide an approval");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        adapters::{OperationAdapter, ReconcileOutcome, StepOutcome, StepRequest},
        contracts::{JobState, OperationPlanV1, PlannedStepV1},
        jobs::{SubmissionPolicy, SubmitJob},
        resources::{self, ObserveResource},
    };
    use async_trait::async_trait;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::Arc;

    struct FakeFirewallAdapter {
        fingerprint: &'static str,
    }

    #[async_trait]
    impl OperationAdapter for FakeFirewallAdapter {
        fn key(&self) -> &'static str {
            "firewall"
        }

        fn actions(&self) -> &[&'static str] {
            &["firewall.disable"]
        }

        async fn plan(&self, _request: PlanRequest) -> Result<OperationPlanV1> {
            bail!("not used")
        }

        async fn external_fingerprint(&self, _request: &PlanRequest) -> Result<String> {
            Ok(self.fingerprint.into())
        }

        async fn execute_step(&self, _request: StepRequest) -> Result<StepOutcome> {
            bail!("not used")
        }

        async fn reconcile(&self, _request: StepRequest) -> Result<ReconcileOutcome> {
            bail!("not used")
        }
    }

    async fn pending_approval(
        idempotency_key: &str,
        expires_at: i64,
    ) -> (SqlitePool, super::super::contracts::JobSummaryV1) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: "firewall",
                display_name: "firewall",
                node_id: None,
                provider: Some("ufw"),
                namespace: "test",
                scope_key: "local",
                alias: "firewall",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let job = jobs::submit(
            &pool,
            SubmitJob {
                action: "firewall.disable".into(),
                concurrency_key: resource.id.clone(),
                resource,
                actor: ActorRef {
                    actor_type: ActorType::ApiToken,
                    id: Some("token".into()),
                    source: Some("api".into()),
                },
                ingress: "api".into(),
                input: serde_json::json!({}),
                plan: OperationPlanV1 {
                    schema_version: 1,
                    title: "Disable firewall".into(),
                    risk: "high".into(),
                    changes: vec![],
                    preview: None,
                    external_fingerprint: "enabled".into(),
                    steps: vec![PlannedStepV1 {
                        kind: "execute".into(),
                        name: "disable".into(),
                        retry_class: "never".into(),
                        recovery_class: "reconcile".into(),
                    }],
                },
                idempotency_scope: "api:token".into(),
                idempotency_key: idempotency_key.into(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
                policy: SubmissionPolicy::RequireApproval {
                    requirement: "always".into(),
                    reason: "irreversible".into(),
                    expires_at,
                },
            },
        )
        .await
        .unwrap();
        (pool, job)
    }

    #[tokio::test]
    async fn machine_actor_cannot_approve_and_human_approval_queues_job() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        let resource = resources::observe(
            &pool,
            ObserveResource {
                kind: "firewall",
                display_name: "firewall",
                node_id: None,
                provider: Some("ufw"),
                namespace: "test",
                scope_key: "local",
                alias: "firewall",
            },
            None,
            "test",
        )
        .await
        .unwrap();
        let job = jobs::submit(
            &pool,
            SubmitJob {
                action: "firewall.disable".into(),
                concurrency_key: resource.id.clone(),
                resource,
                actor: ActorRef {
                    actor_type: ActorType::ApiToken,
                    id: Some("token".into()),
                    source: Some("api".into()),
                },
                ingress: "api".into(),
                input: serde_json::json!({}),
                plan: OperationPlanV1 {
                    schema_version: 1,
                    title: "Disable firewall".into(),
                    risk: "high".into(),
                    changes: vec![],
                    preview: None,
                    external_fingerprint: "enabled".into(),
                    steps: vec![PlannedStepV1 {
                        kind: "execute".into(),
                        name: "disable".into(),
                        retry_class: "never".into(),
                        recovery_class: "reconcile".into(),
                    }],
                },
                idempotency_scope: "api:token".into(),
                idempotency_key: "one".into(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
                policy: SubmissionPolicy::RequireApproval {
                    requirement: "always".into(),
                    reason: "irreversible".into(),
                    expires_at: unix_now() + 900,
                },
            },
        )
        .await
        .unwrap();
        let approval_id = job.approval_id.unwrap();
        let mut adapters = AdapterRegistry::new();
        adapters
            .register(Arc::new(FakeFirewallAdapter {
                fingerprint: "enabled",
            }))
            .unwrap();
        assert!(approve(
            &pool,
            &adapters,
            &approval_id,
            ActorRef {
                actor_type: ActorType::ApiToken,
                id: Some("token".into()),
                source: None,
            },
            None,
        )
        .await
        .is_err());
        let approved = approve(
            &pool,
            &adapters,
            &approval_id,
            ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner".into()),
                source: Some("web".into()),
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(approved.state, JobState::Queued);
        let audit: (String, String, String) = sqlx::query_as(
            "SELECT action, resource_id, outcome FROM audit_log \
             WHERE request_id = ? AND action = 'approval.approve'",
        )
        .bind(&approved.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            audit,
            ("approval.approve".into(), approval_id, "approved".into())
        );
        let events = events::list_after(&pool, 0, 20).await.unwrap();
        let event_types: Vec<&str> = events
            .iter()
            .filter(|event| event.job_id.as_deref() == Some(&approved.id))
            .map(|event| event.event_type.as_str())
            .collect();
        assert!(event_types.ends_with(&["approval.approved.v1", "job.queued.v1"]));
    }

    #[tokio::test]
    async fn changed_provider_fingerprint_marks_approval_stale() {
        let (pool, job) = pending_approval("stale", unix_now() + 900).await;
        let approval_id = job.approval_id.unwrap();
        let mut adapters = AdapterRegistry::new();
        adapters
            .register(Arc::new(FakeFirewallAdapter {
                fingerprint: "disabled",
            }))
            .unwrap();
        let expired = approve(
            &pool,
            &adapters,
            &approval_id,
            ActorRef {
                actor_type: ActorType::Human,
                id: Some("owner".into()),
                source: Some("web".into()),
            },
            None,
        )
        .await
        .unwrap();
        assert_eq!(expired.state, JobState::Expired);
        assert_eq!(
            get(&pool, &approval_id).await.unwrap().unwrap().status,
            "stale"
        );
        let event_types: Vec<String> =
            sqlx::query_scalar("SELECT event_type FROM events WHERE job_id = ? ORDER BY sequence")
                .bind(&expired.id)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(event_types.ends_with(&["approval.stale.v1".into(), "job.expired.v1".into(),]));
    }

    #[tokio::test]
    async fn expiry_transitions_job_events_and_audit_atomically() {
        let (pool, job) = pending_approval("expire", unix_now() - 1).await;
        let approval_id = job.approval_id.unwrap();
        assert_eq!(expire_pending(&pool, unix_now()).await.unwrap(), 1);
        assert_eq!(
            jobs::get(&pool, &job.id).await.unwrap().unwrap().state,
            JobState::Expired
        );
        assert_eq!(
            get(&pool, &approval_id).await.unwrap().unwrap().status,
            "expired"
        );
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE request_id = ? \
             AND action = 'approval.expire' AND outcome = 'expired'",
        )
        .bind(&job.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_count, 1);
    }
}
