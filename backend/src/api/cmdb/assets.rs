use crate::{
    auth,
    cmdb::{assets as service, contracts::AssetRecord},
    error::{AppError, Result},
    operations::contracts::{ActorRef, ActorType},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;
use serde_json::Value;

const MAX_LIMIT: i64 = 200;

async fn user(state: &AppState, jar: &CookieJar, admin: bool) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    let u = auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    let allowed = if admin {
        matches!(u.role.as_str(), "owner" | "admin")
    } else {
        matches!(u.role.as_str(), "owner" | "admin" | "operator")
    };
    if !allowed {
        return Err(AppError::Forbidden);
    }
    Ok(u)
}

fn context(user: &auth::User) -> service::MutationContext {
    MutationContextBuilder::build(user)
}
struct MutationContextBuilder;
impl MutationContextBuilder {
    fn build(user: &auth::User) -> service::MutationContext {
        service::MutationContext {
            actor: ActorRef {
                actor_type: ActorType::Human,
                id: Some(user.id.clone()),
                source: Some("cmdb_api".into()),
            },
            correlation_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

fn domain_error(error: anyhow::Error) -> AppError {
    let message = error.to_string();
    if message.contains("not found") || message.contains("does not exist") {
        AppError::NotFound
    } else if message.contains("conflict")
        || message.contains("already exists")
        || message.contains("UNIQUE")
    {
        AppError::Conflict("CMDB request conflicts with current state".into())
    } else {
        AppError::BadRequest(message)
    }
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}
fn default_limit() -> i64 {
    100
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>> {
    user(&state, &jar, false).await?;
    if q.limit < 1 || q.limit > MAX_LIMIT || q.offset < 0 {
        return Err(AppError::BadRequest(
            "limit must be 1..200 and offset must be non-negative".into(),
        ));
    }
    let rows = service::list(&state.db, q.limit, q.offset)
        .await
        .map_err(domain_error)?;
    Ok(Json(
        serde_json::json!({"assets": rows, "limit": q.limit, "offset": q.offset}),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
) -> Result<Json<AssetRecord>> {
    user(&state, &jar, false).await?;
    let record = service::get(&state.db, &selector)
        .await
        .map_err(domain_error)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub class_key: String,
    pub type_key: String,
    pub name: String,
    #[serde(default)]
    pub friendly_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial_number: Option<String>,
    #[serde(default)]
    pub part_number: Option<String>,
    #[serde(default)]
    pub location_id: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub notes: String,
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    req: Option<Json<CreateRequest>>,
) -> Result<Json<AssetRecord>> {
    let u = user(&state, &jar, true).await?;
    let req = req
        .ok_or(AppError::BadRequest("request body is required".into()))?
        .0;
    let input = service::CreateAssetInput {
        class_key: req.class_key,
        type_key: req.type_key,
        name: req.name,
        friendly_name: req.friendly_name,
        description: req.description,
        manufacturer: req.manufacturer,
        model: req.model,
        serial_number: req.serial_number,
        part_number: req.part_number,
        location_id: req.location_id,
        metadata: req.metadata,
        notes: req.notes,
    };
    let record = service::create_manual(&state.db, input, context(&u))
        .await
        .map_err(domain_error)?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub asset_id: String,
    pub expected_revision: i64,
}
pub async fn rename(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    req: Option<Json<RenameRequest>>,
) -> Result<Json<AssetRecord>> {
    let u = user(&state, &jar, true).await?;
    let req = req
        .ok_or(AppError::BadRequest("request body is required".into()))?
        .0;
    if req.expected_revision < 0 {
        return Err(AppError::BadRequest(
            "expected_revision must be non-negative".into(),
        ));
    }
    let record = service::rename(
        &state.db,
        &selector,
        &req.asset_id,
        req.expected_revision,
        context(&u),
    )
    .await
    .map_err(domain_error)?;
    Ok(Json(record))
}

#[derive(Deserialize)]
pub struct RetirementRequest {
    pub retired: bool,
    pub expected_revision: i64,
}
pub async fn retirement(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(selector): Path<String>,
    req: Option<Json<RetirementRequest>>,
) -> Result<Json<AssetRecord>> {
    let u = user(&state, &jar, true).await?;
    let req = req
        .ok_or(AppError::BadRequest("request body is required".into()))?
        .0;
    let record = service::set_retired(
        &state.db,
        &selector,
        req.retired,
        req.expected_revision,
        context(&u),
    )
    .await
    .map_err(domain_error)?;
    Ok(Json(record))
}
