use crate::{auth, error::{AppError, Result}, AppState};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;

async fn require_admin(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let sid = jar.get("vt_session").map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &sid).await.map_err(AppError::Internal)?.ok_or(AppError::Unauthorized)?;
    if !matches!(user.role.as_str(), "owner" | "admin") { return Err(AppError::Forbidden); }
    Ok(user)
}

#[derive(Serialize)]
pub struct LlamaProcess {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
}

#[derive(Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_used_mb: u64,
    pub vram_total_mb: u64,
    pub utilization_pct: u64,
}

#[derive(Serialize)]
pub struct LlamaStatus {
    pub processes: Vec<LlamaProcess>,
    pub gpu: Option<GpuInfo>,
}

fn find_llama_processes() -> Vec<LlamaProcess> {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};
    let sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::new()),
    );
    sys.processes()
        .values()
        .filter(|p| {
            let name = p.name().to_lowercase();
            name.contains("llama") || name == "server"
                && p.cmd().iter().any(|a| a.to_lowercase().contains("llama"))
        })
        .map(|p| LlamaProcess {
            pid: p.pid().as_u32(),
            name: p.name().to_string(),
            cmd: p.cmd().join(" "),
        })
        .collect()
}

fn nvidia_smi_info() -> Option<GpuInfo> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,memory.used,memory.total,utilization.gpu", "--format=csv,noheader,nounits"])
        .output().ok()?;
    if !out.status.success() { return None; }
    let line = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = line.trim().splitn(4, ',').map(|s| s.trim()).collect();
    if parts.len() < 4 { return None; }
    Some(GpuInfo {
        name: parts[0].to_string(),
        vram_used_mb: parts[1].parse().unwrap_or(0),
        vram_total_mb: parts[2].parse().unwrap_or(0),
        utilization_pct: parts[3].parse().unwrap_or(0),
    })
}

pub async fn llama_status(State(state): State<AppState>, jar: CookieJar) -> Result<Json<LlamaStatus>> {
    require_admin(&state, &jar).await?;
    Ok(Json(LlamaStatus {
        processes: find_llama_processes(),
        gpu: nvidia_smi_info(),
    }))
}

pub async fn llama_unload(State(state): State<AppState>, jar: CookieJar) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    let procs = find_llama_processes();
    if procs.is_empty() {
        return Ok(Json(serde_json::json!({ "ok": true, "killed": 0, "message": "No llama processes found" })));
    }
    let mut killed = 0usize;
    for p in &procs {
        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(p.pid as i32), Signal::SIGTERM);
            killed += 1;
        }
        #[cfg(not(unix))]
        {
            let _ = std::process::Command::new("taskkill").args(["/PID", &p.pid.to_string(), "/F"]).output();
            killed += 1;
        }
    }
    Ok(Json(serde_json::json!({ "ok": true, "killed": killed, "message": format!("Sent SIGTERM to {} process(es)", killed) })))
}
