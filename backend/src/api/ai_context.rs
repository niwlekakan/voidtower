use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde_json::Value;
use std::path::Path;

use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};

async fn require_any_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn project_root(state: &AppState) -> std::path::PathBuf {
    // frontend_dir is frontend/dist — go up two levels to reach repo root
    state
        .config
        .frontend_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn walk_rs_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}

fn walk_tsx_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("tsx") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.push(stem.to_string());
            }
        }
    }
    out.sort();
    out
}

fn extract_routes(mod_rs: &str) -> Vec<Value> {
    let mut routes = Vec::new();
    for line in mod_rs.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with(".route(") {
            continue;
        }
        // .route("/api/foo", get(handler).post(handler2))
        let inner = trimmed.trim_start_matches(".route(").trim_end_matches(')');
        let mut parts = inner.splitn(2, ',');
        let path = parts
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .to_string();
        let methods_raw = parts.next().unwrap_or("").trim().to_string();
        // extract method names: get(, post(, put(, delete(, patch(
        let mut methods = Vec::new();
        for verb in &["get(", "post(", "put(", "delete(", "patch("] {
            if methods_raw.contains(verb) {
                methods.push(verb.trim_end_matches('(').to_uppercase());
            }
        }
        if !path.is_empty() {
            routes.push(serde_json::json!({ "path": path, "methods": methods }));
        }
    }
    routes
}

pub async fn get_context(State(state): State<AppState>, jar: CookieJar) -> Result<Json<Value>> {
    require_any_user(&state, &jar).await?;

    let root = project_root(&state);

    // Backend API modules
    let api_modules = walk_rs_files(&root.join("backend/src/api"));

    // Frontend pages
    let pages = walk_tsx_files(&root.join("frontend/src/pages"));

    // Frontend AIOS panels
    let aios_panels = walk_tsx_files(&root.join("frontend/src/aios/panels"));

    // Route list from mod.rs
    let routes = std::fs::read_to_string(root.join("backend/src/api/mod.rs"))
        .map(|src| extract_routes(&src))
        .unwrap_or_default();

    let context = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "architecture": {
            "backend": "Rust — axum 0.7, sqlx/SQLite, tokio, portable-pty, reqwest",
            "frontend": "React 18 + TypeScript + Vite + Zustand + xterm.js",
            "auth": "session cookie (vt_session) — all /api/* require valid session except /api/health and /api/auth/login",
            "db": "SQLite at data_dir/voidtower.db — migrations via sqlx migrate",
            "secrets": "AES-256-GCM encrypted values stored in secrets table"
        },
        "backend_api_modules": api_modules,
        "frontend_pages": pages,
        "aios_panels": aios_panels,
        "routes": routes,
        "key_patterns": {
            "new_api_handler": "pub async fn handler(State(state): State<AppState>, jar: CookieJar, Json(req): Json<Req>) -> Result<Json<Value>>",
            "auth_guard": "let user = auth::validate_session(&state.db, &session_id).await?.ok_or(Unauthorized)?",
            "db_query": "sqlx::query_as::<_, MyStruct>(\"SELECT ...\").fetch_all(&state.db).await?",
            "alert": "api::alerts::create_alert(&state.db, title, msg, severity, category, resource_type, resource_id).await",
            "error_type": "AppError::NotFound | AppError::Unauthorized | AppError::Forbidden | AppError::BadRequest(msg) | AppError::Internal(anyhow)",
            "new_page": "export default function MyPage() — add to Sidebar.tsx nav items and App.tsx routes",
            "new_native_panel": "implement NativePanelShell from aios/panels/NativePanelShell.tsx, add to PANEL_REGISTRY in AiosLayout.tsx"
        },
        "templates": templates_list(),
        "important_files": {
            "routing": "backend/src/api/mod.rs",
            "app_state": "backend/src/main.rs (AppState struct)",
            "db_migrations": "backend/migrations/",
            "sidebar_nav": "frontend/src/components/layout/Sidebar.tsx",
            "aios_layout": "frontend/src/aios/AiosLayout.tsx (PANEL_REGISTRY + keyboard shortcuts)",
            "aios_store": "frontend/src/aios/store/aios.ts",
            "api_client": "frontend/src/api/client.ts",
            "api_types": "frontend/src/api/types.ts"
        }
    });

    // This bundle is the ROADMAP-named "Odysseus context bundle" — it's built from
    // directory listings and static strings today, but `routes` is parsed out of a
    // real source file and could pick up secret material if that file ever
    // embedded one (e.g. a route path or comment). Route it through the same
    // redaction choke point as the MCP tools before it reaches an AI context.
    let serialized = serde_json::to_string(&context).map_err(|e| AppError::Internal(e.into()))?;
    let redacted = super::mcp::redact::redact_for_ai(&state, &serialized).await;
    let context: Value =
        serde_json::from_str(&redacted).map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(context))
}

// ── extension templates ───────────────────────────────────────────────────────

pub fn get_template(name: &str) -> std::result::Result<String, String> {
    let t = match name {
        "new_api_endpoint" => r#"// backend/src/api/my_feature.rs
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use crate::{auth, error::{AppError, Result}, AppState};

#[derive(Deserialize)]
pub struct CreateMyThingRequest { pub name: String }

#[derive(Serialize, sqlx::FromRow)]
pub struct MyThing { pub id: String, pub name: String, pub created_at: i64 }

pub async fn list(State(s): State<AppState>, jar: CookieJar) -> Result<Json<Vec<MyThing>>> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&s.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    let items = sqlx::query_as::<_, MyThing>("SELECT id, name, created_at FROM my_things ORDER BY name")
        .fetch_all(&s.db).await.map_err(AppError::Database)?;
    Ok(Json(items))
}

pub async fn create(State(s): State<AppState>, jar: CookieJar, Json(req): Json<CreateMyThingRequest>) -> Result<Json<serde_json::Value>> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    auth::validate_session(&s.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    sqlx::query("INSERT INTO my_things (id, name, created_at) VALUES (?, ?, ?)")
        .bind(&id).bind(&req.name).bind(now).execute(&s.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "ok": true, "id": id })))
}

// In backend/src/api/mod.rs add:
// pub mod my_feature;
// .route("/api/my-things", get(my_feature::list).post(my_feature::create))"#,

        "new_tower_page" => r#"// frontend/src/pages/MyPage.tsx
import { useState, useEffect } from 'react'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'

export default function MyPage() {
  const [items, setItems] = useState<unknown[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api.myFeature.list()
      .then(setItems)
      .catch(e => notify.error(e instanceof ApiClientError ? e.message : 'Failed'))
      .finally(() => setLoading(false))
  }, [])

  return (
    <div style={{ padding: 24 }}>
      <h1 style={{ fontSize: 18, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 20 }}>
        My Feature
      </h1>
      {loading ? (
        <p style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : (
        <pre>{JSON.stringify(items, null, 2)}</pre>
      )}
    </div>
  )
}

// In frontend/src/components/layout/Sidebar.tsx add to NAV_ITEMS:
// { key: 'my-feature', label: 'My Feature', icon: SomeIcon, path: '/my-feature' }
// In frontend/src/App.tsx add route:
// <Route path="/my-feature" element={<MyPage />} />"#,

        "new_native_panel" => r#"// frontend/src/aios/panels/myPanel.tsx
import { useState, useEffect } from 'react'
import { NativePanelShell, NativeRow, IconBtn, EmptyState, LoadingState } from '@/aios/panels/NativePanelShell'
import { api } from '@/api/client'

export function NativeMyPanel() {
  const [items, setItems] = useState<unknown[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    api.myFeature.list().then(setItems).catch(() => {}).finally(() => setLoading(false))
  }, [])

  if (loading) return <LoadingState />
  if (!items.length) return <EmptyState message="No items found" />

  return (
    <NativePanelShell>
      {items.map((item: any) => (
        <NativeRow key={item.id} title={item.name} subtitle={item.description} />
      ))}
    </NativePanelShell>
  )
}

// In frontend/src/aios/AiosLayout.tsx add to PANEL_REGISTRY:
// 'my-panel': { component: NativeMyPanel, title: 'My Panel', icon: SomeIcon }"#,

        "new_background" => r#"// frontend/src/backgrounds/MyBackground.tsx
import { useEffect, useRef } from 'react'

export default function MyBackground() {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const ctx = canvas.getContext('2d')!
    let frame: number

    const resize = () => { canvas.width = window.innerWidth; canvas.height = window.innerHeight }
    resize()
    window.addEventListener('resize', resize)

    const draw = () => {
      ctx.clearRect(0, 0, canvas.width, canvas.height)
      // TODO: render animation frame
      frame = requestAnimationFrame(draw)
    }
    frame = requestAnimationFrame(draw)

    return () => { cancelAnimationFrame(frame); window.removeEventListener('resize', resize) }
  }, [])

  return <canvas ref={canvasRef} style={{ position: 'fixed', inset: 0, zIndex: 0, pointerEvents: 'none' }} />
}

// Register in frontend/src/pages/Themes.tsx BACKGROUNDS array:
// { id: 'my-bg', label: 'My Background', component: MyBackground }"#,

        "new_background_animation" => r#"// Same as new_background — see that template"#,

        "new_catalog_entry" => r#"// backend/data/catalog/my-app.json
{
  "id": "my-app",
  "name": "My App",
  "description": "Short description of what the app does.",
  "category": "Tools",
  "icon": "🛠️",
  "version_hint": "latest",
  "links": { "docs": "https://example.com/docs", "github": "https://github.com/example/my-app" },
  "web_port": 8080,
  "web_path": "/",
  "compose": {
    "services": {
      "my-app": {
        "image": "example/my-app:latest",
        "restart": "unless-stopped",
        "ports": ["8080:8080"],
        "volumes": ["my-app-data:/data"],
        "environment": { "TZ": "UTC" }
      }
    },
    "volumes": { "my-app-data": {} }
  }
}"#,

        "new_mcp_tool" => r#"// In backend/src/api/mcp.rs — add to handle_tools_list:
{
    "name": "my_tool",
    "description": "Does something useful",
    "inputSchema": {
        "type": "object",
        "properties": {
            "param": { "type": "string", "description": "The param" }
        },
        "required": ["param"]
    }
}

// Add to handle_tools_call match:
"my_tool" => tool_my_tool(state, args).await,

// Add implementation:
async fn tool_my_tool(state: &AppState, args: Value) -> std::result::Result<String, String> {
    let param = args.get("param").and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'param'".to_string())?;
    // ... do work ...
    serde_json::to_string(&serde_json::json!({ "result": param })).map_err(|e| e.to_string())
}"#,

        _ => return Err(format!("Unknown template: {name}. Available: new_api_endpoint, new_tower_page, new_native_panel, new_background, new_catalog_entry, new_mcp_tool")),
    };
    Ok(t.to_string())
}

fn templates_list() -> serde_json::Value {
    serde_json::json!([
        "new_api_endpoint",
        "new_tower_page",
        "new_native_panel",
        "new_background",
        "new_catalog_entry",
        "new_mcp_tool"
    ])
}

// ── read_file / search_code helpers (used by MCP tools) ──────────────────────

pub fn safe_project_root_from_frontend_dir(frontend_dir: &Path) -> std::path::PathBuf {
    frontend_dir
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// Maximum bytes this project-reading helper will load. The caller truncates its
/// own output; this bound stops an oversized file from being read into memory first.
const MAX_PROJECT_FILE_BYTES: u64 = 1024 * 1024;

/// The only trees `read_file` serves — the same trees `search_project_code`
/// searches. Configuration, runtime state, credentials, VCS metadata, dependency
/// trees, and build output are outside the readable surface even when they live
/// under the project root (`backend/dev-config/bootstrap-token`,
/// `backend/dev-data/*`, `.env`, `target/`, …).
const READABLE_PROJECT_SOURCE_TREES: &[&str] = &["backend/src", "frontend/src"];

/// Path components whose contents are never project source: runtime data, VCS
/// metadata, dependency trees, and credential stores.
const DENIED_PROJECT_PATH_COMPONENTS: &[&str] = &[
    ".git",
    ".ssh",
    ".aws",
    ".gnupg",
    "dev-config",
    "dev-data",
    "node_modules",
    "target",
];

/// File extensions that carry credentials or raw database pages.
const DENIED_PROJECT_EXTENSIONS: &[&str] = &[
    "pem", "key", "p12", "pfx", "jks", "keystore", "kdbx", "db", "db-wal", "db-shm", "sqlite",
    "sqlite3",
];

/// Basename markers `scripts/check-repository-hygiene.sh` treats as credential
/// material. Kept in sync in spirit, not by machine: the repository check runs on
/// tracked paths, this check runs at request time on the resolved path.
const CREDENTIAL_BASENAME_MARKERS: &[&str] =
    &["key", "token", "password", "passwd", "credential", "secret"];

fn is_denied_project_basename(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "auth"
            | ".netrc"
            | "private_key"
            | "private-key"
            | "bootstrap-token"
            | ".npmrc"
            | ".pypirc"
    ) {
        return true;
    }
    if name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name.starts_with("id_ecdsa")
        || name.starts_with("id_dsa")
        || (name.starts_with("id_") && name.contains("_sk"))
    {
        return true;
    }
    if !CREDENTIAL_BASENAME_MARKERS
        .iter()
        .any(|marker| name.contains(marker))
    {
        return false;
    }
    // A marked basename is denied outright, or when it carries a format that can
    // hold a secret. Source extensions (`.rs`, `.ts`, `.tsx`) are deliberately not
    // in that set, so `backend/src/api/secrets.rs` stays readable.
    match name.rsplit_once('.') {
        None => true,
        Some((_, extension)) => matches!(
            extension,
            "json" | "yaml" | "yml" | "txt" | "toml" | "env" | "ini" | "conf" | "cfg"
        ),
    }
}

fn is_denied_project_path(path: &Path) -> bool {
    let denied_component = path.components().any(|component| match component {
        std::path::Component::Normal(part) => {
            let part = part.to_string_lossy().to_ascii_lowercase();
            DENIED_PROJECT_PATH_COMPONENTS.contains(&part.as_str())
        }
        _ => false,
    });
    if denied_component {
        return true;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    let name = name.to_ascii_lowercase();
    if name == ".env" || name.starts_with(".env.") {
        return true;
    }
    if Path::new(&name)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| DENIED_PROJECT_EXTENSIONS.contains(&extension))
    {
        return true;
    }
    is_denied_project_basename(&name)
}

/// A readable target must be a single-link regular file within the size bound.
/// Checking this before opening matters: opening a FIFO or device node can block
/// or consume a stream, so those types must never reach `open`.
fn check_readable_file_metadata(metadata: &std::fs::Metadata) -> std::result::Result<(), String> {
    if !metadata.is_file() {
        return Err("Path is not a regular file".to_string());
    }
    if metadata.len() > MAX_PROJECT_FILE_BYTES {
        return Err("File exceeds the readable size limit".to_string());
    }
    if has_multiple_hard_links(metadata) {
        return Err("Path is not a readable regular file".to_string());
    }
    Ok(())
}

#[cfg(unix)]
fn has_multiple_hard_links(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() > 1
}

#[cfg(not(unix))]
fn has_multiple_hard_links(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub fn read_project_file(root: &Path, rel_path: &str) -> std::result::Result<String, String> {
    use std::io::Read;

    if rel_path.trim().is_empty() {
        return Err("Missing file path".to_string());
    }
    let rel = Path::new(rel_path);
    if rel.is_absolute()
        || rel.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err("Path must stay inside the project source trees".to_string());
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|_| "Project root unavailable".to_string())?;
    let resolved = canonical_root
        .join(rel)
        .canonicalize()
        .map_err(|e| format!("Read error: {e}"))?;

    // Allow-list on the fully resolved path: only the project source trees are
    // readable, so a symlink inside a source tree cannot redirect the read to
    // configuration, credential material, or a path outside the root.
    let inside_source_tree = READABLE_PROJECT_SOURCE_TREES
        .iter()
        .filter_map(|tree| canonical_root.join(tree).canonicalize().ok())
        .any(|tree| resolved.starts_with(&tree));
    if !inside_source_tree {
        return Err("Path is not available to this tool".to_string());
    }
    // Defence in depth: credential names, credential formats, and state
    // directories stay refused even if one ever appears inside a source tree.
    let resolved_relative = resolved
        .strip_prefix(&canonical_root)
        .map_err(|_| "Path is not available to this tool".to_string())?;
    if is_denied_project_path(rel) || is_denied_project_path(resolved_relative) {
        return Err("Path is not available to this tool".to_string());
    }

    // Inspect before opening so special files never reach `open`, then take
    // metadata and content from one handle: no stat→read re-resolution.
    let pre_open = std::fs::symlink_metadata(&resolved).map_err(|e| format!("Read error: {e}"))?;
    check_readable_file_metadata(&pre_open)?;

    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Refuse a last-moment symlink swap of the final path component.
        options.custom_flags(nix::libc::O_NOFOLLOW);
    }
    let file = options
        .open(&resolved)
        .map_err(|e| format!("Read error: {e}"))?;
    let metadata = file.metadata().map_err(|e| format!("Read error: {e}"))?;
    check_readable_file_metadata(&metadata)?;

    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Read error: {e}"))?;
    if bytes.len() as u64 > MAX_PROJECT_FILE_BYTES {
        return Err("File exceeds the readable size limit".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8 text".to_string())
}

pub fn search_project_code(root: &Path, query: &str) -> std::result::Result<String, String> {
    if query.is_empty() || query.len() > 200 {
        return Err("Query must be 1-200 chars".to_string());
    }
    let search_dirs = ["backend/src", "frontend/src"];
    let mut hits: Vec<String> = Vec::new();

    for dir in &search_dirs {
        let target = root.join(dir);
        if let Ok(output) = std::process::Command::new("grep")
            .args([
                "-rn",
                "--include=*.rs",
                "--include=*.ts",
                "--include=*.tsx",
                "-m",
                "5",
                query,
                target.to_str().unwrap_or(""),
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            // Make paths relative to root
            for line in text.lines().take(30) {
                let rel = line
                    .strip_prefix(root.to_str().unwrap_or(""))
                    .unwrap_or(line);
                hits.push(rel.trim_start_matches('/').to_string());
            }
        }
    }

    if hits.is_empty() {
        Ok(format!("No results for: {query}"))
    } else {
        Ok(hits.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// `get_context`'s `routes` field is parsed out of a real source file on
    /// disk (`backend/src/api/mod.rs`) via `extract_routes` — this proves that
    /// if that file ever embedded a registered secret verbatim (e.g. in a route
    /// path or a comment), this bundle must not repeat it back into an AI
    /// context. Fixture a minimal project root with a `mod.rs` containing a
    /// route line that embeds a registered secret, and confirm it's stripped.
    #[tokio::test]
    async fn redaction_corpus_never_appears_in_get_context_output() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        let session_id = crate::api::mcp::test_support::user_with_session(&pool).await;

        let secret_value = "fakevendor_XYZ123456789registeredsecretvalue";
        let key: [u8; 32] = [5u8; 32];
        let enc = crate::api::secrets::encrypt(&key, secret_value).unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, description, value_enc, created_at, updated_at) VALUES ('s1', 'leaked-route', NULL, ?, 0, 0)",
        )
        .bind(&enc)
        .execute(&pool)
        .await
        .unwrap();

        let root = std::env::temp_dir().join(format!(
            "vt-redact-test-get-context-{}-{}",
            std::process::id(),
            secret_value.len(),
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("backend/src/api")).unwrap();
        std::fs::create_dir_all(root.join("frontend/dist")).unwrap();
        std::fs::write(
            root.join("backend/src/api/mod.rs"),
            format!(".route(\"/api/secret-{secret_value}\", get(handler))\n"),
        )
        .unwrap();

        let mut state = crate::api::mcp::test_support::build(pool);
        state.secrets_key = Arc::new(key);
        state.config = Arc::new(crate::config::Config {
            frontend_dir: root.join("frontend/dist"),
            ..crate::config::Config::default()
        });

        let jar = CookieJar::new().add(axum_extra::extract::cookie::Cookie::new(
            "vt_session",
            session_id,
        ));

        let resp = get_context(State(state), jar).await.unwrap();
        let body_text = serde_json::to_string(&resp.0).unwrap();

        let _ = std::fs::remove_dir_all(&root);

        assert!(
            !body_text.contains(secret_value),
            "registered secret leaked into get_context routes"
        );
        assert!(
            body_text.contains("REDACTED"),
            "expected the route path to show redaction happened"
        );
        assert!(
            body_text.contains("architecture"),
            "context bundle should still serialize normally"
        );
    }
}
