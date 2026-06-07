use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::State,
    http::header,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use rand::Rng;
use serde::{Deserialize, Serialize};

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn require_owner(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if user.role != "owner" {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

// ---------------------------------------------------------------------------
// Export / import types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct ExportedProxyRule {
    pub domain: String,
    pub upstream: String,
    pub ssl: bool,
    pub allow_embed: bool,
    pub enabled: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ExportedAutomationJob {
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub schedule: Option<String>,
    pub enabled: bool,
    pub timeout_secs: i64,
}

#[derive(Serialize, Deserialize)]
pub struct ExportedAlertRule {
    pub id: String,
    pub name: Option<String>,
    pub severity: String,
    pub state: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExportedTag {
    pub name: String,
    pub color: String,
}

#[derive(Serialize, Deserialize)]
pub struct ConfigExport {
    pub voidtower_version: String,
    pub exported_at: i64,
    pub instance_name: String,
    pub proxy_rules: Vec<ExportedProxyRule>,
    pub automation_jobs: Vec<ExportedAutomationJob>,
    pub alert_rules: Vec<ExportedAlertRule>,
    pub tags: Vec<ExportedTag>,
}

// ---------------------------------------------------------------------------
// POST /api/disaster/export-config
// ---------------------------------------------------------------------------

pub async fn export_config(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Response> {
    require_owner(&state, &jar).await?;

    let instance_name: String = sqlx::query_scalar(
        "SELECT value FROM settings WHERE key = 'instance_name'",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .flatten()
    .unwrap_or_else(|| "VoidTower".into());

    // Proxy rules
    #[derive(sqlx::FromRow)]
    struct ProxyRow {
        domain: String,
        upstream: String,
        ssl: bool,
        allow_embed: bool,
        enabled: bool,
    }
    let proxy_rules: Vec<ExportedProxyRule> = sqlx::query_as::<_, ProxyRow>(
        "SELECT domain, upstream, ssl, allow_embed, enabled FROM proxy_configs ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| ExportedProxyRule {
        domain: r.domain,
        upstream: r.upstream,
        ssl: r.ssl,
        allow_embed: r.allow_embed,
        enabled: r.enabled,
    })
    .collect();

    // Automation jobs
    #[derive(sqlx::FromRow)]
    struct AutoRow {
        name: String,
        description: Option<String>,
        command: String,
        schedule: Option<String>,
        enabled: bool,
        timeout_secs: i64,
    }
    let automation_jobs: Vec<ExportedAutomationJob> = sqlx::query_as::<_, AutoRow>(
        "SELECT name, description, command, schedule, enabled, timeout_secs FROM automation_jobs ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| ExportedAutomationJob {
        name: r.name,
        description: r.description,
        command: r.command,
        schedule: r.schedule,
        enabled: r.enabled,
        timeout_secs: r.timeout_secs,
    })
    .collect();

    // Alert rules (read-only export — not re-imported for safety)
    #[derive(sqlx::FromRow)]
    struct AlertRow {
        id: String,
        name: Option<String>,
        severity: String,
        state: String,
    }
    let alert_rules: Vec<ExportedAlertRule> = sqlx::query_as::<_, AlertRow>(
        "SELECT id, name, severity, state FROM alerts ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| ExportedAlertRule {
        id: r.id,
        name: r.name,
        severity: r.severity,
        state: r.state,
    })
    .collect();

    // Tags
    #[derive(sqlx::FromRow)]
    struct TagRow {
        name: String,
        color: String,
    }
    let tags: Vec<ExportedTag> = sqlx::query_as::<_, TagRow>(
        "SELECT name, color FROM tags ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| ExportedTag { name: r.name, color: r.color })
    .collect();

    let version = option_env!("VOIDTOWER_VERSION")
        .unwrap_or("unknown")
        .to_string();

    let export = ConfigExport {
        voidtower_version: version,
        exported_at: unix_now(),
        instance_name,
        proxy_rules,
        automation_jobs,
        alert_rules,
        tags,
    };

    let json = serde_json::to_string_pretty(&export)
        .map_err(|e| AppError::Internal(e.into()))?;

    let ts = export.exported_at;
    let filename = format!("voidtower-config-{ts}.json");
    let disposition = format!("attachment; filename=\"{filename}\"");

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_DISPOSITION, disposition.as_str()),
        ],
        json,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// POST /api/disaster/import-config
// ---------------------------------------------------------------------------

pub async fn import_config(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<ConfigExport>,
) -> Result<Json<serde_json::Value>> {
    let user = require_owner(&state, &jar).await?;
    let now = unix_now();

    // Update instance name
    sqlx::query(
        "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES ('instance_name', ?, ?)",
    )
    .bind(&payload.instance_name)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    // Upsert proxy rules (by domain)
    let mut proxies_applied: u32 = 0;
    for rule in &payload.proxy_rules {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO proxy_configs (id, domain, upstream, ssl, allow_embed, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(domain) DO UPDATE SET
               upstream = excluded.upstream,
               ssl = excluded.ssl,
               allow_embed = excluded.allow_embed,
               enabled = excluded.enabled",
        )
        .bind(&id)
        .bind(&rule.domain)
        .bind(&rule.upstream)
        .bind(rule.ssl)
        .bind(rule.allow_embed)
        .bind(rule.enabled)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        proxies_applied += 1;
    }

    // Upsert automation jobs (by name)
    let mut automations_applied: u32 = 0;
    for job in &payload.automation_jobs {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO automation_jobs (id, name, description, command, schedule, enabled, timeout_secs, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
               description = excluded.description,
               command = excluded.command,
               schedule = excluded.schedule,
               enabled = excluded.enabled,
               timeout_secs = excluded.timeout_secs,
               updated_at = excluded.updated_at",
        )
        .bind(&id)
        .bind(&job.name)
        .bind(&job.description)
        .bind(&job.command)
        .bind(&job.schedule)
        .bind(job.enabled)
        .bind(job.timeout_secs)
        .bind(now)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        automations_applied += 1;
    }

    // Upsert tags (by name)
    let mut tags_applied: u32 = 0;
    for tag in &payload.tags {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tags (id, name, color, created_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET color = excluded.color",
        )
        .bind(&id)
        .bind(&tag.name)
        .bind(&tag.color)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        tags_applied += 1;
    }

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "disaster.import-config",
        Some("settings"),
        None,
        "success",
        None,
        Some(&format!(
            "proxies={proxies_applied},automations={automations_applied},tags={tags_applied}"
        )),
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "applied": {
            "proxies": proxies_applied,
            "automations": automations_applied,
            "tags": tags_applied,
        }
    })))
}

// ---------------------------------------------------------------------------
// POST /api/disaster/emergency-reset-admin
// ---------------------------------------------------------------------------

fn random_alphanum(n: usize) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..n)
        .map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char)
        .collect()
}

pub async fn emergency_reset_admin(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_owner(&state, &jar).await?;

    // Find the oldest owner account
    let target = sqlx::query_as::<_, (String, String)>(
        "SELECT id, username FROM users WHERE role = 'owner' ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No owner account found")))?;

    let (target_id, target_username) = target;

    let temp_password = random_alphanum(16);
    let new_hash = auth::hash_password(&temp_password)
        .map_err(|e| AppError::Internal(e))?;

    let now = unix_now();
    sqlx::query(
        "UPDATE users SET password_hash = ?, force_password_change = 1, updated_at = ? WHERE id = ?",
    )
    .bind(&new_hash)
    .bind(now)
    .bind(&target_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "disaster.emergency-reset-admin",
        Some("user"),
        Some(&target_id),
        "success",
        None,
        Some(&format!("target_username={target_username}")),
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "username": target_username,
        "temporary_password": temp_password,
    })))
}

// ---------------------------------------------------------------------------
// POST /api/disaster/emergency-disable
// ---------------------------------------------------------------------------

pub async fn emergency_disable(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_owner(&state, &jar).await?;
    let now = unix_now();

    // Disable Odysseus
    sqlx::query(
        "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES ('odysseus.emergency_disabled', 'true', ?)",
    )
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "disaster.emergency-disable.odysseus",
        Some("integration"),
        Some("odysseus"),
        "success",
        None,
        None,
    )
    .await;

    // Disable all enabled automation jobs
    let automations_disabled = sqlx::query(
        "UPDATE automation_jobs SET enabled = 0, updated_at = ? WHERE enabled = 1",
    )
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .rows_affected();

    audit::log(
        &state.db,
        Some(&user.id),
        "human",
        "disaster.emergency-disable.automations",
        Some("automation"),
        None,
        "success",
        None,
        Some(&format!("count={automations_disabled}")),
    )
    .await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "disabled": {
            "odysseus": true,
            "automations": automations_disabled,
        }
    })))
}

// ---------------------------------------------------------------------------
// CLI helpers (called from main.rs without HTTP)
// ---------------------------------------------------------------------------

pub async fn cli_export(pool: &sqlx::SqlitePool, output_path: Option<&str>) -> anyhow::Result<()> {
    // Reuse the same query logic, but without auth
    let instance_name: String = sqlx::query_scalar(
        "SELECT value FROM settings WHERE key = 'instance_name'",
    )
    .fetch_optional(pool)
    .await?
    .flatten()
    .unwrap_or_else(|| "VoidTower".into());

    #[derive(sqlx::FromRow)]
    struct ProxyRow { domain: String, upstream: String, ssl: bool, allow_embed: bool, enabled: bool }
    let proxy_rules: Vec<ExportedProxyRule> = sqlx::query_as::<_, ProxyRow>(
        "SELECT domain, upstream, ssl, allow_embed, enabled FROM proxy_configs ORDER BY created_at",
    )
    .fetch_all(pool).await?
    .into_iter()
    .map(|r| ExportedProxyRule { domain: r.domain, upstream: r.upstream, ssl: r.ssl, allow_embed: r.allow_embed, enabled: r.enabled })
    .collect();

    #[derive(sqlx::FromRow)]
    struct AutoRow { name: String, description: Option<String>, command: String, schedule: Option<String>, enabled: bool, timeout_secs: i64 }
    let automation_jobs: Vec<ExportedAutomationJob> = sqlx::query_as::<_, AutoRow>(
        "SELECT name, description, command, schedule, enabled, timeout_secs FROM automation_jobs ORDER BY created_at",
    )
    .fetch_all(pool).await?
    .into_iter()
    .map(|r| ExportedAutomationJob { name: r.name, description: r.description, command: r.command, schedule: r.schedule, enabled: r.enabled, timeout_secs: r.timeout_secs })
    .collect();

    #[derive(sqlx::FromRow)]
    struct AlertRow { id: String, name: Option<String>, severity: String, state: String }
    let alert_rules: Vec<ExportedAlertRule> = sqlx::query_as::<_, AlertRow>(
        "SELECT id, name, severity, state FROM alerts ORDER BY created_at",
    )
    .fetch_all(pool).await?
    .into_iter()
    .map(|r| ExportedAlertRule { id: r.id, name: r.name, severity: r.severity, state: r.state })
    .collect();

    #[derive(sqlx::FromRow)]
    struct TagRow { name: String, color: String }
    let tags: Vec<ExportedTag> = sqlx::query_as::<_, TagRow>(
        "SELECT name, color FROM tags ORDER BY created_at",
    )
    .fetch_all(pool).await?
    .into_iter()
    .map(|r| ExportedTag { name: r.name, color: r.color })
    .collect();

    let version = option_env!("VOIDTOWER_VERSION").unwrap_or("unknown").to_string();
    let export = ConfigExport {
        voidtower_version: version,
        exported_at: unix_now(),
        instance_name,
        proxy_rules,
        automation_jobs,
        alert_rules,
        tags,
    };

    let json = serde_json::to_string_pretty(&export)?;

    match output_path {
        Some(path) => {
            std::fs::write(path, &json)?;
            println!("Config exported to {path}");
        }
        None => println!("{json}"),
    }

    Ok(())
}

pub async fn cli_import(pool: &sqlx::SqlitePool, input_path: &str) -> anyhow::Result<()> {
    let raw = std::fs::read_to_string(input_path)?;
    let payload: ConfigExport = serde_json::from_str(&raw)?;
    let now = unix_now();

    sqlx::query(
        "INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES ('instance_name', ?, ?)",
    )
    .bind(&payload.instance_name)
    .bind(now)
    .execute(pool)
    .await?;

    let mut proxies = 0u32;
    for rule in &payload.proxy_rules {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO proxy_configs (id, domain, upstream, ssl, allow_embed, enabled, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(domain) DO UPDATE SET
               upstream = excluded.upstream, ssl = excluded.ssl,
               allow_embed = excluded.allow_embed, enabled = excluded.enabled",
        )
        .bind(&id).bind(&rule.domain).bind(&rule.upstream)
        .bind(rule.ssl).bind(rule.allow_embed).bind(rule.enabled).bind(now)
        .execute(pool).await?;
        proxies += 1;
    }

    let mut automations = 0u32;
    for job in &payload.automation_jobs {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO automation_jobs (id, name, description, command, schedule, enabled, timeout_secs, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET
               description = excluded.description, command = excluded.command,
               schedule = excluded.schedule, enabled = excluded.enabled,
               timeout_secs = excluded.timeout_secs, updated_at = excluded.updated_at",
        )
        .bind(&id).bind(&job.name).bind(&job.description).bind(&job.command)
        .bind(&job.schedule).bind(job.enabled).bind(job.timeout_secs).bind(now).bind(now)
        .execute(pool).await?;
        automations += 1;
    }

    let mut tags = 0u32;
    for tag in &payload.tags {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO tags (id, name, color, created_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET color = excluded.color",
        )
        .bind(&id).bind(&tag.name).bind(&tag.color).bind(now)
        .execute(pool).await?;
        tags += 1;
    }

    println!("Import complete: {proxies} proxies, {automations} automations, {tags} tags applied.");
    Ok(())
}
