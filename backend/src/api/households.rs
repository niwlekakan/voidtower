use axum::{
    body::to_bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    audit, auth,
    error::{AppError, Result},
    household::{self, GrantPermission},
    AppState,
};

const MAX_NAME_BYTES: usize = 128;
const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateHouseholdRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddMemberRequest {
    pub user_id: String,
    pub role: String,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantResourceRequest {
    pub user_id: String,
    pub resource_id: String,
    pub permission: String,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Household {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Membership {
    pub household_id: String,
    pub user_id: String,
    pub role: String,
    pub status: String,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ResourceGrant {
    pub household_id: String,
    pub resource_id: String,
    pub user_id: String,
    pub permission: String,
    pub granted_by: String,
    pub expires_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Household administration writes a bounded audit row for every membership and grant change.
/// Only canonical identifiers and the permission/role token are recorded: names, credentials, and
/// provider material never reach the audit table.
async fn audit_household(
    state: &AppState,
    actor_id: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
    details: Option<String>,
) {
    audit::log(
        &state.db,
        Some(actor_id),
        "human",
        action,
        Some(resource_type),
        Some(resource_id),
        "success",
        None,
        details.as_deref(),
    )
    .await;
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

async fn require_manager(
    state: &AppState,
    jar: &CookieJar,
    household_id: &str,
) -> Result<auth::User> {
    let user = require_user(state, jar).await?;
    if !household::can_manage_household(&state.db, &user.id, &user.role, household_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES || name.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "name must be 1-128 bytes and contain no control characters".into(),
        ));
    }
    Ok(())
}

fn validate_user_id(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 128 || !value.is_ascii() {
        return Err(AppError::BadRequest("user_id is invalid".into()));
    }
    Ok(())
}

fn validate_expiry(expires_at: Option<i64>, timestamp: i64) -> Result<()> {
    if expires_at.is_some_and(|value| value <= timestamp) {
        return Err(AppError::BadRequest(
            "expires_at must be in the future".into(),
        ));
    }
    Ok(())
}

async fn parse_json_body<T: serde::de::DeserializeOwned>(
    request: axum::extract::Request,
) -> Result<T> {
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !content_type {
        return Err(AppError::RequestBody {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
        });
    }
    let body = to_bytes(request.into_body(), MAX_BODY_BYTES)
        .await
        .map_err(|_| AppError::PayloadTooLarge)?;
    serde_json::from_slice(&body).map_err(|_| AppError::RequestBody {
        status: StatusCode::UNPROCESSABLE_ENTITY,
    })
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    request: axum::extract::Request,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    let request: CreateHouseholdRequest = parse_json_body(request).await?;
    validate_name(&request.name)?;
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query(
        "INSERT INTO households (id, name, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&request.name)
    .bind(&user.id)
    .bind(timestamp)
    .bind(timestamp)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query(
        "INSERT INTO household_memberships
         (household_id, user_id, role, created_at, updated_at)
         VALUES (?, ?, 'owner', ?, ?)",
    )
    .bind(&id)
    .bind(&user.id)
    .bind(timestamp)
    .bind(timestamp)
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    audit_household(&state, &user.id, "household.create", "household", &id, None).await;
    Ok(Json(serde_json::json!({
        "household": Household { id, name: request.name, created_by: user.id, created_at: timestamp, updated_at: timestamp }
    })))
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    let memberships: Vec<Membership> = sqlx::query_as(
        "SELECT household_id, user_id, role, status, expires_at, revoked_at, created_at, updated_at
         FROM household_memberships WHERE user_id = ? ORDER BY created_at, household_id",
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "memberships": memberships })))
}

pub async fn list_members(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_manager(&state, &jar, &household_id).await?;
    let memberships: Vec<Membership> = sqlx::query_as(
        "SELECT household_id, user_id, role, status, expires_at, revoked_at, created_at, updated_at
         FROM household_memberships WHERE household_id = ? ORDER BY created_at, user_id",
    )
    .bind(&household_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "memberships": memberships })))
}

pub async fn add_member(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
    request: axum::extract::Request,
) -> Result<Json<serde_json::Value>> {
    let actor = require_manager(&state, &jar, &household_id).await?;
    let request: AddMemberRequest = parse_json_body(request).await?;
    validate_user_id(&request.user_id)?;
    if !matches!(
        request.role.as_str(),
        "owner" | "admin" | "member" | "guest" | "device"
    ) {
        return Err(AppError::BadRequest("role is invalid".into()));
    }
    let timestamp = now();
    validate_expiry(request.expires_at, timestamp)?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE id = ?")
        .bind(&request.user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }
    let result = sqlx::query(
        "INSERT INTO household_memberships
         (household_id, user_id, role, expires_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&household_id)
    .bind(&request.user_id)
    .bind(&request.role)
    .bind(request.expires_at)
    .bind(timestamp)
    .bind(timestamp)
    .execute(&state.db)
    .await;
    if let Err(error) = result {
        if matches!(error, sqlx::Error::Database(ref database) if database.is_unique_violation()) {
            return Err(AppError::Conflict(
                "The user is already a household member".into(),
            ));
        }
        return Err(AppError::Internal(error.into()));
    }
    audit_household(
        &state,
        &actor.id,
        "household.member.add",
        "household",
        &household_id,
        Some(format!("member={};role={}", request.user_id, request.role)),
    )
    .await;
    Ok(Json(serde_json::json!({
        "household_id": household_id,
        "user_id": request.user_id,
        "role": request.role,
        "granted_by": actor.id
    })))
}

pub async fn revoke_member(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, user_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let actor = require_manager(&state, &jar, &household_id).await?;
    validate_user_id(&user_id)?;
    if actor.id == user_id {
        return Err(AppError::BadRequest(
            "cannot revoke the current household session".into(),
        ));
    }
    let target_role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM household_memberships WHERE household_id = ? AND user_id = ? AND status = 'active'",
    )
    .bind(&household_id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let Some(target_role) = target_role else {
        return Err(AppError::NotFound);
    };
    if target_role == "owner" {
        let owners: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM household_memberships
             WHERE household_id = ? AND role = 'owner' AND status = 'active'",
        )
        .bind(&household_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if owners <= 1 {
            return Err(AppError::Conflict(
                "a household must retain an active owner".into(),
            ));
        }
    }
    if !household::revoke_membership(&state.db, &household_id, &user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        return Err(AppError::NotFound);
    }
    audit_household(
        &state,
        &actor.id,
        "household.member.revoke",
        "household",
        &household_id,
        Some(format!("member={user_id}")),
    )
    .await;
    Ok(Json(
        serde_json::json!({ "revoked": true, "user_id": user_id }),
    ))
}

pub async fn list_grants(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_manager(&state, &jar, &household_id).await?;
    let grants: Vec<ResourceGrant> = sqlx::query_as(
        "SELECT household_id, resource_id, user_id, permission, granted_by, expires_at, revoked_at, created_at, updated_at
         FROM resource_grants WHERE household_id = ? ORDER BY created_at, resource_id, user_id",
    )
    .bind(&household_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "grants": grants })))
}

pub async fn grant_resource(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
    request: axum::extract::Request,
) -> Result<Json<serde_json::Value>> {
    let actor = require_manager(&state, &jar, &household_id).await?;
    let request: GrantResourceRequest = parse_json_body(request).await?;
    validate_user_id(&request.user_id)?;
    let permission = GrantPermission::parse(&request.permission)
        .ok_or_else(|| AppError::BadRequest("permission must be read, operate, or admin".into()))?;
    let timestamp = now();
    validate_expiry(request.expires_at, timestamp)?;
    if !household::is_active_member(&state.db, &request.user_id, &household_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        return Err(AppError::BadRequest(
            "resource grants require an active household member".into(),
        ));
    }
    let resource_exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM resources WHERE id = ?)")
            .bind(&request.resource_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    if resource_exists == 0 {
        return Err(AppError::NotFound);
    }
    if permission == GrantPermission::Admin && actor.role != "owner" {
        return Err(AppError::Forbidden);
    }
    sqlx::query(
        "INSERT INTO resource_grants
         (household_id, resource_id, user_id, permission, granted_by, expires_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(household_id, resource_id, user_id) DO UPDATE SET
           permission = excluded.permission, granted_by = excluded.granted_by,
           expires_at = excluded.expires_at, revoked_at = NULL, updated_at = excluded.updated_at",
    )
    .bind(&household_id)
    .bind(&request.resource_id)
    .bind(&request.user_id)
    .bind(request.permission)
    .bind(&actor.id)
    .bind(request.expires_at)
    .bind(timestamp)
    .bind(timestamp)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    audit_household(
        &state,
        &actor.id,
        "household.grant",
        "resource",
        &request.resource_id,
        Some(format!(
            "member={};permission={}",
            request.user_id,
            permission_string(permission)
        )),
    )
    .await;
    Ok(Json(serde_json::json!({
        "household_id": household_id,
        "resource_id": request.resource_id,
        "user_id": request.user_id,
        "permission": permission_string(permission)
    })))
}

fn permission_string(permission: GrantPermission) -> &'static str {
    match permission {
        GrantPermission::Read => "read",
        GrantPermission::Operate => "operate",
        GrantPermission::Admin => "admin",
    }
}

pub async fn revoke_grant(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, resource_id, user_id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>> {
    let actor = require_manager(&state, &jar, &household_id).await?;
    validate_user_id(&user_id)?;
    if !household::revoke_resource_grant(
        &state.db,
        &household_id,
        &resource_id,
        &user_id,
        crate::api::mcp::action_registry::GRANT_SCOPED_ACTIONS,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    {
        return Err(AppError::NotFound);
    }
    audit_household(
        &state,
        &actor.id,
        "household.grant.revoke",
        "resource",
        &resource_id,
        Some(format!("member={user_id}")),
    )
    .await;
    Ok(Json(serde_json::json!({ "revoked": true })))
}

/// The caller's own active grants in one household.
///
/// A grant-constrained member needs the permission it holds to know whether the canonical
/// grant-scoped action is available to it; the manager-only grant table is deliberately not
/// readable by a member, so this returns only the caller's rows. It never discloses another
/// member's grant and cannot be used to enumerate a household the caller does not belong to.
pub async fn my_grants(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if !household::is_active_member(&state.db, &user.id, &household_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        return Err(AppError::Forbidden);
    }
    let grants: Vec<ResourceGrant> = sqlx::query_as(
        "SELECT household_id, resource_id, user_id, permission, granted_by, expires_at, revoked_at, created_at, updated_at
         FROM resource_grants
         WHERE household_id = ? AND user_id = ? AND revoked_at IS NULL
         ORDER BY resource_id",
    )
    .bind(&household_id)
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "grants": grants })))
}
