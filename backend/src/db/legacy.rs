use super::schema;
use anyhow::{Context, Result};
use sqlx::{Acquire, Row, Sqlite, SqlitePool, Transaction};
use std::path::{Path, PathBuf};

const LEGACY_COLUMNS: &[(&str, &str, &str)] = &[
    (
        "users",
        "force_password_change",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("users", "totp_secret", "TEXT"),
    ("users", "totp_enabled", "INTEGER NOT NULL DEFAULT 0"),
    ("users", "auth_source", "TEXT NOT NULL DEFAULT 'local'"),
    ("users", "oidc_subject", "TEXT"),
    ("users", "expires_at", "INTEGER"),
    ("proxy_configs", "allow_embed", "INTEGER NOT NULL DEFAULT 0"),
    ("proxy_configs", "embed_port", "INTEGER"),
    ("proxy_configs", "sso_protect", "INTEGER NOT NULL DEFAULT 0"),
    ("proxy_configs", "custom_headers", "TEXT"),
    ("proxy_configs", "rate_limit_rpm", "INTEGER"),
    ("proxy_configs", "basic_auth_user", "TEXT"),
    ("proxy_configs", "basic_auth_pass_hash", "TEXT"),
    (
        "proxy_configs",
        "websocket_extended",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    (
        "proxy_configs",
        "cache_static",
        "INTEGER NOT NULL DEFAULT 0",
    ),
    ("proxy_configs", "health_status", "TEXT"),
    ("proxy_configs", "health_checked_at", "INTEGER"),
    ("proxy_configs", "health_latency_ms", "INTEGER"),
    ("backup_configs", "last_check_at", "INTEGER"),
    ("backup_configs", "last_check_status", "TEXT"),
    ("backup_configs", "last_restore_test_at", "INTEGER"),
    ("backup_configs", "last_restore_test_status", "TEXT"),
    ("backup_configs", "restore_test_schedule", "TEXT"),
    ("deployed_apps", "primary_port", "INTEGER"),
    (
        "deployed_apps",
        "origin",
        "TEXT NOT NULL DEFAULT 'voidtower'",
    ),
    ("deployed_apps", "owner_user_id", "TEXT"),
    ("deployed_apps", "storage_root", "TEXT"),
    ("deployed_apps", "target_node_id", "TEXT"),
    ("audit_log", "source", "TEXT"),
    ("secrets", "version", "INTEGER NOT NULL DEFAULT 0"),
    ("api_tokens", "secret_ids", "TEXT"),
    ("ssh_sessions", "password_enc", "TEXT"),
];

fn quoted_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

async fn has_migration_ledger(pool: &SqlitePool) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count != 0)
}

async fn has_application_tables(pool: &SqlitePool) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master \
         WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count != 0)
}

fn backup_path(db_path: &Path) -> Result<PathBuf> {
    let parent = db_path
        .parent()
        .context("database path has no parent directory")?;
    let file_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .context("database filename is not valid UTF-8")?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Ok(parent.join(format!("{file_name}.pre-migration-v1-{timestamp}.bak")))
}

async fn create_backup(
    connection: &mut sqlx::pool::PoolConnection<Sqlite>,
    db_path: &Path,
) -> Result<PathBuf> {
    let backup = backup_path(db_path)?;
    sqlx::query("VACUUM INTO ?")
        .bind(backup.to_string_lossy().as_ref())
        .execute(&mut **connection)
        .await
        .with_context(|| {
            format!(
                "failed to create pre-migration backup at {}",
                backup.display()
            )
        })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&backup, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to protect backup at {}", backup.display()))?;
    }

    Ok(backup)
}

async fn column_exists(
    transaction: &mut Transaction<'_, Sqlite>,
    table: &str,
    column: &str,
) -> Result<bool> {
    let pragma = format!("PRAGMA table_info({})", quoted_identifier(table));
    let rows = sqlx::query(&pragma).fetch_all(&mut **transaction).await?;
    Ok(rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("name").ok())
        .any(|name| name == column))
}

async fn normalize(transaction: &mut Transaction<'_, Sqlite>) -> Result<()> {
    let tables_only = schema::BASELINE_SQL
        .split("\nCREATE INDEX")
        .next()
        .context("baseline migration has no table section")?;
    sqlx::query(tables_only)
        .execute(&mut **transaction)
        .await
        .context("failed to create missing legacy tables")?;

    for (table, column, definition) in LEGACY_COLUMNS {
        if !column_exists(transaction, table, column).await? {
            let statement = format!(
                "ALTER TABLE {} ADD COLUMN {} {definition}",
                quoted_identifier(table),
                quoted_identifier(column)
            );
            sqlx::query(&statement)
                .execute(&mut **transaction)
                .await
                .with_context(|| format!("failed to add legacy column {table}.{column}"))?;
        }
    }

    sqlx::query(schema::BASELINE_SQL)
        .execute(&mut **transaction)
        .await
        .context("failed to create canonical indexes after legacy normalization")?;

    schema::validate_baseline_connection(transaction).await?;
    schema::validate_integrity(transaction).await?;
    Ok(())
}

pub(crate) async fn prepare_untracked_database(
    pool: &SqlitePool,
    db_path: &Path,
) -> Result<Option<PathBuf>> {
    if has_migration_ledger(pool).await? || !has_application_tables(pool).await? {
        return Ok(None);
    }

    let mut connection = pool.acquire().await?;
    let backup = create_backup(&mut connection, db_path).await?;
    let mut transaction = connection
        .begin()
        .await
        .context("failed to begin legacy normalization transaction")?;

    normalize(&mut transaction).await.with_context(|| {
        format!(
            "legacy normalization failed; the database was not changed and backup remains at {}",
            backup.display()
        )
    })?;
    transaction
        .commit()
        .await
        .context("failed to commit legacy normalization")?;

    Ok(Some(backup))
}
