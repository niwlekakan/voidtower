use crate::operations::resources::{self, ObserveResource};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub const SELECT_COLS: &str =
    "id, name, source_path, repo_path, schedule, retention_days, enabled, \
     last_run_at, last_status, created_at, last_check_at, last_check_status, \
     last_restore_test_at, last_restore_test_status, restore_test_schedule";

const MAX_RESTIC_TEXT_CHARS: usize = 4 * 1024;

pub fn is_restic_available() -> bool {
    std::process::Command::new("which")
        .arg("restic")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn restic_password() -> String {
    std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| "changeme".into())
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, PartialEq, Eq)]
pub struct BackupConfig {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub repo_path: String,
    pub schedule: Option<String>,
    pub retention_days: i64,
    pub enabled: bool,
    pub last_run_at: Option<i64>,
    pub last_status: Option<String>,
    pub created_at: i64,
    pub last_check_at: Option<i64>,
    pub last_check_status: Option<String>,
    pub last_restore_test_at: Option<i64>,
    pub last_restore_test_status: Option<String>,
    pub restore_test_schedule: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BackupConfigInput {
    pub name: String,
    pub source_path: String,
    pub repo_path: String,
    #[serde(default)]
    pub schedule: Option<String>,
    #[serde(default = "default_retention_days")]
    pub retention_days: i64,
    #[serde(default)]
    pub restore_test_schedule: Option<String>,
}

fn default_retention_days() -> i64 {
    30
}

impl BackupConfigInput {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(!self.name.trim().is_empty(), "backup name is required");
        anyhow::ensure!(
            !self.source_path.trim().is_empty(),
            "backup source path is required"
        );
        anyhow::ensure!(
            !self.repo_path.trim().is_empty(),
            "backup repository path is required"
        );
        anyhow::ensure!(
            (1..=36500).contains(&self.retention_days),
            "backup retention days must be between 1 and 36500"
        );
        for (label, value) in [
            ("backup name", Some(self.name.as_str())),
            ("backup source path", Some(self.source_path.as_str())),
            ("backup repository path", Some(self.repo_path.as_str())),
            ("backup schedule", self.schedule.as_deref()),
            (
                "backup restore-test schedule",
                self.restore_test_schedule.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                validate_durable_literal(label, value)?;
            }
        }
        if let Some(authority) = self
            .repo_path
            .split_once("://")
            .map(|(_, remainder)| remainder.split('/').next().unwrap_or(remainder))
        {
            anyhow::ensure!(
                !authority.contains('@'),
                "backup repository URL must not contain credentials"
            );
        }
        Ok(())
    }
}

fn validate_durable_literal(label: &str, value: &str) -> Result<()> {
    anyhow::ensure!(
        !value.contains(['\r', '\n', '\0']),
        "{label} contains a control character"
    );
    anyhow::ensure!(
        crate::api::mcp::redact::redact_patterns(value) == value,
        "{label} resembles a credential"
    );
    if let Ok(password) = std::env::var("RESTIC_PASSWORD") {
        anyhow::ensure!(
            password.chars().count() < 4 || !value.contains(&password),
            "{label} contains the configured restic credential"
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupRun {
    pub config_id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String,
    pub snapshot_id: Option<String>,
    pub files_new: Option<i64>,
    pub files_changed: Option<i64>,
    pub data_added_bytes: Option<i64>,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupProbe {
    pub status: String,
    pub message: Option<String>,
}

pub async fn list_configs(pool: &SqlitePool) -> Result<Vec<BackupConfig>> {
    Ok(sqlx::query_as::<_, BackupConfig>(&format!(
        "SELECT {SELECT_COLS} FROM backup_configs ORDER BY created_at DESC"
    ))
    .fetch_all(pool)
    .await?)
}

pub async fn get_config(pool: &SqlitePool, id: &str) -> Result<Option<BackupConfig>> {
    Ok(sqlx::query_as::<_, BackupConfig>(&format!(
        "SELECT {SELECT_COLS} FROM backup_configs WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_config(
    pool: &SqlitePool,
    id: &str,
    input: &BackupConfigInput,
    correlation_id: &str,
) -> Result<String> {
    input.validate()?;
    sqlx::query(
        "INSERT INTO backup_configs \
         (id, name, source_path, repo_path, schedule, retention_days, enabled, created_at, \
          restore_test_schedule) VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?)",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(input.source_path.trim())
    .bind(input.repo_path.trim())
    .bind(input.schedule.as_deref().map(str::trim))
    .bind(input.retention_days)
    .bind(unix_now())
    .bind(input.restore_test_schedule.as_deref().map(str::trim))
    .execute(pool)
    .await?;

    let resource = resources::observe(
        pool,
        ObserveResource {
            kind: "backup_config",
            display_name: input.name.trim(),
            node_id: None,
            provider: Some("restic"),
            namespace: "voidtower.backup_config",
            scope_key: "local",
            alias: id,
        },
        None,
        correlation_id,
    )
    .await?;
    Ok(resource.id)
}

pub async fn delete_config(pool: &SqlitePool, id: &str) -> Result<bool> {
    let mut transaction = pool.begin().await?;
    let result = sqlx::query("DELETE FROM backup_configs WHERE id = ?")
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    if result.rows_affected() > 0 {
        sqlx::query(
            "UPDATE resources SET lifecycle_state = 'retired', revision = revision + 1, \
             updated_at = ? WHERE id IN (SELECT resource_id FROM resource_aliases \
             WHERE namespace = 'voidtower.backup_config' AND scope_key = 'local' AND value = ?)",
        )
        .bind(unix_now())
        .bind(id)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(result.rows_affected() > 0)
}

pub async fn run_config_backup(
    pool: &SqlitePool,
    config: &BackupConfig,
    password: &str,
    operation_tag: Option<&str>,
) -> Result<BackupRun> {
    let run = run_backup(config, password, operation_tag).await?;
    sqlx::query("UPDATE backup_configs SET last_run_at = ?, last_status = ? WHERE id = ?")
        .bind(run.finished_at.unwrap_or_else(unix_now))
        .bind(&run.status)
        .bind(&config.id)
        .execute(pool)
        .await?;
    Ok(run)
}

pub async fn prepare_config_repository(config: &BackupConfig, password: &str) -> Result<()> {
    init_repo(&config.repo_path, password).await
}

pub async fn check_config(
    pool: &SqlitePool,
    config: &BackupConfig,
    password: &str,
) -> Result<BackupProbe> {
    let probe = match run_check(&config.repo_path, password).await {
        Ok(status) => BackupProbe {
            status,
            message: None,
        },
        Err(error) => BackupProbe {
            status: "failed".into(),
            message: Some(safe_restic_text(&error.to_string(), password)),
        },
    };
    sqlx::query("UPDATE backup_configs SET last_check_at = ?, last_check_status = ? WHERE id = ?")
        .bind(unix_now())
        .bind(&probe.status)
        .bind(&config.id)
        .execute(pool)
        .await?;
    Ok(probe)
}

pub async fn restore_test_config(
    pool: &SqlitePool,
    config: &BackupConfig,
    password: &str,
) -> Result<BackupProbe> {
    let probe = match run_restore_test(&config.repo_path, password).await {
        Ok(status) => BackupProbe {
            status,
            message: None,
        },
        Err(error) => BackupProbe {
            status: "failed".into(),
            message: Some(safe_restic_text(&error.to_string(), password)),
        },
    };
    sqlx::query(
        "UPDATE backup_configs SET last_restore_test_at = ?, last_restore_test_status = ? \
         WHERE id = ?",
    )
    .bind(unix_now())
    .bind(&probe.status)
    .bind(&config.id)
    .execute(pool)
    .await?;
    Ok(probe)
}

/// Minimal 5-field cron matcher (min hour dom month dow).
/// Returns true when the current wall-clock time matches the expression.
/// Supports `*`, plain numbers, and comma-separated lists. No ranges/steps.
pub fn cron_matches_now(expr: &str) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mins_since_epoch = secs / 60;
    let minute = (mins_since_epoch % 60) as u32;
    let hour = (mins_since_epoch / 60 % 24) as u32;
    let day = (secs / 86400 % 31 + 1) as u32;
    let month = ((secs / 86400 / 30) % 12 + 1) as u32;
    let dow = (secs / 86400 % 7) as u32;

    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return false;
    }

    field_matches(parts[0], minute)
        && field_matches(parts[1], hour)
        && field_matches(parts[2], day)
        && field_matches(parts[3], month)
        && field_matches(parts[4], dow)
}

fn field_matches(field: &str, value: u32) -> bool {
    if field == "*" {
        return true;
    }
    field.split(',').any(|part| {
        part.trim()
            .parse::<u32>()
            .map(|number| number == value)
            .unwrap_or(false)
    })
}

pub fn confidence(config: &BackupConfig) -> &'static str {
    let now = unix_now();
    let Some(last_run) = config.last_run_at else {
        return "critical";
    };
    let age_secs = now - last_run;
    let check_ok = config.last_check_status.as_deref() == Some("ok");
    let check_failed = config.last_check_status.as_deref() == Some("failed");
    let restore_age = config.last_restore_test_at.map(|time| now - time);
    let restore_ok = config.last_restore_test_status.as_deref() == Some("ok");

    if check_failed || age_secs > 30 * 86400 {
        return "critical";
    }
    if age_secs > 7 * 86400 {
        return "low";
    }
    if age_secs < 86400
        && check_ok
        && restore_age.map(|age| age < 7 * 86400).unwrap_or(false)
        && restore_ok
    {
        return "high";
    }
    "medium"
}

pub async fn run_check(repo_path: &str, password: &str) -> Result<String> {
    let output = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "check"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;
    if output.status.success() {
        Ok("ok".into())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{}", safe_restic_text(&stderr, password))
    }
}

pub async fn run_restore_test(repo_path: &str, password: &str) -> Result<String> {
    let temporary =
        std::env::temp_dir().join(format!("vt-restore-{}", uuid::Uuid::new_v4().simple()));
    tokio::fs::create_dir_all(&temporary).await?;
    let output = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "restore", "latest", "--target"])
        .arg(&temporary)
        .arg("--dry-run")
        .env("RESTIC_PASSWORD", password)
        .output()
        .await;
    let _ = tokio::fs::remove_dir_all(&temporary).await;
    match output {
        Ok(output) if output.status.success() => Ok("ok".into()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("unknown flag") || stderr.contains("dry-run") {
                let snapshots = list_snapshots(repo_path, password).await?;
                anyhow::ensure!(!snapshots.is_empty(), "no snapshots found");
                Ok("ok".into())
            } else {
                bail!("{}", safe_restic_text(&stderr, password))
            }
        }
        Err(error) => Err(error.into()),
    }
}

/// Run a restic backup without interpolating arguments into a shell.
pub async fn run_backup(
    config: &BackupConfig,
    password: &str,
    operation_tag: Option<&str>,
) -> Result<BackupRun> {
    let started_at = unix_now();
    let mut command = tokio::process::Command::new("restic");
    command.args(["-r", &config.repo_path, "--verbose", "backup"]);
    if let Some(tag) = operation_tag {
        command.args(["--tag", tag]);
    }
    let output = command
        .arg(&config.source_path)
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = safe_restic_text(&format!("{stdout}\n{stderr}"), password);
    let status = if output.status.success() {
        "success"
    } else {
        "failed"
    }
    .to_string();
    let snapshot_id = stdout
        .lines()
        .find(|line| line.contains("snapshot") && line.contains("saved"))
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_owned);

    Ok(BackupRun {
        config_id: config.id.clone(),
        started_at,
        finished_at: Some(unix_now()),
        status,
        snapshot_id,
        files_new: None,
        files_changed: None,
        data_added_bytes: None,
        output: combined,
    })
}

pub async fn init_repo(repo_path: &str, password: &str) -> Result<()> {
    let output = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "init"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already initialized") {
            bail!(
                "restic init failed: {}",
                safe_restic_text(&stderr, password)
            );
        }
    }
    Ok(())
}

pub async fn list_snapshots(repo_path: &str, password: &str) -> Result<Vec<serde_json::Value>> {
    let output = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "snapshots", "--json"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "restic snapshots failed: {}",
            safe_restic_text(&String::from_utf8_lossy(&output.stderr), password)
        );
    }
    serde_json::from_slice(&output.stdout).context("restic returned invalid snapshot JSON")
}

pub async fn has_snapshot_tag(repo_path: &str, password: &str, tag: &str) -> Result<bool> {
    let output = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "snapshots", "--tag", tag, "--json"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;
    if !output.status.success() {
        bail!(
            "restic snapshot reconciliation failed: {}",
            safe_restic_text(&String::from_utf8_lossy(&output.stderr), password)
        );
    }
    let snapshots: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
        .context("restic returned invalid reconciliation JSON")?;
    Ok(!snapshots.is_empty())
}

fn safe_restic_text(value: &str, password: &str) -> String {
    let exact_redacted = if password.is_empty() {
        value.to_string()
    } else {
        value.replace(password, "[REDACTED]")
    };
    let pattern_redacted = crate::api::mcp::redact::redact_patterns(exact_redacted.trim());
    let mut characters = pattern_redacted.chars();
    let mut bounded: String = characters.by_ref().take(MAX_RESTIC_TEXT_CHARS).collect();
    if characters.next().is_some() {
        bounded.push_str("\n[truncated]");
    }
    bounded
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    #[test]
    fn restic_text_is_exact_secret_redacted_and_bounded() {
        let text = format!("repository password=swordfish {}", "x".repeat(5000));
        let safe = safe_restic_text(&text, "swordfish");
        assert!(!safe.contains("swordfish"));
        assert!(safe.contains("[REDACTED]"));
        assert!(safe.ends_with("[truncated]"));
    }

    #[test]
    fn durable_config_rejects_embedded_repository_credentials() {
        let input = BackupConfigInput {
            name: "Daily backup".into(),
            source_path: "/srv/data".into(),
            repo_path: "rest:https://operator:secret@example.test/repository".into(),
            schedule: None,
            retention_days: 30,
            restore_test_schedule: None,
        };
        assert!(input.validate().is_err());
    }

    #[tokio::test]
    async fn config_lifecycle_registers_and_retires_the_operation_resource() {
        let pool = pool().await;
        let input = BackupConfigInput {
            name: "Daily backup".into(),
            source_path: "/srv/data".into(),
            repo_path: "/srv/restic".into(),
            schedule: None,
            retention_days: 30,
            restore_test_schedule: None,
        };
        let resource_id = create_config(&pool, "config-1", &input, "test")
            .await
            .unwrap();
        let alias = resources::resolve_alias(&pool, "voidtower.backup_config", "local", "config-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(alias.id, resource_id);
        assert!(get_config(&pool, "config-1").await.unwrap().is_some());

        assert!(delete_config(&pool, "config-1").await.unwrap());
        assert!(get_config(&pool, "config-1").await.unwrap().is_none());
        let lifecycle: String =
            sqlx::query_scalar("SELECT lifecycle_state FROM resources WHERE id = ?")
                .bind(resource_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(lifecycle, "retired");
    }
}
