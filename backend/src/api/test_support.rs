#![cfg(test)]
//! Shared `AppState` builder for tests that exercise handlers against an
//! in-memory DB. Not for HTTP-layer concerns (no real bind/listeners) — just
//! enough field wiring for handler bodies to run without duplicating this
//! boilerplate across every test module.

use crate::AppState;
use sqlx::SqlitePool;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, RwLock};

pub(crate) fn build(db: SqlitePool) -> AppState {
    let service_provider = Arc::new(crate::operations::adapters::services::SystemdServiceProvider);
    build_with_service_provider(db, service_provider)
}

pub(crate) fn build_with_service_provider(
    db: SqlitePool,
    service_provider: Arc<dyn crate::operations::adapters::services::ServiceProvider>,
) -> AppState {
    let secrets_key = Arc::new([0u8; 32]);
    let test_config = crate::config::Config {
        data_dir: std::env::temp_dir().join(format!("voidtower-api-tests-{}", std::process::id())),
        ..crate::config::Config::default()
    };
    std::fs::create_dir_all(&test_config.data_dir)
        .expect("create isolated API-test data directory");
    let test_db_path = test_config.db_path();
    if !test_db_path.exists() {
        std::fs::write(&test_db_path, b"test-only lock identity")
            .expect("create isolated API-test lock identity");
    }
    let config = Arc::new(test_config);
    let operation_adapters = Arc::new(
        crate::operations::adapters::AdapterRegistry::staged_with_service_provider(
            db.clone(),
            secrets_key.clone(),
            config.data_dir.clone(),
            service_provider.clone(),
        )
        .unwrap(),
    );
    AppState {
        db,
        config,
        metrics_tx: broadcast::channel(1).0,
        latest_metrics: Arc::new(RwLock::new(None)),
        agents_tx: broadcast::channel(1).0,
        secrets_key,
        token_sessions: Arc::new(RwLock::new(HashMap::new())),
        token_session_lock: Arc::new(tokio::sync::Mutex::new(())),
        compatibility_idempotency_lock: Arc::new(tokio::sync::Mutex::new(())),
        login_limiter: Arc::new(std::sync::Mutex::new(HashMap::new())),
        deploy_registry: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        operation_adapters,
        operation_runtime: crate::operations::runtime::RuntimeControl::new(),
        service_provider,
    }
}

/// Creates the same complete numbered schema used by production startup.
pub(crate) async fn setup_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    crate::db::run_migrations(&pool).await.unwrap();
    pool
}

/// Inserts a session-authenticated test user and returns a valid session id
/// suitable for a `vt_session` cookie.
pub(crate) async fn user_with_session(pool: &SqlitePool) -> String {
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
         VALUES ('u1', 'tester', 'x', 'admin', 0, 0)",
    )
    .execute(pool)
    .await
    .unwrap();
    crate::auth::create_session(pool, "u1", None, None)
        .await
        .unwrap()
        .id
}

pub(crate) async fn user_with_role_session(pool: &SqlitePool, role: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role, created_at, updated_at) \
         VALUES (?, ?, 'x', ?, 0, 0)",
    )
    .bind(&user_id)
    .bind(format!("test-{role}-{user_id}"))
    .bind(role)
    .execute(pool)
    .await
    .unwrap();
    crate::auth::create_session(pool, &user_id, None, None)
        .await
        .unwrap()
        .id
}
