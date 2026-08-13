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
    let secrets_key = Arc::new([0u8; 32]);
    let operation_adapters = Arc::new(
        crate::operations::adapters::AdapterRegistry::staged(db.clone(), secrets_key.clone())
            .unwrap(),
    );
    AppState {
        db,
        config: Arc::new(crate::config::Config::default()),
        metrics_tx: broadcast::channel(1).0,
        latest_metrics: Arc::new(RwLock::new(None)),
        agents_tx: broadcast::channel(1).0,
        secrets_key,
        token_sessions: Arc::new(RwLock::new(HashMap::new())),
        login_limiter: Arc::new(std::sync::Mutex::new(HashMap::new())),
        deploy_registry: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        operation_adapters,
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
