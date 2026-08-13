mod legacy;
mod lock;
mod schema;
mod seeds;

use anyhow::{Context, Result};
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    SqlitePool,
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn init_pool(db_path: &Path) -> Result<SqlitePool> {
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _migration_lock = lock::acquire(db_path, Duration::from_secs(5)).await?;

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .with_context(|| format!("failed to open SQLite database at {}", db_path.display()))?;

    if let Some(backup) = legacy::prepare_untracked_database(&pool, db_path).await? {
        tracing::info!(
            database = %db_path.display(),
            backup = %backup.display(),
            "adopted untracked VoidTower database into numbered migrations"
        );
    }

    run_migrations(&pool).await?;
    seeds::run(&pool)
        .await
        .context("post-migration data initialization failed")?;

    Ok(pool)
}

pub(crate) async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    MIGRATOR
        .run(pool)
        .await
        .context("numbered SQLite migration failed")?;

    let mut connection = pool.acquire().await?;
    schema::validate_connection(&mut connection)
        .await
        .context("migrated schema validation failed")?;
    schema::validate_integrity(&mut connection)
        .await
        .context("migrated database integrity validation failed")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{sqlite::SqlitePoolOptions, Row};
    use std::path::{Path, PathBuf};

    fn temp_db(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}.db", uuid::Uuid::new_v4()))
    }

    async fn in_memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap()
    }

    fn backup_count(db_path: &Path) -> usize {
        backup_paths(db_path).len()
    }

    fn backup_paths(db_path: &Path) -> Vec<PathBuf> {
        let Some(parent) = db_path.parent() else {
            return Vec::new();
        };
        let Some(file_name) = db_path.file_name().and_then(|name| name.to_str()) else {
            return Vec::new();
        };
        let prefix = format!("{file_name}.pre-migration-v1-");
        std::fs::read_dir(parent)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
            .map(|entry| entry.path())
            .collect()
    }

    #[tokio::test]
    async fn fresh_database_runs_numbered_baseline() {
        let pool = in_memory_pool().await;
        run_migrations(&pool).await.unwrap();

        let versions: Vec<(i64, bool)> =
            sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(versions, vec![(1, true), (2, true)]);
    }

    #[tokio::test]
    async fn operation_resource_backfill_is_idempotent_and_stable() {
        let pool = in_memory_pool().await;
        run_migrations(&pool).await.unwrap();
        seeds::run(&pool).await.unwrap();

        let before: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT r.id, a.namespace, a.value FROM resources r \
             JOIN resource_aliases a ON a.resource_id = r.id ORDER BY a.namespace, a.value",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(before.len(), 7);

        seeds::run(&pool).await.unwrap();
        let after: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT r.id, a.namespace, a.value FROM resources r \
             JOIN resource_aliases a ON a.resource_id = r.id ORDER BY a.namespace, a.value",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn fresh_schema_matches_golden_file() {
        let pool = in_memory_pool().await;
        run_migrations(&pool).await.unwrap();

        let golden = include_str!("../../tests/schema_golden.sql");
        let golden_statements: std::collections::BTreeMap<&str, &str> = golden
            .split("\n;\n")
            .map(str::trim)
            .filter(|statement| !statement.is_empty())
            .map(|statement| {
                let name = statement
                    .strip_prefix("CREATE TABLE ")
                    .and_then(|rest| rest.split_whitespace().next())
                    .expect("golden schema statement must be CREATE TABLE");
                (name, statement)
            })
            .collect();
        let live_tables: Vec<(String, String)> = sqlx::query_as(
            "SELECT name, sql FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '_sqlx_migrations' \
             ORDER BY name",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(live_tables.len(), golden_statements.len());
        for (name, live_sql) in &live_tables {
            let golden_sql = golden_statements
                .get(name.as_str())
                .unwrap_or_else(|| panic!("missing golden schema for {name}"));
            assert_eq!(live_sql.trim(), *golden_sql);
        }
    }

    #[tokio::test]
    async fn v0_9_database_is_backed_up_upgraded_and_preserved() {
        let db_path = temp_db("voidtower-v090-numbered-upgrade");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
        sqlx::query(include_str!("../../tests/schema_v0_9_0_seed.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
             VALUES ('legacy-user', 'legacy', 'hash-must-survive', 'owner', 1, 2)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        assert_eq!(backup_count(&db_path), 0);
        let pool = init_pool(&db_path).await.unwrap();
        let row = sqlx::query(
            "SELECT username, password_hash, role, force_password_change, auth_source \
             FROM users WHERE id = 'legacy-user'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("username"), "legacy");
        assert_eq!(row.get::<String, _>("password_hash"), "hash-must-survive");
        assert_eq!(row.get::<String, _>("role"), "owner");
        assert_eq!(row.get::<i64, _>("force_password_change"), 0);
        assert_eq!(row.get::<String, _>("auth_source"), "local");
        assert_eq!(backup_count(&db_path), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&backup_paths(&db_path)[0])
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        pool.close().await;

        let pool = init_pool(&db_path).await.unwrap();
        assert_eq!(
            backup_count(&db_path),
            1,
            "tracked startup made another backup"
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn incompatible_legacy_schema_fails_without_mutating_original() {
        let db_path = temp_db("voidtower-incompatible-upgrade");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
        sqlx::query(include_str!("../../tests/schema_incompatible_users.sql"))
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let error = init_pool(&db_path).await.unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("users.id"), "unexpected error: {message}");

        let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
        let column_type: String =
            sqlx::query_scalar("SELECT type FROM pragma_table_info('users') WHERE name = 'id'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(column_type, "INTEGER");
        let ledger_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ledger_count, 0);
        let application_tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            application_tables, 1,
            "failed normalization did not roll back"
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn legacy_foreign_key_violation_fails_with_actionable_metadata() {
        let db_path = temp_db("voidtower-invalid-foreign-key");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(schema::BASELINE_SQL)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO api_tokens (id, user_id, name, token_hash, created_at) \
             VALUES ('orphan-token', 'missing-user', 'fixture', 'fixture-hash', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;

        let error = init_pool(&db_path).await.unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("table=api_tokens"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("parent=users"),
            "unexpected error: {message}"
        );
        assert_eq!(backup_count(&db_path), 1);

        let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
        let token_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_tokens")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(token_count, 1, "failed adoption did not preserve the row");
        let ledger_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ledger_count, 0);
        pool.close().await;
    }

    #[tokio::test]
    async fn current_untracked_schema_is_adopted_and_unknown_tables_survive() {
        let db_path = temp_db("voidtower-current-untracked");
        let url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new().connect(&url).await.unwrap();
        sqlx::query(schema::BASELINE_SQL)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(include_str!("../../tests/schema_unknown_plugin_table.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO plugin_owned_state VALUES ('keep', 'unchanged')")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let pool = init_pool(&db_path).await.unwrap();
        let value: String =
            sqlx::query_scalar("SELECT value FROM plugin_owned_state WHERE key = 'keep'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(value, "unchanged");
        assert_eq!(backup_count(&db_path), 1);
        pool.close().await;
    }

    #[tokio::test]
    async fn tampered_migration_checksum_is_rejected() {
        let db_path = temp_db("voidtower-tampered-migration");
        let pool = init_pool(&db_path).await.unwrap();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let error = init_pool(&db_path).await.unwrap_err();
        let message = format!("{error:#}");
        assert!(
            message.contains("checksum") || message.contains("modified"),
            "unexpected checksum error: {message}"
        );
    }

    #[tokio::test]
    async fn concurrent_initialization_converges_on_one_version() {
        let db_path = temp_db("voidtower-concurrent-init");
        let first_path = db_path.clone();
        let second_path = db_path.clone();
        let (first, second) = tokio::join!(init_pool(&first_path), init_pool(&second_path));
        let first = first.unwrap();
        let second = second.unwrap();

        let versions: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
                .fetch_one(&first)
                .await
                .unwrap();
        assert_eq!(versions, 2);
        first.close().await;
        second.close().await;
    }
}
