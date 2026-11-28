use axum::{
    body::{to_bytes, Body, Bytes},
    extract::{Path, Request, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode, Uri},
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::{
    audit::{self, PendingAudit},
    auth,
    error::{AppError, Result},
    household::GrantPermission,
    operations::events::{self, PendingEvent},
    AppState,
};

pub const MAX_BODY_BYTES: usize = 64 * 1024;
pub const PRIVACY_SCHEMA_VERSION: i64 = 1;
const MAX_ARTIFACT_BYTES: usize = MAX_BODY_BYTES * 4;
const MAX_LIST_ITEMS: i64 = 200;
const MAX_LIST_CURSOR: i64 = 1_000_000_000;
const MAX_FIELD_CHARS: usize = 128;
const MAX_FAILURE_CODE_CHARS: usize = 64;
const MAX_PRIVACY_ATTEMPTS: i64 = 3;
const PUBLIC_FAILURE_CODES: &[&str] = &[
    "authorization_revoked",
    "legal_hold",
    "multi_household_scope_requires_review",
    "retention_blocked",
    "retry_exhausted",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentRequest {
    pub user_id: String,
    pub purpose: String,
    pub domain: String,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    #[serde(default)]
    pub ai_cloud_export: bool,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub retention_until: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivacyRequestInput {
    pub kind: String,
    pub user_id: String,
    pub purpose: String,
    pub domain: String,
    #[serde(default)]
    pub resource_id: Option<String>,
    #[serde(default)]
    pub destination: Option<String>,
    pub minimization: String,
    pub idempotency_key: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ConsentView {
    pub id: String,
    pub schema_version: i64,
    pub household_id: String,
    pub user_id: String,
    pub purpose: String,
    pub domain: String,
    pub resource_id: Option<String>,
    pub destination: String,
    pub ai_cloud_export: bool,
    pub status: String,
    pub expires_at: Option<i64>,
    pub retention_until: Option<i64>,
    pub withdrawn_at: Option<i64>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentListQuery {
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
}

fn parse_consent_list_query(uri: &Uri) -> Result<ConsentListQuery> {
    let mut query = ConsentListQuery {
        limit: None,
        cursor: None,
    };
    for pair in uri.query().unwrap_or_default().split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair
            .split_once('=')
            .ok_or_else(|| AppError::BadRequest("query parameter is invalid".into()))?;
        if !matches!(name, "limit" | "cursor") {
            return Err(AppError::BadRequest("query parameter is invalid".into()));
        }
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(AppError::BadRequest(format!("{name} is invalid")));
        }
        match name {
            "limit" if query.limit.is_none() => query.limit = Some(value.to_owned()),
            "cursor" if query.cursor.is_none() => query.cursor = Some(value.to_owned()),
            "limit" | "cursor" => {
                return Err(AppError::BadRequest(format!("{name} is invalid")));
            }
            _ => return Err(AppError::BadRequest(format!("{name} is invalid"))),
        }
    }
    Ok(query)
}

#[derive(Debug, Serialize)]
pub struct ConsentListEnvelope {
    pub schema_version: i64,
    pub consents: Vec<ConsentView>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct ConsentEnvelope {
    pub schema_version: i64,
    pub consent: ConsentView,
}

#[derive(Debug, Serialize)]
pub struct PrivacyRequestEnvelope {
    pub schema_version: i64,
    pub request: PrivacyRequestView,
    pub replayed: bool,
}

#[derive(Debug, Serialize)]
pub struct ConsentWithdrawalEnvelope {
    pub schema_version: i64,
    pub withdrawn: bool,
    pub consent_id: String,
}

#[derive(Debug, Serialize)]
pub struct PrivacyMutationEnvelope {
    pub schema_version: i64,
    pub request_id: String,
    pub cancelled: bool,
    pub cancel_requested: bool,
    pub queued: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct PrivacyRequestRow {
    id: String,
    schema_version: i64,
    household_id: String,
    user_id: String,
    kind: String,
    purpose: String,
    domain: String,
    resource_id: Option<String>,
    destination: String,
    minimization: String,
    idempotency_key: String,
    state: String,
    failure_code: Option<String>,
    correlation_id: String,
    requested_by: String,
    created_at: i64,
    updated_at: i64,
    completed_at: Option<i64>,
    artifact_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PrivacyRequestView {
    pub id: String,
    pub schema_version: i64,
    pub household_id: String,
    pub user_id: String,
    pub kind: String,
    pub purpose: String,
    pub domain: String,
    pub resource_id: Option<String>,
    pub destination: String,
    pub minimization: String,
    pub idempotency_key: String,
    pub state: String,
    pub failure: Option<PrivacyFailureView>,
    pub correlation_id: String,
    pub requested_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    pub artifact_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PrivacyFailureView {
    pub code: String,
    pub message: &'static str,
}

impl From<PrivacyRequestRow> for PrivacyRequestView {
    fn from(row: PrivacyRequestRow) -> Self {
        let failure = row.failure_code.map(|code| PrivacyFailureView {
            code: if PUBLIC_FAILURE_CODES.contains(&code.as_str())
                && code.chars().count() <= MAX_FAILURE_CODE_CHARS
            {
                code
            } else {
                "privacy_operation_failed".into()
            },
            message: "Privacy operation failed and requires operator review.",
        });
        Self {
            id: row.id,
            schema_version: row.schema_version,
            household_id: row.household_id,
            user_id: row.user_id,
            kind: row.kind,
            purpose: row.purpose,
            domain: row.domain,
            resource_id: row.resource_id,
            destination: row.destination,
            minimization: row.minimization,
            idempotency_key: row.idempotency_key,
            state: row.state,
            failure,
            correlation_id: row.correlation_id,
            requested_by: row.requested_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
            completed_at: row.completed_at,
            artifact_id: row.artifact_id,
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn validate_field(name: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.chars().count() > MAX_FIELD_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(AppError::BadRequest(format!("{name} is invalid")));
    }
    Ok(())
}

fn validate_id(name: &str, value: &str) -> Result<()> {
    validate_field(name, value)?;
    if !value.is_ascii() {
        return Err(AppError::BadRequest(format!("{name} is invalid")));
    }
    Ok(())
}

fn validate_expiry(
    expires_at: Option<i64>,
    retention_until: Option<i64>,
    timestamp: i64,
) -> Result<()> {
    if expires_at.is_some_and(|value| value <= timestamp)
        || retention_until.is_some_and(|value| value < timestamp)
        || matches!((expires_at, retention_until), (Some(expires), Some(retention)) if retention < expires)
    {
        return Err(AppError::BadRequest(
            "consent expiry and retention timestamps are invalid".into(),
        ));
    }
    Ok(())
}

fn parse_body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T> {
    serde_json::from_slice(body)
        .map_err(|_| AppError::BadRequest("invalid JSON request body".into()))
}

fn parse_privacy_body<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T> {
    let value: Value = parse_body(body)?;
    if value.get("destination").is_some_and(Value::is_null) {
        return Err(AppError::BadRequest("destination is invalid".into()));
    }
    serde_json::from_value(value)
        .map_err(|_| AppError::BadRequest("invalid JSON request body".into()))
}

fn valid_idempotency_key(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | ':' | '-')
        })
}

fn require_json_content_type(headers: &HeaderMap) -> Result<()> {
    let is_json = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Err(AppError::RequestBody {
            status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
        });
    }
    Ok(())
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

async fn authenticated_json_body(request: Request) -> Result<(HeaderMap, Bytes)> {
    let headers = request.headers().clone();
    let body = to_bytes(request.into_body(), MAX_BODY_BYTES)
        .await
        .map_err(|_| AppError::PayloadTooLarge)?;
    Ok((headers, body))
}

async fn require_active_member(
    state: &AppState,
    user_id: &str,
    household_id: &str,
) -> Result<String> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM household_memberships
         WHERE household_id = ? AND user_id = ? AND status = 'active'
           AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?)",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(now())
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    role.ok_or(AppError::Forbidden)
}

async fn is_manager(state: &AppState, user: &auth::User, household_id: &str) -> Result<bool> {
    let role = require_active_member(state, &user.id, household_id).await?;
    Ok(matches!(role.as_str(), "owner" | "admin")
        || matches!(user.role.as_str(), "owner" | "admin"))
}

async fn has_resource_permission_in_household(
    pool: &SqlitePool,
    household_id: &str,
    user_id: &str,
    role: &str,
    resource_id: &str,
    permission: GrantPermission,
) -> std::result::Result<bool, sqlx::Error> {
    if matches!(role, "owner" | "admin") {
        return Ok(true);
    }
    let now = now();
    let required_permission = match permission {
        GrantPermission::Read => 1,
        GrantPermission::Operate => 2,
        GrantPermission::Admin => 3,
    };
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM resource_grants g
             JOIN household_memberships m
               ON m.household_id = g.household_id AND m.user_id = g.user_id
             WHERE g.household_id = ? AND g.user_id = ? AND g.resource_id = ?
               AND m.status = 'active' AND m.revoked_at IS NULL
               AND (m.expires_at IS NULL OR m.expires_at > ?)
               AND g.revoked_at IS NULL
               AND (g.expires_at IS NULL OR g.expires_at > ?)
               AND CASE g.permission WHEN 'admin' THEN 3 WHEN 'operate' THEN 2
                    WHEN 'read' THEN 1 ELSE 0 END >= ?
         )",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(resource_id)
    .bind(now)
    .bind(now)
    .bind(required_permission)
    .fetch_one(pool)
    .await?;
    Ok(allowed != 0)
}

async fn authorize_subject_membership(
    state: &AppState,
    actor: &auth::User,
    household_id: &str,
    subject_id: &str,
) -> Result<String> {
    validate_id("user_id", subject_id)?;
    let subject_role = require_active_member(state, subject_id, household_id).await?;
    let manager = is_manager(state, actor, household_id).await?;
    if actor.id != subject_id && !manager {
        return Err(AppError::Forbidden);
    }
    Ok(subject_role)
}

async fn authorize_subject(
    state: &AppState,
    actor: &auth::User,
    household_id: &str,
    subject_id: &str,
    resource_id: Option<&str>,
) -> Result<()> {
    let subject_role = authorize_subject_membership(state, actor, household_id, subject_id).await?;
    if let Some(resource_id) = resource_id {
        validate_id("resource_id", resource_id)?;
        if !has_resource_permission_in_household(
            &state.db,
            household_id,
            subject_id,
            &subject_role,
            resource_id,
            GrantPermission::Read,
        )
        .await
        .map_err(|error| AppError::Internal(error.into()))?
        {
            return Err(AppError::ResourceGrantRequired);
        }
    }
    Ok(())
}

async fn append_lifecycle_records(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    actor: &auth::User,
    action: &str,
    resource_id: Option<&str>,
    correlation_id: &str,
    payload: serde_json::Value,
) -> Result<()> {
    audit::append(
        tx,
        PendingAudit {
            user_id: Some(&actor.id),
            actor_type: "human",
            action,
            resource_type: Some("household_privacy"),
            resource_id,
            outcome: "success",
            ip_address: None,
            request_id: Some(correlation_id),
            details: Some("bounded household privacy lifecycle change"),
            source: Some("household_privacy_api"),
        },
    )
    .await
    .map_err(AppError::Internal)?;
    events::append(
        tx,
        PendingEvent {
            event_type: format!("{action}.v1"),
            // Durable events are a bounded invalidation/history channel. Keep the authorized
            // human linkage in the audit row above, but never persist identity material here.
            actor: None,
            resource_id: None,
            job_id: None,
            approval_id: None,
            correlation_id: correlation_id.to_owned(),
            causation_id: None,
            payload,
        },
    )
    .await
    .map_err(AppError::Internal)?;
    Ok(())
}

fn parse_list_bound(
    name: &str,
    raw: Option<&str>,
    default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64> {
    let value = raw
        .map(|value| value.parse::<i64>())
        .transpose()
        .map_err(|_| AppError::BadRequest(format!("{name} is invalid")))?
        .unwrap_or(default);
    if !(minimum..=maximum).contains(&value) {
        return Err(AppError::BadRequest(format!("{name} is invalid")));
    }
    Ok(value)
}

pub async fn list_consents(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
    uri: Uri,
) -> Result<Json<ConsentListEnvelope>> {
    let actor = require_user(&state, &jar).await?;
    validate_id("household_id", &household_id)?;
    if !is_manager(&state, &actor, &household_id).await? {
        return Err(AppError::Forbidden);
    }
    let query = parse_consent_list_query(&uri)?;
    let limit = parse_list_bound(
        "limit",
        query.limit.as_deref(),
        MAX_LIST_ITEMS,
        1,
        MAX_LIST_ITEMS,
    )?;
    let cursor = parse_list_bound("cursor", query.cursor.as_deref(), 0, 0, MAX_LIST_CURSOR)?;

    let mut consents: Vec<ConsentView> = sqlx::query_as(
        "SELECT id, schema_version, household_id, user_id, purpose, domain, resource_id, destination,
                ai_cloud_export, status, expires_at, retention_until, withdrawn_at,
                created_by, created_at, updated_at
         FROM household_consents WHERE household_id = ?
         ORDER BY created_at DESC, id LIMIT ? OFFSET ?",
    )
    .bind(&household_id)
    .bind(limit + 1)
    .bind(cursor)
    .fetch_all(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    let has_more = consents.len() > limit as usize;
    if has_more {
        consents.pop();
    }
    let next_cursor = if has_more {
        let next = cursor + limit;
        if next > MAX_LIST_CURSOR {
            return Err(AppError::BadRequest("cursor is invalid".into()));
        }
        Some(next.to_string())
    } else {
        None
    };
    Ok(Json(ConsentListEnvelope {
        schema_version: PRIVACY_SCHEMA_VERSION,
        consents,
        next_cursor,
        has_more,
    }))
}

pub async fn create_consent(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
    request: Request,
) -> Result<Json<ConsentEnvelope>> {
    let actor = require_user(&state, &jar).await?;
    let (headers, body) = authenticated_json_body(request).await?;
    require_json_content_type(&headers)?;
    let request: ConsentRequest = parse_privacy_body(&body)?;
    validate_id("household_id", &household_id)?;
    validate_id("user_id", &request.user_id)?;
    validate_field("purpose", &request.purpose)?;
    validate_field("domain", &request.domain)?;
    let purpose_allowed = matches!(
        request.purpose.as_str(),
        "privacy_export" | "ai_assistance" | "cloud_export" | "support"
    );
    let domain_allowed = matches!(
        request.domain.as_str(),
        "identity" | "inventory" | "telemetry" | "configuration"
    );
    if !purpose_allowed || !domain_allowed {
        return Err(AppError::BadRequest("purpose or domain is invalid".into()));
    }
    if let Some(resource_id) = request.resource_id.as_deref() {
        validate_id("resource_id", resource_id)?;
    }
    let destination = request.destination.as_deref().unwrap_or("local");
    if !matches!(destination, "local" | "cloud") {
        return Err(AppError::BadRequest("destination is invalid".into()));
    }
    if destination == "cloud" && !request.ai_cloud_export {
        return Err(AppError::BadRequest(
            "cloud destination requires ai_cloud_export=true".into(),
        ));
    }
    let timestamp = now();
    validate_expiry(request.expires_at, request.retention_until, timestamp)?;
    authorize_subject(
        &state,
        &actor,
        &household_id,
        &request.user_id,
        request.resource_id.as_deref(),
    )
    .await?;

    let id = Uuid::new_v4().to_string();
    let correlation_id = Uuid::new_v4().to_string();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    sqlx::query(
        "INSERT INTO household_consents
         (id, schema_version, household_id, user_id, purpose, domain, resource_id, destination, ai_cloud_export,
          expires_at, retention_until, created_by, created_at, updated_at)
         VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&household_id)
    .bind(&request.user_id)
    .bind(&request.purpose)
    .bind(&request.domain)
    .bind(request.resource_id.as_deref())
    .bind(destination)
    .bind(request.ai_cloud_export)
    .bind(request.expires_at)
    .bind(request.retention_until)
    .bind(&actor.id)
    .bind(timestamp)
    .bind(timestamp)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    append_lifecycle_records(
        &mut tx,
        &actor,
        "consent.granted",
        request.resource_id.as_deref(),
        &correlation_id,
        json!({"consent_id": id, "purpose": request.purpose, "domain": request.domain, "destination": destination,
               "ai_cloud_export": request.ai_cloud_export}),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let consent: ConsentView = sqlx::query_as(
        "SELECT id, schema_version, household_id, user_id, purpose, domain, resource_id, destination,
                ai_cloud_export, status, expires_at, retention_until, withdrawn_at,
                created_by, created_at, updated_at FROM household_consents WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    Ok(Json(ConsentEnvelope {
        schema_version: PRIVACY_SCHEMA_VERSION,
        consent,
    }))
}

pub async fn withdraw_consent(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, consent_id)): Path<(String, String)>,
) -> Result<Json<ConsentWithdrawalEnvelope>> {
    let actor = require_user(&state, &jar).await?;
    validate_id("household_id", &household_id)?;
    validate_id("consent_id", &consent_id)?;
    let consent: (String, Option<String>) = sqlx::query_as(
        "SELECT user_id, resource_id FROM household_consents WHERE id = ? AND household_id = ? AND status = 'active'",
    )
    .bind(&consent_id)
    .bind(&household_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.into()))?
    .ok_or(AppError::NotFound)?;
    authorize_subject(&state, &actor, &household_id, &consent.0, None).await?;
    let timestamp = now();
    let correlation_id = Uuid::new_v4().to_string();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let updated = sqlx::query(
        "UPDATE household_consents SET status = 'withdrawn', withdrawn_at = ?, updated_at = ? WHERE id = ? AND status = 'active'",
    )
    .bind(timestamp)
    .bind(timestamp)
    .bind(&consent_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    if updated.rows_affected() != 1 {
        tx.rollback()
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
        return Err(AppError::Conflict("consent state changed".into()));
    }
    append_lifecycle_records(
        &mut tx,
        &actor,
        "consent.withdrawn",
        consent.1.as_deref(),
        &correlation_id,
        json!({"consent_id": consent_id}),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    Ok(Json(ConsentWithdrawalEnvelope {
        schema_version: PRIVACY_SCHEMA_VERSION,
        withdrawn: true,
        consent_id,
    }))
}

async fn active_consent(
    pool: &SqlitePool,
    household_id: &str,
    user_id: &str,
    purpose: &str,
    domain: &str,
    resource_id: Option<&str>,
    destination: &str,
) -> Result<bool> {
    let allowed: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM household_consents
           WHERE household_id = ? AND user_id = ? AND purpose = ? AND domain = ?
             AND status = 'active'
             AND (expires_at IS NULL OR expires_at > ?)
             AND (retention_until IS NULL OR retention_until > ?)
             AND (resource_id IS NULL OR resource_id = ?)
             AND ((? = 'local' AND destination = 'local')
                  OR (? = 'cloud' AND destination = 'cloud' AND ai_cloud_export = 1))
         )",
    )
    .bind(household_id)
    .bind(user_id)
    .bind(purpose)
    .bind(domain)
    .bind(now())
    .bind(now())
    .bind(resource_id)
    .bind(destination)
    .bind(destination)
    .fetch_one(pool)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    Ok(allowed != 0)
}

async fn privacy_request_row(
    pool: &SqlitePool,
    request_id: &str,
    household_id: &str,
) -> Result<PrivacyRequestRow> {
    sqlx::query_as(
        "SELECT id, schema_version, household_id, user_id, kind, purpose, domain, resource_id, destination,
                minimization, idempotency_key, state, failure_code, correlation_id, requested_by,
                created_at, updated_at, completed_at, artifact_id
         FROM privacy_requests WHERE id = ? AND household_id = ?",
    )
    .bind(request_id)
    .bind(household_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| AppError::Internal(error.into()))?
    .ok_or(AppError::NotFound)
}

fn ensure_same_intent(
    existing: &PrivacyRequestRow,
    request: &PrivacyRequestInput,
    destination: &str,
) -> Result<()> {
    if existing.purpose != request.purpose
        || existing.domain != request.domain
        || existing.resource_id.as_deref() != request.resource_id.as_deref()
        || existing.destination != destination
        || existing.minimization != request.minimization
    {
        return Err(AppError::Conflict(
            "idempotency key is already bound to a different privacy request".into(),
        ));
    }
    Ok(())
}

pub async fn create_privacy_request(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(household_id): Path<String>,
    request: Request,
) -> Result<(StatusCode, Json<PrivacyRequestEnvelope>)> {
    let actor = require_user(&state, &jar).await?;
    let (headers, body) = authenticated_json_body(request).await?;
    require_json_content_type(&headers)?;
    let request: PrivacyRequestInput = parse_privacy_body(&body)?;
    validate_id("household_id", &household_id)?;
    validate_id("user_id", &request.user_id)?;
    validate_field("kind", &request.kind)?;
    validate_field("purpose", &request.purpose)?;
    validate_field("domain", &request.domain)?;
    validate_field("minimization", &request.minimization)?;
    validate_field("idempotency_key", &request.idempotency_key)?;
    if request.idempotency_key.chars().count() > 128
        || !valid_idempotency_key(&request.idempotency_key)
    {
        return Err(AppError::BadRequest("idempotency_key is invalid".into()));
    }
    if !matches!(request.kind.as_str(), "export" | "deletion")
        || !matches!(
            request.domain.as_str(),
            "identity" | "inventory" | "telemetry" | "configuration"
        )
        || !matches!(request.minimization.as_str(), "metadata" | "records")
    {
        return Err(AppError::BadRequest(
            "privacy request field is invalid".into(),
        ));
    }
    let expected_purpose = if request.kind == "export" {
        "privacy_export"
    } else {
        "privacy_deletion"
    };
    if request.purpose != expected_purpose {
        return Err(AppError::BadRequest(
            "purpose does not match request kind".into(),
        ));
    }
    if let Some(resource_id) = request.resource_id.as_deref() {
        validate_id("resource_id", resource_id)?;
        if request.kind == "deletion" && request.domain != "inventory" {
            return Err(AppError::BadRequest(
                "resource-scoped deletion is supported only for inventory".into(),
            ));
        }
    }
    let destination = request.destination.as_deref().unwrap_or("local");
    if !matches!(destination, "local" | "cloud") {
        return Err(AppError::BadRequest("destination is invalid".into()));
    }
    if destination == "cloud" && request.kind != "export" {
        return Err(AppError::BadRequest(
            "deletion destination must remain local".into(),
        ));
    }
    authorize_subject_membership(&state, &actor, &household_id, &request.user_id).await?;
    if let Some(existing) = sqlx::query_as::<_, PrivacyRequestRow>(
        "SELECT id, schema_version, household_id, user_id, kind, purpose, domain, resource_id, destination,
                minimization, idempotency_key, state, failure_code, correlation_id, requested_by,
                created_at, updated_at, completed_at, artifact_id
         FROM privacy_requests WHERE household_id = ? AND user_id = ? AND kind = ? AND idempotency_key = ?",
    )
    .bind(&household_id)
    .bind(&request.user_id)
    .bind(&request.kind)
    .bind(&request.idempotency_key)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.into()))?
    {
        ensure_same_intent(&existing, &request, destination)?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(PrivacyRequestEnvelope {
                schema_version: PRIVACY_SCHEMA_VERSION,
                request: PrivacyRequestView::from(existing),
                replayed: true,
            }),
        ));
    }

    authorize_subject(
        &state,
        &actor,
        &household_id,
        &request.user_id,
        request.resource_id.as_deref(),
    )
    .await?;

    if request.kind == "export"
        && !active_consent(
            &state.db,
            &household_id,
            &request.user_id,
            &request.purpose,
            &request.domain,
            request.resource_id.as_deref(),
            destination,
        )
        .await?
    {
        if destination == "cloud" {
            return Err(AppError::CloudExportDisabled);
        }
        return Err(AppError::ConsentRequired);
    }

    let id = Uuid::new_v4().to_string();
    let correlation_id = Uuid::new_v4().to_string();
    let timestamp = now();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let inserted = sqlx::query(
        "INSERT INTO privacy_requests
         (id, schema_version, household_id, user_id, kind, purpose, domain, resource_id, destination, minimization,
          idempotency_key, correlation_id, requested_by, created_at, updated_at)
         VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (household_id, user_id, kind, idempotency_key) DO NOTHING",
    )
    .bind(&id)
    .bind(&household_id)
    .bind(&request.user_id)
    .bind(&request.kind)
    .bind(&request.purpose)
    .bind(&request.domain)
    .bind(request.resource_id.as_deref())
    .bind(destination)
    .bind(&request.minimization)
    .bind(&request.idempotency_key)
    .bind(&correlation_id)
    .bind(&actor.id)
    .bind(timestamp)
    .bind(timestamp)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(error.into()))?
    .rows_affected();
    if inserted == 0 {
        tx.rollback()
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
        let existing = sqlx::query_as::<_, PrivacyRequestRow>(
            "SELECT id, schema_version, household_id, user_id, kind, purpose, domain, resource_id, destination,
                    minimization, idempotency_key, state, failure_code, correlation_id, requested_by,
                    created_at, updated_at, completed_at, artifact_id
             FROM privacy_requests WHERE household_id = ? AND user_id = ? AND kind = ? AND idempotency_key = ?",
        )
        .bind(&household_id)
        .bind(&request.user_id)
        .bind(&request.kind)
        .bind(&request.idempotency_key)
        .fetch_one(&state.db)
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
        ensure_same_intent(&existing, &request, destination)?;
        return Ok((
            StatusCode::ACCEPTED,
            Json(PrivacyRequestEnvelope {
                schema_version: PRIVACY_SCHEMA_VERSION,
                request: PrivacyRequestView::from(existing),
                replayed: true,
            }),
        ));
    }
    append_lifecycle_records(
        &mut tx,
        &actor,
        "privacy.requested",
        request.resource_id.as_deref(),
        &correlation_id,
        json!({"request_id": id,
               "kind": request.kind, "purpose": request.purpose, "domain": request.domain,
               "destination": destination, "minimization": request.minimization}),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let row = privacy_request_row(&state.db, &id, &household_id).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(PrivacyRequestEnvelope {
            schema_version: PRIVACY_SCHEMA_VERSION,
            request: PrivacyRequestView::from(row),
            replayed: false,
        }),
    ))
}

pub async fn get_privacy_request(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, request_id)): Path<(String, String)>,
) -> Result<Json<PrivacyRequestEnvelope>> {
    let actor = require_user(&state, &jar).await?;
    validate_id("household_id", &household_id)?;
    validate_id("request_id", &request_id)?;
    let row = privacy_request_row(&state.db, &request_id, &household_id).await?;
    authorize_subject(&state, &actor, &household_id, &row.user_id, None).await?;
    Ok(Json(PrivacyRequestEnvelope {
        schema_version: PRIVACY_SCHEMA_VERSION,
        request: PrivacyRequestView::from(row),
        replayed: false,
    }))
}

pub async fn cancel_privacy_request(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, request_id)): Path<(String, String)>,
) -> Result<Json<PrivacyMutationEnvelope>> {
    let actor = require_user(&state, &jar).await?;
    validate_id("household_id", &household_id)?;
    validate_id("request_id", &request_id)?;
    let row = privacy_request_row(&state.db, &request_id, &household_id).await?;
    authorize_subject(&state, &actor, &household_id, &row.user_id, None).await?;
    if row.state == "cancelled" {
        return Ok(Json(PrivacyMutationEnvelope {
            schema_version: PRIVACY_SCHEMA_VERSION,
            request_id,
            cancelled: true,
            cancel_requested: false,
            queued: false,
        }));
    }
    if !matches!(row.state.as_str(), "queued" | "processing") {
        return Err(AppError::Conflict(
            "privacy request is already terminal".into(),
        ));
    }
    let timestamp = now();
    let correlation_id = Uuid::new_v4().to_string();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let updated = sqlx::query(
        "UPDATE privacy_requests SET state = CASE WHEN state = 'queued' THEN 'cancelled' ELSE state END,
         cancel_requested = 1, completed_at = CASE WHEN state = 'queued' THEN ? ELSE completed_at END,
         updated_at = ? WHERE id = ? AND state IN ('queued', 'processing')",
    )
    .bind(timestamp)
    .bind(timestamp)
    .bind(&request_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    if updated.rows_affected() != 1 {
        return Err(AppError::Conflict("privacy request changed state".into()));
    }
    append_lifecycle_records(
        &mut tx,
        &actor,
        if row.state == "queued" {
            "privacy.cancelled"
        } else {
            "privacy.cancel_requested"
        },
        None,
        &correlation_id,
        json!({ "request_id": request_id }),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    Ok(Json(PrivacyMutationEnvelope {
        schema_version: PRIVACY_SCHEMA_VERSION,
        request_id,
        cancelled: row.state == "queued",
        cancel_requested: row.state == "processing",
        queued: false,
    }))
}

pub async fn retry_privacy_request(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, request_id)): Path<(String, String)>,
) -> Result<Json<PrivacyMutationEnvelope>> {
    let actor = require_user(&state, &jar).await?;
    validate_id("household_id", &household_id)?;
    validate_id("request_id", &request_id)?;
    let row = privacy_request_row(&state.db, &request_id, &household_id).await?;
    authorize_subject(&state, &actor, &household_id, &row.user_id, None).await?;
    if !matches!(row.state.as_str(), "failed" | "cancelled") {
        return Err(AppError::Conflict(
            "privacy request is not retryable".into(),
        ));
    }
    let timestamp = now();
    let correlation_id = Uuid::new_v4().to_string();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    let updated = sqlx::query(
        "UPDATE privacy_requests SET state = 'queued', failure_code = NULL,
         completed_at = NULL, cancel_requested = 0, lease_owner = NULL,
         lease_expires_at = NULL, updated_at = ?
         WHERE id = ? AND state IN ('failed', 'cancelled') AND attempt_count < ?",
    )
    .bind(timestamp)
    .bind(&request_id)
    .bind(MAX_PRIVACY_ATTEMPTS)
    .execute(&mut *tx)
    .await
    .map_err(|error| AppError::Internal(error.into()))?;
    if updated.rows_affected() != 1 {
        tx.rollback()
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
        return Err(AppError::Conflict("privacy request changed state".into()));
    }
    append_lifecycle_records(
        &mut tx,
        &actor,
        "privacy.retry_requested",
        None,
        &correlation_id,
        json!({ "request_id": request_id }),
    )
    .await?;
    tx.commit()
        .await
        .map_err(|error| AppError::Internal(error.into()))?;
    Ok(Json(PrivacyMutationEnvelope {
        schema_version: PRIVACY_SCHEMA_VERSION,
        request_id,
        cancelled: false,
        cancel_requested: false,
        queued: true,
    }))
}

pub async fn get_privacy_artifact(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((household_id, request_id)): Path<(String, String)>,
) -> Result<Response> {
    let actor = require_user(&state, &jar).await?;
    validate_id("household_id", &household_id)?;
    validate_id("request_id", &request_id)?;
    let row = privacy_request_row(&state.db, &request_id, &household_id).await?;
    authorize_subject(&state, &actor, &household_id, &row.user_id, None).await?;
    if row.state != "succeeded" {
        return Err(AppError::NotFound);
    }
    let Some((artifact_id, relative_path, byte_length, sha256)): Option<(
        String,
        String,
        i64,
        String,
    )> = sqlx::query_as(
        "SELECT id, relative_path, byte_length, sha256 FROM privacy_artifacts WHERE request_id = ?",
    )
    .bind(&request_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| AppError::Internal(error.into()))?
    else {
        return Err(AppError::NotFound);
    };
    let expected_length = usize::try_from(byte_length).map_err(|_| AppError::NotFound)?;
    if row.artifact_id.as_deref() != Some(artifact_id.as_str())
        || expected_length > MAX_ARTIFACT_BYTES
    {
        return Err(AppError::NotFound);
    }
    let expected_path = format!("privacy/exports/{artifact_id}.json");
    if relative_path != expected_path {
        return Err(AppError::NotFound);
    }
    let mut bytes = Vec::with_capacity(expected_length);
    tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(state.config.data_dir.join(&relative_path))
        .await
        .map_err(|_| AppError::NotFound)?
        .take(
            u64::try_from(expected_length)
                .unwrap_or(0)
                .saturating_add(1),
        )
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| AppError::NotFound)?;
    if bytes.len() != expected_length || hex::encode(Sha256::digest(&bytes)) != sha256 {
        return Err(AppError::NotFound);
    }
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    response.headers_mut().insert(
        axum::http::header::CONTENT_DISPOSITION,
        axum::http::HeaderValue::from_str(&format!("attachment; filename=\"{request_id}.json\""))
            .map_err(|_| AppError::NotFound)?,
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn privacy_failure_messages_are_bounded_and_non_diagnostic() {
        const {
            assert!(MAX_FAILURE_CODE_CHARS < MAX_FIELD_CHARS);
        }
        assert_eq!(
            PrivacyFailureView {
                code: "x".into(),
                message: "Privacy operation failed and requires operator review."
            }
            .message
            .len(),
            54
        );
    }
}
