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

fn effective_conf_dir() -> &'static str {
    DOCKER_NGINX_CONF_DIR
}

/// Best-effort Docker host-gateway IP, resolved from wherever *this* process
/// (VoidTower's own backend) happens to run. Only meaningful for VoidTower's own
/// outbound connections (see `proxy_health`) — nginx-proxy runs as a separate
/// Docker container attached to its own custom networks (`vt-proxy` / its compose
/// project's `default`), not the `docker0` bridge, so an IP guessed here is not
/// guaranteed reachable from inside that container regardless of whether VoidTower
/// itself is bare-metal or containerized. Do not use this for nginx conf upstreams —
/// see `rewrite_upstream_for_docker`.
pub(crate) fn docker_host_ip() -> String {
    if let Ok(out) = std::process::Command::new("getent")
        .args(["hosts", "host.docker.internal"])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        if let Some(ip) = s.split_whitespace().next() {
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    if let Ok(out) = std::process::Command::new("ip")
        .args(["addr", "show", "docker0"])
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let t = line.trim();
            if t.starts_with("inet ") {
                if let Some(cidr) = t.split_whitespace().nth(1) {
                    if let Some(ip) = cidr.split('/').next() {
                        return ip.to_string();
                    }
                }
            }
        }
    }
    "172.17.0.1".to_string()
}

/// Rewrite localhost/127.0.0.1 in an upstream URL to `host.docker.internal`, so nginx
/// running inside the nginx-proxy Docker container can reach services bound on the
/// real host's loopback interface — whether VoidTower itself is installed bare-metal
/// or in Docker. Resolution happens *inside the nginx-proxy container* via the
/// `host-gateway` `extra_hosts` entry on its compose service (see
/// `app-vault/apps/nginx-proxy.yml`), not by guessing a bridge-gateway IP from
/// whatever host this backend process happens to run on (which may not even be the
/// same Docker network nginx-proxy is attached to — it uses custom networks, not
/// the default `docker0` bridge).
pub(crate) fn rewrite_upstream_for_docker(upstream: &str) -> String {
    upstream
        .replace("//localhost:", "//host.docker.internal:")
        .replace("//127.0.0.1:", "//host.docker.internal:")
}

/// Best-effort local firewall port opener — tries ufw → firewalld → iptables, first
/// match wins, silently no-ops if none are present/active. Shared by every feature
/// that publishes a Docker-container port straight onto the host (App Vault embed
/// ports, the AI proxy port) and therefore needs the host's own firewall to allow
/// it through too.
pub(crate) fn open_firewall_port(port: &str) {
    let tcp = format!("{port}/tcp");
    if std::process::Command::new("ufw")
        .args(["status"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Status: active"))
        .unwrap_or(false)
    {
        let _ = std::process::Command::new("ufw")
            .args(["allow", &tcp, "comment", "VoidTower"])
            .output();
        return;
    }
    if std::process::Command::new("firewall-cmd")
        .args(["--state"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let _ = std::process::Command::new("firewall-cmd")
            .args(["--permanent", "--add-port", &tcp, "--quiet"])
            .output();
        let _ = std::process::Command::new("firewall-cmd").args(["--reload", "--quiet"]).output();
        return;
    }
    if std::process::Command::new("iptables")
        .args(["-C", "INPUT", "-p", "tcp", "--dport", port, "-j", "ACCEPT"])
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        let _ = std::process::Command::new("iptables")
            .args(["-I", "INPUT", "-p", "tcp", "--dport", port, "-j", "ACCEPT"])
            .output();
    }
}

/// Counterpart to `open_firewall_port` — removes the rule again so a port doesn't
/// stay open once the feature that needed it (AI proxy, embed proxy) is disabled or
/// moved to a different port. Best-effort, same backend detection order.
pub(crate) fn close_firewall_port(port: &str) {
    let tcp = format!("{port}/tcp");
    if std::process::Command::new("ufw")
        .args(["status"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Status: active"))
        .unwrap_or(false)
    {
        let _ = std::process::Command::new("ufw").args(["delete", "allow", &tcp]).output();
        return;
    }
    if std::process::Command::new("firewall-cmd")
        .args(["--state"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        let _ = std::process::Command::new("firewall-cmd")
            .args(["--permanent", "--remove-port", &tcp, "--quiet"])
            .output();
        let _ = std::process::Command::new("firewall-cmd").args(["--reload", "--quiet"]).output();
        return;
    }
    let _ = std::process::Command::new("iptables")
        .args(["-D", "INPUT", "-p", "tcp", "--dport", port, "-j", "ACCEPT"])
        .output();
}

fn conf_path(domain: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(effective_conf_dir())
        .join(format!("voidtower-{domain}.conf"))

}

fn parsed_custom_headers(cfg: &ProxyConfig) -> Vec<CustomHeader> {
    cfg.custom_headers
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

fn custom_header_lines(cfg: &ProxyConfig) -> String {
    parsed_custom_headers(cfg)
        .iter()
        .map(|h| format!("        add_header {} \"{}\" always;\n", h.name.trim(), h.value.replace('"', "\\\"")))
        .collect()
}

/// nginx `limit_req_zone` zone names must be a bare identifier — sanitize the
/// domain down to one and keep it short.
fn zone_name(domain: &str) -> String {
    let mut s: String = domain
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    s.truncate(28);
    format!("rl_{s}")
}

/// `limit_req_zone` must live in `http` context — conf.d files are included
/// there, so it goes at the top of the per-domain conf file, outside `server {}`.
fn rate_limit_zone_decl(cfg: &ProxyConfig) -> String {
    match cfg.rate_limit_rpm {
        Some(rpm) if rpm > 0 => format!(
            "limit_req_zone $binary_remote_addr zone={}:10m rate={rpm}r/m;\n\n",
            zone_name(&cfg.domain)
        ),
        _ => String::new(),
    }
}

fn rate_limit_use_line(cfg: &ProxyConfig) -> String {
    match cfg.rate_limit_rpm {
        Some(rpm) if rpm > 0 => format!("        limit_req zone={} burst=20 nodelay;\n", zone_name(&cfg.domain)),
        _ => String::new(),
    }
}

/// Sibling htpasswd file path — lives in the already bind-mounted conf.d dir so
/// no extra Docker volume is needed; nginx's `auth_basic_user_file` can point at
/// any readable path, not just `*.conf`.
fn htpasswd_path(domain: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(effective_conf_dir()).join(format!("voidtower-{domain}.htpasswd"))
}

/// nginx's `ngx_http_auth_basic_module` special-cases the `{SHA}` prefix as
/// raw-SHA1-then-base64 — the one portable htpasswd format that needs no extra
/// nginx module and doesn't depend on the image's libc `crypt()` support.
fn htpasswd_hash(password: &str) -> String {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(password.as_bytes());
    format!("{{SHA}}{}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, digest))
}

fn write_htpasswd_file(domain: &str, user: &str, pass_hash: &str) -> Result<()> {
    let path = htpasswd_path(domain);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&path, format!("{user}:{pass_hash}\n"))
        .map_err(|e| AppError::BadRequest(format!("Cannot write htpasswd file: {e}")))?;
    Ok(())
}

fn remove_htpasswd_file(domain: &str) {
    let _ = std::fs::remove_file(htpasswd_path(domain));
}

fn auth_basic_lines(cfg: &ProxyConfig) -> String {
    if cfg.basic_auth_user.is_some() {
        format!(
            "        auth_basic \"Restricted\";\n        auth_basic_user_file {};\n",
            htpasswd_path(&cfg.domain).display()
        )
    } else {
        String::new()
    }
}

fn gzip_server_lines(cfg: &ProxyConfig) -> &'static str {
    if cfg.cache_static {
        "    gzip on;\n    gzip_types text/css application/javascript application/json image/svg+xml;\n"
    } else {
        ""
    }
}

fn static_cache_location(upstream: &str, cfg: &ProxyConfig) -> String {
    if !cfg.cache_static {
        return String::new();
    }
    format!(
        r#"
    location ~* \.(jpg|jpeg|png|gif|ico|css|js|woff2?|svg)$ {{
        proxy_pass {upstream};
        proxy_set_header Host $host;
        expires 7d;
        add_header Cache-Control "public, immutable";
    }}
"#
    )
}

/// Body of `location / { ... }` — shared between the SSL and non-SSL templates.
fn proxy_location_inner(upstream: &str, cfg: &ProxyConfig, embed: &str, sso: &str) -> String {
    let timeout_lines = if cfg.websocket_extended {
        "        proxy_buffering off;\n        proxy_read_timeout 3600s;\n        proxy_send_timeout 3600s;\n"
    } else {
        "        proxy_read_timeout 300s;\n"
    };
    let limit_line = rate_limit_use_line(cfg);
    let auth = auth_basic_lines(cfg);
    let headers = custom_header_lines(cfg);
    format!(
        r#"        proxy_pass {upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
{timeout_lines}{limit_line}{auth}{headers}{embed}{sso}"#
    )
}

fn embed_headers() -> &'static str {
    "        proxy_hide_header X-Frame-Options;\n        add_header X-Frame-Options \"ALLOWALL\" always;\n        add_header Content-Security-Policy \"frame-ancestors *\" always;"
}

/// Authentik's embedded outpost, reached over the `vt-proxy` Docker network by the
/// alias set in app-vault/apps/authentik.yml. Not a localhost/127.0.0.1 upstream,
/// so it does not go through `rewrite_upstream_for_docker`.
const AUTHENTIK_OUTPOST_UPSTREAM: &str = "http://authentik:9000";

/// Lines spliced inside `location /` to gate it behind Authentik's forward-auth check.
fn sso_auth_lines() -> &'static str {
    "\n        auth_request /outpost.goauthentik.io/auth/nginx;\n        auth_request_set $auth_cookie $upstream_http_set_cookie;\n        error_page 401 = @goauthentik_proxy_signin;"
}

/// Sibling `location` blocks (outpost proxy + sign-in redirect) required by `sso_auth_lines`.
///
/// When `allow_embed` is set, the outpost location also gets the X-Frame-Options
/// strip/CSP relax — otherwise an embedded app gated behind Authentik renders fine
/// in an iframe once authenticated, but the login/MFA challenge page itself (served
/// from this location on first visit) is framed with whatever headers Authentik's
/// outpost sets, which commonly include `X-Frame-Options: DENY` and silently fails
/// to render.
fn sso_locations(allow_embed: bool) -> String {
    let embed = if allow_embed { format!("\n{}", embed_headers()) } else { String::new() };
    format!(
        r#"
    location /outpost.goauthentik.io {{
        proxy_pass {AUTHENTIK_OUTPOST_UPSTREAM}/outpost.goauthentik.io;
        proxy_set_header Host $host;
        proxy_set_header X-Original-URL $scheme://$http_host$request_uri;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        add_header Set-Cookie $auth_cookie;{embed}
    }}

    location @goauthentik_proxy_signin {{
        internal;
        add_header Set-Cookie $auth_cookie;
        return 302 /outpost.goauthentik.io/start?rd=$request_uri;
    }}
"#
    )
}

// Port-based nginx config for app embeds — no server_name, listen on a unique
// port so any LAN client can reach it via http://<server-ip>:<embed_port>/
// without requiring any DNS or /etc/hosts configuration.
pub fn write_nginx_port_conf(slug: &str, upstream: &str, port: u16) -> Result<()> {
    let path = conf_path(&format!("embed-port-{slug}"));
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let upstream = rewrite_upstream_for_docker(upstream);
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

/// Writes the per-domain conf (and sibling htpasswd file, if basic auth is set)
/// derived entirely from `cfg`. `nginx_conf_content` builds the actual text —
/// kept as a single source of truth so the dry-run preview can never drift
/// from what's actually written to disk.
fn write_nginx_conf(cfg: &ProxyConfig) -> Result<()> {
    let path = conf_path(&cfg.domain);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let (Some(user), Some(hash)) = (&cfg.basic_auth_user, &cfg.basic_auth_pass_hash) {
        write_htpasswd_file(&cfg.domain, user, hash)?;
    } else {
        remove_htpasswd_file(&cfg.domain);
    }
    let content = nginx_conf_content(cfg);
    std::fs::write(&path, content)
        .map_err(|e| AppError::BadRequest(format!("Cannot write nginx config: {e}")))?;
    Ok(())
}

fn remove_nginx_conf(domain: &str) {
    let _ = std::fs::remove_file(conf_path(domain));
    remove_htpasswd_file(domain);
}

fn nginx_conf_content(cfg: &ProxyConfig) -> String {
    let domain = &cfg.domain;
    let upstream = rewrite_upstream_for_docker(&cfg.upstream);
    let embed = if cfg.allow_embed { format!("\n{}", embed_headers()) } else { String::new() };
    let sso = if cfg.sso_protect { sso_auth_lines() } else { "" };
    let sso_locs = if cfg.sso_protect { sso_locations(cfg.allow_embed) } else { String::new() };
    let zone_decl = rate_limit_zone_decl(cfg);
    let gzip_lines = gzip_server_lines(cfg);
    let static_loc = static_cache_location(&upstream, cfg);
    let loc_inner = proxy_location_inner(&upstream, cfg, &embed, sso);

    if cfg.ssl {
        format!(
            r#"# Managed by VoidTower — do not edit manually
{zone_decl}server {{
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
{gzip_lines}{sso_locs}
    location / {{
{loc_inner}
    }}
{static_loc}}}
"#
        )
    } else {
        format!(
            r#"# Managed by VoidTower — do not edit manually
{zone_decl}server {{
    listen 80;
    server_name {domain};
{gzip_lines}{sso_locs}
    location / {{
{loc_inner}
    }}
{static_loc}}}
"#
        )
    }
}

fn nginx_active() -> bool {
    docker_nginx_container_id().is_some()
}

fn reload_nginx() -> std::result::Result<String, String> {
    let id = docker_nginx_container_id()
        .ok_or_else(|| "nginx-proxy container is not running — deploy it from App Vault".to_string())?;
    let out = std::process::Command::new("docker")
        .args(["exec", &id, "nginx", "-s", "reload"])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok("nginx-proxy reloaded".into())
    } else {
        Err(format!(
            "docker exec reload failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

// ─── Types ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, sqlx::FromRow)]
pub struct ProxyConfig {
    pub id: String,
    pub domain: String,
    pub upstream: String,
    pub ssl: bool,
    pub enabled: bool,
    pub allow_embed: bool,
    pub sso_protect: bool,
    pub created_at: i64,
    pub custom_headers: Option<String>,
    pub rate_limit_rpm: Option<i64>,
    pub basic_auth_user: Option<String>,
    #[serde(skip_serializing)]
    pub basic_auth_pass_hash: Option<String>,
    pub websocket_extended: bool,
    pub cache_static: bool,
    pub health_status: Option<String>,
    pub health_checked_at: Option<i64>,
    pub health_latency_ms: Option<i64>,
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
    pub sso_protect: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub custom_headers: Vec<CustomHeader>,
    #[serde(default)]
    pub rate_limit_rpm: Option<i64>,
    #[serde(default)]
    pub basic_auth_user: Option<String>,
    #[serde(default)]
    pub basic_auth_password: Option<String>,
    #[serde(default)]
    pub websocket_extended: bool,
    #[serde(default)]
    pub cache_static: bool,
}

/// Resolves the (user, pass_hash) pair to persist for basic auth from a request:
/// empty/absent username clears it; a username with no new password keeps the
/// existing hash (editing other fields shouldn't force a password re-entry);
/// a username with no existing match and no new password is rejected.
fn resolve_basic_auth(
    user_in: &Option<String>,
    password_in: &Option<String>,
    existing: Option<&ProxyConfig>,
) -> Result<(Option<String>, Option<String>)> {
    let user = user_in.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(user) = user else {
        return Ok((None, None));
    };
    let password = password_in.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if let Some(pw) = password {
        return Ok((Some(user.to_string()), Some(htpasswd_hash(pw))));
    }
    if let Some(existing) = existing {
        if existing.basic_auth_user.as_deref() == Some(user) {
            if let Some(hash) = &existing.basic_auth_pass_hash {
                return Ok((Some(user.to_string()), Some(hash.clone())));
            }
        }
    }
    Err(AppError::BadRequest("Basic auth password is required".into()))
}

/// Extra dry-run plan rows for the fields added on top of the original
/// domain/upstream/ssl/embed/sso set — only listed when actually set, so a
/// plain proxy's preview stays as short as it always was.
fn extra_change_rows(cfg: &ProxyConfig) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    if !parsed_custom_headers(cfg).is_empty() {
        let names: Vec<String> = parsed_custom_headers(cfg).into_iter().map(|h| h.name).collect();
        rows.push(serde_json::json!({ "label": "Custom headers", "value": names.join(", ") }));
    }
    if let Some(rpm) = cfg.rate_limit_rpm {
        rows.push(serde_json::json!({ "label": "Rate limit", "value": format!("{rpm} req/min per IP") }));
    }
    if let Some(user) = &cfg.basic_auth_user {
        rows.push(serde_json::json!({ "label": "Basic auth", "value": format!("enabled (user: {user})") }));
    }
    if cfg.websocket_extended {
        rows.push(serde_json::json!({ "label": "WebSocket passthrough", "value": "extended (buffering off, 3600s timeout)" }));
    }
    if cfg.cache_static {
        rows.push(serde_json::json!({ "label": "Static cache + gzip", "value": "enabled (7d expires on static assets)" }));
    }
    rows
}

/// Builds the `ProxyConfig` that will be persisted and used to render the nginx
/// conf, resolving basic auth against any pre-existing row (for updates).
fn build_proxy_config(
    id: String,
    domain: String,
    req: &CreateRequest,
    created_at: i64,
    existing: Option<&ProxyConfig>,
) -> Result<ProxyConfig> {
    let (basic_auth_user, basic_auth_pass_hash) =
        resolve_basic_auth(&req.basic_auth_user, &req.basic_auth_password, existing)?;
    let headers: Vec<CustomHeader> = req
        .custom_headers
        .iter()
        .filter(|h| !h.name.trim().is_empty())
        .cloned()
        .collect();
    for h in &headers {
        if !h.name.trim().chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(AppError::BadRequest(format!(
                "Invalid header name '{}': only letters, digits and hyphens allowed",
                h.name
            )));
        }
        if h.value.contains('\n') || h.value.contains('\r') {
            return Err(AppError::BadRequest("Header values cannot contain newlines".into()));
        }
    }
    let custom_headers = if headers.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&headers).map_err(|e| AppError::Internal(e.into()))?)
    };
    Ok(ProxyConfig {
        id,
        domain,
        upstream: req.upstream.clone(),
        ssl: req.ssl,
        enabled: existing.map(|e| e.enabled).unwrap_or(true),
        allow_embed: req.allow_embed,
        sso_protect: req.sso_protect,
        created_at,
        custom_headers,
        rate_limit_rpm: req.rate_limit_rpm.filter(|&r| r > 0),
        basic_auth_user,
        basic_auth_pass_hash,
        websocket_extended: req.websocket_extended,
        cache_static: req.cache_static,
        health_status: existing.and_then(|e| e.health_status.clone()),
        health_checked_at: existing.and_then(|e| e.health_checked_at),
        health_latency_ms: existing.and_then(|e| e.health_latency_ms),
    })
}

/// True when Authentik SSO is configured and enabled — required before any proxy
/// can be flagged `sso_protect`, so the UI can't produce a gate pointing at nothing.
async fn oidc_is_enabled(db: &sqlx::SqlitePool) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT enabled FROM oidc_config WHERE id = 'default'")
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

#[derive(Clone, Copy, PartialEq)]
enum NginxMode { Docker, None }

struct NginxSetupStatus {
    mode: NginxMode,
    conf_d_exists: bool,
    conf_d_writable: bool,
    can_reload: bool,
    container_running: bool,
}

fn check_nginx_setup() -> NginxSetupStatus {
    let docker_conf_d = std::path::Path::new(DOCKER_NGINX_CONF_DIR);

    let writable = |dir: &std::path::Path| -> bool {
        let tmp = dir.join(".vt-write-test");
        if std::fs::write(&tmp, b"").is_ok() { let _ = std::fs::remove_file(&tmp); true } else { false }
    };

    if let Some(id) = docker_nginx_container_id() {
        let conf_d_exists = docker_conf_d.exists();
        let conf_d_writable = conf_d_exists && writable(docker_conf_d);
        let can_reload = std::process::Command::new("docker")
            .args(["exec", &id, "echo", "ok"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        return NginxSetupStatus { mode: NginxMode::Docker, conf_d_exists, conf_d_writable, can_reload, container_running: true };
    }

    if docker_conf_d.exists() {
        let conf_d_writable = writable(docker_conf_d);
        return NginxSetupStatus { mode: NginxMode::Docker, conf_d_exists: true, conf_d_writable, can_reload: false, container_running: false };
    }

    NginxSetupStatus { mode: NginxMode::None, conf_d_exists: false, conf_d_writable: false, can_reload: false, container_running: false }
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
                        "label": "VoidTower needs Docker socket access — add user to docker group and restart VoidTower",
                        "cmd": "sudo usermod -aG docker $(whoami)"
                    }));
                }
            }
        }
        NginxMode::None => {
            steps.push(serde_json::json!({
                "label": "Deploy nginx-proxy from App Vault",
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
            "can_reload": s.can_reload,
        },
        "steps": steps,
        "combined_cmd": combined,
    })))
}


pub async fn list(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let proxies = sqlx::query_as::<_, ProxyConfig>(
        "SELECT * FROM proxy_configs ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let nginx_ok = tokio::task::spawn_blocking(|| docker_nginx_container_id().is_some()).await.unwrap();

    Ok(Json(serde_json::json!({
        "proxies": proxies,
        "nginx_available": nginx_ok,
        "nginx_backend": if nginx_ok { "docker" } else { "none" },
        "sites_dir": effective_conf_dir(),
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

    if req.sso_protect && !oidc_is_enabled(&state.db).await {
        return Err(AppError::BadRequest(
            "Authentik SSO is not configured — set it up under Settings before protecting a proxy with it".into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let now = unix_now();
    let cfg = build_proxy_config(id.clone(), req.domain.clone(), &req, now, None)?;

    if req.dry_run {
        let conf_file = conf_path(&req.domain);
        let exists = conf_file.exists();
        let content = nginx_conf_content(&cfg);
        let mut changes = vec![
            serde_json::json!({ "label": "Domain", "value": req.domain }),
            serde_json::json!({ "label": "Upstream", "value": req.upstream }),
            serde_json::json!({ "label": "SSL", "value": if req.ssl { "yes (Let's Encrypt)" } else { "no" } }),
            serde_json::json!({ "label": "Allow embed", "value": if req.allow_embed { "yes (strips X-Frame-Options)" } else { "no" } }),
            serde_json::json!({ "label": "Authentik protection", "value": if req.sso_protect { "enabled — visitors must authenticate via Authentik first" } else { "disabled" } }),
        ];
        changes.extend(extra_change_rows(&cfg));
        changes.push(serde_json::json!({ "label": "Config file", "value": conf_file.display().to_string() }));
        changes.push(serde_json::json!({ "label": "Config file action", "value": if exists { "overwrite existing" } else { "create new" } }));
        changes.push(serde_json::json!({ "label": "Rollback", "value": "Delete conf file and reload nginx" }));
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Create Nginx Proxy",
                "risk": if req.sso_protect { "medium" } else { "low" },
                "changes": changes,
                "preview": content,
            }
        })));
    }

    sqlx::query(
        "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, sso_protect, created_at, custom_headers, rate_limit_rpm, basic_auth_user, basic_auth_pass_hash, websocket_extended, cache_static) VALUES (?,?,?,?,1,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&id)
    .bind(&req.domain)
    .bind(&req.upstream)
    .bind(req.ssl)
    .bind(req.allow_embed)
    .bind(req.sso_protect)
    .bind(now)
    .bind(&cfg.custom_headers)
    .bind(cfg.rate_limit_rpm)
    .bind(&cfg.basic_auth_user)
    .bind(&cfg.basic_auth_pass_hash)
    .bind(cfg.websocket_extended)
    .bind(cfg.cache_static)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::BadRequest(format!("Domain '{}' already has a proxy rule", req.domain))
        } else {
            AppError::Internal(e.into())
        }
    })?;

    write_nginx_conf(&cfg)?;

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
        "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, sso_protect, created_at) VALUES (?,?,?,?,1,?,0,?)",
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

    write_nginx_conf(&ProxyConfig {
        id: id.clone(),
        domain: domain.to_string(),
        upstream: upstream.to_string(),
        ssl,
        enabled: true,
        allow_embed,
        created_at: now,
        ..Default::default()
    })?;
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

    // Fetch the full current row — needed both for old domain/enabled state and
    // to preserve basic auth's existing hash / health_* fields across the edit.
    let existing = sqlx::query_as::<_, ProxyConfig>("SELECT * FROM proxy_configs WHERE id = ?")
        .bind(&proxy_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;

    let old_domain = existing.domain.clone();
    let enabled = existing.enabled;

    if req.sso_protect && !oidc_is_enabled(&state.db).await {
        return Err(AppError::BadRequest(
            "Authentik SSO is not configured — set it up under Settings before protecting a proxy with it".into(),
        ));
    }

    let cfg = build_proxy_config(proxy_id.clone(), req.domain.clone(), &req, existing.created_at, Some(&existing))?;

    if req.dry_run {
        let conf_file = conf_path(&req.domain);
        let content = nginx_conf_content(&cfg);
        let domain_changed = old_domain != req.domain;
        let mut changes = vec![
            serde_json::json!({ "label": "Domain", "value": format!("{} → {}", old_domain, req.domain) }),
            serde_json::json!({ "label": "Upstream", "value": req.upstream }),
            serde_json::json!({ "label": "SSL", "value": if req.ssl { "yes (Let's Encrypt)" } else { "no" } }),
            serde_json::json!({ "label": "Allow embed", "value": if req.allow_embed { "yes (strips X-Frame-Options)" } else { "no" } }),
            serde_json::json!({ "label": "Authentik protection", "value": if req.sso_protect { "enabled — visitors must authenticate via Authentik first" } else { "disabled" } }),
        ];
        changes.extend(extra_change_rows(&cfg));
        changes.push(serde_json::json!({ "label": "Config file", "value": conf_file.display().to_string() }));
        changes.push(serde_json::json!({ "label": "Config file action", "value": if domain_changed { "rename old conf + write new" } else { "overwrite in place" } }));
        changes.push(serde_json::json!({ "label": "Nginx reload", "value": if enabled { "yes" } else { "no (proxy is disabled)" } }));
        changes.push(serde_json::json!({ "label": "Rollback", "value": "Revert domain/upstream fields and re-save" }));
        return Ok(Json(serde_json::json!({
            "dry_run": true,
            "plan": {
                "title": "Update Nginx Proxy",
                "risk": if req.sso_protect && enabled { "medium" } else { "low" },
                "changes": changes,
                "preview": if enabled { Some(content) } else { None::<String> },
            }
        })));
    }

    sqlx::query(
        "UPDATE proxy_configs SET domain = ?, upstream = ?, ssl = ?, allow_embed = ?, sso_protect = ?, custom_headers = ?, rate_limit_rpm = ?, basic_auth_user = ?, basic_auth_pass_hash = ?, websocket_extended = ?, cache_static = ? WHERE id = ?",
    )
    .bind(&req.domain)
    .bind(&req.upstream)
    .bind(req.ssl)
    .bind(req.allow_embed)
    .bind(req.sso_protect)
    .bind(&cfg.custom_headers)
    .bind(cfg.rate_limit_rpm)
    .bind(&cfg.basic_auth_user)
    .bind(&cfg.basic_auth_pass_hash)
    .bind(cfg.websocket_extended)
    .bind(cfg.cache_static)
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
        write_nginx_conf(&cfg)?;
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
        "INSERT INTO proxy_configs (id, domain, upstream, ssl, enabled, allow_embed, sso_protect, created_at) VALUES (?,?,?,?,?,?,0,?)",
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

    write_nginx_conf(&ProxyConfig {
        id: id.clone(),
        domain: domain.clone(),
        upstream: req.upstream.clone(),
        ssl: false,
        enabled: true,
        allow_embed: true,
        created_at: now,
        ..Default::default()
    })?;
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
                let Some(id) = docker_nginx_container_id() else {
                    return (false, "nginx-proxy container is not running — deploy it from App Vault".to_string());
                };
                match std::process::Command::new("docker").args(["exec", &id, "nginx", "-t"]).output() {
                    Ok(o) => (o.status.success(), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))),
                    Err(e) => (false, format!("docker exec failed: {e}")),
                }
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

        Err("nginx-proxy container is not running — deploy it from App Vault".to_string())
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

        (String::new(), vec!["nginx-proxy container is not running — deploy it from App Vault".to_string()])
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

        serde_json::json!({ "active": false, "state": "not deployed", "pid": null, "mode": "docker" })
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

    let cfg = sqlx::query_as::<_, ProxyConfig>("SELECT * FROM proxy_configs WHERE id = ?")
        .bind(&proxy_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;

    let new_enabled = !cfg.enabled;

    sqlx::query("UPDATE proxy_configs SET enabled = ? WHERE id = ?")
        .bind(new_enabled)
        .bind(&proxy_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if new_enabled {
        write_nginx_conf(&cfg)?;
    } else {
        remove_nginx_conf(&cfg.domain);
    }

    let reload_msg = reload_nginx().unwrap_or_else(|e| format!("warning: {e}"));

    Ok(Json(serde_json::json!({ "ok": true, "enabled": new_enabled, "nginx": reload_msg })))
}

// ── Health check ──────────────────────────────────────────────────────────────

/// On-demand upstream reachability check for a single proxy entry. This connects
/// directly from VoidTower's own backend process, not from inside the nginx-proxy
/// container, so it rewrites `localhost`/`127.0.0.1` using a best-effort Docker
/// host-gateway IP guess (`docker_host_ip`) rather than the `host.docker.internal`
/// hostname `rewrite_upstream_for_docker` writes into nginx confs — that hostname
/// only resolves inside nginx-proxy's own container. This tests "is the backend
/// alive", not "is the whole proxy chain working", which the existing nginx
/// `test`/reload actions already cover.
pub async fn proxy_health(
    State(state): State<AppState>,
    jar: CookieJar,
    Path(proxy_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let cfg = sqlx::query_as::<_, ProxyConfig>("SELECT * FROM proxy_configs WHERE id = ?")
        .bind(&proxy_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;

    let host_ip = docker_host_ip();
    let target = cfg.upstream
        .replace("//localhost:", &format!("//{host_ip}:"))
        .replace("//127.0.0.1:", &format!("//{host_ip}:"));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Internal(e.into()))?;

    let started = std::time::Instant::now();
    let reachable = client.get(&target).send().await.is_ok();
    let latency_ms = started.elapsed().as_millis() as i64;

    let status = if reachable { "up" } else { "down" };
    let checked_at = unix_now();

    sqlx::query(
        "UPDATE proxy_configs SET health_status = ?, health_checked_at = ?, health_latency_ms = ? WHERE id = ?",
    )
    .bind(status)
    .bind(checked_at)
    .bind(latency_ms)
    .bind(&proxy_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(serde_json::json!({
        "status": status,
        "latency_ms": latency_ms,
        "checked_at": checked_at,
    })))
}
