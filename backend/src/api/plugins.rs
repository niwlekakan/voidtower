use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
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

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &sid)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

#[derive(sqlx::FromRow, Serialize, Clone)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub entry: String,
    pub icon: Option<String>,
    pub nav_group: Option<String>,
    pub enabled: bool,
    pub installed_at: i64,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct PluginManifest {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_version")]
    version: String,
    author: Option<String>,
    #[serde(default = "default_entry")]
    entry: String,
    icon: Option<String>,
    nav_group: Option<String>,
}

#[allow(dead_code)]
fn default_version() -> String {
    "1.0.0".into()
}
#[allow(dead_code)]
fn default_entry() -> String {
    "index.html".into()
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct InstallRequest {
    pub url: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct UpdateRequest {
    pub enabled: Option<bool>,
}

#[allow(dead_code)]
fn plugins_dir(state: &AppState) -> std::path::PathBuf {
    state.config.data_dir.join("plugins")
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Vec<Plugin>>> {
    require_user(&state, &jar).await?;
    let rows = sqlx::query_as::<_, Plugin>(
        "SELECT id, name, description, version, author, entry, icon, nav_group, enabled, installed_at \
         FROM plugins ORDER BY installed_at ASC",
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

pub async fn install(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Plugin>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "plugin mutations require a canonical operation adapter".into(),
    ))
}

pub async fn uninstall(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_id): Path<String>,
) -> Result<Json<Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "plugin mutations require a canonical operation adapter".into(),
    ))
}

pub async fn update(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(_id): Path<String>,
) -> Result<Json<Value>> {
    require_admin(&state, &jar).await?;
    Err(AppError::FeatureUnavailable(
        "plugin mutations require a canonical operation adapter".into(),
    ))
}

pub async fn serve_asset(
    State(state): State<AppState>,
    jar: CookieJar,
    Path((id, path)): Path<(String, String)>,
) -> Result<Response> {
    require_user(&state, &jar).await?;

    if id.contains("..") || path.contains("..") {
        return Err(AppError::BadRequest("Invalid path".into()));
    }

    let file_path = plugins_dir(&state).join(&id).join(&path);

    if !file_path.exists() {
        return Err(AppError::NotFound);
    }

    let content = tokio::fs::read(&file_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let mime = mime_for_path(&path);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(content))
        .unwrap())
}

fn mime_for_path(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") || path.ends_with(".mjs") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".ico") {
        "image/x-icon"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else if path.ends_with(".woff") {
        "font/woff"
    } else if path.ends_with(".gif") {
        "image/gif"
    } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    }
}

#[allow(dead_code)]
fn extract_zip(
    bytes: &[u8],
    plugins_base: &std::path::Path,
) -> std::result::Result<PluginManifest, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| format!("Invalid zip: {e}"))?;

    // First pass: find plugin.json and determine prefix
    let (manifest_str, prefix) = {
        let mut found: Option<(String, String)> = None;
        for i in 0..archive.len() {
            let mut file = archive
                .by_index(i)
                .map_err(|e| format!("Zip read error: {e}"))?;
            let name = file.name().to_string();
            if (name == "plugin.json" || name.ends_with("/plugin.json")) && !name.contains("..") {
                let mut content = String::new();
                file.read_to_string(&mut content)
                    .map_err(|e| format!("Read error: {e}"))?;
                let prefix = name
                    .rfind('/')
                    .map(|p| name[..=p].to_string())
                    .unwrap_or_default();
                found = Some((content, prefix));
                break;
            }
        }
        found.ok_or_else(|| "plugin.json not found in zip".to_string())?
    };

    let manifest: PluginManifest =
        serde_json::from_str(&manifest_str).map_err(|e| format!("Invalid plugin.json: {e}"))?;

    if manifest.id.is_empty() || manifest.id.contains('/') || manifest.id.contains("..") {
        return Err("Invalid plugin id".into());
    }

    let plugin_dir = plugins_base.join(&manifest.id);
    std::fs::create_dir_all(&plugin_dir).map_err(|e| format!("Cannot create plugin dir: {e}"))?;

    // Second pass: extract all files
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Zip read error: {e}"))?;
        let raw = file.name().to_string();

        let rel = if prefix.is_empty() {
            raw.clone()
        } else {
            match raw.strip_prefix(&prefix) {
                Some(r) => r.to_string(),
                None => continue,
            }
        };

        if rel.is_empty() || rel.contains("..") {
            continue;
        }

        let dest = plugin_dir.join(&rel);

        if file.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| format!("mkdir: {e}"))?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
            }
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)
                .map_err(|e| format!("Read error: {e}"))?;
            std::fs::write(&dest, &contents).map_err(|e| format!("Write error: {e}"))?;
        }
    }

    Ok(manifest)
}
