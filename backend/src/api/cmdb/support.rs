use crate::{
    api::role_guard,
    auth,
    cmdb::assets::{self, MutationContext},
    error::{AppError, Result},
    operations::contracts::{ActorRef, ActorType},
    AppState,
};
use axum::{
    body::Bytes,
    extract::{rejection::BytesRejection, Query},
    http::{StatusCode, Uri},
    response::IntoResponse,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{de::DeserializeOwned, Deserialize, Deserializer};

pub const MAX_LIST_LIMIT: i64 = 200;
pub const MAX_BODY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy)]
pub enum Access {
    Read,
    Write,
}

#[derive(Debug)]
pub struct Nullable<T>(pub Option<Option<T>>);

impl<T> Default for Nullable<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<'de, T> Deserialize<'de> for Nullable<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| Self(Some(value)))
    }
}

pub async fn require_user(state: &AppState, jar: &CookieJar, access: Access) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    match access {
        Access::Read => role_guard::require_operator(&user)?,
        Access::Write => role_guard::require_admin(&user)?,
    }
    Ok(user)
}

pub fn mutation_context(user: &auth::User) -> MutationContext {
    MutationContext {
        actor: ActorRef {
            actor_type: ActorType::Human,
            id: Some(user.id.clone()),
            source: Some("cmdb_api".into()),
        },
        correlation_id: uuid::Uuid::new_v4().to_string(),
    }
}

pub fn validate_page(limit: i64, offset: i64) -> Result<()> {
    if !(1..=MAX_LIST_LIMIT).contains(&limit) || offset < 0 {
        return Err(AppError::BadRequest(
            "limit must be 1..200 and offset must be non-negative".into(),
        ));
    }
    Ok(())
}

pub fn parse_query<T: DeserializeOwned>(uri: &Uri) -> Result<T> {
    Query::<T>::try_from_uri(uri)
        .map(|Query(query)| query)
        .map_err(|_| AppError::BadRequest("invalid query parameters".into()))
}

pub fn parse_json<T: DeserializeOwned>(
    body: std::result::Result<Bytes, BytesRejection>,
) -> Result<T> {
    let body = match body {
        Ok(body) => body,
        Err(error) => {
            if error.into_response().status() == StatusCode::PAYLOAD_TOO_LARGE {
                return Err(AppError::PayloadTooLarge);
            }
            return Err(AppError::BadRequest(
                "request body could not be read".into(),
            ));
        }
    };
    if body.is_empty() {
        return Err(AppError::BadRequest("request body is required".into()));
    }
    serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("invalid JSON request body".into()))
}

pub async fn resolve_asset(
    state: &AppState,
    selector: &str,
) -> Result<crate::cmdb::contracts::AssetRecord> {
    if selector.trim().is_empty() || selector.chars().count() > 128 {
        return Err(AppError::BadRequest("invalid asset selector".into()));
    }
    assets::get(&state.db, selector)
        .await
        .map_err(|error| {
            tracing::error!("CMDB asset selector resolution failed: {error:#}");
            AppError::Internal(anyhow::anyhow!("CMDB asset selector resolution failed"))
        })?
        .ok_or(AppError::NotFound)
}
