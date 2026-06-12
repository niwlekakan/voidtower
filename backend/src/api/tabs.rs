use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

const VALID_KINDS: &[&str] = &["iframe", "markdown", "builtin"];

fn validate_kind(kind: &str) -> Result<()> {
    if VALID_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Invalid tab kind '{kind}' — must be one of {VALID_KINDS:?}"
        )))
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CustomTabRow {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub kind: String,
    pub config: String,
    pub sort_order: i64,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
pub struct CustomTab {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub kind: String,
    pub config: serde_json::Value,
    pub sort_order: i64,
    pub created_at: i64,
}

impl From<CustomTabRow> for CustomTab {
    fn from(row: CustomTabRow) -> Self {
        let config = serde_json::from_str(&row.config).unwrap_or(serde_json::Value::Null);
        CustomTab {
            id: row.id,
            title: row.title,
            icon: row.icon,
            kind: row.kind,
            config,
            sort_order: row.sort_order,
            created_at: row.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateTabRequest {
    pub title: String,
    pub icon: Option<String>,
    pub kind: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTabRequest {
    pub title: Option<String>,
    pub icon: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderRequest {
    pub ids: Vec<String>,
}

/// Portable representation of a custom tab, used by export/import.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedTab {
    pub title: String,
    pub icon: Option<String>,
    pub kind: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ImportTabsRequest {
    pub tabs: Vec<ExportedTab>,
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<CustomTab>>> {
    let user = require_user(&state, &jar).await?;

    let rows = sqlx::query_as::<_, CustomTabRow>(
        "SELECT id, title, icon, kind, config, sort_order, created_at FROM custom_tabs WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(rows.into_iter().map(CustomTab::from).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateTabRequest>,
) -> Result<Json<CustomTab>> {
    let user = require_user(&state, &jar).await?;
    validate_kind(&req.kind)?;

    if req.title.trim().is_empty() {
        return Err(AppError::BadRequest("Tab title is required".to_string()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = unix_now();
    let config_str = serde_json::to_string(&req.config).map_err(|e| AppError::Internal(e.into()))?;

    let next_order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM custom_tabs WHERE user_id = ?")
        .bind(&user.id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO custom_tabs (id, user_id, title, icon, kind, config, sort_order, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&req.title)
    .bind(&req.icon)
    .bind(&req.kind)
    .bind(&config_str)
    .bind(next_order)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(CustomTab {
        id,
        title: req.title,
        icon: req.icon,
        kind: req.kind,
        config: req.config,
        sort_order: next_order,
        created_at: now,
    }))
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<UpdateTabRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    let existing = sqlx::query_as::<_, CustomTabRow>(
        "SELECT id, title, icon, kind, config, sort_order, created_at FROM custom_tabs WHERE id = ? AND user_id = ?",
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or(AppError::NotFound)?;

    let title = req.title.unwrap_or(existing.title);
    let icon = req.icon.or(existing.icon);
    let config_str = match req.config {
        Some(c) => serde_json::to_string(&c).map_err(|e| AppError::Internal(e.into()))?,
        None => existing.config,
    };

    sqlx::query("UPDATE custom_tabs SET title = ?, icon = ?, config = ? WHERE id = ? AND user_id = ?")
        .bind(&title)
        .bind(&icon)
        .bind(&config_str)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    let result = sqlx::query("DELETE FROM custom_tabs WHERE id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// GET /api/tabs/export — dump the current user's custom tabs.
pub async fn export(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<ExportedTab>>> {
    let user = require_user(&state, &jar).await?;

    let rows = sqlx::query_as::<_, CustomTabRow>(
        "SELECT id, title, icon, kind, config, sort_order, created_at FROM custom_tabs WHERE user_id = ? ORDER BY sort_order ASC, created_at ASC",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let tabs = rows
        .into_iter()
        .map(|row| ExportedTab {
            title: row.title,
            icon: row.icon,
            kind: row.kind,
            config: serde_json::from_str(&row.config).unwrap_or(serde_json::Value::Null),
        })
        .collect();

    Ok(Json(tabs))
}

/// POST /api/tabs/import — append the given tabs to the current user's tab list.
pub async fn import(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ImportTabsRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let now = unix_now();

    let mut next_order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM custom_tabs WHERE user_id = ?")
        .bind(&user.id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

    let mut imported: u32 = 0;
    for tab in &req.tabs {
        validate_kind(&tab.kind)?;
        if tab.title.trim().is_empty() {
            return Err(AppError::BadRequest("Tab title is required".to_string()));
        }

        let id = uuid::Uuid::new_v4().to_string();
        let config_str = serde_json::to_string(&tab.config).map_err(|e| AppError::Internal(e.into()))?;

        sqlx::query(
            "INSERT INTO custom_tabs (id, user_id, title, icon, kind, config, sort_order, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&user.id)
        .bind(&tab.title)
        .bind(&tab.icon)
        .bind(&tab.kind)
        .bind(&config_str)
        .bind(next_order)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(AppError::Database)?;

        next_order += 1;
        imported += 1;
    }

    Ok(Json(serde_json::json!({ "ok": true, "imported": imported })))
}

pub async fn reorder(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ReorderRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;

    for (idx, id) in req.ids.iter().enumerate() {
        sqlx::query("UPDATE custom_tabs SET sort_order = ? WHERE id = ? AND user_id = ?")
            .bind(idx as i64)
            .bind(id)
            .bind(&user.id)
            .execute(&state.db)
            .await
            .map_err(AppError::Database)?;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        // seed a user (custom_tabs.user_id has no FK enforcement without PRAGMA foreign_keys, but keep realistic)
        sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES ('u1', 'tester', 'x', 'admin', 0, 0)")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn test_create_and_list_tab() {
        let pool = setup_db().await;
        let now = unix_now();
        let config = serde_json::json!({ "url": "https://example.com" });
        let config_str = serde_json::to_string(&config).unwrap();

        sqlx::query("INSERT INTO custom_tabs (id, user_id, title, icon, kind, config, sort_order, created_at) VALUES ('t1', 'u1', 'Grafana', NULL, 'iframe', ?, 0, ?)")
            .bind(&config_str)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let rows = sqlx::query_as::<_, CustomTabRow>(
            "SELECT id, title, icon, kind, config, sort_order, created_at FROM custom_tabs WHERE user_id = 'u1' ORDER BY sort_order ASC, created_at ASC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 1);
        let tab: CustomTab = rows[0].clone().into();
        assert_eq!(tab.title, "Grafana");
        assert_eq!(tab.config["url"], "https://example.com");
    }

    #[test]
    fn test_validate_kind() {
        assert!(validate_kind("iframe").is_ok());
        assert!(validate_kind("markdown").is_ok());
        assert!(validate_kind("builtin").is_ok());
        assert!(validate_kind("video").is_err());
    }

    #[tokio::test]
    async fn test_export_and_import_roundtrip() {
        let pool = setup_db().await;
        let now = unix_now();
        let config = serde_json::json!({ "url": "https://grafana.example.com" });
        let config_str = serde_json::to_string(&config).unwrap();

        sqlx::query("INSERT INTO custom_tabs (id, user_id, title, icon, kind, config, sort_order, created_at) VALUES ('t1', 'u1', 'Grafana', NULL, 'iframe', ?, 0, ?)")
            .bind(&config_str)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();

        let rows = sqlx::query_as::<_, CustomTabRow>(
            "SELECT id, title, icon, kind, config, sort_order, created_at FROM custom_tabs WHERE user_id = 'u1' ORDER BY sort_order ASC, created_at ASC",
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        let exported: Vec<ExportedTab> = rows
            .into_iter()
            .map(|row| ExportedTab {
                title: row.title,
                icon: row.icon,
                kind: row.kind,
                config: serde_json::from_str(&row.config).unwrap(),
            })
            .collect();
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].config["url"], "https://grafana.example.com");

        // Re-import into the same user — should append a second tab
        for tab in &exported {
            validate_kind(&tab.kind).unwrap();
            let id = uuid::Uuid::new_v4().to_string();
            let config_str = serde_json::to_string(&tab.config).unwrap();
            sqlx::query("INSERT INTO custom_tabs (id, user_id, title, icon, kind, config, sort_order, created_at) VALUES (?, 'u1', ?, ?, ?, ?, 1, ?)")
                .bind(&id)
                .bind(&tab.title)
                .bind(&tab.icon)
                .bind(&tab.kind)
                .bind(&config_str)
                .bind(now)
                .execute(&pool)
                .await
                .unwrap();
        }

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM custom_tabs WHERE user_id = 'u1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_reorder() {
        let pool = setup_db().await;
        let now = unix_now();
        for (i, id) in ["t1", "t2", "t3"].iter().enumerate() {
            sqlx::query("INSERT INTO custom_tabs (id, user_id, title, kind, config, sort_order, created_at) VALUES (?, 'u1', ?, 'iframe', '{}', ?, ?)")
                .bind(id)
                .bind(format!("Tab {i}"))
                .bind(i as i64)
                .bind(now)
                .execute(&pool)
                .await
                .unwrap();
        }

        // Reverse the order
        for (idx, id) in ["t3", "t2", "t1"].iter().enumerate() {
            sqlx::query("UPDATE custom_tabs SET sort_order = ? WHERE id = ? AND user_id = 'u1'")
                .bind(idx as i64)
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }

        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM custom_tabs WHERE user_id = 'u1' ORDER BY sort_order ASC")
            .fetch_all(&pool)
            .await
            .unwrap();

        assert_eq!(ids, vec!["t3", "t2", "t1"]);
    }
}
