use crate::{
    audit, auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Query, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

// ─── Auth helpers ────────────────────────────────────────────────────────────

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn require_operator(user: &auth::User) -> Result<()> {
    match user.role.as_str() {
        "owner" | "admin" | "operator" => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

fn require_admin(user: &auth::User) -> Result<()> {
    match user.role.as_str() {
        "owner" | "admin" => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}

// ─── Path guard ──────────────────────────────────────────────────────────────

const BLOCKED_PREFIXES: &[&str] = &["/proc", "/sys", "/dev", "/run/lock"];
const MAX_READ_BYTES: u64 = 2 * 1024 * 1024; // 2 MB text file limit

fn guard_path(raw: &str) -> Result<PathBuf> {
    if raw.is_empty() {
        return Err(AppError::BadRequest("Path is empty".into()));
    }
    let p = PathBuf::from(raw);
    if !p.is_absolute() {
        return Err(AppError::BadRequest("Path must be absolute".into()));
    }
    for blocked in BLOCKED_PREFIXES {
        if p.starts_with(blocked) {
            return Err(AppError::Forbidden);
        }
    }
    // Resolve symlinks to detect traversal, but only if the path exists
    if p.exists() {
        let canon = p.canonicalize()
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        for blocked in BLOCKED_PREFIXES {
            if canon.starts_with(blocked) {
                return Err(AppError::Forbidden);
            }
        }
        return Ok(canon);
    }
    Ok(p)
}

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub permissions: String,
    pub is_symlink: bool,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub path: String,
    pub entries: Vec<FileEntry>,
    pub parent: Option<String>,
}

#[derive(Serialize)]
pub struct ReadResponse {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub truncated: bool,
}

#[derive(Serialize)]
pub struct RootsResponse {
    pub roots: Vec<FsRoot>,
}

#[derive(Serialize)]
pub struct FsRoot {
    pub label: String,
    pub path: String,
}

#[derive(Deserialize)]
pub struct PathQuery {
    pub path: String,
}

#[derive(Deserialize)]
pub struct WriteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct MkdirRequest {
    pub path: String,
}

#[derive(Deserialize)]
pub struct RenameRequest {
    pub from: String,
    pub to: String,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

#[cfg(unix)]
fn mode_string(meta: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!("{:o}", meta.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn mode_string(_meta: &std::fs::Metadata) -> String {
    String::from("644")
}

fn modified_ts(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ─── Handlers ────────────────────────────────────────────────────────────────

pub async fn roots(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<RootsResponse>> {
    let user = require_user(&state, &jar).await?;
    require_operator(&user)?;

    let mut roots = vec![
        FsRoot { label: "Root".into(), path: "/".into() },
        FsRoot { label: "Home".into(), path: "/root".into() },
    ];

    // Add VoidTower data/config directories if they exist
    for (label, path) in [
        ("VoidTower data", "/var/lib/voidtower"),
        ("VoidTower config", "/etc/voidtower"),
    ] {
        if Path::new(path).exists() {
            roots.push(FsRoot { label: label.into(), path: path.into() });
        }
    }

    // Add common mount points
    for mount in ["/mnt", "/media", "/srv", "/opt", "/home"] {
        if let Ok(mut rd) = std::fs::read_dir(mount) {
            while let Some(Ok(entry)) = rd.next() {
                if entry.path().is_dir() {
                    roots.push(FsRoot {
                        label: entry.file_name().to_string_lossy().into_owned(),
                        path: entry.path().to_string_lossy().into_owned(),
                    });
                }
            }
        }
    }

    Ok(Json(RootsResponse { roots }))
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<PathQuery>,
) -> Result<Json<ListResponse>> {
    let user = require_user(&state, &jar).await?;
    require_operator(&user)?;

    let path = guard_path(&q.path)?;

    if !path.is_dir() {
        return Err(AppError::BadRequest("Not a directory".into()));
    }

    let parent = path.parent().map(|p| p.to_string_lossy().into_owned());

    let mut entries = Vec::new();
    let mut rd = fs::read_dir(&path)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    while let Ok(Some(entry)) = rd.next_entry().await {
        let meta = match entry.metadata().await {
            Ok(m) => m,
            Err(_) => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let full_path = entry.path().to_string_lossy().into_owned();
        entries.push(FileEntry {
            name,
            path: full_path,
            is_dir: meta.is_dir(),
            size: if meta.is_file() { meta.len() } else { 0 },
            modified: modified_ts(&meta),
            permissions: mode_string(&meta),
            is_symlink: meta.file_type().is_symlink(),
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(Json(ListResponse {
        path: path.to_string_lossy().into_owned(),
        entries,
        parent,
    }))
}

pub async fn read_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<PathQuery>,
) -> Result<Json<ReadResponse>> {
    let user = require_user(&state, &jar).await?;
    require_operator(&user)?;

    let path = guard_path(&q.path)?;

    let meta = fs::metadata(&path)
        .await
        .map_err(|_| AppError::NotFound)?;

    if meta.is_dir() {
        return Err(AppError::BadRequest("Path is a directory".into()));
    }

    let size = meta.len();
    let truncated = size > MAX_READ_BYTES;
    let read_len = size.min(MAX_READ_BYTES) as usize;

    let mut file = fs::File::open(&path)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    use tokio::io::AsyncReadExt;
    let mut buf = vec![0u8; read_len];
    file.read_exact(&mut buf)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let content = String::from_utf8_lossy(&buf).into_owned();

    Ok(Json(ReadResponse {
        path: path.to_string_lossy().into_owned(),
        content,
        size,
        truncated,
    }))
}

pub async fn write_file(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<WriteRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_admin(&user)?;

    let path = guard_path(&req.path)?;

    if path.is_dir() {
        return Err(AppError::BadRequest("Path is a directory".into()));
    }

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }

    fs::write(&path, req.content.as_bytes())
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let path_str = path.to_string_lossy();
    audit::log(
        &state.db, Some(&user.id), "human", "file.write",
        Some("file"), Some(&path_str), "success", None, None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "path": path_str })))
}

pub async fn mkdir(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<MkdirRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_admin(&user)?;

    let path = guard_path(&req.path)?;

    fs::create_dir_all(&path)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let path_str = path.to_string_lossy();
    audit::log(
        &state.db, Some(&user.id), "human", "file.mkdir",
        Some("file"), Some(&path_str), "success", None, None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "path": path_str })))
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<PathQuery>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_admin(&user)?;

    let path = guard_path(&q.path)?;

    // Safety: refuse to delete root or blocked paths
    if path == Path::new("/") {
        return Err(AppError::Forbidden);
    }

    let meta = fs::metadata(&path)
        .await
        .map_err(|_| AppError::NotFound)?;

    let path_str = path.to_string_lossy().into_owned();
    if meta.is_dir() {
        fs::remove_dir_all(&path)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    } else {
        fs::remove_file(&path)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }

    audit::log(
        &state.db, Some(&user.id), "human", "file.delete",
        Some("file"), Some(&path_str), "success", None, None,
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn rename(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<RenameRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    require_admin(&user)?;

    let from = guard_path(&req.from)?;
    let to = guard_path(&req.to)?;

    let from_str = from.to_string_lossy().into_owned();
    let to_str = to.to_string_lossy().into_owned();

    fs::rename(&from, &to)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    audit::log(
        &state.db, Some(&user.id), "human", "file.rename",
        Some("file"), Some(&from_str), "success", None,
        Some(&format!("→ {to_str}")),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

// ─── Activity ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct FileActivityEntry {
    pub timestamp: i64,
    pub action: String,
    pub actor_type: String,
    pub username: Option<String>,
    pub role: Option<String>,
    pub details: Option<String>,
    pub outcome: String,
}

#[derive(Serialize)]
pub struct ActivityResponse {
    pub path: String,
    pub entries: Vec<FileActivityEntry>,
}

pub async fn activity(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<PathQuery>,
) -> Result<Json<ActivityResponse>> {
    let user = require_user(&state, &jar).await?;
    require_operator(&user)?;

    let path = guard_path(&q.path)?;
    let path_str = path.to_string_lossy().into_owned();

    // Join with users to get username + role for each entry
    let rows = sqlx::query(
        "SELECT al.timestamp, al.action, al.actor_type, al.details, al.outcome,
                u.username, u.role
         FROM audit_log al
         LEFT JOIN users u ON u.id = al.user_id
         WHERE al.resource_type = 'file' AND al.resource_id = ?
         ORDER BY al.timestamp DESC LIMIT 50",
    )
    .bind(&path_str)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    use sqlx::Row;
    let entries = rows
        .into_iter()
        .map(|r| FileActivityEntry {
            timestamp: r.get("timestamp"),
            action: r.get("action"),
            actor_type: r.get("actor_type"),
            username: r.get("username"),
            role: r.get("role"),
            details: r.get("details"),
            outcome: r.get("outcome"),
        })
        .collect();

    Ok(Json(ActivityResponse { path: path_str, entries }))
}

// ─── Raw file serve (images, PDFs, downloads) ─────────────────────────────────

fn mime_for_ext(ext: &str) -> &'static str {
    match ext {
        "png"  => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif"  => "image/gif",
        "webp" => "image/webp",
        "svg"  => "image/svg+xml",
        "bmp"  => "image/bmp",
        "ico"  => "image/x-icon",
        "pdf"  => "application/pdf",
        "mp4"  => "video/mp4",
        "webm" => "video/webm",
        "mp3"  => "audio/mpeg",
        "wav"  => "audio/wav",
        _      => "application/octet-stream",
    }
}

pub async fn serve_raw(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<PathQuery>,
) -> std::result::Result<axum::response::Response, AppError> {
    let user = require_user(&state, &jar).await?;
    require_operator(&user)?;

    let path = guard_path(&q.path)?;
    if path.is_dir() { return Err(AppError::BadRequest("Path is a directory".into())); }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mime = mime_for_ext(&ext);
    let bytes = tokio::fs::read(&path).await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let disposition = if mime == "application/octet-stream" {
        format!("attachment; filename=\"{name}\"")
    } else {
        format!("inline; filename=\"{name}\"")
    };

    Ok(axum::response::Response::builder()
        .header("Content-Type", mime)
        .header("Content-Disposition", disposition)
        .header("Cache-Control", "private, max-age=60")
        .body(axum::body::Body::from(bytes))
        .unwrap())
}
