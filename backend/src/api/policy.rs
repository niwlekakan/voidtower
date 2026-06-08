use crate::{
    auth,
    error::{AppError, Result},
    policy::{self, PolicyRule},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid)
        .await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

// ── List ────────────────────────────────────────────────────────────────────

pub async fn list_rules(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<PolicyRule>>> {
    require_admin(&state, &jar).await?;
    let rules: Vec<PolicyRule> = sqlx::query_as(
        "SELECT id, name, actor_type, action, resource_type, resource_tag,
                effect, priority, enabled, created_at
         FROM policy_rules ORDER BY priority ASC, created_at ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(rules))
}

// ── Create ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateRuleReq {
    pub name: String,
    pub actor_type: String,
    pub action: String,
    pub resource_type: String,
    pub resource_tag: Option<String>,
    pub effect: String,
    pub priority: Option<i64>,
}

pub async fn create_rule(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRuleReq>,
) -> Result<Json<PolicyRule>> {
    require_admin(&state, &jar).await?;

    if !matches!(req.effect.as_str(), "allow" | "deny") {
        return Err(AppError::BadRequest("effect must be 'allow' or 'deny'".into()));
    }
    if !matches!(req.actor_type.as_str(), "api_token" | "automation" | "*") {
        return Err(AppError::BadRequest("actor_type must be 'api_token', 'automation', or '*'".into()));
    }

    let id = Uuid::new_v4().to_string();
    let priority = req.priority.unwrap_or(100);
    let now = unix_now();

    sqlx::query(
        "INSERT INTO policy_rules (id,name,actor_type,action,resource_type,resource_tag,effect,priority,enabled,created_at)
         VALUES (?,?,?,?,?,?,?,?,1,?)",
    )
    .bind(&id).bind(&req.name).bind(&req.actor_type).bind(&req.action)
    .bind(&req.resource_type).bind(&req.resource_tag).bind(&req.effect)
    .bind(priority).bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let rule: PolicyRule = sqlx::query_as(
        "SELECT id, name, actor_type, action, resource_type, resource_tag,
                effect, priority, enabled, created_at
         FROM policy_rules WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(rule))
}

// ── Update ──────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateRuleReq {
    pub name: Option<String>,
    pub actor_type: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub resource_tag: Option<Option<String>>,
    pub effect: Option<String>,
    pub priority: Option<i64>,
    pub enabled: Option<bool>,
}

pub async fn update_rule(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
    Json(req): Json<UpdateRuleReq>,
) -> Result<Json<PolicyRule>> {
    require_admin(&state, &jar).await?;

    let existing: Option<PolicyRule> = sqlx::query_as(
        "SELECT id, name, actor_type, action, resource_type, resource_tag,
                effect, priority, enabled, created_at
         FROM policy_rules WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let existing = existing.ok_or(AppError::NotFound)?;

    let name         = req.name.unwrap_or(existing.name);
    let actor_type   = req.actor_type.unwrap_or(existing.actor_type);
    let action       = req.action.unwrap_or(existing.action);
    let resource_type = req.resource_type.unwrap_or(existing.resource_type);
    let resource_tag = req.resource_tag.unwrap_or(existing.resource_tag);
    let effect       = req.effect.unwrap_or(existing.effect);
    let priority     = req.priority.unwrap_or(existing.priority);
    let enabled      = req.enabled.map(|b| b as i64).unwrap_or(existing.enabled as i64);

    sqlx::query(
        "UPDATE policy_rules SET name=?,actor_type=?,action=?,resource_type=?,resource_tag=?,
         effect=?,priority=?,enabled=? WHERE id=?",
    )
    .bind(&name).bind(&actor_type).bind(&action).bind(&resource_type)
    .bind(&resource_tag).bind(&effect).bind(priority).bind(enabled).bind(&id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let updated: PolicyRule = sqlx::query_as(
        "SELECT id, name, actor_type, action, resource_type, resource_tag,
                effect, priority, enabled, created_at
         FROM policy_rules WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(updated))
}

// ── Delete ──────────────────────────────────────────────────────────────────

pub async fn delete_rule(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let rows = sqlx::query("DELETE FROM policy_rules WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .rows_affected();
    if rows == 0 { return Err(AppError::NotFound); }
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Dry-run check ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CheckReq {
    pub actor_type: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
}

#[derive(Serialize)]
pub struct CheckResult {
    pub verdict: String,
    pub reason: Option<String>,
}

pub async fn check_policy(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CheckReq>,
) -> Result<Json<CheckResult>> {
    require_admin(&state, &jar).await?;
    let verdict = policy::check(&state.db, &req.actor_type, &req.action, &req.resource_type, &req.resource_id).await;
    Ok(Json(match verdict {
        policy::PolicyVerdict::Allow => CheckResult { verdict: "allow".into(), reason: None },
        policy::PolicyVerdict::Deny(reason) => CheckResult { verdict: "deny".into(), reason: Some(reason) },
    }))
}
