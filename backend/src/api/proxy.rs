use crate::{
    audit, auth,
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

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn validate_domain(d: &str) -> Result<()> {
    if d.is_empty() || d.len() > 253 {
        return Err(AppError::BadRequest("Invalid domain length".into()));
    }
    // Allow hostname, subdomain.host.tld, and wildcard subdomain *.host.tld
    let stripped = d.strip_prefix("*.").unwrap_or(d);
    let ok = stripped.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !stripped.starts_with('.')
        && !stripped.ends_with('.');
    if !ok {
        return Err(AppError::BadRequest(
            "Domain must contain only letters, digits, dots and hyphens".into(),
        ));
    }
    Ok(())
}

fn validate_upstream(u: &str) -> Result<()> {
    if !u.starts_with("http://") && !u.starts_with("https://") {
        return Err(AppError::BadRequest(
            "Upstream must start with http:// or https://".into(),
        ));
    }
    // Block cloud metadata endpoints and unspecified addresses
    let lower = u.to_lowercase();
    if lower.contains("169.254.") || lower.contains("//0.0.0.0") || lower.contains("[::ffff:0]") {
        return Err(AppError::BadRequest("Upstream address is not permitted".into()));
    }
    Ok(())
}

// Detect nginx sites directory (Debian/Ubuntu vs Arch/RHEL)
fn nginx_sites_dir() -> &'static str {
    if std::path::Path::new("/etc/nginx/sites-enabled").exists() {
        "/etc/nginx/sites-enabled"
    } else {
        "/etc/nginx/conf.d"
    }
}

/// Host-side bind-mount path for the Docker nginx-proxy container's conf.d.
/// VoidTower writes proxy configs here; the container picks them up on reload.
const DOCKER_NGINX_CONF_DIR: &str = "/var/lib/voidtower/nginx/conf.d";

/// Returns the container ID of the running vt-nginx-proxy container, or None.
fn docker_nginx_container_id() -> Option<String> {
    let out = std::process::Command::new("docker")
        .args([
            "ps",
            "--filter", "label=com.docker.compose.project=vt-nginx-proxy",
            "--filter", "status=running",
            "--format", "{{.ID}}",
            "--latest",
        ])
        .output()
        .ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if id.is_empty() { None } else { Some(id) }
}

/// Returns the conf.d path to write proxy configs to — Docker bind-mount when
/// nginx-proxy container is running, system nginx conf.d otherwise.
fn effective_conf_dir() -> String {
    if docker_nginx_container_id().is_some() {
        DOCKER_NGINX_CONF_DIR.to_string()
    } else {
        nginx_sites_dir().to_string()
    }
}

fn conf_path(domain: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(effective_conf_dir())
        .join(format!("voidtower-{domain}.conf"))
}

fn embed_headers() -> &'static str {
    "        proxy_hide_header X-Frame-Options;\n        add_header X-Frame-Options \"ALLOWALL\" always;\n        add_header Content-Security-Policy \"frame-ancestors *\" always;"
}

// Port-based nginx config for app embeds — no server_name, listen on a unique
// port so any LAN client can reach it via http://<server-ip>:<embed_port>/
// without requiring any DNS or /etc/hosts configuration.
pub fn write_nginx_port_conf(slug: &str, upstream: &str, port: u16) -> Result<()> {
    let path = conf_path(&format!("embed-port-{slug}"));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let content = format!(
        r#"# Managed by VoidTower — do not edit manually
server {{
    listen 0.0.0.0:{port};

    location / {{
        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;
        proxy_hide_header X-Frame-Options;
        add_header X-Frame-Options "ALLOWALL" always;
        add_header Content-Security-Policy "frame-ancestors *" always;
    }}
}}
"#
    );
    std::fs::write(&path, content)
        .map_err(|e| AppError::BadRequest(format!("Cannot write nginx config: {e}")))?;
    Ok(())
}

fn write_nginx_conf(domain: &str, upstream: &str, ssl: bool, allow_embed: bool) -> Result<()> {
    let path = conf_path(domain);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let embed = if allow_embed { format!("\n{}", embed_headers()) } else { String::new() };
    let content = if ssl {
        format!(
            r#"# Managed by VoidTower — do not edit manually
server {{
    listen 80;
    server_name {domain};
    return 301 https://$server_name$request_uri;
}}

server {{
    listen 443 ssl http2;
    server_name {domain};

    ssl_certificate     /etc/letsencrypt/live/{domain}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    location / {{
        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;{embed}
    }}
}}
"#
        )
    } else {
        format!(
            r#"# Managed by VoidTower — do not edit manually
server {{
    listen 80;
    server_name {domain};

    location / {{
        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;{embed}
    }}
}}
"#
        )
    };

    std::fs::write(&path, content)
        .map_err(|e| AppError::BadRequest(format!("Cannot write nginx config: {e}")))?;

    Ok(())
}

fn remove_nginx_conf(domain: &str) {
    let _ = std::fs::remove_file(conf_path(domain));
}

fn nginx_conf_content(domain: &str, upstream: &str, ssl: bool, allow_embed: bool) -> String {
    let embed = if allow_embed { format!("\n{}", embed_headers()) } else { String::new() };
    if ssl {
        format!(
            r#"# Managed by VoidTower — do not edit manually
server {{
    listen 80;
    server_name {domain};
    return 301 https://$server_name$request_uri;
}}

server {{
    listen 443 ssl http2;
    server_name {domain};

    ssl_certificate     /etc/letsencrypt/live/{domain}/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/{domain}/privkey.pem;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    location / {{
        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;{embed}
    }}
}}
"#
        )
    } else {
        format!(
            r#"# Managed by VoidTower — do not edit manually
server {{
    listen 80;
    server_name {domain};

    location / {{
        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;{embed}
    }}
}}
"#
        )
    }
}

fn is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

// nginx -t outputs "syntax is ok" to stderr even when it exits non-zero due to
// permission errors on the PID file. Check the output text instead of exit code.
fn nginx_test_ok() -> bool {
    if let Some(id) = docker_nginx_container_id() {
        let Ok(out) = std::process::Command::new("docker")
            .args(["exec", &id, "nginx", "-t"])
            .output()
        else { return false; };
        let stderr = String::from_utf8_lossy(&out.stderr);
        return stderr.contains("syntax is ok") || stderr.contains("test is successful");
    }
    let nginx = nginx_bin();
    let Ok(out) = std::process::Command::new(&nginx).args(["-t"]).output() else {
        return false;
    };
    let stderr = String::from_utf8_lossy(&out.stderr);
    stderr.contains("syntax is ok") || stderr.contains("test is successful")
}

fn nginx_active() -> bool {
    if docker_nginx_container_id().is_some() {
        return true;
    }
    std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "nginx"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn reload_nginx() -> std::result::Result<String, String> {
    // Docker mode: reload nginx inside the running nginx-proxy container
    if let Some(id) = docker_nginx_container_id() {
        let out = std::process::Command::new("docker")
            .args(["exec", &id, "nginx", "-s", "reload"])
            .output()
            .map_err(|e| e.to_string())?;
        return if out.status.success() {
            Ok("nginx-proxy reloaded".into())
        } else {
            Err(format!(
                "docker exec reload failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        };
    }

    // System nginx fallback
    let nginx = nginx_bin();
    if !nginx_test_ok() {
        let msg = std::process::Command::new(&nginx)
            .args(["-t"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
            .unwrap_or_else(|e| format!("nginx not found: {e}"));
        return Err(msg);
    }

    let systemctl = systemctl_bin();
    let attempts: Vec<Vec<&str>> = vec![
        vec![nginx.as_str(), "-s", "reload"],
        vec!["sudo", "-n", "systemctl", "reload", "nginx"],
        vec![systemctl.as_str(), "reload", "nginx"],
        vec!["sudo", "-n", nginx.as_str(), "-s", "reload"],
    ];

    for args in &attempts {
        let Ok(out) = std::process::Command::new(args[0]).args(&args[1..]).output() else { continue };
        if out.status.success() {
            return Ok("nginx reloaded".into());
        }
    }

    Err("Could not reload nginx — ensure VoidTower has permission to reload nginx (see Proxies page for setup instructions)".into())
}

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProxyConfig {
    pub id: String,
    pub domain: String,
    pub upstream: String,
    pub ssl: bool,
    pub enabled: bool,
    pub allow_embed: bool,
    pub created_at: i64,
}

#[derive(Deserialize)]
pub struct CreateRequest {
    pub domain: String,
    pub upstream: String,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub allow_embed: bool,
    #[serde(default)]
    pub dry_run: bool,
}

fn which_path(cmd: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn nginx_bin() -> String {
    which_path("nginx").unwrap_or_else(|| "/usr/sbin/nginx".into())
}

fn systemctl_bin() -> String {
    which_path("systemctl").unwrap_or_else(|| "/usr/bin/systemctl".into())
}

fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[allow(dead_code)]
fn detect_package_manager() -> Option<(&'static str, Vec<&'static str>)> {
    if which("pacman") {
        Some(("pacman", vec!["-S", "--noconfirm", "nginx"]))
    } else if which("apt-get") {
        Some(("apt-get", vec!["install", "-y", "nginx"]))
    } else if which("dnf") {
        Some(("dnf", vec!["install", "-y", "nginx"]))
    } else if which("yum") {
        Some(("yum", vec!["install", "-y", "nginx"]))
    } else if which("zypper") {
        Some(("zypper", vec!["install", "-y", "nginx"]))
    } else {
        None
    }
}

fn nginx_conf_path() -> &'static str {
    if std::path::Path::new("/etc/nginx/nginx.conf").exists() {
        "/etc/nginx/nginx.conf"
    } else if std::path::Path::new("/etc/nginx/conf/nginx.conf").exists() {
        "/etc/nginx/conf/nginx.conf"
    } else {
        "/etc/nginx/nginx.conf"
    }
}

fn conf_d_has_include() -> bool {
    let conf = std::fs::read_to_string(nginx_conf_path()).unwrap_or_default();
    // sites-enabled and conf.d are both valid include targets
    conf.contains("conf.d") || conf.contains("sites-enabled")
}

fn conf_d_writable() -> bool {
    // Test the directory we actually write proxy configs to, not a hardcoded path
    let dir = std::path::Path::new(nginx_sites_dir());
    if !dir.exists() { return false; }
    let tmp = dir.join(".voidtower-write-test");
    if std::fs::write(&tmp, b"").is_ok() {
        let _ = std::fs::remove_file(&tmp);
        true
    } else {
        false
    }
}

fn can_reload_nginx() -> bool {
    // Root can always reload nginx directly (Docker container case)
    if is_root() {
        return true;
    }
    let sc = systemctl_bin();
    // sudo -n -l <cmd> exits 0 if <cmd> is allowed without a password.
    if let Ok(o) = std::process::Command::new("sudo")
        .args(["-n", "-l", &sc, "reload", "nginx"])
        .output()
    {
        let stderr = String::from_utf8_lossy(&o.stderr);
        if stderr.contains("syntax error") || stderr.contains("parse error") {
            return false; // broken sudoers
        }
        if o.status.success() {
            return true;
        }
    }
    // Fallback: our managed file exists and no broken sudoers detected
    std::path::Path::new("/etc/sudoers.d/voidtower-nginx").exists()
}

fn current_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| {
            std::process::Command::new("id")
                .arg("-un")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "voidtower".to_string())
        })
}

#[derive(Clone, Copy, PartialEq)]
enum NginxMode { Docker, System, None }

struct NginxSetupStatus {
    mode: NginxMode,
    conf_d_exists: bool,
    conf_d_writable: bool,
    has_include: bool,
    can_reload: bool,
    /// Docker mode only: false means the container is not running yet (vs socket access issue)
    container_running: bool,
}

fn check_nginx_setup() -> NginxSetupStatus {
    let docker_conf_d = std::path::Path::new(DOCKER_NGINX_CONF_DIR);

    if let Some(id) = docker_nginx_container_id() {
        let conf_d_exists = docker_conf_d.exists();
        let conf_d_writable = conf_d_exists && {
            let tmp = docker_conf_d.join(".vt-write-test");
            if std::fs::write(&tmp, b"").is_ok() { let _ = std::fs::remove_file(&tmp); true } else { false }
        };
        let can_reload = std::process::Command::new("docker")
            .args(["exec", &id, "echo", "ok"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        return NginxSetupStatus {
            mode: NginxMode::Docker,
            conf_d_exists,
            conf_d_writable,
            has_include: true,
            can_reload,
            container_running: true,
        };
    }

    // Docker directory exists but container not yet running — user chose Docker mode.
    // Stay in Docker mode so system nginx paths (which require root) are not used.
    if docker_conf_d.exists() {
        let conf_d_writable = {
            let tmp = docker_conf_d.join(".vt-write-test");
            if std::fs::write(&tmp, b"").is_ok() { let _ = std::fs::remove_file(&tmp); true } else { false }
        };
        return NginxSetupStatus {
            mode: NginxMode::Docker,
            conf_d_exists: true,
            conf_d_writable,
            has_include: true,
            can_reload: false,
            container_running: false,
        };
    }

    if which("nginx") {
        let conf_d_exists = std::path::Path::new("/etc/nginx/conf.d").exists();
        return NginxSetupStatus {
            mode: NginxMode::System,
            conf_d_exists,
            conf_d_writable: conf_d_writable(),
            has_include: conf_d_has_include(),
            can_reload: can_reload_nginx(),
            container_running: false,
        };
    }
    NginxSetupStatus {
        mode: NginxMode::None,
        conf_d_exists: false,
        conf_d_writable: false,
        has_include: false,
        can_reload: false,
        container_running: false,
    }
}

// ─── Public wrappers for use by other modules ────────────────────────────────

pub fn nginx_active_pub() -> bool { nginx_active() }

pub fn reload_nginx_pub() -> std::result::Result<String, String> { reload_nginx() }

// ─── Handlers ────────────────────────────────────────────────────────────────

pub async fn nginx_setup_status(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let s = tokio::task::spawn_blocking(check_nginx_setup).await.unwrap();
    let mut steps: Vec<serde_json::Value> = Vec::new();

    let mode_str = match s.mode {
        NginxMode::Docker => "docker",
        NginxMode::System => "system",
        NginxMode::None   => "none",
    };

    match s.mode {
        NginxMode::Docker => {
            if !s.conf_d_exists {
                steps.push(serde_json::json!({
                    "label": "Create nginx conf.d bind-mount directory",
                    "cmd": format!("sudo mkdir -p {DOCKER_NGINX_CONF_DIR} && sudo chown $(whoami) {DOCKER_NGINX_CONF_DIR}")
                }));
            } else if !s.conf_d_writable {
                steps.push(serde_json::json!({
                    "label": format!("Grant write access to {DOCKER_NGINX_CONF_DIR}"),
                    "cmd": format!("sudo chown $(whoami) {DOCKER_NGINX_CONF_DIR}")
                }));
            }
            if !s.can_reload {
                if !s.container_running {
                    steps.push(serde_json::json!({
                        "label": "Deploy and start the nginx-proxy container from App Vault",
                        "cmd": null,
                        "app_id": "nginx-proxy"
                    }));
                } else {
                    steps.push(serde_json::json!({
                        "label": "VoidTower needs Docker socket access",
                        "cmd": format!("sudo usermod -aG docker {}", current_username())
                    }));
                }
            }
        }
        NginxMode::System => {
            let user = current_username();
            let conf = nginx_conf_path();
            if !s.conf_d_exists {
                steps.push(serde_json::json!({
                    "label": "Create conf.d directory",
                    "cmd": "sudo mkdir -p /etc/nginx/conf.d"
                }));
            }
            if !s.has_include {
                steps.push(serde_json::json!({
                    "label": "Add include to nginx.conf",
                    "cmd": format!("sudo sed -i 's/http {{/http {{\\n    include \\/etc\\/nginx\\/conf.d\\/*.conf;/' {conf}")
                }));
            }
            if !s.conf_d_writable {
                let sites_dir = nginx_sites_dir();
                steps.push(serde_json::json!({
                    "label": format!("Grant write access to {sites_dir}"),
                    "cmd": format!("sudo chown -R {user}:{user} {sites_dir}")
                }));
            }
            if !s.can_reload {
                let ng = nginx_bin();
                let sc = systemctl_bin();
                let args = [
                    format!("{user} ALL=(ALL) NOPASSWD: {sc} start nginx"),
                    format!("{user} ALL=(ALL) NOPASSWD: {sc} stop nginx"),
                    format!("{user} ALL=(ALL) NOPASSWD: {sc} restart nginx"),
                    format!("{user} ALL=(ALL) NOPASSWD: {sc} reload nginx"),
                    format!("{user} ALL=(ALL) NOPASSWD: {ng} -t"),
                    format!("{user} ALL=(ALL) NOPASSWD: {ng} -s reload"),
                    format!("{user} ALL=(ALL) NOPASSWD: /usr/bin/tail -n 100 /var/log/nginx/error.log"),
                    format!("{user} ALL=(ALL) NOPASSWD: /usr/bin/tail -n 100 /var/log/nginx/access.log"),
                ]
                .iter()
                .map(|l| format!("  '{l}' \\"))
                .collect::<Vec<_>>()
                .join("\n");
                steps.push(serde_json::json!({ "label": "Allow passwordless nginx management",
                    "cmd": format!("printf '%s\\n' \\\n{args}\n  | sudo tee /etc/sudoers.d/voidtower-nginx > /dev/null")
                }));
            }
        }
        NginxMode::None => {
            // No nginx at all — suggest Docker app first, system nginx as fallback
            steps.push(serde_json::json!({
                "label": "Deploy nginx-proxy from App Vault (recommended)",
                "cmd": null,
                "app_id": "nginx-proxy"
            }));
        }
    }

    let combined = if steps.is_empty() || s.mode == NginxMode::None {
        None
    } else {
        let cmds: Vec<&str> = steps.iter().filter_map(|s| s["cmd"].as_str()).collect();
        if cmds.is_empty() { None } else { Some(cmds.join(" && \\\n")) }
    };

    Ok(Json(serde_json::json!({
        "ready": steps.is_empty(),
        "mode": mode_str,
        "checks": {
            "conf_d_exists": s.conf_d_exists,
            "conf_d_writable": s.conf_d_writable,
            "has_include": s.has_include,
            "can_reload": s.can_reload,
        },
        "steps": steps,
        "combined_cmd": combined,
    })))
}

pub async fn nginx_install_cmd(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let (pm, install_cmd, enable_cmd) = if which("pacman") {
        ("pacman", "sudo pacman -S --noconfirm nginx", "sudo systemctl enable --now nginx")
    } else if which("apt-get") {
        ("apt-get", "sudo apt-get install -y nginx", "sudo systemctl enable --now nginx")
    } else if which("dnf") {
        ("dnf", "sudo dnf install -y nginx", "sudo systemctl enable --now nginx")
    } else if which("yum") {
        ("yum", "sudo yum install -y nginx", "sudo systemctl enable --now nginx")
    } else if which("zypper") {
        ("zypper", "sudo zypper install -y nginx", "sudo systemctl enable --now nginx")
    } else {
        return Err(AppError::BadRequest(
            "No supported package manager detected (apt/dnf/pacman/zypper)".into(),
        ));
    };

    Ok(Json(serde_json::json!({
        "pm": pm,
        "cmd": format!("{install_cmd} && {enable_cmd}"),
        "docker_app_id": "nginx-proxy",
    })))
}

pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let proxies = sqlx::query_as::<_, ProxyConfig>(
        "SELECT id, domain, upstream, ssl, enabled, allow_embed, created_at FROM proxy_configs ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (nginx_ok, nginx_backend) = tokio::task::spawn_blocking(|| {
        let docker = docker_nginx_container_id().is_some();
        let system = !docker && which("nginx") && (nginx_test_ok() || nginx_active());
        let ok = docker || system;
        let backend = if docker { "docker" } else if system { "system" } else { "none" };
        (ok, backend)
    }).await.unwrap();

    Ok(Json(serde_json::json!({
        "proxies": proxies,
        "nginx_available": nginx_ok,
        "nginx_backend": nginx_backend,
        "sites_dir": if nginx_ok { effective_conf_dir() } else { nginx_sites_dir().to_string() },
    })))
}

pub async fn create(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<CreateRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    validate_domain(&req.domain)?;
    validate_upstream(&req.upstream)?;

    if req.dry_run {
        let conf_file = conf_path(&req.domain);
        let exists = conf_file.exists();
        let content = nginx_conf_content(&req.domain, &req.upstream, req.ssl, req.allow_embed);
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Create Nginx Proxy",
                "risk": "low",
                "changes": [
                    { "label": "Domain", "value": req.domain },
                    { "label": "Upstream", "value": req.upstream },
                    { "label": "SSL", "value": if req.ssl { "yes (Let's Encrypt)" } else { "no" } },
                    { "label": "Allow embed", "value": if req.allow_embed { "yes (strips X-Frame-Options)" } else { "no" } },
                    { "label": "Config file", "value": conf_file.display().to_string() },
                    { "label": "Config file action", "value": if exists { "overwrite existing" } else { "create new" } },
                    { "label": "Rollback", "value": "Delete conf file and reload nginx" },
                ],
                "preview": content,
            }
        })));
    }

    let id = Uuid::new_v4().to_string();
    let now = unix_now();

    sqlx::query(
        "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, created_at) VALUES (?,?,?,?,1,?,?)",
    )
    .bind(&id)
    .bind(&req.domain)
    .bind(&req.upstream)
    .bind(req.ssl)
    .bind(req.allow_embed)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::BadRequest(format!("Domain '{}' already has a proxy rule", req.domain))
        } else {
            AppError::Internal(e.into())
        }
    })?;

    write_nginx_conf(&req.domain, &req.upstream, req.ssl, req.allow_embed)?;

    let reload_msg = reload_nginx().unwrap_or_else(|e| format!("warning: {e}"));

    audit::log(
        &state.db, Some(&user.id), "human", "proxy.create",
        Some("proxy"), Some(&id), "success", None,
        Some(&format!("domain={},upstream={}", req.domain, req.upstream)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "id": id, "nginx": reload_msg })))
}

/// Shared helper: insert a proxy record and write/reload nginx.
/// Used by both the HTTP `create` handler and `apps::expose_app`.
pub async fn create_proxy_record(
    db: &sqlx::SqlitePool,
    domain: &str,
    upstream: &str,
    ssl: bool,
    allow_embed: bool,
) -> Result<String> {
    validate_domain(domain)?;
    validate_upstream(upstream)?;

    let id = Uuid::new_v4().to_string();
    let now = unix_now();

    sqlx::query(
        "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, created_at) VALUES (?,?,?,?,1,?,?)",
    )
    .bind(&id)
    .bind(domain)
    .bind(upstream)
    .bind(ssl)
    .bind(allow_embed)
    .bind(now)
    .execute(db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::BadRequest(format!("Domain '{domain}' already has a proxy rule"))
        } else {
            AppError::Internal(e.into())
        }
    })?;

    write_nginx_conf(domain, upstream, ssl, allow_embed)?;
    let _ = reload_nginx();
    Ok(id)
}

pub async fn delete_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(proxy_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    let row: Option<(String,)> =
        sqlx::query_as("SELECT domain FROM proxy_configs WHERE id = ?")
            .bind(&proxy_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

    let (domain,) = row.ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;

    sqlx::query("DELETE FROM proxy_configs WHERE id = ?")
        .bind(&proxy_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    remove_nginx_conf(&domain);
    let reload_msg = reload_nginx().unwrap_or_else(|e| format!("warning: {e}"));

    audit::log(
        &state.db, Some(&user.id), "human", "proxy.delete",
        Some("proxy"), Some(&proxy_id), "success", None,
        Some(&format!("domain={domain}")),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "nginx": reload_msg })))
}

pub async fn update_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(proxy_id): Path<String>,
    Json(req): Json<CreateRequest>,
) -> Result<Json<serde_json::Value>> {
    let user = require_admin(&state, &jar).await?;

    validate_domain(&req.domain)?;
    validate_upstream(&req.upstream)?;

    // Fetch current row to get the old domain and enabled state
    let row: Option<(String, bool)> =
        sqlx::query_as("SELECT domain, enabled FROM proxy_configs WHERE id = ?")
            .bind(&proxy_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

    let (old_domain, enabled) = row.ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;

    if req.dry_run {
        let conf_file = conf_path(&req.domain);
        let content = nginx_conf_content(&req.domain, &req.upstream, req.ssl, req.allow_embed);
        let domain_changed = old_domain != req.domain;
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Update Nginx Proxy",
                "risk": "low",
                "changes": [
                    { "label": "Domain", "value": format!("{} → {}", old_domain, req.domain) },
                    { "label": "Upstream", "value": req.upstream },
                    { "label": "SSL", "value": if req.ssl { "yes (Let's Encrypt)" } else { "no" } },
                    { "label": "Allow embed", "value": if req.allow_embed { "yes (strips X-Frame-Options)" } else { "no" } },
                    { "label": "Config file", "value": conf_file.display().to_string() },
                    { "label": "Config file action", "value": if domain_changed { "rename old conf + write new" } else { "overwrite in place" } },
                    { "label": "Nginx reload", "value": if enabled { "yes" } else { "no (proxy is disabled)" } },
                    { "label": "Rollback", "value": "Revert domain/upstream fields and re-save" },
                ],
                "preview": if enabled { Some(content) } else { None::<String> },
            }
        })));
    }

    sqlx::query(
        "UPDATE proxy_configs SET domain = ?, upstream = ?, ssl = ?, allow_embed = ? WHERE id = ?",
    )
    .bind(&req.domain)
    .bind(&req.upstream)
    .bind(req.ssl)
    .bind(req.allow_embed)
    .bind(&proxy_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::BadRequest(format!("Domain '{}' already has a proxy rule", req.domain))
        } else {
            AppError::Internal(e.into())
        }
    })?;

    // Remove old conf when domain changed (regardless of enabled state)
    if old_domain != req.domain {
        remove_nginx_conf(&old_domain);
    }

    // Only write/reload nginx if the proxy is currently enabled
    let reload_msg = if enabled {
        write_nginx_conf(&req.domain, &req.upstream, req.ssl, req.allow_embed)?;
        reload_nginx().unwrap_or_else(|e| format!("warning: {e}"))
    } else {
        "proxy is disabled — nginx not updated".into()
    };

    audit::log(
        &state.db, Some(&user.id), "human", "proxy.update",
        Some("proxy"), Some(&proxy_id), "success", None,
        Some(&format!("domain={},upstream={}", req.domain, req.upstream)),
    ).await;

    Ok(Json(serde_json::json!({ "ok": true, "nginx": reload_msg })))
}


// ── AI auto-proxy ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AiAutoReq {
    pub upstream: String,
}

pub async fn ai_auto_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<AiAutoReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;
    validate_upstream(&req.upstream)?;

    let hostname = tokio::task::spawn_blocking(|| {
        std::process::Command::new("hostname")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "localhost".to_string())
    })
    .await
    .unwrap();

    let domain = format!("ai.{hostname}");

    // Remove existing proxy for this domain so we can recreate with allow_embed=true
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM proxy_configs WHERE domain = ?")
        .bind(&domain)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if let Some((existing_id,)) = existing {
        sqlx::query("DELETE FROM proxy_configs WHERE id = ?")
            .bind(&existing_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        remove_nginx_conf(&domain);
    }

    let id = Uuid::new_v4().to_string();
    let now = unix_now();

    sqlx::query(
        "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, created_at) VALUES (?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&domain)
    .bind(&req.upstream)
    .bind(false)
    .bind(true)
    .bind(true)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    write_nginx_conf(&domain, &req.upstream, false, true)?;
    let reload_msg = reload_nginx().unwrap_or_else(|e| format!("warning: {e}"));

    audit::log(
        &state.db, None, "human", "proxy.ai-auto",
        Some("proxy"), Some(&id), "success", None,
        Some(&format!("domain={domain},upstream={}", req.upstream)),
    ).await;

    Ok(Json(serde_json::json!({
        "ok": true,
        "domain": domain,
        "url": format!("http://{domain}"),
        "nginx": reload_msg,
    })))
}

// ── nginx management ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct NginxActionReq {
    pub action: String, // start | stop | restart | reload | test
}

pub async fn nginx_action(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<NginxActionReq>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let action = req.action.as_str();

    match action {
        "test" => {
            let out = tokio::task::spawn_blocking(|| {
                if let Some(id) = docker_nginx_container_id() {
                    if let Ok(o) = std::process::Command::new("docker")
                        .args(["exec", &id, "nginx", "-t"])
                        .output()
                    {
                        let output = format!(
                            "{}{}",
                            String::from_utf8_lossy(&o.stdout),
                            String::from_utf8_lossy(&o.stderr),
                        );
                        return (o.status.success(), output);
                    }
                    return (false, "docker exec failed".to_string());
                }
                let nginx = nginx_bin();
                for cmd in &[
                    vec!["sudo", nginx.as_str(), "-t"],
                    vec![nginx.as_str(), "-t"],
                ] {
                    if let Ok(o) = std::process::Command::new(cmd[0]).args(&cmd[1..]).output() {
                        let stdout = String::from_utf8_lossy(&o.stdout).into_owned();
                        let stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                        return (o.status.success(), format!("{stdout}{stderr}"));
                    }
                }
                (false, "nginx binary not found".to_string())
            })
            .await
            .unwrap();

            return Ok(Json(serde_json::json!({ "ok": out.0, "output": out.1 })));
        }
        "start" | "stop" | "restart" | "reload" => {}
        _ => {
            return Err(AppError::BadRequest(
                "action must be start|stop|restart|reload|test".into(),
            ));
        }
    }

    let action = action.to_string();
    let result = tokio::task::spawn_blocking(move || {
        // Docker mode: manage the nginx-proxy container directly
        if let Some(id) = docker_nginx_container_id() {
            let docker_action = match action.as_str() {
                "reload"  => return reload_nginx(),
                "restart" => "restart",
                "stop"    => "stop",
                "start"   => "start",
                _         => return Err(format!("Unknown action: {action}")),
            };
            let out = std::process::Command::new("docker")
                .args([docker_action, &id])
                .output()
                .map_err(|e| e.to_string())?;
            return if out.status.success() {
                Ok(format!("nginx-proxy container {action} succeeded"))
            } else {
                Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
            };
        }

        // System nginx fallback
        let systemctl = systemctl_bin();
        let cmds: &[&[&str]] = &[
            &["sudo", systemctl.as_str(), action.as_str(), "nginx"],
            &[systemctl.as_str(), action.as_str(), "nginx"],
        ];
        for cmd in cmds {
            if let Ok(out) = std::process::Command::new(cmd[0]).args(&cmd[1..]).output() {
                if out.status.success() {
                    return Ok(format!("nginx {action} succeeded"));
                }
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                if !stderr.is_empty()
                    && !stderr.contains("not found")
                    && !stderr.contains("No such file")
                {
                    return Err(stderr.trim().to_string());
                }
            }
        }
        Err(format!(
            "Could not {action} nginx — run the sudoers setup command on the Proxies page"
        ))
    })
    .await
    .unwrap();

    match result {
        Ok(msg) => Ok(Json(serde_json::json!({ "ok": true, "message": msg }))),
        Err(e) => Ok(Json(serde_json::json!({ "ok": false, "message": e }))),
    }
}

pub async fn nginx_logs(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let result = tokio::task::spawn_blocking(|| {
        // Docker mode: get logs from the nginx-proxy container
        if let Some(id) = docker_nginx_container_id() {
            if let Ok(out) = std::process::Command::new("docker")
                .args(["logs", "--tail", "100", &id])
                .output()
            {
                let text = format!(
                    "{}{}",
                    String::from_utf8_lossy(&out.stdout),
                    String::from_utf8_lossy(&out.stderr),
                );
                let lines: Vec<String> = text.lines().map(String::from).collect();
                return ("docker:nginx-proxy".to_string(), lines);
            }
        }

        // System nginx fallback
        let candidates = [
            "/var/log/nginx/error.log",
            "/var/log/nginx/access.log",
        ];
        for path in &candidates {
            for cmd in &[
                vec!["sudo", "/usr/bin/tail", "-n", "100", path],
                vec!["tail", "-n", "100", path],
            ] {
                if let Ok(out) = std::process::Command::new(cmd[0]).args(&cmd[1..]).output() {
                    if out.status.success() {
                        let text = String::from_utf8_lossy(&out.stdout).into_owned();
                        let lines: Vec<String> = text.lines().map(String::from).collect();
                        return (path.to_string(), lines);
                    }
                }
            }
            if let Ok(content) = std::fs::read_to_string(path) {
                let lines: Vec<String> = content.lines().map(String::from).collect();
                let lines: Vec<String> = lines.into_iter().rev().take(100).collect::<Vec<_>>().into_iter().rev().collect();
                return (path.to_string(), lines);
            }
        }
        (String::new(), vec!["No nginx log files readable. Run the sudoers setup command on the Proxies page.".to_string()])
    })
    .await
    .unwrap();

    Ok(Json(serde_json::json!({ "path": result.0, "lines": result.1 })))
}

pub async fn nginx_status(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let status = tokio::task::spawn_blocking(|| {
        // Docker mode: check container status
        if let Some(id) = docker_nginx_container_id() {
            let out = std::process::Command::new("docker")
                .args(["inspect", "--format",
                    "{{.State.Status}} {{.State.Pid}}",
                    &id])
                .output();
            if let Ok(out) = out {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let mut parts = text.splitn(2, ' ');
                let state = parts.next().unwrap_or("unknown");
                let pid: Option<u32> = parts.next().and_then(|s| s.parse().ok());
                return serde_json::json!({
                    "active": state == "running",
                    "state": format!("active ({})", state),
                    "pid": pid,
                    "mode": "docker",
                });
            }
        }

        // System nginx fallback
        let systemctl = systemctl_bin();
        let out = std::process::Command::new(&systemctl)
            .args(["status", "nginx", "--no-pager", "-l"])
            .output();

        let Ok(out) = out else {
            return serde_json::json!({ "active": false, "state": "unknown", "pid": null, "mode": "system" });
        };

        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let mut state_str = "inactive".to_string();
        let mut pid: Option<u32> = None;

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Active:") {
                state_str = trimmed
                    .trim_start_matches("Active:")
                    .split_whitespace()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            if trimmed.starts_with("Main PID:") {
                pid = trimmed
                    .trim_start_matches("Main PID:")
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok());
            }
        }

        let active = state_str.contains("active (running)");
        serde_json::json!({ "active": active, "state": state_str, "pid": pid, "mode": "system" })
    })
    .await
    .unwrap();

    Ok(Json(status))
}

pub async fn toggle(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(proxy_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let row: Option<(String, bool, String, bool, String, bool)> = sqlx::query_as(
        "SELECT id, enabled, domain, ssl, upstream, allow_embed FROM proxy_configs WHERE id = ?",
    )
    .bind(&proxy_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let (_, enabled, domain, ssl, upstream, allow_embed) =
        row.ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;

    let new_enabled = !enabled;

    sqlx::query("UPDATE proxy_configs SET enabled = ? WHERE id = ?")
        .bind(new_enabled)
        .bind(&proxy_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if new_enabled {
        write_nginx_conf(&domain, &upstream, ssl, allow_embed)?;
    } else {
        remove_nginx_conf(&domain);
    }

    let reload_msg = reload_nginx().unwrap_or_else(|e| format!("warning: {e}"));

    Ok(Json(serde_json::json!({ "ok": true, "enabled": new_enabled, "nginx": reload_msg })))
}
