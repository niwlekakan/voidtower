//! Durable local execution for household privacy requests.
//!
//! This worker deliberately has a smaller authority than the canonical operation worker: it can
//! only read bounded local records, write protected local export artifacts, and delete the
//! explicitly documented subject-owned rows. It never contacts providers or deletes canonical
//! resources, users, audit history, or durable request tombstones.

use crate::{
    audit::{self, PendingAudit},
    operations::{
        contracts::{ActorRef, ActorType},
        events::{self, PendingEvent},
    },
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::{os::unix::fs::PermissionsExt, path::Path, sync::OnceLock};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const MAX_EXPORT_BYTES: usize = 256 * 1024;
const MAX_EXPORT_ROWS: i64 = 200;
const MAX_ATTEMPTS: i64 = 3;
const PRIVACY_ROOT: &str = "privacy/exports";
static PRIVACY_EXECUTOR_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkOutcome {
    Idle,
    Completed { request_id: String, state: String },
}

#[derive(Debug, FromRow)]
struct RequestRow {
    id: String,
    household_id: String,
    user_id: String,
    kind: String,
    purpose: String,
    domain: String,
    resource_id: Option<String>,
    destination: String,
    minimization: String,
    correlation_id: String,
    requested_by: String,
    cancel_requested: i64,
    artifact_id: Option<String>,
}

#[derive(Debug, FromRow)]
struct GrantRow {
    resource_id: String,
    permission: String,
    expires_at: Option<i64>,
}

pub async fn run_once(
    pool: &SqlitePool,
    artifact_root: &Path,
    worker_id: &str,
    now: i64,
    lease_seconds: i64,
) -> Result<WorkOutcome> {
    let _executor_guard = PRIVACY_EXECUTOR_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    if worker_id.trim().is_empty() || lease_seconds <= 0 {
        bail!("privacy worker identity and lease must be valid");
    }
    reconcile_orphan_artifacts(pool, artifact_root).await?;
    recover_expired(pool, now).await?;
    let Some(request) = claim_next(pool, worker_id, now, lease_seconds).await? else {
        return Ok(WorkOutcome::Idle);
    };
    execute_claimed(pool, artifact_root, worker_id, now, request).await
}

async fn reconcile_orphan_artifacts(pool: &SqlitePool, artifact_root: &Path) -> Result<()> {
    ensure_private_artifact_directories(artifact_root).await?;
    let exports_dir = artifact_root.join(PRIVACY_ROOT);
    let mut entries = tokio::fs::read_dir(&exports_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let artifact_id = name.strip_suffix(".json.partial");
        let Some(artifact_id) = artifact_id else {
            continue;
        };
        if Uuid::parse_str(artifact_id).is_err() {
            continue;
        }
        let relative_path: Option<String> =
            sqlx::query_scalar("SELECT relative_path FROM privacy_artifacts WHERE id = ?")
                .bind(artifact_id)
                .fetch_optional(pool)
                .await?;
        if let Some(relative_path) = relative_path {
            let final_path = artifact_root.join(relative_path);
            if !tokio::fs::try_exists(&final_path).await? {
                tokio::fs::rename(entry.path(), final_path).await?;
            } else {
                tokio::fs::remove_file(entry.path()).await?;
            }
        } else {
            tokio::fs::remove_file(entry.path()).await?;
        }
    }
    Ok(())
}

async fn recover_expired(pool: &SqlitePool, now: i64) -> Result<()> {
    let exhausted: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM privacy_requests
         WHERE state = 'processing' AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?
           AND attempt_count >= ?
         ORDER BY created_at, id",
    )
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .fetch_all(pool)
    .await?;
    for request_id in exhausted {
        let mut tx = pool.begin().await?;
        let request = request_row(&mut *tx, &request_id).await?;
        let updated = sqlx::query(
            "UPDATE privacy_requests
             SET state = 'failed', failure_code = 'retry_exhausted', completed_at = ?,
                 lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
             WHERE id = ? AND state = 'processing' AND lease_expires_at IS NOT NULL
               AND lease_expires_at <= ? AND attempt_count >= ?",
        )
        .bind(now)
        .bind(now)
        .bind(&request_id)
        .bind(now)
        .bind(MAX_ATTEMPTS)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() == 1 {
            append_transition(
                &mut tx,
                &request,
                now,
                "failed",
                Some("retry_exhausted"),
                None,
            )
            .await?;
            tx.commit().await?;
        } else {
            tx.rollback().await?;
        }
    }
    sqlx::query(
        "UPDATE privacy_requests
         SET state = 'queued', lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
         WHERE state = 'processing' AND lease_expires_at IS NOT NULL AND lease_expires_at <= ?
           AND attempt_count < ?",
    )
    .bind(now)
    .bind(now)
    .bind(MAX_ATTEMPTS)
    .execute(pool)
    .await?;
    Ok(())
}

async fn claim_next(
    pool: &SqlitePool,
    worker_id: &str,
    now: i64,
    lease_seconds: i64,
) -> Result<Option<RequestRow>> {
    let expires = now
        .checked_add(lease_seconds)
        .context("privacy worker lease overflow")?;
    let mut tx = pool.begin().await?;
    let request_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM privacy_requests WHERE state = 'queued' AND attempt_count < ?
         ORDER BY created_at, id LIMIT 1",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(request_id) = request_id else {
        tx.rollback().await?;
        return Ok(None);
    };
    let claimed = sqlx::query(
        "UPDATE privacy_requests
         SET state = 'processing', lease_owner = ?, lease_expires_at = ?,
             attempt_count = attempt_count + 1, updated_at = ?
         WHERE id = ? AND state = 'queued'",
    )
    .bind(worker_id)
    .bind(expires)
    .bind(now)
    .bind(&request_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if claimed != 1 {
        tx.rollback().await?;
        return Ok(None);
    }
    let request = request_row(&mut *tx, &request_id).await?;
    tx.commit().await?;
    Ok(Some(request))
}

async fn request_row<'a, E>(executor: E, request_id: &str) -> Result<RequestRow>
where
    E: sqlx::Executor<'a, Database = Sqlite>,
{
    sqlx::query_as(
        "SELECT id, household_id, user_id, kind, purpose, domain, resource_id,
                destination, minimization, correlation_id, requested_by,
                cancel_requested, artifact_id
         FROM privacy_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_one(executor)
    .await
    .map_err(Into::into)
}

async fn execute_claimed(
    pool: &SqlitePool,
    artifact_root: &Path,
    worker_id: &str,
    now: i64,
    request: RequestRow,
) -> Result<WorkOutcome> {
    if request.cancel_requested != 0 {
        return finish(pool, &request, worker_id, now, "cancelled", None, None).await;
    }
    let expected_purpose = match request.kind.as_str() {
        "export" => Some("privacy_export"),
        "deletion" => Some("privacy_deletion"),
        _ => None,
    };
    if expected_purpose != Some(request.purpose.as_str()) {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("purpose_kind_mismatch"),
            None,
        )
        .await;
    }
    if !authorized(pool, &request, now).await? {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("authorization_revoked"),
            None,
        )
        .await;
    }
    if request.destination != "local" {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("cloud_export_unavailable"),
            None,
        )
        .await;
    }
    match request.kind.as_str() {
        "export" => execute_export(pool, artifact_root, worker_id, now, request).await,
        "deletion" => execute_deletion(pool, artifact_root, worker_id, now, request).await,
        _ => {
            finish(
                pool,
                &request,
                worker_id,
                now,
                "failed",
                Some("unsupported_request_kind"),
                None,
            )
            .await
        }
    }
}

async fn authorized(pool: &SqlitePool, request: &RequestRow, now: i64) -> Result<bool> {
    let target_active: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM household_memberships
         WHERE household_id = ? AND user_id = ? AND status = 'active'
           AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?))",
    )
    .bind(&request.household_id)
    .bind(&request.user_id)
    .bind(now)
    .fetch_one(pool)
    .await?;
    if target_active == 0 {
        return Ok(false);
    }
    let requester_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM household_memberships
         WHERE household_id = ? AND user_id = ? AND status = 'active'
           AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(&request.household_id)
    .bind(&request.requested_by)
    .bind(now)
    .fetch_optional(pool)
    .await?;
    if request.requested_by != request.user_id
        && !requester_role
            .as_deref()
            .is_some_and(|role| matches!(role, "owner" | "admin"))
    {
        return Ok(false);
    }
    if let Some(resource_id) = request.resource_id.as_deref() {
        let target_role: String = sqlx::query_scalar(
            "SELECT role FROM household_memberships
             WHERE household_id = ? AND user_id = ? AND status = 'active'",
        )
        .bind(&request.household_id)
        .bind(&request.user_id)
        .fetch_one(pool)
        .await?;
        if !matches!(target_role.as_str(), "owner" | "admin") {
            let granted: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM resource_grants
                 WHERE household_id = ? AND resource_id = ? AND user_id = ?
                   AND permission IN ('read', 'operate', 'admin') AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > ?))",
            )
            .bind(&request.household_id)
            .bind(resource_id)
            .bind(&request.user_id)
            .bind(now)
            .fetch_one(pool)
            .await?;
            if granted == 0 {
                return Ok(false);
            }
        }
    }
    if request.kind == "export" {
        let consent: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM household_consents
             WHERE household_id = ? AND user_id = ? AND purpose = 'privacy_export'
               AND domain = ? AND status = 'active'
               AND (expires_at IS NULL OR expires_at > ?)
               AND (retention_until IS NULL OR retention_until > ?)
               AND (resource_id IS NULL OR resource_id = ?)
               AND destination = 'local')",
        )
        .bind(&request.household_id)
        .bind(&request.user_id)
        .bind(&request.domain)
        .bind(now)
        .bind(now)
        .bind(request.resource_id.as_deref())
        .fetch_one(pool)
        .await?;
        if consent == 0 {
            return Ok(false);
        }
    }
    Ok(true)
}

async fn authorized_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    request: &RequestRow,
    now: i64,
) -> Result<bool> {
    let target_active: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM household_memberships
         WHERE household_id = ? AND user_id = ? AND status = 'active'
           AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?))",
    )
    .bind(&request.household_id)
    .bind(&request.user_id)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?;
    if target_active == 0 {
        return Ok(false);
    }
    let requester_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM household_memberships
         WHERE household_id = ? AND user_id = ? AND status = 'active'
           AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(&request.household_id)
    .bind(&request.requested_by)
    .bind(now)
    .fetch_optional(&mut **tx)
    .await?;
    if request.requested_by != request.user_id
        && !requester_role
            .as_deref()
            .is_some_and(|role| matches!(role, "owner" | "admin"))
    {
        return Ok(false);
    }
    if let Some(resource_id) = request.resource_id.as_deref() {
        let target_role: String = sqlx::query_scalar(
            "SELECT role FROM household_memberships
             WHERE household_id = ? AND user_id = ? AND status = 'active'",
        )
        .bind(&request.household_id)
        .bind(&request.user_id)
        .fetch_one(&mut **tx)
        .await?;
        if !matches!(target_role.as_str(), "owner" | "admin") {
            let granted: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM resource_grants
                 WHERE household_id = ? AND resource_id = ? AND user_id = ?
                   AND permission IN ('read', 'operate', 'admin') AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > ?))",
            )
            .bind(&request.household_id)
            .bind(resource_id)
            .bind(&request.user_id)
            .bind(now)
            .fetch_one(&mut **tx)
            .await?;
            if granted == 0 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

async fn consent_active_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    request: &RequestRow,
    now: i64,
) -> Result<bool> {
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM household_consents
           WHERE household_id = ? AND user_id = ? AND purpose = ? AND domain = ?
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > ?)
             AND (retention_until IS NULL OR retention_until > ?)
             AND (resource_id IS NULL OR resource_id = ?)
             AND destination = 'local'
         )",
    )
    .bind(&request.household_id)
    .bind(&request.user_id)
    .bind(&request.purpose)
    .bind(&request.domain)
    .bind(now)
    .bind(now)
    .bind(request.resource_id.as_deref())
    .fetch_one(&mut **tx)
    .await?;
    Ok(allowed != 0)
}

async fn execute_export(
    pool: &SqlitePool,
    artifact_root: &Path,
    worker_id: &str,
    now: i64,
    request: RequestRow,
) -> Result<WorkOutcome> {
    if let Some(artifact_id) = request.artifact_id.as_deref() {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "succeeded",
            None,
            Some(artifact_id),
        )
        .await;
    }
    let document = export_document(pool, &request, now).await?;
    let bytes = serde_json::to_vec(&document)?;
    if bytes.len() > MAX_EXPORT_BYTES {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("export_too_large"),
            None,
        )
        .await;
    }
    let artifact_id = Uuid::new_v4().to_string();
    let relative_path = format!("{PRIVACY_ROOT}/{artifact_id}.json");
    let path = artifact_root.join(&relative_path);
    path.parent()
        .context("privacy artifact path has no parent")?;
    ensure_private_artifact_directories(artifact_root).await?;
    let temporary = path.with_extension("json.partial");
    let mut temporary_file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(&temporary)
        .await?;
    temporary_file.write_all(&bytes).await?;
    temporary_file.sync_data().await?;
    if !claim_is_active(pool, &request.id, worker_id, now).await? {
        let outcome = finish_cancelled_if_requested(pool, &request, worker_id, now).await?;
        if matches!(outcome, WorkOutcome::Idle) {
            let _ = tokio::fs::remove_file(&temporary).await;
        }
        return Ok(outcome);
    }
    let digest = hex::encode(Sha256::digest(&bytes));
    let mut tx = pool.begin().await?;
    if !claim_is_active_tx(&mut tx, &request.id, worker_id, now).await? {
        tx.rollback().await?;
        let outcome = finish_cancelled_if_requested(pool, &request, worker_id, now).await?;
        let _ = tokio::fs::remove_file(&temporary).await;
        return Ok(outcome);
    }
    if !consent_active_in_transaction(&mut tx, &request, now).await? {
        tx.rollback().await?;
        let _ = tokio::fs::remove_file(&temporary).await;
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("consent_revoked"),
            None,
        )
        .await;
    }
    if let Err(error) = sqlx::query(
        "INSERT INTO privacy_artifacts
         (id, request_id, relative_path, byte_length, sha256, created_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&artifact_id)
    .bind(&request.id)
    .bind(&relative_path)
    .bind(i64::try_from(bytes.len())?)
    .bind(&digest)
    .bind(now)
    .execute(&mut *tx)
    .await
    {
        tx.rollback().await?;
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    if let Err(error) = finish_in_transaction(
        &mut tx,
        &request,
        worker_id,
        now,
        "succeeded",
        None,
        Some(&artifact_id),
    )
    .await
    {
        tx.rollback().await?;
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error);
    }
    if let Err(error) = tx.commit().await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    tokio::fs::rename(&temporary, &path).await?;
    Ok(WorkOutcome::Completed {
        request_id: request.id,
        state: "succeeded".into(),
    })
}

async fn ensure_private_artifact_directories(artifact_root: &Path) -> Result<()> {
    tokio::fs::create_dir_all(artifact_root).await?;
    let root_metadata = tokio::fs::symlink_metadata(artifact_root).await?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        bail!("privacy artifact root is invalid");
    }
    let privacy_dir = artifact_root.join("privacy");
    tokio::fs::create_dir_all(&privacy_dir).await?;
    let privacy_metadata = tokio::fs::symlink_metadata(&privacy_dir).await?;
    if privacy_metadata.file_type().is_symlink() || !privacy_metadata.is_dir() {
        bail!("privacy artifact directory is invalid");
    }
    let exports_dir = privacy_dir.join("exports");
    tokio::fs::create_dir_all(&exports_dir).await?;
    let exports_metadata = tokio::fs::symlink_metadata(&exports_dir).await?;
    if exports_metadata.file_type().is_symlink() || !exports_metadata.is_dir() {
        bail!("privacy export directory is invalid");
    }
    std::fs::set_permissions(&exports_dir, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

async fn export_document(pool: &SqlitePool, request: &RequestRow, now: i64) -> Result<Value> {
    let grants: Vec<GrantRow> = if request.domain == "inventory" {
        let query = if let Some(resource_id) = request.resource_id.as_deref() {
            sqlx::query_as(
                "SELECT resource_id, permission, expires_at FROM resource_grants
                 WHERE household_id = ? AND user_id = ? AND resource_id = ?
                   AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)
                 ORDER BY resource_id LIMIT ?",
            )
            .bind(&request.household_id)
            .bind(&request.user_id)
            .bind(resource_id)
            .bind(now)
            .bind(MAX_EXPORT_ROWS)
        } else {
            sqlx::query_as(
                "SELECT resource_id, permission, expires_at FROM resource_grants
                 WHERE household_id = ? AND user_id = ?
                   AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)
                 ORDER BY resource_id LIMIT ?",
            )
            .bind(&request.household_id)
            .bind(&request.user_id)
            .bind(now)
            .bind(MAX_EXPORT_ROWS)
        };
        query.fetch_all(pool).await?
    } else {
        Vec::new()
    };
    let username: Option<String> =
        if request.minimization == "records" && request.domain == "identity" {
            sqlx::query_scalar("SELECT username FROM users WHERE id = ?")
                .bind(&request.user_id)
                .fetch_optional(pool)
                .await?
        } else {
            None
        };
    let grant_count = grants.len();
    let grant_values: Vec<Value> = grants
        .into_iter()
        .map(|grant| {
            json!({
                "resource_id": grant.resource_id,
                "permission": grant.permission,
                "expires_at": grant.expires_at,
            })
        })
        .collect();
    let data = if request.minimization == "metadata" {
        json!({ "grant_count": grant_count })
    } else {
        json!({
            "username": username,
            "resource_grants": grant_values,
        })
    };
    let subject = if request.minimization == "metadata" {
        json!({ "present": true })
    } else {
        json!({ "user_id": request.user_id })
    };
    Ok(json!({
        "schema_version": 1,
        "request_id": request.id,
        "domain": request.domain,
        "minimization": request.minimization,
        "subject": subject,
        "data": data,
    }))
}

async fn execute_deletion(
    pool: &SqlitePool,
    artifact_root: &Path,
    worker_id: &str,
    now: i64,
    request: RequestRow,
) -> Result<WorkOutcome> {
    if request.domain == "telemetry" {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("domain_not_supported"),
            None,
        )
        .await;
    }
    if request.resource_id.is_some() && request.domain != "inventory" {
        return finish(
            pool,
            &request,
            worker_id,
            now,
            "failed",
            Some("resource_scope_unsupported"),
            None,
        )
        .await;
    }
    let subject_wide = matches!(request.domain.as_str(), "identity" | "configuration");
    if subject_wide {
        let other_memberships: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM household_memberships
             WHERE user_id = ? AND household_id <> ?",
        )
        .bind(&request.user_id)
        .bind(&request.household_id)
        .fetch_one(pool)
        .await?;
        if other_memberships != 0 {
            return finish(
                pool,
                &request,
                worker_id,
                now,
                "failed",
                Some("multi_household_scope_requires_review"),
                None,
            )
            .await;
        }
    }
    let global_scope = i64::from(subject_wide);
    let mut tx = pool.begin().await?;
    if !authorized_in_transaction(&mut tx, &request, now).await? {
        finish_in_transaction(
            &mut tx,
            &request,
            worker_id,
            now,
            "failed",
            Some("authorization_revoked"),
            None,
        )
        .await?;
        tx.commit().await?;
        return Ok(WorkOutcome::Completed {
            request_id: request.id,
            state: "failed".into(),
        });
    }
    let held: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM privacy_holds
         WHERE user_id = ? AND domain = ?
           AND (? = 1 OR household_id = ?)
           AND (? IS NULL OR resource_id IS NULL OR resource_id = ?))",
    )
    .bind(&request.user_id)
    .bind(&request.domain)
    .bind(global_scope)
    .bind(&request.household_id)
    .bind(request.resource_id.as_deref())
    .bind(request.resource_id.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    if held != 0 {
        finish_in_transaction(
            &mut tx,
            &request,
            worker_id,
            now,
            "failed",
            Some("legal_hold"),
            None,
        )
        .await?;
        tx.commit().await?;
        return Ok(WorkOutcome::Completed {
            request_id: request.id,
            state: "failed".into(),
        });
    }
    let retained: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM household_consents
         WHERE user_id = ? AND domain = ?
           AND (? = 1 OR household_id = ?)
           AND retention_until IS NOT NULL AND retention_until > ?
           AND (? IS NULL OR resource_id IS NULL OR resource_id = ?))",
    )
    .bind(&request.user_id)
    .bind(&request.domain)
    .bind(global_scope)
    .bind(&request.household_id)
    .bind(now)
    .bind(request.resource_id.as_deref())
    .bind(request.resource_id.as_deref())
    .fetch_one(&mut *tx)
    .await?;
    if retained != 0 {
        finish_in_transaction(
            &mut tx,
            &request,
            worker_id,
            now,
            "failed",
            Some("retention_blocked"),
            None,
        )
        .await?;
        tx.commit().await?;
        return Ok(WorkOutcome::Completed {
            request_id: request.id,
            state: "failed".into(),
        });
    }
    if request.cancel_requested != 0
        || !claim_is_active_tx(&mut tx, &request.id, worker_id, now).await?
    {
        if request.cancel_requested != 0
            || sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(SELECT 1 FROM privacy_requests
                 WHERE id = ? AND state = 'processing' AND lease_owner = ?
                   AND cancel_requested = 1 AND lease_expires_at > ?)",
            )
            .bind(&request.id)
            .bind(worker_id)
            .bind(now)
            .fetch_one(&mut *tx)
            .await?
                != 0
        {
            finish_in_transaction(&mut tx, &request, worker_id, now, "cancelled", None, None)
                .await?;
            tx.commit().await?;
            return Ok(WorkOutcome::Completed {
                request_id: request.id,
                state: "cancelled".into(),
            });
        }
        tx.rollback().await?;
        return Ok(WorkOutcome::Idle);
    }
    match request.domain.as_str() {
        "inventory" => {
            if let Some(resource_id) = request.resource_id.as_deref() {
                sqlx::query(
                    "DELETE FROM resource_grants WHERE household_id = ? AND user_id = ? AND resource_id = ?",
                )
                .bind(&request.household_id)
                .bind(&request.user_id)
                .bind(resource_id)
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query("DELETE FROM resource_grants WHERE household_id = ? AND user_id = ?")
                    .bind(&request.household_id)
                    .bind(&request.user_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        "identity" => {
            for statement in [
                "DELETE FROM api_tokens WHERE user_id = ?",
                "DELETE FROM sessions WHERE user_id = ?",
            ] {
                sqlx::query(statement)
                    .bind(&request.user_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        "configuration" => {
            for statement in [
                "DELETE FROM user_nav_config WHERE user_id = ?",
                "DELETE FROM member_settings WHERE user_id = ?",
                "DELETE FROM member_storage WHERE user_id = ?",
                "DELETE FROM member_app_access WHERE user_id = ?",
                "DELETE FROM custom_tabs WHERE user_id = ?",
                "DELETE FROM member_drives WHERE user_id = ?",
            ] {
                sqlx::query(statement)
                    .bind(&request.user_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        _ => unreachable!("request domain is validated at intake"),
    }
    sqlx::query(
        "DELETE FROM household_consents
         WHERE household_id = ? AND user_id = ? AND domain = ?
           AND (? IS NULL OR resource_id = ? OR resource_id IS NULL)",
    )
    .bind(&request.household_id)
    .bind(&request.user_id)
    .bind(&request.domain)
    .bind(request.resource_id.as_deref())
    .bind(request.resource_id.as_deref())
    .execute(&mut *tx)
    .await?;
    remove_subject_artifacts(
        &mut tx,
        artifact_root,
        &request.household_id,
        &request.user_id,
        &request.domain,
        request.resource_id.as_deref(),
    )
    .await?;
    finish_in_transaction(&mut tx, &request, worker_id, now, "succeeded", None, None).await?;
    tx.commit().await?;
    Ok(WorkOutcome::Completed {
        request_id: request.id,
        state: "succeeded".into(),
    })
}

async fn remove_subject_artifacts(
    tx: &mut Transaction<'_, Sqlite>,
    artifact_root: &Path,
    household_id: &str,
    user_id: &str,
    domain: &str,
    resource_id: Option<&str>,
) -> Result<()> {
    let artifacts: Vec<(String, String)> = sqlx::query_as(
        "SELECT artifacts.id, artifacts.relative_path
         FROM privacy_artifacts AS artifacts
         JOIN privacy_requests AS requests ON requests.id = artifacts.request_id
         WHERE requests.household_id = ? AND requests.user_id = ? AND requests.domain = ?
           AND (? IS NULL OR requests.resource_id = ? OR requests.resource_id IS NULL)",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(domain)
    .bind(resource_id)
    .bind(resource_id)
    .fetch_all(&mut **tx)
    .await?;
    for (artifact_id, relative_path) in &artifacts {
        let expected_path = format!("{PRIVACY_ROOT}/{artifact_id}.json");
        if relative_path != &expected_path {
            bail!("privacy artifact path is invalid");
        }
        match tokio::fs::remove_file(artifact_root.join(relative_path)).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    sqlx::query(
        "DELETE FROM privacy_artifacts
         WHERE request_id IN (
             SELECT id FROM privacy_requests
             WHERE household_id = ? AND user_id = ? AND domain = ?
               AND (? IS NULL OR resource_id = ? OR resource_id IS NULL)
         )",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(domain)
    .bind(resource_id)
    .bind(resource_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn claim_is_active(
    pool: &SqlitePool,
    request_id: &str,
    worker_id: &str,
    now: i64,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let active = claim_is_active_tx(&mut tx, request_id, worker_id, now).await?;
    tx.rollback().await?;
    Ok(active)
}

async fn claim_is_active_tx(
    tx: &mut Transaction<'_, Sqlite>,
    request_id: &str,
    worker_id: &str,
    now: i64,
) -> Result<bool> {
    let active: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM privacy_requests
         WHERE id = ? AND state = 'processing' AND lease_owner = ?
           AND cancel_requested = 0 AND lease_expires_at > ?)",
    )
    .bind(request_id)
    .bind(worker_id)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?;
    Ok(active != 0)
}

async fn finish_cancelled_if_requested(
    pool: &SqlitePool,
    request: &RequestRow,
    worker_id: &str,
    now: i64,
) -> Result<WorkOutcome> {
    let mut tx = pool.begin().await?;
    let requested: i64 = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM privacy_requests
         WHERE id = ? AND state = 'processing' AND lease_owner = ?
           AND cancel_requested = 1 AND lease_expires_at > ?)",
    )
    .bind(&request.id)
    .bind(worker_id)
    .bind(now)
    .fetch_one(&mut *tx)
    .await?;
    if requested == 0 {
        tx.rollback().await?;
        return Ok(WorkOutcome::Idle);
    }
    finish_in_transaction(&mut tx, request, worker_id, now, "cancelled", None, None).await?;
    tx.commit().await?;
    Ok(WorkOutcome::Completed {
        request_id: request.id.clone(),
        state: "cancelled".into(),
    })
}

async fn finish(
    pool: &SqlitePool,
    request: &RequestRow,
    worker_id: &str,
    now: i64,
    state: &str,
    failure_code: Option<&str>,
    artifact_id: Option<&str>,
) -> Result<WorkOutcome> {
    let mut tx = pool.begin().await?;
    finish_in_transaction(
        &mut tx,
        request,
        worker_id,
        now,
        state,
        failure_code,
        artifact_id,
    )
    .await?;
    tx.commit().await?;
    Ok(WorkOutcome::Completed {
        request_id: request.id.clone(),
        state: state.into(),
    })
}

async fn append_transition(
    tx: &mut Transaction<'_, Sqlite>,
    request: &RequestRow,
    _now: i64,
    state: &str,
    failure_code: Option<&str>,
    artifact_id: Option<&str>,
) -> Result<()> {
    let event_state = state.to_owned();
    let event_type = format!("privacy.{state}.v1");
    audit::append(
        tx,
        PendingAudit {
            user_id: Some(&request.requested_by),
            actor_type: "system",
            action: &event_type,
            resource_type: Some("household_privacy"),
            resource_id: None,
            outcome: if state == "succeeded" {
                "success"
            } else {
                "failure"
            },
            ip_address: None,
            request_id: Some(&request.correlation_id),
            details: Some("bounded local privacy executor transition"),
            source: Some("privacy_executor"),
        },
    )
    .await?;
    events::append(
        tx,
        PendingEvent {
            event_type,
            actor: Some(ActorRef {
                actor_type: ActorType::System,
                id: None,
                source: Some("privacy_executor".into()),
            }),
            resource_id: None,
            job_id: None,
            approval_id: None,
            correlation_id: request.correlation_id.clone(),
            causation_id: None,
            payload: json!({
                "request_id": request.id,
                "state": event_state,
                "failure_code": failure_code,
                "artifact_id": artifact_id,
            }),
        },
    )
    .await?;
    Ok(())
}

async fn finish_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    request: &RequestRow,
    worker_id: &str,
    now: i64,
    state: &str,
    failure_code: Option<&str>,
    artifact_id: Option<&str>,
) -> Result<()> {
    let completed_at = if state == "processing" {
        None
    } else {
        Some(now)
    };
    let updated = sqlx::query(
        "UPDATE privacy_requests
         SET state = ?, failure_code = ?, completed_at = ?, artifact_id = ?,
             lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
         WHERE id = ? AND state = 'processing' AND lease_owner = ?",
    )
    .bind(state)
    .bind(failure_code)
    .bind(completed_at)
    .bind(artifact_id)
    .bind(now)
    .bind(&request.id)
    .bind(worker_id)
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        bail!("privacy request lease is no longer owned");
    }
    append_transition(tx, request, now, state, failure_code, artifact_id).await
}

pub(crate) async fn run_loop(
    pool: SqlitePool,
    artifact_root: std::path::PathBuf,
    worker_id: String,
    config: crate::config::OperationRuntimeConfig,
    control: crate::operations::runtime::RuntimeControl,
) {
    let lease_seconds = i64::try_from(config.lease_seconds).unwrap_or(30);
    loop {
        if control.is_cancelled() {
            return;
        }
        match run_once(
            &pool,
            &artifact_root,
            &worker_id,
            crate::operations::unix_now(),
            lease_seconds,
        )
        .await
        {
            Ok(WorkOutcome::Completed { request_id, state }) => {
                tracing::info!(
                    event_code = "privacy_request_completed",
                    request_id = %request_id,
                    state = %state
                );
            }
            Ok(WorkOutcome::Idle) => {}
            Err(_) => tracing::warn!(error_code = "privacy_executor_cycle_failed"),
        }
        tokio::select! {
            _ = control.cancelled() => return,
            _ = tokio::time::sleep(config.idle_poll_interval()) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::SqlitePool;
    use std::{fs, path::PathBuf};
    use uuid::Uuid;

    async fn fixture() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        db::run_migrations(&pool).await.unwrap();
        for (id, username, role) in [("u1", "owner", "owner"), ("u2", "member", "member")] {
            sqlx::query(
                "INSERT INTO users (id, username, password_hash, role, created_at, updated_at)
                 VALUES (?, ?, 'not-a-secret-fixture', ?, 0, 0)",
            )
            .bind(id)
            .bind(username)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO households (id, name, created_by, created_at, updated_at)
             VALUES ('h1', 'Home', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for (user_id, role) in [("u1", "owner"), ("u2", "member")] {
            sqlx::query(
                "INSERT INTO household_memberships
                 (household_id, user_id, role, created_at, updated_at)
                 VALUES ('h1', ?, ?, 0, 0)",
            )
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO resources
             (id, kind, display_name, lifecycle_state, revision, created_at, updated_at)
             VALUES ('r1', 'host', 'Home host', 'active', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO resource_grants
             (household_id, resource_id, user_id, permission, granted_by, created_at, updated_at)
             VALUES ('h1', 'r1', 'u2', 'read', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn artifact_root(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("voidtower-privacy-{test_name}-{}", Uuid::new_v4()))
    }

    async fn insert_export(pool: &SqlitePool, request_id: &str) {
        insert_export_for_resource(pool, request_id, "r1").await;
    }

    async fn insert_export_for_resource(pool: &SqlitePool, request_id: &str, resource_id: &str) {
        sqlx::query(
            "INSERT INTO household_consents
             (id, household_id, user_id, purpose, domain, resource_id, destination,
              ai_cloud_export, expires_at, retention_until, created_by, created_at, updated_at)
             VALUES (?, 'h1', 'u2', 'privacy_export', 'inventory', ?, 'local', 0,
                     NULL, NULL, 'u1', 0, 0)",
        )
        .bind(format!("consent-{request_id}"))
        .bind(resource_id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, resource_id, destination,
              minimization, idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES (?, 'h1', 'u2', 'export', 'privacy_export', 'inventory', ?, 'local',
                     'records', ?, ?, 'u1', 0, 0)",
        )
        .bind(request_id)
        .bind(resource_id)
        .bind(format!("key-{request_id}"))
        .bind(format!("corr-{request_id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn export_is_bounded_local_and_redacts_secret_columns() {
        let pool = fixture().await;
        insert_export(&pool, "req-export").await;
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('token-1', 'u2', 'fixture', 'raw-token-hash-must-not-export', '[]', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("export");

        let result = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-export".into(),
                state: "succeeded".into()
            }
        );
        let (artifact_id, relative_path, byte_length): (String, String, i64) = sqlx::query_as(
            "SELECT id, relative_path, byte_length FROM privacy_artifacts WHERE request_id = 'req-export'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!artifact_id.is_empty());
        assert!(relative_path.starts_with("privacy/exports/"));
        let artifact = fs::read(root.join(relative_path)).unwrap();
        assert_eq!(artifact.len() as i64, byte_length);
        let text = String::from_utf8(artifact).unwrap();
        assert!(text.contains("inventory"));
        assert!(!text.contains("raw-token-hash-must-not-export"));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM privacy_requests WHERE id = 'req-export'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "succeeded"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn operate_grant_allows_scoped_export_execution() {
        let pool = fixture().await;
        sqlx::query(
            "UPDATE resource_grants SET permission = 'operate'
             WHERE household_id = 'h1' AND resource_id = 'r1' AND user_id = 'u2'",
        )
        .execute(&pool)
        .await
        .unwrap();
        insert_export(&pool, "req-operate-export").await;
        let root = artifact_root("operate-grant");

        let result = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-operate-export".into(),
                state: "succeeded".into()
            }
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn metadata_export_omits_subject_and_resource_values() {
        let pool = fixture().await;
        insert_export(&pool, "req-metadata-export").await;
        sqlx::query(
            "UPDATE privacy_requests SET minimization = 'metadata' WHERE id = 'req-metadata-export'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("metadata-export");

        run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        let relative_path: String = sqlx::query_scalar(
            "SELECT relative_path FROM privacy_artifacts WHERE request_id = 'req-metadata-export'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let text = fs::read_to_string(root.join(relative_path)).unwrap();
        assert!(text.contains("metadata"));
        assert!(!text.contains("u2"));
        assert!(!text.contains("r1"));
        assert!(!text.contains("resource_grants"));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn unscoped_export_omits_expired_and_revoked_grants() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO resources
             (id, kind, display_name, lifecycle_state, revision, created_at, updated_at)
             VALUES ('r2', 'service', 'Expired resource', 'active', 0, 0, 0),
                    ('r3', 'service', 'Revoked resource', 'active', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO resource_grants
             (household_id, resource_id, user_id, permission, granted_by, expires_at, created_at, updated_at)
             VALUES ('h1', 'r2', 'u2', 'read', 'u1', 50, 0, 0),
                    ('h1', 'r3', 'u2', 'read', 'u1', NULL, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE resource_grants SET revoked_at = 10 WHERE resource_id = 'r3'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO household_consents
             (id, household_id, user_id, purpose, domain, destination, created_by, created_at, updated_at)
             VALUES ('consent-unscoped-export', 'h1', 'u2', 'privacy_export', 'inventory',
                     'local', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-unscoped-export', 'h1', 'u2', 'export', 'privacy_export', 'inventory',
                     'local', 'records', 'key-unscoped-export', 'corr-unscoped-export', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("stale-grants");

        run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        let relative_path: String = sqlx::query_scalar(
            "SELECT relative_path FROM privacy_artifacts WHERE request_id = 'req-unscoped-export'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let text = fs::read_to_string(root.join(relative_path)).unwrap();
        assert!(text.contains("r1"));
        assert!(!text.contains("r2"));
        assert!(!text.contains("r3"));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn export_rejects_a_symlinked_privacy_directory() {
        let pool = fixture().await;
        insert_export(&pool, "req-symlink-directory").await;
        let root = artifact_root("symlink-root");
        let target = artifact_root("symlink-target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&target).unwrap();
        std::os::unix::fs::symlink(&target, root.join("privacy")).unwrap();

        let error = run_once(&pool, &root, "worker-1", 100, 30)
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("privacy artifact directory is invalid"));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(target);
    }

    #[tokio::test]
    async fn successful_deletion_removes_subject_export_artifact_and_record() {
        let pool = fixture().await;
        insert_export(&pool, "req-export-before-deletion").await;
        let root = artifact_root("deletion-artifact-cleanup");
        run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        let relative_path: String = sqlx::query_scalar(
            "SELECT relative_path FROM privacy_artifacts
             WHERE request_id = 'req-export-before-deletion'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(root.join(&relative_path).exists());

        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, state, created_at, updated_at,
              completed_at)
             VALUES ('req-configuration-export', 'h1', 'u2', 'export', 'privacy_export',
                     'configuration', 'local', 'metadata', 'key-configuration-export',
                     'corr-configuration-export', 'u1', 'succeeded', 101, 101, 101)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_artifacts
             (id, request_id, relative_path, byte_length, sha256, created_at)
             VALUES ('artifact-configuration-export', 'req-configuration-export',
                     'privacy/exports/artifact-configuration-export.json', 0, ?, 101)",
        )
        .bind(hex::encode(Sha256::digest([])))
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-deletion-after-export', 'h1', 'u2', 'deletion', 'privacy_deletion',
                     'inventory', 'local', 'records', 'key-delete-export', 'corr-delete-export',
                     'u1', 101, 101)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = run_once(&pool, &root, "worker-1", 101, 30).await.unwrap();
        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-deletion-after-export".into(),
                state: "succeeded".into()
            }
        );
        assert!(!root.join(&relative_path).exists());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_artifacts
                 WHERE request_id = 'req-export-before-deletion'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_artifacts
                 WHERE request_id = 'req-configuration-export'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn deletion_honors_legal_hold_and_successfully_removes_scoped_configuration() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-held', 'h1', 'u2', 'deletion', 'privacy_deletion', 'configuration', 'local',
                     'records', 'key-held', 'corr-held', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_holds
             (id, household_id, user_id, domain, reason_code, created_at)
             VALUES ('hold-1', 'h1', 'u2', 'configuration', 'legal_review', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("hold");

        let held = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();
        assert_eq!(
            held,
            WorkOutcome::Completed {
                request_id: "req-held".into(),
                state: "failed".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-held'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "legal_hold"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn identity_deletion_removes_user_owned_authentication_rows() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('token-identity-delete', 'u2', 'fixture', 'hash', '[]', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, expires_at, created_at)
             VALUES ('session-identity-delete', 'u2', 1000, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-identity-delete', 'h1', 'u2', 'deletion', 'privacy_deletion',
                     'identity', 'local', 'records', 'key-identity-delete', 'corr-identity-delete',
                     'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("identity-delete");

        let result = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-identity-delete".into(),
                state: "succeeded".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM api_tokens WHERE id = 'token-identity-delete'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sessions WHERE id = 'session-identity-delete'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn identity_deletion_fails_closed_for_a_subject_in_multiple_households() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO households (id, name, created_by, created_at, updated_at)
             VALUES ('h2', 'Second home', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO household_memberships
             (household_id, user_id, role, created_at, updated_at)
             VALUES ('h2', 'u2', 'member', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('token-multi-household', 'u2', 'fixture', 'hash', '[]', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-multi-household', 'h1', 'u2', 'deletion', 'privacy_deletion',
                     'identity', 'local', 'records', 'key-multi-household', 'corr-multi-household',
                     'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("multi-household");

        let result = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-multi-household".into(),
                state: "failed".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-multi-household'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "multi_household_scope_requires_review"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM api_tokens WHERE id = 'token-multi-household'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn identity_deletion_fails_closed_for_historical_membership() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO households (id, name, created_by, created_at, updated_at)
             VALUES ('h2', 'Second home', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO household_memberships
             (household_id, user_id, role, status, revoked_at, created_at, updated_at)
             VALUES ('h2', 'u2', 'member', 'revoked', 10, 0, 10)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('token-historical-household', 'u2', 'fixture', 'hash', '[]', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-historical-household', 'h1', 'u2', 'deletion', 'privacy_deletion',
                     'identity', 'local', 'records', 'key-historical-household',
                     'corr-historical-household', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("historical-household");

        run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-historical-household'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "multi_household_scope_requires_review"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM api_tokens WHERE id = 'token-historical-household'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn identity_deletion_honors_a_hold_in_another_household() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO households (id, name, created_by, created_at, updated_at)
             VALUES ('h2', 'Second home', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_holds
             (id, household_id, user_id, domain, reason_code, created_at)
             VALUES ('hold-other-household', 'h2', 'u2', 'identity', 'legal_review', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('token-other-household-hold', 'u2', 'fixture', 'hash', '[]', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-other-household-hold', 'h1', 'u2', 'deletion', 'privacy_deletion',
                     'identity', 'local', 'records', 'key-other-household-hold',
                     'corr-other-household-hold', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("other-household-hold");

        run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-other-household-hold'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "legal_hold"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM api_tokens WHERE id = 'token-other-household-hold'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn malformed_durable_request_fails_before_deletion() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, scopes, created_at)
             VALUES ('token-malformed-request', 'u2', 'fixture', 'hash', '[]', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-malformed-request', 'h1', 'u2', 'export', 'privacy_deletion',
                     'identity', 'local', 'records', 'key-malformed-request', 'corr-malformed-request',
                     'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("malformed-request");

        let result = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();

        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-malformed-request".into(),
                state: "failed".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-malformed-request'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "purpose_kind_mismatch"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM api_tokens WHERE id = 'token-malformed-request'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn broad_deletion_is_blocked_by_scoped_hold_and_retention() {
        let pool = fixture().await;
        for (request_id, correlation_id) in [
            ("req-held-scoped", "corr-held-scoped"),
            ("req-retained-scoped", "corr-retained-scoped"),
        ] {
            sqlx::query(
                "INSERT INTO privacy_requests
                 (id, household_id, user_id, kind, purpose, domain, destination, minimization,
                  idempotency_key, correlation_id, requested_by, created_at, updated_at)
                 VALUES (?, 'h1', 'u2', 'deletion', 'privacy_deletion', 'configuration', 'local',
                         'records', ?, ?, 'u1', 0, 0)",
            )
            .bind(request_id)
            .bind(format!("key-{request_id}"))
            .bind(correlation_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO privacy_holds
             (id, household_id, user_id, domain, resource_id, reason_code, created_at)
             VALUES ('hold-scoped', 'h1', 'u2', 'configuration', 'r1', 'legal_review', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO household_consents
             (id, household_id, user_id, purpose, domain, resource_id, destination,
              ai_cloud_export, expires_at, retention_until, created_by, created_at, updated_at)
             VALUES ('retention-scoped', 'h1', 'u2', 'support', 'configuration', 'r1', 'local',
                     0, NULL, 200, 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("scoped-retention");

        for request_id in ["req-held-scoped", "req-retained-scoped"] {
            let result = run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();
            assert_eq!(
                result,
                WorkOutcome::Completed {
                    request_id: request_id.into(),
                    state: "failed".into()
                }
            );
            if request_id == "req-held-scoped" {
                sqlx::query("DELETE FROM privacy_holds WHERE id = 'hold-scoped'")
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-held-scoped'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "legal_hold"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT failure_code FROM privacy_requests WHERE id = 'req-retained-scoped'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "retention_blocked"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn resource_scoped_deletion_preserves_other_resource_state() {
        let pool = fixture().await;
        sqlx::query(
            "INSERT INTO resources
             (id, kind, display_name, lifecycle_state, revision, created_at, updated_at)
             VALUES ('r2', 'service', 'Second resource', 'active', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO resource_grants
             (household_id, resource_id, user_id, permission, granted_by, created_at, updated_at)
             VALUES ('h1', 'r2', 'u2', 'read', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        insert_export_for_resource(&pool, "req-export-r1", "r1").await;
        insert_export_for_resource(&pool, "req-export-r2", "r2").await;
        sqlx::query(
            "INSERT INTO household_consents
             (id, household_id, user_id, purpose, domain, destination, created_by, created_at, updated_at)
             VALUES ('consent-export-unscoped', 'h1', 'u2', 'privacy_export', 'inventory',
                     'local', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-export-unscoped', 'h1', 'u2', 'export', 'privacy_export', 'inventory',
                     'local', 'records', 'key-export-unscoped', 'corr-export-unscoped', 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("scoped-deletion");
        run_once(&pool, &root, "worker-1", 100, 30).await.unwrap();
        run_once(&pool, &root, "worker-1", 101, 30).await.unwrap();
        run_once(&pool, &root, "worker-1", 102, 30).await.unwrap();
        let r2_path: String = sqlx::query_scalar(
            "SELECT relative_path FROM privacy_artifacts WHERE request_id = 'req-export-r2'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO household_consents
             (id, household_id, user_id, purpose, domain, resource_id, destination,
              ai_cloud_export, created_by, created_at, updated_at)
             VALUES ('consent-extra-r1', 'h1', 'u2', 'support', 'inventory', 'r1', 'local',
                     0, 'u1', 0, 0),
                    ('consent-extra-r2', 'h1', 'u2', 'support', 'inventory', 'r2', 'local',
                     0, 'u1', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, resource_id, destination,
              minimization, idempotency_key, correlation_id, requested_by, created_at, updated_at)
             VALUES ('req-delete-r1', 'h1', 'u2', 'deletion', 'privacy_deletion', 'inventory',
                     'r1', 'local', 'records', 'key-delete-r1', 'corr-delete-r1', 'u1', 102, 102)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = run_once(&pool, &root, "worker-1", 102, 30).await.unwrap();
        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-delete-r1".into(),
                state: "succeeded".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM resource_grants WHERE household_id = 'h1' AND user_id = 'u2' AND resource_id = 'r1'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM resource_grants WHERE household_id = 'h1' AND user_id = 'u2' AND resource_id = 'r2'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM household_consents WHERE user_id = 'u2' AND resource_id = 'r1'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM household_consents WHERE user_id = 'u2' AND resource_id = 'r2'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_artifacts WHERE request_id = 'req-export-r1'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_artifacts WHERE request_id = 'req-export-unscoped'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_artifacts WHERE request_id = 'req-export-r2'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        assert!(root.join(r2_path).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn expired_processing_is_recovered_without_duplicate_claim() {
        let pool = fixture().await;
        insert_export(&pool, "req-recover").await;
        sqlx::query(
            "UPDATE privacy_requests
             SET state = 'processing', lease_owner = 'dead-worker', lease_expires_at = 10,
                 attempt_count = 1
             WHERE id = 'req-recover'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("recovery");

        let result = run_once(&pool, &root, "worker-2", 100, 30).await.unwrap();
        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-recover".into(),
                state: "succeeded".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT attempt_count FROM privacy_requests WHERE id = 'req-recover'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM privacy_artifacts WHERE request_id = 'req-recover'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn expired_processing_at_retry_limit_fails_closed() {
        let pool = fixture().await;
        insert_export(&pool, "req-retry-exhausted").await;
        sqlx::query(
            "UPDATE privacy_requests
             SET state = 'processing', lease_owner = 'dead-worker', lease_expires_at = 10,
                 attempt_count = 3
             WHERE id = 'req-retry-exhausted'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("retry-exhausted");

        assert_eq!(
            run_once(&pool, &root, "worker-2", 100, 30).await.unwrap(),
            WorkOutcome::Idle
        );
        let state: (String, String) = sqlx::query_as(
            "SELECT state, failure_code FROM privacy_requests WHERE id = 'req-retry-exhausted'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(state, ("failed".into(), "retry_exhausted".into()));
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn orphan_artifacts_are_reconciled_before_work() {
        let pool = fixture().await;
        let root = artifact_root("orphan-reconciliation");
        let exports = root.join("privacy/exports");
        fs::create_dir_all(&exports).unwrap();
        let orphan_id = Uuid::new_v4();
        fs::write(exports.join(format!("{orphan_id}.json")), b"orphan").unwrap();
        fs::write(
            exports.join(format!("{orphan_id}.json.partial")),
            b"partial",
        )
        .unwrap();
        let tracked_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO privacy_requests
             (id, household_id, user_id, kind, purpose, domain, destination, minimization,
              idempotency_key, correlation_id, requested_by, state, created_at, updated_at)
             VALUES ('req-artifact-repair', 'h1', 'u2', 'export', 'privacy_export',
                     'inventory', 'local', 'records', 'key-artifact-repair', 'corr-artifact-repair',
                     'u1', 'succeeded', 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO privacy_artifacts
             (id, request_id, relative_path, byte_length, sha256, created_at)
             VALUES (?, 'req-artifact-repair', ?, 7, '0000000000000000000000000000000000000000000000000000000000000000', 0)",
        )
        .bind(tracked_id.to_string())
        .bind(format!("{PRIVACY_ROOT}/{tracked_id}.json"))
        .execute(&pool)
        .await
        .unwrap();
        fs::write(
            exports.join(format!("{tracked_id}.json.partial")),
            b"tracked",
        )
        .unwrap();

        assert_eq!(
            run_once(&pool, &root, "worker-1", 100, 30).await.unwrap(),
            WorkOutcome::Idle
        );
        assert!(exports.join(format!("{orphan_id}.json")).exists());
        assert!(!exports.join(format!("{orphan_id}.json.partial")).exists());
        assert!(exports.join(format!("{tracked_id}.json")).exists());
        assert!(!exports.join(format!("{tracked_id}.json.partial")).exists());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cancellation_requested_during_processing_finishes_as_cancelled_after_recovery() {
        let pool = fixture().await;
        insert_export(&pool, "req-cancel").await;
        sqlx::query(
            "UPDATE privacy_requests
             SET state = 'processing', lease_owner = 'dead-worker', lease_expires_at = 10,
                 cancel_requested = 1, attempt_count = 1
             WHERE id = 'req-cancel'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = artifact_root("cancel");

        let result = run_once(&pool, &root, "worker-2", 100, 30).await.unwrap();

        assert_eq!(
            result,
            WorkOutcome::Completed {
                request_id: "req-cancel".into(),
                state: "cancelled".into()
            }
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM privacy_requests WHERE id = 'req-cancel'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            "cancelled"
        );
        let _ = fs::remove_dir_all(root);
    }
}
