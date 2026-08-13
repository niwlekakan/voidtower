use crate::{
    auth,
    error::{AppError, Result},
    updates::{
        self as update_provider, ExecutionResult, UpdateRequest, UpdateSnapshot, UpdateTarget,
    },
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

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

#[derive(Clone)]
struct VtDockerStatus {
    status: String,
    detail: Option<String>,
}

static VT_DOCKER_CACHE: OnceLock<Mutex<VtDockerStatus>> = OnceLock::new();

fn vt_docker_cache() -> &'static Mutex<VtDockerStatus> {
    VT_DOCKER_CACHE.get_or_init(|| {
        Mutex::new(VtDockerStatus {
            status: "unknown".into(),
            detail: None,
        })
    })
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
        UpdateSnapshot::VoidTowerDocker(snapshot) => {
            let cached = vt_docker_cache().lock().unwrap().clone();
            VtUpdateInfo {
                mode: "docker".into(),
                current_commit: String::new(),
                remote_commit: String::new(),
                behind: 0,
                ahead: 0,
                commits: vec![],
                backup_tags: vec![],
                fetch_error: None,
                current_image: Some(snapshot.image),
                update_status: Some(cached.status),
                update_detail: cached.detail,
            }
        }
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
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let snapshot = update_provider::snapshot(&UpdateTarget::VoidTower)
        .await
        .map_err(provider_error)?;
    if !matches!(snapshot, UpdateSnapshot::VoidTowerDocker(_)) {
        return Err(AppError::FeatureUnavailable(
            "only available in Docker mode".into(),
        ));
    }
    {
        let mut cached = vt_docker_cache().lock().unwrap();
        cached.status = "checking".into();
        cached.detail = None;
    }
    tokio::spawn(async {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let result = update_provider::execute(
            &UpdateTarget::VoidTower,
            &UpdateRequest::Check,
            &operation_id,
        )
        .await;
        let (status, detail) = match result {
            Ok(ExecutionResult::Completed { message }) if message.contains("newer") => {
                ("update-available", Some(message))
            }
            Ok(ExecutionResult::Completed { .. }) => ("up-to-date", None),
            Ok(ExecutionResult::RestartInitiated { message }) => ("error", Some(message)),
            Err(error) => ("error", Some(error.to_string())),
        };
        let mut cached = vt_docker_cache().lock().unwrap();
        cached.status = status.into();
        cached.detail = detail;
    });
    Ok(Json(serde_json::json!({"ok": true})))
}

#[derive(Deserialize, Default)]
pub struct ApplyReq {
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn apply_vt(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<ApplyReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let snapshot = update_provider::snapshot(&UpdateTarget::VoidTower)
        .await
        .map_err(provider_error)?;
    if req.dry_run {
        let (mode, current, target) = voidtower_plan_values(&snapshot)?;
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": format!("Update VoidTower ({mode})"),
                "risk": "high",
                "changes": [
                    {"label": "Current version", "value": current},
                    {"label": "Target version", "value": target},
                    {"label": "Safety", "value": "Persist rollback point before apply"},
                    {"label": "Downtime", "value": "Brief — UI unavailable during restart"}
                ],
                "preview": null
            }
        })));
    }
    execute_legacy_mutation(UpdateTarget::VoidTower, UpdateRequest::Apply).await
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
    Json(req): Json<RollbackReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    update_provider::validate_backup_tag(&req.tag)
        .map_err(|_| AppError::BadRequest("Invalid backup tag".into()))?;
    if req.dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Rollback VoidTower",
                "risk": "high",
                "changes": [
                    {"label": "Target tag", "value": req.tag},
                    {"label": "Safety", "value": "Snapshot current state before rollback"},
                    {"label": "Action", "value": "checkout + rebuild + restart"}
                ],
                "preview": null
            }
        })));
    }
    execute_legacy_mutation(
        UpdateTarget::VoidTower,
        UpdateRequest::Rollback {
            tag: req.tag.clone(),
        },
    )
    .await
    .map(|Json(mut value)| {
        value["rolling_back_to"] = serde_json::Value::String(req.tag);
        Json(value)
    })
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

static DOCKER_CACHE: OnceLock<Mutex<HashMap<String, DockerImageRow>>> = OnceLock::new();

fn docker_cache() -> &'static Mutex<HashMap<String, DockerImageRow>> {
    DOCKER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
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
    let cache = docker_cache().lock().unwrap();
    Ok(Json(
        containers
            .into_iter()
            .map(|container| {
                cache
                    .get(&container.container_id)
                    .cloned()
                    .unwrap_or(DockerImageRow {
                        container_id: container.container_id,
                        container_name: container.container_name,
                        image: container.image,
                        status: "unknown".into(),
                        detail: None,
                    })
            })
            .collect(),
    ))
}

pub async fn docker_check(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
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
    {
        let mut cache = docker_cache().lock().unwrap();
        for container in &containers {
            cache.insert(
                container.container_id.clone(),
                DockerImageRow {
                    container_id: container.container_id.clone(),
                    container_name: container.container_name.clone(),
                    image: container.image.clone(),
                    status: "checking".into(),
                    detail: None,
                },
            );
        }
    }
    tokio::spawn(async move {
        let operation_id = uuid::Uuid::new_v4().to_string();
        let result = update_provider::execute(
            &UpdateTarget::DockerEngine,
            &UpdateRequest::Check,
            &operation_id,
        )
        .await;
        let refreshed = update_provider::snapshot(&UpdateTarget::DockerEngine).await;
        let mut cache = docker_cache().lock().unwrap();
        match (result, refreshed) {
            (
                Ok(ExecutionResult::Completed { .. }),
                Ok(UpdateSnapshot::DockerEngine { containers }),
            ) => {
                for container in containers {
                    let update_available = !container.local_image_id.is_empty()
                        && container.container_image_id != container.local_image_id;
                    cache.insert(
                        container.container_id.clone(),
                        DockerImageRow {
                            container_id: container.container_id,
                            container_name: container.container_name,
                            image: container.image,
                            status: if update_available {
                                "update-available"
                            } else {
                                "up-to-date"
                            }
                            .into(),
                            detail: update_available
                                .then(|| "New image downloaded and ready to apply".into()),
                        },
                    );
                }
            }
            (Err(error), _) => {
                for row in cache.values_mut().filter(|row| row.status == "checking") {
                    row.status = "error".into();
                    row.detail = Some(error.to_string());
                }
            }
            _ => {}
        }
    });
    Ok(Json(
        serde_json::json!({"ok": true, "message": "Check started"}),
    ))
}

pub async fn docker_apply(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(container_id): Path<String>,
    Json(req): Json<ApplyReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let target = UpdateTarget::DockerImage {
        container_id: container_id.clone(),
    };
    let UpdateSnapshot::DockerImage(snapshot) = update_provider::snapshot(&target)
        .await
        .map_err(|error| AppError::BadRequest(error.to_string()))?
    else {
        return Err(AppError::Internal(anyhow::anyhow!(
            "invalid Docker image snapshot"
        )));
    };
    if req.dry_run {
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Update container",
                "risk": "medium",
                "changes": [
                    {"label": "Container", "value": snapshot.container_name},
                    {"label": "Image", "value": snapshot.image},
                    {"label": "Safety", "value": "Persist current image ID before apply"}
                ],
                "preview": null
            }
        })));
    }
    let response = execute_legacy_mutation(target, UpdateRequest::Apply).await?;
    if let Some(row) = docker_cache().lock().unwrap().get_mut(&container_id) {
        row.status = "up-to-date".into();
        row.detail = None;
    }
    Ok(response)
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
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    execute_legacy_mutation(UpdateTarget::Odysseus, UpdateRequest::Apply).await
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
    Json(req): Json<OsApplyReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    if req.dry_run {
        let UpdateSnapshot::OperatingSystem {
            package_manager,
            packages,
        } = update_provider::snapshot(&UpdateTarget::OperatingSystem)
            .await
            .map_err(provider_error)?
        else {
            return Err(AppError::Internal(anyhow::anyhow!(
                "invalid operating-system update snapshot"
            )));
        };
        let preview = (!packages.is_empty()).then(|| packages.join("\n"));
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": format!("Apply OS updates ({package_manager})"),
                "risk": os_update_risk(packages.len()),
                "changes": [
                    {"label": "Package manager", "value": package_manager},
                    {"label": "Packages to update", "value": packages.len().to_string()},
                    {"label": "Safety", "value": "Persist installed-package manifest before apply"}
                ],
                "preview": preview
            },
            "error": null
        })));
    }
    execute_legacy_mutation(UpdateTarget::OperatingSystem, UpdateRequest::Apply).await
}

async fn execute_legacy_mutation(
    target: UpdateTarget,
    request: UpdateRequest,
) -> Result<Json<serde_json::Value>> {
    let operation_id = uuid::Uuid::new_v4().to_string();
    let rollback = update_provider::prepare_rollback(&target, &operation_id)
        .await
        .map_err(provider_error)?;
    let execution = update_provider::execute(&target, &request, &operation_id)
        .await
        .map_err(provider_error)?;
    let message = match execution {
        ExecutionResult::Completed { message } | ExecutionResult::RestartInitiated { message } => {
            message
        }
    };
    Ok(Json(serde_json::json!({
        "ok": true,
        "operation_id": operation_id,
        "rollback_kind": rollback.kind,
        "message": message
    })))
}

fn voidtower_plan_values(snapshot: &UpdateSnapshot) -> Result<(&'static str, String, String)> {
    match snapshot {
        UpdateSnapshot::VoidTowerGit(snapshot) => Ok((
            "git",
            short(&snapshot.current_commit),
            short(&snapshot.remote_commit),
        )),
        UpdateSnapshot::VoidTowerBinary {
            current_version,
            remote_version,
        } => Ok(("binary", current_version.clone(), remote_version.clone())),
        UpdateSnapshot::VoidTowerDocker(snapshot) => Ok((
            "Docker",
            short(&snapshot.container_image_id),
            short(&snapshot.local_image_id),
        )),
        _ => Err(AppError::Internal(anyhow::anyhow!(
            "invalid VoidTower snapshot"
        ))),
    }
}

fn short(value: &str) -> String {
    value.chars().take(12).collect()
}

fn os_update_risk(count: usize) -> &'static str {
    if count == 0 {
        "low"
    } else if count <= 10 {
        "medium"
    } else {
        "high"
    }
}
