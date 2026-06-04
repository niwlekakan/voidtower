use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::{Path, State}, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
}

#[derive(Deserialize)]
pub struct AssignRequest {
    pub resource_type: String,
    pub resource_id: String,
    pub tag_id: String,
}

#[derive(Deserialize)]
pub struct UnassignRequest {
    pub resource_type: String,
    pub resource_id: String,
    pub tag_id: String,
}

// GET /api/tags
pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<Tag>>> {
    require_admin(&state, &jar).await?;
    let tags = sqlx::query_as::<_, Tag>("SELECT id, name, color, created_at FROM tags ORDER BY name")
        .fetch_all(&state.db).await?;
    Ok(Json(tags))
}

// POST /api/tags
pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateTagRequest>,
) -> Result<Json<Tag>> {
    require_admin(&state, &jar).await?;
    let name = req.name.trim().to_string();
    if name.is_empty() { return Err(AppError::BadRequest("name is required".into())); }
    let color = req.color.unwrap_or_else(|| "#6366f1".into());
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, color) VALUES (?, ?, ?)")
        .bind(&id).bind(&name).bind(&color)
        .execute(&state.db).await
        .map_err(|e| if e.to_string().contains("UNIQUE") {
            AppError::Conflict(format!("tag '{}' already exists", name))
        } else { e.into() })?;
    let tag = sqlx::query_as::<_, Tag>("SELECT id, name, color, created_at FROM tags WHERE id = ?")
        .bind(&id).fetch_one(&state.db).await?;
    Ok(Json(tag))
}

// PATCH /api/tags/:id
#[derive(Deserialize)]
pub struct UpdateTagRequest {
    pub name: Option<String>,
    pub color: Option<String>,
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<Json<Tag>> {
    require_admin(&state, &jar).await?;
    if let Some(name) = &req.name {
        sqlx::query("UPDATE tags SET name = ? WHERE id = ?").bind(name).bind(&id).execute(&state.db).await?;
    }
    if let Some(color) = &req.color {
        sqlx::query("UPDATE tags SET color = ? WHERE id = ?").bind(color).bind(&id).execute(&state.db).await?;
    }
    let tag = sqlx::query_as::<_, Tag>("SELECT id, name, color, created_at FROM tags WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await?.ok_or(AppError::NotFound)?;
    Ok(Json(tag))
}

// DELETE /api/tags/:id  (cascades to resource_tags via FK)
pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    sqlx::query("DELETE FROM tags WHERE id = ?").bind(&id).execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// GET /api/tags/for?type=service&id=sshd
pub async fn tags_for_resource(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<Tag>>> {
    require_admin(&state, &jar).await?;
    let rtype = params.get("type").cloned().unwrap_or_default();
    let rid   = params.get("id").cloned().unwrap_or_default();
    let tags = sqlx::query_as::<_, Tag>(
        "SELECT t.id, t.name, t.color, t.created_at FROM tags t
         JOIN resource_tags rt ON rt.tag_id = t.id
         WHERE rt.resource_type = ? AND rt.resource_id = ?
         ORDER BY t.name"
    ).bind(&rtype).bind(&rid).fetch_all(&state.db).await?;
    Ok(Json(tags))
}

// GET /api/tags/map?type=service  — returns { resource_id: [Tag, ...] }
pub async fn tags_map(
    State(state): State<AppState>,
    jar: CookieJar,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<std::collections::HashMap<String, Vec<Tag>>>> {
    require_admin(&state, &jar).await?;
    let rtype = params.get("type").cloned().unwrap_or_default();

    #[derive(sqlx::FromRow)]
    struct Row { resource_id: String, id: String, name: String, color: String, created_at: i64 }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT rt.resource_id, t.id, t.name, t.color, t.created_at FROM tags t
         JOIN resource_tags rt ON rt.tag_id = t.id
         WHERE rt.resource_type = ?
         ORDER BY rt.resource_id, t.name"
    ).bind(&rtype).fetch_all(&state.db).await?;

    let mut map: std::collections::HashMap<String, Vec<Tag>> = std::collections::HashMap::new();
    for r in rows {
        map.entry(r.resource_id).or_default().push(Tag { id: r.id, name: r.name, color: r.color, created_at: r.created_at });
    }
    Ok(Json(map))
}

// POST /api/tags/assign
pub async fn assign(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AssignRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO resource_tags (resource_type, resource_id, tag_id) VALUES (?, ?, ?)"
    ).bind(&req.resource_type).bind(&req.resource_id).bind(&req.tag_id)
    .execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// POST /api/tags/unassign
pub async fn unassign(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<UnassignRequest>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    sqlx::query(
        "DELETE FROM resource_tags WHERE resource_type = ? AND resource_id = ? AND tag_id = ?"
    ).bind(&req.resource_type).bind(&req.resource_id).bind(&req.tag_id)
    .execute(&state.db).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
