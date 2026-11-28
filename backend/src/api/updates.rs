use crate::{
    auth,
    error::{AppError, Result},
    operations::{
        invocation::{CredentialContext, PreparedInvocation},
        update_adoption,
    },
    updates::{self as update_provider, UpdateSnapshot, UpdateTarget},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use super::operation_adoption::{self, CompatibilityResult};

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

fn provider_error(error: anyhow::Error) -> AppError {
    AppError::Internal(error)
}

// ─── VoidTower updates ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}

#[derive(Serialize)]
pub struct VtUpdateInfo {
    pub mode: String,
    pub current_commit: String,
    pub remote_commit: String,
    pub behind: usize,
    pub ahead: usize,
    pub commits: Vec<CommitInfo>,
    pub backup_tags: Vec<String>,
    pub fetch_error: Option<String>,
    pub current_image: Option<String>,
    pub update_status: Option<String>,
    pub update_detail: Option<String>,
}

pub async fn vt_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<VtUpdateInfo>> {
    require_admin(&state, &jar).await?;
    let snapshot = update_provider::snapshot(&UpdateTarget::VoidTower)
        .await
        .map_err(provider_error)?;
    Ok(Json(match snapshot {
        UpdateSnapshot::VoidTowerGit(snapshot) => VtUpdateInfo {
            mode: "git".into(),
            current_commit: short(&snapshot.current_commit),
            remote_commit: short(&snapshot.remote_commit),
            behind: snapshot.behind,
            ahead: snapshot.ahead,
            commits: vec![],
            backup_tags: snapshot.backup_tags,
            fetch_error: None,
            current_image: None,
            update_status: None,
            update_detail: None,
        },
        UpdateSnapshot::VoidTowerBinary {
            current_version,
            remote_version,
        } => VtUpdateInfo {
            mode: "git".into(),
            behind: usize::from(remote_version != "unknown" && remote_version != current_version),
            current_commit: current_version,
            remote_commit: remote_version,
            ahead: 0,
            commits: vec![],
            backup_tags: vec![],
            fetch_error: None,
            current_image: None,
            update_status: None,
            update_detail: None,
        },
        UpdateSnapshot::VoidTowerDocker(snapshot) => VtUpdateInfo {
            mode: "docker".into(),
            current_commit: String::new(),
            remote_commit: String::new(),
            behind: 0,
            ahead: 0,
            commits: vec![],
            backup_tags: vec![],
            fetch_error: None,
            current_image: Some(snapshot.image),
            update_status: Some(image_status(
                &snapshot.container_image_id,
                &snapshot.local_image_id,
            )),
            update_detail: image_detail(&snapshot.container_image_id, &snapshot.local_image_id),
        },
        _ => {
            return Err(AppError::Internal(anyhow::anyhow!(
                "invalid VoidTower snapshot"
            )))
        }
    }))
}

pub async fn check_vt(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_action(
        &state,
        &jar,
        "update.voidtower.check",
        None,
        serde_json::json!({}),
        &headers,
    )
    .await
}

#[derive(Deserialize, Default)]
pub struct ApplyReq {
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn apply_vt(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<ApplyReq>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        "update.voidtower.apply",
        None,
        serde_json::json!({}),
        req.dry_run,
        &headers,
    )
    .await
}

#[derive(Deserialize)]
pub struct RollbackReq {
    pub tag: String,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn rollback_vt(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<RollbackReq>,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(&state, &jar, None).await?;
    update_adoption::authorize(&credential, "update.voidtower.rollback")?;
    update_provider::validate_backup_tag(&req.tag)
        .map_err(|_| AppError::BadRequest("Invalid backup tag".into()))?;
    prepare_or_submit_with_credential(
        &state,
        &credential,
        "update.voidtower.rollback",
        None,
        serde_json::json!({"tag": req.tag}),
        req.dry_run,
        &headers,
    )
    .await
}

// ─── Docker image updates ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct DockerImageRow {
    pub container_id: String,
    pub container_name: String,
    pub image: String,
    pub status: String,
    pub detail: Option<String>,
}

pub async fn docker_info(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Vec<DockerImageRow>>> {
    require_admin(&state, &jar).await?;
    let UpdateSnapshot::DockerEngine { containers } =
        update_provider::snapshot(&UpdateTarget::DockerEngine)
            .await
            .map_err(provider_error)?
    else {
        return Err(AppError::Internal(anyhow::anyhow!(
            "invalid Docker update snapshot"
        )));
    };
    Ok(Json(
        containers
            .into_iter()
            .map(|container| DockerImageRow {
                container_id: container.container_id,
                container_name: container.container_name,
                image: container.image,
                status: image_status(&container.container_image_id, &container.local_image_id),
                detail: image_detail(&container.container_image_id, &container.local_image_id),
            })
            .collect(),
    ))
}

pub async fn docker_check(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_action(
        &state,
        &jar,
        "update.docker.check",
        None,
        serde_json::json!({}),
        &headers,
    )
    .await
}

pub async fn docker_apply(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(container_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ApplyReq>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        "update.docker.apply",
        Some(&container_id),
        serde_json::json!({}),
        req.dry_run,
        &headers,
    )
    .await
}

// ─── Odysseus bare-metal updates ─────────────────────────────────────────────

#[derive(Serialize)]
pub struct OdyInfo {
    pub installed: bool,
    pub mode: String,
    pub current_commit: String,
    pub remote_commit: String,
    pub behind: usize,
    pub ahead: usize,
    pub fetch_error: Option<String>,
}

pub async fn odysseus_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<OdyInfo>> {
    require_admin(&state, &jar).await?;
    let UpdateSnapshot::Odysseus(snapshot) = update_provider::snapshot(&UpdateTarget::Odysseus)
        .await
        .map_err(provider_error)?
    else {
        return Err(AppError::Internal(anyhow::anyhow!(
            "invalid Odysseus snapshot"
        )));
    };
    Ok(Json(OdyInfo {
        installed: snapshot.installed,
        mode: if snapshot.installed { "git" } else { "none" }.into(),
        current_commit: short(&snapshot.current_commit),
        remote_commit: short(&snapshot.remote_commit),
        behind: snapshot.behind,
        ahead: snapshot.ahead,
        fetch_error: None,
    }))
}

pub async fn apply_odysseus(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    submit_action(
        &state,
        &jar,
        "update.odysseus.apply",
        None,
        serde_json::json!({}),
        &headers,
    )
    .await
}

// ─── OS package updates ───────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct OsUpdateInfo {
    pub package_manager: String,
    pub available: bool,
    pub count: usize,
    pub packages: Vec<String>,
    pub error: Option<String>,
}

pub async fn os_info(State(state): State<AppState>, jar: CookieJar) -> Result<Json<OsUpdateInfo>> {
    require_admin(&state, &jar).await?;
    match update_provider::snapshot(&UpdateTarget::OperatingSystem).await {
        Ok(UpdateSnapshot::OperatingSystem {
            package_manager,
            packages,
        }) => Ok(Json(OsUpdateInfo {
            package_manager,
            available: !packages.is_empty(),
            count: packages.len(),
            packages,
            error: None,
        })),
        Ok(_) => Err(AppError::Internal(anyhow::anyhow!(
            "invalid operating-system update snapshot"
        ))),
        Err(error) => Ok(Json(OsUpdateInfo {
            package_manager: "unknown".into(),
            available: false,
            count: 0,
            packages: vec![],
            error: Some(error.to_string()),
        })),
    }
}

#[derive(Deserialize)]
pub struct OsApplyReq {
    pub dry_run: bool,
}

pub async fn apply_os(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<OsApplyReq>,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        &state,
        &jar,
        "update.os.apply",
        None,
        serde_json::json!({}),
        req.dry_run,
        &headers,
    )
    .await
}

async fn prepare_or_submit(
    state: &AppState,
    jar: &CookieJar,
    action: &str,
    container_selector: Option<&str>,
    input: serde_json::Value,
    dry_run: bool,
    headers: &HeaderMap,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(state, jar, None).await?;
    prepare_or_submit_with_credential(
        state,
        &credential,
        action,
        container_selector,
        input,
        dry_run,
        headers,
    )
    .await
}

async fn prepare_or_submit_with_credential(
    state: &AppState,
    credential: &CredentialContext,
    action: &str,
    container_selector: Option<&str>,
    input: serde_json::Value,
    dry_run: bool,
    headers: &HeaderMap,
) -> CompatibilityResult<Response> {
    let adopted =
        update_adoption::resolve_target(&state.db, credential, action, container_selector).await?;
    if dry_run {
        let prepared =
            operation_adoption::prepare(state, credential, &adopted.resource.id, action, input)
                .await?;
        return legacy_plan_response(prepared);
    }
    operation_adoption::submit(
        state,
        credential,
        &adopted.resource.id,
        action,
        input,
        headers,
    )
    .await
}

async fn submit_action(
    state: &AppState,
    jar: &CookieJar,
    action: &str,
    container_selector: Option<&str>,
    input: serde_json::Value,
    headers: &HeaderMap,
) -> CompatibilityResult<Response> {
    prepare_or_submit(
        state,
        jar,
        action,
        container_selector,
        input,
        false,
        headers,
    )
    .await
}

fn legacy_plan_response(prepared: PreparedInvocation) -> CompatibilityResult<Response> {
    let view = prepared.view();
    let mut plan =
        serde_json::to_value(view.operation).map_err(|error| AppError::Internal(error.into()))?;
    plan["risk"] = serde_json::Value::String("high".into());
    Ok(Json(serde_json::json!({
        "dry_run": true,
        "plan": plan,
        "policy": view.policy,
        "resource": view.resource,
    }))
    .into_response())
}

fn image_status(container_image_id: &str, local_image_id: &str) -> String {
    if container_image_id.is_empty() || local_image_id.is_empty() {
        "unknown"
    } else if container_image_id == local_image_id {
        "up-to-date"
    } else {
        "update-available"
    }
    .into()
}

fn image_detail(container_image_id: &str, local_image_id: &str) -> Option<String> {
    (image_status(container_image_id, local_image_id) == "update-available")
        .then(|| "A newer local image is ready to apply".into())
}

fn short(value: &str) -> String {
    value.chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_status_is_derived_without_process_local_state() {
        assert_eq!(image_status("sha256:same", "sha256:same"), "up-to-date");
        assert_eq!(
            image_status("sha256:running", "sha256:local"),
            "update-available"
        );
        assert_eq!(image_status("", "sha256:local"), "unknown");
        assert_eq!(image_status("sha256:running", ""), "unknown");
    }
}
