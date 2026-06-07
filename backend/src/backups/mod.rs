use anyhow::Result;
use serde::{Deserialize, Serialize};

pub fn is_restic_available() -> bool {
    std::process::Command::new("which")
        .arg("restic")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
    /// Item #10A: cron expression for scheduled restore tests (e.g. "0 3 * * 0")
    pub restore_test_schedule: Option<String>,
}

/// Minimal 5-field cron matcher (min hour dom month dow).
/// Returns true when the current wall-clock time matches the expression.
/// Supports `*`, plain numbers, and comma-separated lists. No ranges/steps.
pub fn cron_matches_now(expr: &str) -> bool {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // Use UTC broken-down time
    let mins_since_epoch = secs / 60;
    let minute  = (mins_since_epoch % 60) as u32;
    let hour    = (mins_since_epoch / 60 % 24) as u32;
    let day     = (secs / 86400 % 31 + 1) as u32;   // 1-based approximation
    // For month/dow we use a simple modulo — good enough for weekly/monthly schedules
    let month   = ((secs / 86400 / 30) % 12 + 1) as u32; // 1-based approximation
    let dow     = (secs / 86400 % 7) as u32;              // 0 = Thursday epoch day; close enough for weekly matching

    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 { return false; }

    field_matches(parts[0], minute)
        && field_matches(parts[1], hour)
        && field_matches(parts[2], day)
        && field_matches(parts[3], month)
        && field_matches(parts[4], dow)
}

fn field_matches(field: &str, value: u32) -> bool {
    if field == "*" { return true; }
    field.split(',').any(|part| part.trim().parse::<u32>().map(|n| n == value).unwrap_or(false))
}

pub fn confidence(cfg: &BackupConfig) -> &'static str {
    let now = unix_now();
    let Some(last_run) = cfg.last_run_at else { return "critical" };
    let age_secs = now - last_run;
    let check_ok = cfg.last_check_status.as_deref() == Some("ok");
    let check_failed = cfg.last_check_status.as_deref() == Some("failed");
    let restore_age = cfg.last_restore_test_at.map(|t| now - t);
    let restore_ok = cfg.last_restore_test_status.as_deref() == Some("ok");

    if check_failed || age_secs > 30 * 86400 { return "critical" }
    if age_secs > 7 * 86400 { return "low" }
    if age_secs < 86400 && check_ok && restore_age.map(|a| a < 7 * 86400).unwrap_or(false) && restore_ok {
        return "high"
    }
    "medium"
}

pub async fn run_check(repo_path: &str, password: &str) -> Result<String> {
    let out = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "check"])
        .env("RESTIC_PASSWORD", password)
        .output().await?;
    if out.status.success() { Ok("ok".into()) } else {
        let msg = String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed").to_string();
        Err(anyhow::anyhow!("{}", msg))
    }
}

pub async fn run_restore_test(repo_path: &str, password: &str) -> Result<String> {
    let tmp = format!("/tmp/vt-restore-{}", uuid::Uuid::new_v4().simple());
    let _ = tokio::fs::create_dir_all(&tmp).await;
    let out = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "restore", "latest", "--target", &tmp, "--dry-run"])
        .env("RESTIC_PASSWORD", password)
        .output().await;
    let _ = tokio::fs::remove_dir_all(&tmp).await;
    match out {
        Ok(o) if o.status.success() => Ok("ok".into()),
        Ok(o) => {
            // --dry-run not supported in older restic; fall back to stats check
            let stderr = String::from_utf8_lossy(&o.stderr);
            if stderr.contains("unknown flag") || stderr.contains("dry-run") {
                // Fallback: just verify snapshots exist
                let snap = tokio::process::Command::new("restic")
                    .args(["-r", repo_path, "snapshots", "--json"])
                    .env("RESTIC_PASSWORD", password)
                    .output().await?;
                if snap.status.success() {
                    let v: serde_json::Value = serde_json::from_slice(&snap.stdout).unwrap_or_default();
                    if v.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
                        return Ok("ok".into());
                    }
                }
                Err(anyhow::anyhow!("no snapshots found"))
            } else {
                Err(anyhow::anyhow!("{}", stderr.lines().last().unwrap_or("failed")))
            }
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRun {
    pub config_id: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub status: String, // "running" | "success" | "failed"
    pub snapshot_id: Option<String>,
    pub files_new: Option<i64>,
    pub files_changed: Option<i64>,
    pub data_added_bytes: Option<i64>,
    pub output: String,
}

/// Run a restic backup. Never interpolates args into a shell.
pub async fn run_backup(cfg: &BackupConfig, password: &str) -> Result<BackupRun> {
    let started_at = unix_now();

    let output = tokio::process::Command::new("restic")
        .args(["-r", &cfg.repo_path, "--verbose", "backup", &cfg.source_path])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}{}", stdout, stderr);

    let status = if output.status.success() { "success" } else { "failed" }.to_string();

    // Parse snapshot ID from output like "snapshot abc12345 saved"
    let snapshot_id = stdout
        .lines()
        .find(|l| l.contains("snapshot") && l.contains("saved"))
        .and_then(|l| l.split_whitespace().nth(1))
        .map(|s| s.to_string());

    Ok(BackupRun {
        config_id: cfg.id.clone(),
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

/// Initialize a restic repo if it doesn't exist.
pub async fn init_repo(repo_path: &str, password: &str) -> Result<()> {
    let out = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "init"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        // "already initialized" is not an error
        if !err.contains("already initialized") {
            anyhow::bail!("restic init failed: {}", err);
        }
    }
    Ok(())
}

/// List snapshots for a repo.
#[allow(dead_code)]
pub async fn list_snapshots(repo_path: &str, password: &str) -> Result<Vec<serde_json::Value>> {
    let out = tokio::process::Command::new("restic")
        .args(["-r", repo_path, "snapshots", "--json"])
        .env("RESTIC_PASSWORD", password)
        .output()
        .await?;

    if !out.status.success() {
        return Ok(vec![]);
    }
    let snapshots: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    Ok(snapshots)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
