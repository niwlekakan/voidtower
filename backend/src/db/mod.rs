mod legacy;
pub(crate) mod lock;
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

/// Durable marker recording that the database file was rebuilt to drop legacy
/// plaintext residue.
///
/// Bump `PLAINTEXT_RESIDUE_SCRUB_VERSION` whenever a new migration deletes
/// plaintext, so the rebuild runs again for databases that already recorded the
/// previous version.
pub(crate) const PLAINTEXT_RESIDUE_SCRUB_KEY: &str = "db.legacy_plaintext_residue_scrub";
const PLAINTEXT_RESIDUE_SCRUB_VERSION: &str = "1";

/// Rebuild the database file once so credential plaintext deleted by an earlier
/// release cannot be recovered from free pages or the WAL.
///
/// The legacy credential migrations delete plaintext rows but SQLite keeps the
/// freed cell content in the file, and the migration itself runs inside a
/// transaction so it cannot rebuild the database. `VACUUM` rewrites the file
/// from live data only, and the truncating WAL checkpoint is what copies that
/// rewrite over the old page images outside SQLite. The marker is recorded only
/// after the checkpoint reports a complete rewrite, so a busy or partial
/// checkpoint leaves the retry armed instead of silently keeping the residue
/// forever. The migration lock serializes the rebuild against another
/// controller converging the same database file.
pub(crate) async fn scrub_legacy_plaintext_residue(
    pool: &SqlitePool,
    db_path: &Path,
) -> Result<bool> {
    let _migration_lock = lock::acquire(db_path, Duration::from_secs(5)).await?;

    let applied: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(PLAINTEXT_RESIDUE_SCRUB_KEY)
        .fetch_optional(pool)
        .await
        .context("plaintext residue marker read failed")?;
    if applied.as_deref() == Some(PLAINTEXT_RESIDUE_SCRUB_VERSION) {
        return Ok(false);
    }

    sqlx::query("VACUUM")
        .execute(pool)
        .await
        .context("plaintext residue database rebuild failed")?;
    let (busy, log_pages, checkpointed): (i64, i64, i64) =
        sqlx::query_as("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(pool)
            .await
            .context("plaintext residue WAL checkpoint failed")?;
    anyhow::ensure!(
        busy == 0 && (log_pages <= 0 || checkpointed == log_pages),
        "plaintext residue rebuild is incomplete: {checkpointed} of {log_pages} WAL pages \
         checkpointed (busy={busy})"
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(PLAINTEXT_RESIDUE_SCRUB_KEY)
    .bind(PLAINTEXT_RESIDUE_SCRUB_VERSION)
    .bind(now)
    .execute(pool)
    .await
    .context("plaintext residue marker write failed")?;
    Ok(true)
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
        assert_eq!(
            versions,
            vec![
                (1, true),
                (2, true),
                (3, true),
                (4, true),
                (5, true),
                (6, true),
                (7, true),
                (8, true),
                (9, true),
                (10, true)
            ]
        );
    }

    #[tokio::test]
    async fn npm_adapter_columns_are_additive_and_default_to_visible_docker() {
        let pool = in_memory_pool().await;
        run_migrations(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO deployed_apps \
             (id, app_id, app_name, project_name, status, deployed_at, compose_path) \
             VALUES ('legacy-row', 'nextcloud', 'Nextcloud', 'nextcloud', 'running', 1, '/tmp/compose.yml')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let (kind, hidden, url, hostname): (String, i64, Option<String>, Option<String>) =
            sqlx::query_as(
                "SELECT kind, hidden, url, hostname FROM deployed_apps WHERE id = 'legacy-row'",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(kind, "docker", "existing rows must keep the docker default");
        assert_eq!(
            hidden, 0,
            "existing rows must stay visible in the app vault"
        );
        assert_eq!(url, None);
        assert_eq!(hostname, None);

        let null_kind = sqlx::query("UPDATE deployed_apps SET kind = NULL WHERE id = 'legacy-row'")
            .execute(&pool)
            .await;
        assert!(null_kind.is_err(), "kind must stay NOT NULL");
    }

    #[tokio::test]
    async fn cmdb_schema_keeps_resource_uuid_canonical_and_strong_identity_unique() {
        let pool = in_memory_pool().await;
        run_migrations(&pool).await.unwrap();
        let now = 1_i64;

        sqlx::query(
            "INSERT INTO cmdb_classes \
             (key, label, is_builtin, enabled, created_at, updated_at) \
             VALUES ('hw', 'Hardware', 1, 1, ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cmdb_types \
             (key, class_key, label, is_builtin, enabled, created_at, updated_at) \
             VALUES ('hdd', 'hw', 'Hard disk', 1, 1, ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

        let missing_resource = sqlx::query(
            "INSERT INTO cmdb_assets \
             (resource_id, asset_id, class_key, type_key, created_at, updated_at) \
             VALUES ('missing', 'VT-hw-hdd-0000', 'hw', 'hdd', ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await;
        assert!(missing_resource.is_err());

        for (resource_id, asset_id) in [
            ("resource-a", "VT-hw-hdd-0001"),
            ("resource-b", "VT-hw-hdd-0002"),
        ] {
            sqlx::query(
                "INSERT INTO resources \
                 (id, kind, display_name, lifecycle_state, revision, created_at, updated_at) \
                 VALUES (?, 'physical_disk', ?, 'active', 0, ?, ?)",
            )
            .bind(resource_id)
            .bind(asset_id)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO cmdb_assets \
                 (resource_id, asset_id, class_key, type_key, created_at, updated_at) \
                 VALUES (?, ?, 'hw', 'hdd', ?, ?)",
            )
            .bind(resource_id)
            .bind(asset_id)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        }

        let duplicate_asset_id = sqlx::query(
            "UPDATE cmdb_assets SET asset_id = 'VT-hw-hdd-0001' \
             WHERE resource_id = 'resource-b'",
        )
        .execute(&pool)
        .await;
        assert!(duplicate_asset_id.is_err());

        for (id, resource_id) in [("identity-a", "resource-a"), ("identity-b", "resource-b")] {
            let inserted = sqlx::query(
                "INSERT INTO cmdb_asset_identities \
                 (id, resource_id, identity_kind, normalized_value, confidence, source, \
                  first_seen_at, last_seen_at) \
                 VALUES (?, ?, 'wwn', '5000c500abcd1234', 'strong', 'test', ?, ?)",
            )
            .bind(id)
            .bind(resource_id)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await;
            if resource_id == "resource-a" {
                inserted.unwrap();
            } else {
                assert!(inserted.is_err());
            }
        }

        for (id, resource_id) in [("serial-a", "resource-a"), ("serial-b", "resource-b")] {
            sqlx::query(
                "INSERT INTO cmdb_asset_identities \
                 (id, resource_id, identity_kind, normalized_value, confidence, source, \
                  first_seen_at, last_seen_at) \
                 VALUES (?, ?, 'serial', 'shared-serial', 'weak', 'test', ?, ?)",
            )
            .bind(id)
            .bind(resource_id)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        }
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
        assert_eq!(versions, 10);
        first.close().await;
        second.close().await;
    }

    /// Every byte SQLite may have written for `db_path`, including the WAL
    /// sidecar, so a residue assertion cannot be satisfied by ignoring the file
    /// the credential was checkpointed into.
    fn database_file_bytes(db_path: &Path) -> Vec<u8> {
        let mut bytes = Vec::new();
        for suffix in ["", "-wal"] {
            let path = PathBuf::from(format!("{}{suffix}", db_path.display()));
            if let Ok(mut data) = std::fs::read(&path) {
                bytes.append(&mut data);
            }
        }
        bytes
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty()
            && haystack
                .windows(needle.len())
                .any(|window| window == needle)
    }

    fn remove_database_files(db_path: &Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", db_path.display()));
        }
    }

    /// Build the state an older release left behind: a plaintext credential row
    /// that was deleted by a connection which did not zero freed cell content,
    /// in a database file that was never rebuilt.
    async fn seed_pre_fix_plaintext_residue(db_path: &Path, sentinel: &str) {
        use sqlx::{ConnectOptions, Connection};

        let pool = init_pool(db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('legacy.plaintext', ?, 1)",
        )
        .bind(sentinel)
        .execute(&pool)
        .await
        .unwrap();
        for index in 0..40 {
            sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES (?, ?, 1)")
                .bind(format!("pad.{index}"))
                .bind("p".repeat(200))
                .execute(&pool)
                .await
                .unwrap();
        }
        let mut legacy = SqliteConnectOptions::new()
            .filename(db_path)
            .pragma("secure_delete", "false")
            .connect()
            .await
            .unwrap();
        sqlx::query("DELETE FROM settings WHERE key = 'legacy.plaintext'")
            .execute(&mut legacy)
            .await
            .unwrap();
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&mut legacy)
            .await;
        legacy.close().await.unwrap();
        pool.close().await;
    }

    async fn scrub_marker(pool: &SqlitePool) -> Option<String> {
        sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(PLAINTEXT_RESIDUE_SCRUB_KEY)
            .fetch_optional(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn plaintext_residue_scrub_removes_pre_fix_credential_bytes() {
        let db_path = temp_db("voidtower-plaintext-residue");
        let sentinel = format!("vt-pre-fix-credential-{}", "R".repeat(400));

        seed_pre_fix_plaintext_residue(&db_path, &sentinel).await;
        assert!(
            contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "fixture must reproduce pre-fix free-page residue before the scrub"
        );

        let pool = init_pool(&db_path).await.unwrap();
        assert!(
            scrub_legacy_plaintext_residue(&pool, &db_path)
                .await
                .unwrap(),
            "the first startup over a pre-fix database must rebuild the file"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM settings WHERE key LIKE 'pad.%'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            40,
            "the rebuild must preserve live rows"
        );
        pool.close().await;

        assert!(
            !contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "the rebuilt database file must not retain deleted credential bytes"
        );

        let pool = init_pool(&db_path).await.unwrap();
        assert!(
            !scrub_legacy_plaintext_residue(&pool, &db_path)
                .await
                .unwrap(),
            "a recorded scrub must not rebuild the file again on the next startup"
        );
        pool.close().await;
        remove_database_files(&db_path);
    }

    #[tokio::test]
    async fn plaintext_residue_scrub_refuses_to_mark_an_incomplete_rebuild() {
        use sqlx::{ConnectOptions, Connection};

        let db_path = temp_db("voidtower-plaintext-residue-busy");
        let sentinel = format!("vt-busy-checkpoint-credential-{}", "B".repeat(400));

        seed_pre_fix_plaintext_residue(&db_path, &sentinel).await;
        let pool = init_pool(&db_path).await.unwrap();

        // A read snapshot pins the WAL: the truncating checkpoint cannot copy
        // the rebuilt pages into the database file while it is held, so the
        // rebuild is only partially applied.
        let mut reader = SqliteConnectOptions::new()
            .filename(&db_path)
            .busy_timeout(Duration::from_millis(100))
            .connect()
            .await
            .unwrap();
        sqlx::query("BEGIN").execute(&mut reader).await.unwrap();
        let _: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM settings")
            .fetch_one(&mut reader)
            .await
            .unwrap();

        let error = scrub_legacy_plaintext_residue(&pool, &db_path)
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("incomplete"),
            "a blocked checkpoint must be reported as an incomplete rebuild: {error:#}"
        );
        assert_eq!(
            scrub_marker(&pool).await,
            None,
            "an incomplete rebuild must not record the marker that suppresses the retry"
        );
        assert!(
            contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "the residue is still present, so the retry must not be suppressed"
        );

        sqlx::query("ROLLBACK").execute(&mut reader).await.unwrap();
        reader.close().await.unwrap();

        assert!(
            scrub_legacy_plaintext_residue(&pool, &db_path)
                .await
                .unwrap(),
            "the retry must complete once the blocking reader is gone"
        );
        assert_eq!(scrub_marker(&pool).await.as_deref(), Some("1"));
        pool.close().await;

        assert!(
            !contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "the completed rebuild must drop the deleted credential bytes"
        );
        remove_database_files(&db_path);
    }

    #[tokio::test]
    async fn plaintext_residue_scrub_waits_for_the_migration_lock() {
        let db_path = temp_db("voidtower-plaintext-residue-lock");
        let sentinel = format!("vt-lock-serialized-credential-{}", "L".repeat(400));

        seed_pre_fix_plaintext_residue(&db_path, &sentinel).await;
        let pool = init_pool(&db_path).await.unwrap();

        // Another controller converging the same database file holds the
        // migration lock. The rebuild must not rewrite the file underneath it.
        let held = lock::acquire(&db_path, Duration::from_secs(2))
            .await
            .unwrap();
        let scrub_pool = pool.clone();
        let scrub_path = db_path.clone();
        let scrub =
            tokio::spawn(
                async move { scrub_legacy_plaintext_residue(&scrub_pool, &scrub_path).await },
            );

        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(
            scrub_marker(&pool).await,
            None,
            "the rebuild must not run while another process holds the migration lock"
        );
        assert!(
            contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "the residue must still be present before the rebuild is allowed to run"
        );

        drop(held);
        let rebuilt = tokio::time::timeout(Duration::from_secs(10), scrub)
            .await
            .expect("the waiting rebuild must finish once the lock is released")
            .expect("the rebuild task must not panic")
            .expect("the waiting rebuild must succeed");
        assert!(rebuilt, "the waiting rebuild must complete");
        assert_eq!(scrub_marker(&pool).await.as_deref(), Some("1"));
        pool.close().await;

        assert!(
            !contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "the serialized rebuild must drop the deleted credential bytes"
        );
        remove_database_files(&db_path);
    }

    #[tokio::test]
    async fn plaintext_residue_scrub_follows_the_legacy_credential_migration() {
        let db_path = temp_db("voidtower-plaintext-residue-migration");
        let sentinel = format!("https://ntfy.invalid/{}", "M".repeat(300));

        let pool = init_pool(&db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('notif_ntfy_url', ?, 1)",
        )
        .bind(&sentinel)
        .execute(&pool)
        .await
        .unwrap();
        crate::api::secrets::migrate_legacy_notification_credentials(&pool, &[0u8; 32])
            .await
            .unwrap();
        // The scrub is only meaningful if the migration left the migrated
        // plaintext behind; assert the residue exists rather than trusting that
        // it does, so this test cannot pass without the rebuild doing work.
        assert!(
            contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "fixture must reproduce plaintext residue from the notification migration"
        );
        assert!(
            scrub_legacy_plaintext_residue(&pool, &db_path)
                .await
                .unwrap(),
            "the upgrade startup must rebuild the file the migration just wrote"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM settings WHERE key = 'notif_ntfy_url'"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0,
            "the plaintext row must still be gone after the rebuild"
        );
        assert_eq!(
            crate::api::secrets::resolve(
                &pool,
                &[0u8; 32],
                &sqlx::query_scalar::<_, String>(
                    "SELECT value FROM settings WHERE key = 'notif.ntfy_url_secret_id'"
                )
                .fetch_one(&pool)
                .await
                .unwrap(),
                crate::api::secrets::NOTIFICATION_WEBHOOK_PURPOSE,
            )
            .await
            .unwrap(),
            sentinel,
            "the encrypted reference must still resolve after the rebuild"
        );
        pool.close().await;

        assert!(
            !contains_bytes(&database_file_bytes(&db_path), sentinel.as_bytes()),
            "the upgrade must not leave the migrated credential in the database file"
        );
        remove_database_files(&db_path);
    }

    /// Claim precedence for two jobs accepted in the same second, mirroring the
    /// predicate in `operations::worker`: earlier `submitted_at` wins, and the
    /// tie-break is the insertion order (`rowid`).
    async fn queued_head(pool: &SqlitePool) -> Option<String> {
        sqlx::query_scalar(
            "SELECT j.id FROM jobs j \
             WHERE j.state = 'queued' \
               AND NOT EXISTS (SELECT 1 FROM jobs prior \
                               WHERE prior.concurrency_key = j.concurrency_key \
                                 AND prior.id <> j.id \
                                 AND prior.state NOT IN \
                                     ('succeeded', 'failed', 'cancelled', 'rejected', 'expired') \
                                 AND (prior.submitted_at, prior.rowid) \
                                   < (j.submitted_at, j.rowid)) \
             ORDER BY j.submitted_at, j.rowid LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    async fn latest_audit_action(pool: &SqlitePool, request_id: &str) -> Option<String> {
        sqlx::query_scalar(
            "SELECT action FROM audit_log WHERE request_id = ? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(request_id)
        .fetch_optional(pool)
        .await
        .unwrap()
    }

    /// The rebuild rewrites the live file from live rows. `docs/api.md` records
    /// the rowid-based submission order and the `ORDER BY rowid DESC` audit read
    /// as a kernel invariant, so pin both across the rebuild: a same-second
    /// predecessor must still be claimed first, and the latest audit row must
    /// still be the last one written.
    #[tokio::test]
    async fn plaintext_residue_scrub_preserves_job_and_audit_row_order() {
        let db_path = temp_db("voidtower-plaintext-residue-ordering");
        let sentinel = format!("vt-ordering-credential-{}", "O".repeat(400));
        seed_pre_fix_plaintext_residue(&db_path, &sentinel).await;

        let pool = init_pool(&db_path).await.unwrap();
        sqlx::query(
            "INSERT INTO resources (id, kind, display_name, created_at, updated_at) \
             VALUES ('res-ordering', 'host', 'Ordering fixture', 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for job_id in ["job-first", "job-second"] {
            sqlx::query(
                "INSERT INTO jobs (id, action, resource_id, resource_revision, actor_type, \
                 ingress, input_json, request_digest, plan_json, plan_digest, \
                 external_fingerprint, state, idempotency_scope, idempotency_key, \
                 concurrency_key, retry_class, recovery_class, submitted_at, updated_at) \
                 VALUES (?, 'service.restart', 'res-ordering', 0, 'human', 'api', '{}', \
                 'digest', '{}', 'plan-digest', 'fingerprint', 'queued', 'fixture', ?, \
                 'fixture-key', 'never', 'manual', 1700000000, 1700000000)",
            )
            .bind(job_id)
            .bind(job_id)
            .execute(&pool)
            .await
            .unwrap();
        }
        for action in ["first", "second"] {
            sqlx::query(
                "INSERT INTO audit_log (id, timestamp, action, request_id) VALUES (?, 1, ?, 'req')",
            )
            .bind(format!("audit-{action}"))
            .bind(action)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(queued_head(&pool).await.as_deref(), Some("job-first"));
        assert_eq!(
            latest_audit_action(&pool, "req").await.as_deref(),
            Some("second")
        );

        assert!(
            scrub_legacy_plaintext_residue(&pool, &db_path)
                .await
                .unwrap(),
            "the fixture must exercise a real rebuild"
        );

        assert_eq!(
            queued_head(&pool).await.as_deref(),
            Some("job-first"),
            "the rebuild must not let the later same-second sibling overtake the earlier job"
        );
        assert_eq!(
            latest_audit_action(&pool, "req").await.as_deref(),
            Some("second"),
            "the rebuild must not change which audit row is the latest"
        );
        pool.close().await;
        remove_database_files(&db_path);
    }
}
