use axum::{extract::{Query, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite};

use crate::{auth, error::{AppError, Result}, AppState};

#[derive(Deserialize)]
pub struct TimelineQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
    pub category: Option<String>,
    pub outcome: Option<String>,
    pub search: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

fn default_limit() -> i64 { 50 }

#[derive(Serialize)]
pub struct TimelineEvent {
    pub id: String,
    pub timestamp: i64,
    pub category: String,
    pub action: String,
    pub actor: String,
    pub actor_type: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub outcome: String,
    pub details: Option<String>,
    pub ip_address: Option<String>,
    pub source: Option<String>,
}

/// Appends the shared WHERE clause (outcome/search/from/to filters) to a query builder,
/// using bound parameters so user-supplied values never reach the SQL string directly.
fn push_timeline_where(qb: &mut QueryBuilder<'_, Sqlite>, q: &TimelineQuery) {
    let mut first = true;

    if let Some(o) = q.outcome.as_deref().filter(|o| *o != "all") {
        qb.push(if first { " WHERE outcome = " } else { " AND outcome = " });
        first = false;
        qb.push_bind(o.to_string());
    }
    if let Some(ref s) = q.search {
        let pattern = format!("%{s}%");
        qb.push(if first { " WHERE (" } else { " AND (" });
        first = false;
        qb.push("action LIKE ").push_bind(pattern.clone());
        qb.push(" OR resource_type LIKE ").push_bind(pattern.clone());
        qb.push(" OR resource_id LIKE ").push_bind(pattern.clone());
        qb.push(" OR details LIKE ").push_bind(pattern);
        qb.push(")");
    }
    if let Some(from) = q.from {
        qb.push(if first { " WHERE timestamp >= " } else { " AND timestamp >= " });
        first = false;
        qb.push_bind(from);
    }
    if let Some(to) = q.to {
        qb.push(if first { " WHERE timestamp <= " } else { " AND timestamp <= " });
        qb.push_bind(to);
    }
}

fn classify(action: &str, resource_type: Option<&str>) -> &'static str {
    if action.starts_with("login") || action.starts_with("logout") || action.starts_with("bootstrap")
        || action.starts_with("create_user") || action.starts_with("delete_user")
        || action.starts_with("change_password") || action.starts_with("revoke_session") {
        return "auth";
    }
    if action.contains("container") || action.contains("compose") || action.contains("exec") {
        return "containers";
    }
    if action.contains("service") {
        return "services";
    }
    if action.contains("backup") || action.contains("snapshot") {
        return "backups";
    }
    if action.contains("secret") {
        return "secrets";
    }
    if action.contains("app") || action.contains("deploy") {
        return "apps";
    }
    if action.contains("proxy") || action.contains("nginx") {
        return "networking";
    }
    if action.contains("alert") {
        return "alerts";
    }
    if action.contains("file") {
        return "files";
    }
    match resource_type {
        Some("container") => "containers",
        Some("service")   => "services",
        Some("backup")    => "backups",
        Some("secret")    => "secrets",
        Some("proxy")     => "networking",
        Some("alert")     => "alerts",
        _                 => "system",
    }
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<TimelineQuery>,
) -> Result<Json<serde_json::Value>> {
    let session_id = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if user.role == "viewer" { return Err(AppError::Forbidden); }

    let limit = q.limit.clamp(1, 200);

    let mut qb: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
        "SELECT a.id, a.timestamp, a.user_id, a.actor_type, a.action,
                a.resource_type, a.resource_id, a.outcome, a.details, a.ip_address, a.source
         FROM audit_log a"
    );
    push_timeline_where(&mut qb, &q);
    qb.push(" ORDER BY a.timestamp DESC LIMIT ").push_bind(limit)
      .push(" OFFSET ").push_bind(q.offset);

    let rows = qb
        .build_query_as::<(String, i64, Option<String>, String, String, Option<String>, Option<String>, String, Option<String>, Option<String>, Option<String>)>()
        .fetch_all(&state.db).await.map_err(AppError::Database)?;

    let mut count_qb: QueryBuilder<'_, Sqlite> = QueryBuilder::new("SELECT COUNT(*) FROM audit_log a");
    push_timeline_where(&mut count_qb, &q);
    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&state.db).await.map_err(AppError::Database)?;

    // Resolve usernames in one query
    let user_ids: Vec<String> = rows.iter().filter_map(|(_, _, uid, ..)| uid.clone()).collect::<std::collections::HashSet<_>>().into_iter().collect();
    let mut username_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if !user_ids.is_empty() {
        let _placeholders = user_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect::<Vec<_>>().join(",");
        // Use a simpler approach: fetch all users and filter
        let all_users = sqlx::query_as::<_, (String, String)>("SELECT id, username FROM users")
            .fetch_all(&state.db).await.unwrap_or_default();
        for (id, name) in all_users {
            if user_ids.contains(&id) { username_map.insert(id, name); }
        }
    }

    let mut events: Vec<TimelineEvent> = rows.into_iter().map(|(id, timestamp, user_id, actor_type, action, resource_type, resource_id, outcome, details, ip_address, source)| {
        let cat = classify(&action, resource_type.as_deref());
        let actor = user_id.as_ref()
            .and_then(|uid| username_map.get(uid))
            .cloned()
            .unwrap_or_else(|| if actor_type == "system" { "system".to_string() } else { "unknown".to_string() });
        TimelineEvent { id, timestamp, category: cat.to_string(), action, actor, actor_type, resource_type, resource_id, outcome, details, ip_address, source }
    }).collect();

    // Client-side category filter (done here to keep SQL simple)
    if let Some(ref cat) = q.category {
        if cat != "all" { events.retain(|e| &e.category == cat); }
    }

    Ok(Json(serde_json::json!({
        "events": events,
        "total": total,
        "limit": limit,
        "offset": q.offset,
    })))
}
