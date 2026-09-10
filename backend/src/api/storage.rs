use crate::{
    audit, auth,
    error::{AppError, Result},
    storage,
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::Deserialize;

// ─── Auth helper (mirrors settings.rs) ───────────────────────────────────────

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") {
        return Err(AppError::Forbidden);
    }
    Ok(user)
}

// ─── GET /api/storage/devices ─────────────────────────────────────────────────

pub async fn list_devices(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    // read-only — any authenticated user is fine; but keep behind admin for consistency
    require_admin(&_state, &jar).await?;
    let devices = storage::list_block_devices().await;
    Ok(Json(serde_json::json!({ "devices": devices })))
}

// ─── GET /api/storage/mounts ──────────────────────────────────────────────────

pub async fn list_mounts_handler(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let mounts = storage::list_mounts().await;
    Ok(Json(serde_json::json!({ "mounts": mounts })))
}

// ─── POST /api/storage/mount ──────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct MountReq {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: Option<String>,
}

pub async fn mount_device(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MountReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = req;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

// ─── POST /api/storage/umount ─────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct UmountReq {
    pub mountpoint: String,
}

pub async fn umount_device(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<UmountReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = req;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

pub async fn get_fstab(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let entries = storage::read_fstab().await;
    Ok(Json(serde_json::json!({ "entries": entries })))
}

// ─── POST /api/storage/fstab ─────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct AddFstabReq {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub options: String,
    pub dump: Option<i32>,
    pub pass: Option<i32>,
}

pub async fn add_fstab(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AddFstabReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = req;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

// ─── DELETE /api/storage/fstab/:idx ──────────────────────────────────────────

pub async fn remove_fstab(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(idx): Path<usize>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = idx;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

// ─── GET /api/storage/smart/:dev ─────────────────────────────────────────────

pub async fn get_smart(
    State(_state): State<AppState>,
    jar: CookieJar,
    Path(dev): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let info = storage::smart_info(&dev).await;
    Ok(Json(serde_json::json!(info)))
}

// ─── GET /api/storage/raid ────────────────────────────────────────────────────

pub async fn get_raid(
    State(_state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&_state, &jar).await?;
    let available = storage::which_cmd("mdadm");
    if !available {
        return Ok(Json(
            serde_json::json!({ "available": false, "arrays": [] }),
        ));
    }
    let arrays = storage::list_raid().await;
    Ok(Json(serde_json::json!({ "available": true, "arrays": arrays })))
}

// ─── POST /api/storage/raid/create ───────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct CreateRaidReq {
    pub name: String,
    pub level: String,
    pub devices: Vec<String>,
}

pub async fn create_raid(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRaidReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = req;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

// ─── POST /api/storage/raid/stop ─────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct StopRaidReq {
    pub path: String,
}

pub async fn stop_raid(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<StopRaidReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = req;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

// ─── POST /api/storage/format ─────────────────────────────────────────────────

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct FormatReq {
    pub device: String,
    pub fstype: String,
    pub label: Option<String>,
}

pub async fn format_device(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<FormatReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let _ = req;
    Err(AppError::FeatureUnavailable(
        "local storage mutations require a canonical operation adapter".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::{Cookie, CookieJar};

    #[tokio::test]
    async fn format_mutation_fails_closed_until_canonical_adapter_exists() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let state = crate::api::mcp::test_support::build(pool);
        let jar = CookieJar::new().add(Cookie::new("vt_session", session));

        let result = format_device(
            State(state),
            jar,
            Json(FormatReq {
                device: "/dev/voidtower-test".into(),
                fstype: "ext4".into(),
                label: None,
            }),
        )
        .await;

        assert!(
            matches!(result, Err(AppError::FeatureUnavailable(ref message)) if message.contains("canonical operation")),
            "storage format must fail closed instead of executing mkfs: {result:?}"
        );
    }

    #[tokio::test]
    async fn storage_mutation_route_returns_stable_feature_error() {
        use axum::{
            body::{to_bytes, Body},
            http::{header, Request, StatusCode},
        };
        use tower::ServiceExt;

        let pool = crate::api::mcp::test_support::setup_db().await;
        let session = crate::api::mcp::test_support::user_with_session(&pool).await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/storage/mount")
                    .header(header::COOKIE, format!("vt_session={session}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"device":"/dev/voidtower-test","mountpoint":"/mnt/voidtower-test","fstype":"ext4"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "feature_unavailable");
        assert_eq!(
            payload["error"]["message"],
            "local storage mutations require a canonical operation adapter"
        );
    }

    #[tokio::test]
    async fn storage_mutation_route_rejects_unauthenticated_call() {
        use axum::{
            body::{to_bytes, Body},
            http::{header, Request, StatusCode},
        };
        use tower::ServiceExt;

        let pool = crate::api::mcp::test_support::setup_db().await;
        let app = crate::api::router(crate::api::mcp::test_support::build(pool));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/storage/mount")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"device":"/dev/voidtower-test","mountpoint":"/mnt/voidtower-test","fstype":"ext4"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["error"]["code"], "unauthorized");
    }

    #[test]
    fn storage_mutation_handlers_have_no_provider_execution_path() {
        let source = include_str!("storage.rs");
        for name in [
            "mount_device",
            "umount_device",
            "add_fstab",
            "remove_fstab",
            "create_raid",
            "stop_raid",
            "format_device",
        ] {
            let start = source
                .find(&format!("pub async fn {name}"))
                .expect("storage mutation handler must exist");
            let body = &source[start..source[start..].find("\n}\n").unwrap() + start + 3];
            assert!(body.contains("require_admin"), "{name} must keep auth first");
            assert!(body.contains("FeatureUnavailable"), "{name} must fail closed");
            assert!(
                !body.contains("Command::new") && !body.contains("run_privileged"),
                "{name} must not execute a provider"
            );
        }
    }
}

// ─── Storage location paths ───────────────────────────────────────────────────
// Persisted in the settings table under storage.paths.* keys.

async fn db_get_path(state: &AppState, key: &str) -> Option<String> {
    sqlx::query_as::<_, (String,)>("SELECT value FROM settings WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|(v,)| v)
}

async fn db_set_path(state: &AppState, key: &str, value: &str) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value, updated_at) VALUES (?, ?, ?)")
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

pub async fn get_storage_paths(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let containers = db_get_path(&state, "storage.paths.containers").await;
    let appvault   = db_get_path(&state, "storage.paths.appvault").await;
    let vms        = db_get_path(&state, "storage.paths.vms").await;
    let backups    = db_get_path(&state, "storage.paths.backups").await;
    Ok(Json(serde_json::json!({
        "containers": containers,
        "appvault":   appvault,
        "vms":        vms,
        "backups":    backups,
    })))
}

#[derive(Deserialize)]
pub struct SetStoragePathsReq {
    pub containers: Option<String>,
    pub appvault:   Option<String>,
    pub vms:        Option<String>,
    pub backups:    Option<String>,
}

pub async fn set_storage_paths(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<SetStoragePathsReq>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let pairs: &[(&str, Option<&String>)] = &[
        ("storage.paths.containers", req.containers.as_ref()),
        ("storage.paths.appvault",   req.appvault.as_ref()),
        ("storage.paths.vms",        req.vms.as_ref()),
        ("storage.paths.backups",    req.backups.as_ref()),
    ];

    for (key, val) in pairs {
        if let Some(v) = val {
            if !v.starts_with('/') {
                return Err(AppError::BadRequest(format!("{key}: path must be absolute")));
            }
            if v.contains([';', '&', '|', '`', '$', '\n']) {
                return Err(AppError::BadRequest(format!("{key}: invalid characters")));
            }
            db_set_path(&state, key, v).await?;
        }
    }

    audit::log(
        &state.db, Some(&user.id), "human", "storage.paths.set",
        Some("storage"), None, "success", None, None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}
