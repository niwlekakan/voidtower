use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde_json::Value;
use std::path::Path;

use crate::{auth, error::{AppError, Result}, AppState};

async fn require_any_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar.get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn project_root(state: &AppState) -> std::path::PathBuf {
    // frontend_dir is frontend/dist — go up two levels to reach repo root
    state.config.frontend_dir
        .parent().and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

fn walk_rs_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
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
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
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
        if !trimmed.starts_with(".route(") { continue; }
        // .route("/api/foo", get(handler).post(handler2))
        let inner = trimmed.trim_start_matches(".route(").trim_end_matches(')');
        let mut parts = inner.splitn(2, ',');
        let path = parts.next().unwrap_or("").trim().trim_matches('"').to_string();
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

pub async fn get_context(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<Value>> {
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
    frontend_dir.parent().and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

pub fn read_project_file(root: &Path, rel_path: &str) -> std::result::Result<String, String> {
    // Sanitize: no path traversal outside root
    let rel = rel_path.trim_start_matches('/');
    if rel.contains("..") {
        return Err("Path traversal not allowed".to_string());
    }
    let full = root.join(rel);
    if !full.starts_with(root) {
        return Err("Path outside project root".to_string());
    }
    std::fs::read_to_string(&full).map_err(|e| format!("Read error: {e}"))
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
            .args(["-rn", "--include=*.rs", "--include=*.ts", "--include=*.tsx",
                   "-m", "5", query, target.to_str().unwrap_or("")])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            // Make paths relative to root
            for line in text.lines().take(30) {
                let rel = line.strip_prefix(root.to_str().unwrap_or("")).unwrap_or(line);
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
