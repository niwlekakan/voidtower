use crate::{
    auth,
    error::{AppError, Result},
    AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StatusCheck {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub target: String,
    pub interval_secs: i64,
    pub enabled: bool,
    pub last_checked_at: Option<i64>,
    pub last_status: Option<String>,
    pub last_latency_ms: Option<i64>,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub name: String,
    pub r#type: String, // "http" | "tcp" | "ping"
    pub target: String,
    pub interval_secs: Option<i64>,
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or(AppError::Unauthorized)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub async fn list(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_user(&state, &jar).await?;
    let checks = sqlx::query_as::<_, StatusCheck>(
        "SELECT id, name, type, target, interval_secs, enabled, last_checked_at, last_status, last_latency_ms, created_at FROM status_checks ORDER BY created_at DESC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "checks": checks })))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRequest>,
) -> Result<Json<StatusCheck>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" {
        return Err(AppError::Forbidden);
    }
    let id = Uuid::new_v4().to_string();
    let interval = req.interval_secs.unwrap_or(60).clamp(10, 3600);
    let ts = now();
    sqlx::query(
        "INSERT INTO status_checks (id, name, type, target, interval_secs, enabled, created_at) VALUES (?, ?, ?, ?, ?, 1, ?)"
    )
    .bind(&id).bind(&req.name).bind(&req.r#type).bind(&req.target).bind(interval).bind(ts)
    .execute(&state.db).await.map_err(AppError::Database)?;

    let check = sqlx::query_as::<_, StatusCheck>(
        "SELECT id, name, type, target, interval_secs, enabled, last_checked_at, last_status, last_latency_ms, created_at FROM status_checks WHERE id = ?"
    )
    .bind(&id).fetch_one(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(check))
}

pub async fn delete(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_user(&state, &jar).await?;
    if user.role == "viewer" || user.role == "operator" {
        return Err(AppError::Forbidden);
    }
    sqlx::query("DELETE FROM status_checks WHERE id = ?")
        .bind(&id).execute(&state.db).await.map_err(AppError::Database)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// Called by the scheduler loop in main.rs
pub async fn run_check(pool: &sqlx::SqlitePool, check: &StatusCheck) {
    let start = std::time::Instant::now();
    let status = do_check(check).await;
    let latency_ms = start.elapsed().as_millis() as i64;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let _ = sqlx::query(
        "UPDATE status_checks SET last_checked_at = ?, last_status = ?, last_latency_ms = ? WHERE id = ?"
    )
    .bind(ts).bind(&status).bind(latency_ms).bind(&check.id)
    .execute(pool).await;

    // Alert on failure
    if status == "down" {
        crate::api::alerts::create_alert(
            pool,
            &format!("Check Failed: {}", check.name),
            &format!("{} ({}) is unreachable", check.name, check.target),
            "critical", "status", Some("check"), Some(&check.id),
        ).await;
    }
}

async fn do_check(check: &StatusCheck) -> String {
    let timeout = tokio::time::Duration::from_secs(10);
    match check.r#type.as_str() {
        "http" | "https" => {
            // Simple TCP connect to the host:port then check HTTP status
            let Ok(url) = check.target.parse::<reqwest_like::Url>() else { return "error".into(); };
            let host = url.host_str().unwrap_or("");
            let port = url.port_or_known_default().unwrap_or(80);
            match tokio::time::timeout(timeout, tokio::net::TcpStream::connect((host, port))).await {
                Ok(Ok(_)) => "up".into(),
                _ => "down".into(),
            }
        }
        "tcp" => {
            // target is "host:port"
            match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(&*check.target)).await {
                Ok(Ok(_)) => "up".into(),
                _ => "down".into(),
            }
        }
        _ => "unknown".into(),
    }
}

// ─── Public status page (no auth) ────────────────────────────────────────────

pub async fn public_page(State(state): State<AppState>) -> axum::response::Html<String> {
    let checks: Vec<StatusCheck> = sqlx::query_as(
        "SELECT id, name, type, target, interval_secs, enabled, last_checked_at, last_status, last_latency_ms, created_at \
         FROM status_checks WHERE enabled = 1 ORDER BY name"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::fs::read_to_string("/etc/hostname").map(|s| s.trim().to_string()))
        .unwrap_or_else(|_| "VoidTower".to_string());

    let all_up = checks.iter().all(|c| c.last_status.as_deref() == Some("up"));
    let any_down = checks.iter().any(|c| c.last_status.as_deref() == Some("down"));
    let (overall_label, overall_color) = if checks.is_empty() {
        ("No checks configured", "#6b7280")
    } else if any_down {
        ("Degraded", "#ef4444")
    } else if all_up {
        ("All systems operational", "#22c55e")
    } else {
        ("Checking…", "#f59e0b")
    };

    fn status_dot(s: Option<&str>) -> (&'static str, &'static str) {
        match s {
            Some("up")    => ("#22c55e", "Up"),
            Some("down")  => ("#ef4444", "Down"),
            Some("error") => ("#f59e0b", "Error"),
            _             => ("#6b7280", "Unknown"),
        }
    }

    fn fmt_latency(ms: Option<i64>) -> String {
        ms.map(|m| format!("{m} ms")).unwrap_or_else(|| "—".into())
    }

    fn fmt_ago(ts: Option<i64>) -> String {
        let Some(ts) = ts else { return "never".into() };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let secs = (now - ts).max(0);
        if secs < 120 { format!("{secs}s ago") }
        else if secs < 7200 { format!("{}m ago", secs / 60) }
        else { format!("{}h ago", secs / 3600) }
    }

    let rows: String = if checks.is_empty() {
        r#"<tr><td colspan="4" style="padding:32px;text-align:center;color:#6b7280">
            No status checks configured yet. Add them in the VoidTower dashboard.
           </td></tr>"#.into()
    } else {
        checks.iter().map(|c| {
            let (dot_color, dot_label) = status_dot(c.last_status.as_deref());
            format!(r#"<tr>
  <td style="padding:14px 16px;font-weight:500;color:#f4f7ff">{name}</td>
  <td style="padding:14px 16px;color:#a8b0c3;font-family:monospace;font-size:13px">{target}</td>
  <td style="padding:14px 16px">
    <span style="display:inline-flex;align-items:center;gap:6px">
      <span style="width:8px;height:8px;border-radius:50%;background:{dot_color};flex-shrink:0"></span>
      <span style="color:{dot_color};font-size:13px;font-weight:500">{dot_label}</span>
    </span>
  </td>
  <td style="padding:14px 16px;color:#687086;font-size:13px">{latency} · checked {ago}</td>
</tr>"#,
                name = c.name,
                target = c.target,
                latency = fmt_latency(c.last_latency_ms),
                ago = fmt_ago(c.last_checked_at),
            )
        }).collect()
    };

    let now_str = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        // Simple UTC time display
        format!("Updated {} UTC", secs)
    };

    let html = format!(r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{hostname} — Status</title>
<meta http-equiv="refresh" content="60">
<style>
  *{{box-sizing:border-box;margin:0;padding:0}}
  body{{background:#050509;color:#f4f7ff;font-family:system-ui,-apple-system,sans-serif;min-height:100vh;padding:40px 20px}}
  .wrap{{max-width:760px;margin:0 auto}}
  .header{{margin-bottom:40px}}
  h1{{font-size:22px;font-weight:600;margin-bottom:6px}}
  .overall{{display:inline-flex;align-items:center;gap:8px;font-size:15px;font-weight:500;margin-bottom:4px}}
  .dot{{width:10px;height:10px;border-radius:50%}}
  .sub{{color:#687086;font-size:13px}}
  table{{width:100%;border-collapse:collapse;background:#0b0d14;border-radius:8px;overflow:hidden;border:1px solid #25283a}}
  thead tr{{background:#11131d;border-bottom:1px solid #25283a}}
  th{{padding:10px 16px;text-align:left;font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:#687086;font-weight:500}}
  tbody tr+tr{{border-top:1px solid #1a1d2e}}
  tbody tr:hover{{background:#0d0f1a}}
  .footer{{margin-top:24px;font-size:12px;color:#3d4160;text-align:center}}
  a{{color:#8b5cf6;text-decoration:none}}
</style>
</head>
<body>
<div class="wrap">
  <div class="header">
    <h1>{hostname}</h1>
    <div class="overall">
      <span class="dot" style="background:{overall_color}"></span>
      <span style="color:{overall_color}">{overall_label}</span>
    </div>
    <p class="sub">{now_str} · auto-refreshes every 60 s</p>
  </div>

  <table>
    <thead>
      <tr>
        <th>Service</th>
        <th>Target</th>
        <th>Status</th>
        <th>Response</th>
      </tr>
    </thead>
    <tbody>{rows}</tbody>
  </table>

  <p class="footer">Powered by <a href="https://github.com/elwla/voidtower">VoidTower</a></p>
</div>
</body>
</html>"#);

    axum::response::Html(html)
}

// Needed only for URL parsing in do_check
mod reqwest_like {
    pub use std::str::FromStr;

    pub struct Url {
        pub scheme: String,
        pub host: String,
        pub port: Option<u16>,
    }

    impl Url {
        pub fn host_str(&self) -> Option<&str> { Some(&self.host) }
        pub fn port_or_known_default(&self) -> Option<u16> {
            self.port.or_else(|| match self.scheme.as_str() {
                "https" => Some(443),
                "http"  => Some(80),
                _ => None,
            })
        }
    }

    impl FromStr for Url {
        type Err = ();
        fn from_str(s: &str) -> Result<Self, ()> {
            // Minimal "scheme://host:port/path" parser
            let s = s.trim_start_matches("http://").trim_start_matches("https://");
            let scheme = if s.starts_with("https") { "https" } else { "http" }.to_string();
            let host_part = s.split('/').next().unwrap_or(s);
            let (host, port) = if let Some((h, p)) = host_part.rsplit_once(':') {
                (h.to_string(), p.parse::<u16>().ok())
            } else {
                (host_part.to_string(), None)
            };
            Ok(Url { scheme, host, port })
        }
    }
}
