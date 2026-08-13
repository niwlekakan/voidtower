use super::{
    contracts::{ActorRef, ActorType, ApprovalViewV1},
    events::{self, PendingEvent},
    jobs, unix_now,
};
use anyhow::{bail, Context, Result};
use sqlx::SqlitePool;

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
    approval_id: &str,
    actor: ActorRef,
    comment: Option<&str>,
) -> Result<super::contracts::JobSummaryV1> {
    decide(pool, approval_id, actor, comment, true).await
}

pub async fn reject(
    pool: &SqlitePool,
    approval_id: &str,
    actor: ActorRef,
    comment: Option<&str>,
) -> Result<super::contracts::JobSummaryV1> {
    decide(pool, approval_id, actor, comment, false).await
}

async fn decide(
    pool: &SqlitePool,
    approval_id: &str,
    actor: ActorRef,
    comment: Option<&str>,
    approved: bool,
) -> Result<super::contracts::JobSummaryV1> {
    if actor.actor_type != ActorType::Human || actor.id.is_none() {
        bail!("only an authenticated human can decide an approval");
    }
    let now = unix_now();
    let mut transaction = pool.begin().await?;
    let row: (String, String, i64, i64, String, i64, String, i64) = sqlx::query_as(
        "SELECT a.job_id, a.status, a.expires_at, a.resource_revision, j.state, r.revision, \
                j.resource_id, CASE WHEN a.request_digest != j.request_digest \
                    OR a.plan_digest != j.plan_digest \
                    OR a.external_fingerprint != j.external_fingerprint THEN 1 ELSE 0 END \
         FROM approvals a JOIN jobs j ON j.id = a.job_id \
         JOIN resources r ON r.id = j.resource_id WHERE a.id = ?",
    )
    .bind(approval_id)
    .fetch_optional(&mut *transaction)
    .await?
    .context("approval not found")?;
    if row.1 != "pending" || row.4 != "awaiting_approval" {
        bail!("approval is no longer pending");
    }

    let stale = row.2 <= now || row.3 != row.5 || row.7 != 0;
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
    .bind(&row.0)
    .execute(&mut *transaction)
    .await?;
    events::append(
        &mut transaction,
        PendingEvent {
            event_type: event_type.into(),
            actor: Some(actor),
            resource_id: Some(row.6),
            job_id: Some(row.0.clone()),
            approval_id: Some(approval_id.into()),
            correlation_id: row.0.clone(),
            causation_id: None,
            payload: serde_json::json!({"status": approval_status, "job_state": job_state}),
        },
    )
    .await?;
    transaction.commit().await?;
    jobs::get(pool, &row.0)
        .await?
        .context("approved job disappeared")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::{
        contracts::{JobState, OperationPlanV1, PlannedStepV1},
        jobs::{SubmissionPolicy, SubmitJob},
        resources::{self, ObserveResource},
    };
    use sqlx::sqlite::SqlitePoolOptions;

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
        assert!(approve(
            &pool,
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
    }
}
