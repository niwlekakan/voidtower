use crate::{
    auth,
    error::{AppError, Result},
    networking::proxy::{self as proxy_provider, NginxAction},
    AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use super::{
    bearer_auth::AuthenticatedApiToken,
    operation_adoption::{self, CompatibilityResource, CompatibilityResult},
};

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

/// Host-side bind-mount path for the Docker nginx-proxy container's conf.d.
/// VoidTower writes proxy configs here; the container picks them up on reload.
const DOCKER_NGINX_CONF_DIR: &str = proxy_provider::NGINX_CONF_DIR;

/// Returns the container ID of the running vt-nginx-proxy container, or None.
fn docker_nginx_container_id() -> Option<String> {
    proxy_provider::running_container_id()
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

fn parsed_custom_headers(cfg: &ProxyConfig) -> Vec<CustomHeader> {
    cfg.custom_headers
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

fn custom_header_lines(cfg: &ProxyConfig) -> String {
    parsed_custom_headers(cfg)
        .iter()
        .map(|h| {
            format!(
                "        add_header {} \"{}\" always;\n",
                h.name.trim(),
                h.value.replace('"', "\\\"")
            )
        })
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
        Some(rpm) if rpm > 0 => format!(
            "        limit_req zone={} burst=20 nodelay;\n",
            zone_name(&cfg.domain)
        ),
        _ => String::new(),
    }
}

/// Sibling htpasswd file path — lives in the already bind-mounted conf.d dir so
/// no extra Docker volume is needed; nginx's `auth_basic_user_file` can point at
/// any readable path, not just `*.conf`.
fn htpasswd_path(domain: &str) -> std::path::PathBuf {
    proxy_provider::htpasswd_path(domain).expect("validated proxy domain")
}

/// nginx's `ngx_http_auth_basic_module` special-cases the `{SHA}` prefix as
/// raw-SHA1-then-base64 — the one portable htpasswd format that needs no extra
/// nginx module and doesn't depend on the image's libc `crypt()` support.
fn htpasswd_hash(password: &str) -> String {
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(password.as_bytes());
    format!(
        "{{SHA}}{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, digest)
    )
}

fn write_htpasswd_file(domain: &str, user: &str, pass_hash: &str) -> Result<()> {
    proxy_provider::write_htpasswd(domain, user, pass_hash)
        .map_err(|error| AppError::BadRequest(error.to_string()))
}

fn remove_htpasswd_file_checked(domain: &str) -> Result<()> {
    proxy_provider::remove_htpasswd(domain).map_err(|error| AppError::BadRequest(error.to_string()))
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
    let embed = if allow_embed {
        format!("\n{}", embed_headers())
    } else {
        String::new()
    };
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

/// Writes the per-domain conf (and sibling htpasswd file, if basic auth is set)
/// derived entirely from `cfg`. `nginx_conf_content` builds the actual text —
/// kept as a single source of truth so the dry-run preview can never drift
/// from what's actually written to disk.
pub(crate) fn write_nginx_conf(cfg: &ProxyConfig) -> Result<()> {
    if let (Some(user), Some(hash)) = (&cfg.basic_auth_user, &cfg.basic_auth_pass_hash) {
        write_htpasswd_file(&cfg.domain, user, hash)?;
    } else {
        remove_htpasswd_file_checked(&cfg.domain)?;
    }
    let content = nginx_conf_content(cfg);
    proxy_provider::write_conf(&cfg.domain, &content)
        .map_err(|error| AppError::BadRequest(error.to_string()))?;
    Ok(())
}

pub(crate) fn remove_nginx_conf_checked(domain: &str) -> Result<()> {
    proxy_provider::remove_conf(domain).map_err(|error| AppError::BadRequest(error.to_string()))?;
    remove_htpasswd_file_checked(domain)
}

pub(crate) fn nginx_conf_content(cfg: &ProxyConfig) -> String {
    let domain = &cfg.domain;
    let upstream = rewrite_upstream_for_docker(&cfg.upstream);
    let embed = if cfg.allow_embed {
        format!("\n{}", embed_headers())
    } else {
        String::new()
    };
    let sso = if cfg.sso_protect {
        sso_auth_lines()
    } else {
        ""
    };
    let sso_locs = if cfg.sso_protect {
        sso_locations(cfg.allow_embed)
    } else {
        String::new()
    };
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
    proxy_provider::snapshot().is_ok_and(|snapshot| snapshot.active)
}

pub(crate) fn reload_nginx() -> std::result::Result<String, String> {
    proxy_provider::execute(NginxAction::Reload)
        .map(|result| result.message)
        .map_err(|error| error.to_string())
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
    pub basic_auth_secret_id: Option<String>,
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
    let password = password_in
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
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
    Err(AppError::BadRequest(
        "Basic auth password is required".into(),
    ))
}

/// Builds the `ProxyConfig` that will be persisted and used to render the nginx
/// conf, resolving basic auth against any pre-existing row (for updates).
pub(crate) fn build_proxy_config(
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
        if !h
            .name
            .trim()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            return Err(AppError::BadRequest(format!(
                "Invalid header name '{}': only letters, digits and hyphens allowed",
                h.name
            )));
        }
        if h.value.contains('\n') || h.value.contains('\r') {
            return Err(AppError::BadRequest(
                "Header values cannot contain newlines".into(),
            ));
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

#[derive(Clone, Copy, PartialEq)]
enum NginxMode {
    Docker,
    None,
}

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
        if std::fs::write(&tmp, b"").is_ok() {
            let _ = std::fs::remove_file(&tmp);
            true
        } else {
            false
        }
    };

    if let Some(id) = docker_nginx_container_id() {
        let conf_d_exists = docker_conf_d.exists();
        let conf_d_writable = conf_d_exists && writable(docker_conf_d);
        let can_reload = std::process::Command::new("docker")
            .args(["exec", &id, "echo", "ok"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        return NginxSetupStatus {
            mode: NginxMode::Docker,
            conf_d_exists,
            conf_d_writable,
            can_reload,
            container_running: true,
        };
    }

    if docker_conf_d.exists() {
        let conf_d_writable = writable(docker_conf_d);
        return NginxSetupStatus {
            mode: NginxMode::Docker,
            conf_d_exists: true,
            conf_d_writable,
            can_reload: false,
            container_running: false,
        };
    }

    NginxSetupStatus {
        mode: NginxMode::None,
        conf_d_exists: false,
        conf_d_writable: false,
        can_reload: false,
        container_running: false,
    }
}

// ─── Public wrappers for use by other modules ────────────────────────────────

pub fn nginx_active_pub() -> bool {
    nginx_active()
}

async fn ensure_proxy_provider_available() -> Result<()> {
    let snapshot = tokio::task::spawn_blocking(proxy_provider::snapshot)
        .await
        .map_err(|error| AppError::Internal(error.into()))?
        .map_err(|error| AppError::FeatureUnavailable(error.to_string()))?;
    snapshot
        .container_id
        .is_some()
        .then_some(())
        .ok_or_else(|| {
            AppError::FeatureUnavailable(
                "nginx-proxy is not deployed — deploy it from App Vault".into(),
            )
        })
}

async fn resolve_proxy_service(
    state: &AppState,
    credential: &crate::operations::invocation::CredentialContext,
    action: &str,
) -> CompatibilityResult<crate::operations::contracts::ResourceRef> {
    operation_adoption::resolve_available(
        state,
        credential,
        "reverse_proxy_service",
        "voidtower.singleton",
        "local",
        "reverse-proxy",
        &[action],
    )
    .await
}

async fn observe_proxy_rule(
    state: &AppState,
    credential: &crate::operations::invocation::CredentialContext,
    proxy_id: &str,
    action: &str,
) -> CompatibilityResult<crate::operations::contracts::ResourceRef> {
    let config = sqlx::query_as::<_, ProxyConfig>("SELECT * FROM proxy_configs WHERE id = ?")
        .bind(proxy_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| AppError::Internal(error.into()))?
        .ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;
    observe_proxy_config(state, credential, &config, action).await
}

async fn observe_proxy_config(
    state: &AppState,
    credential: &crate::operations::invocation::CredentialContext,
    config: &ProxyConfig,
    action: &str,
) -> CompatibilityResult<crate::operations::contracts::ResourceRef> {
    operation_adoption::observe_available(
        state,
        credential,
        CompatibilityResource {
            kind: "proxy_rule",
            display_name: &config.domain,
            node_id: None,
            provider: Some("nginx"),
            namespace: "voidtower.proxy_config",
            scope_key: "local",
            alias: &config.id,
        },
        &[action],
    )
    .await
}

fn canonical_rule_input(req: &CreateRequest) -> CompatibilityResult<serde_json::Value> {
    if req
        .basic_auth_password
        .as_deref()
        .is_some_and(|password| !password.trim().is_empty())
    {
        return Err(AppError::BadRequest(
            "Basic auth passwords must be stored in Secrets and supplied as basic_auth_secret_id"
                .into(),
        )
        .into());
    }
    Ok(serde_json::json!({
        "domain": req.domain,
        "upstream": req.upstream,
        "ssl": req.ssl,
        "allow_embed": req.allow_embed,
        "sso_protect": req.sso_protect,
        "custom_headers": req.custom_headers,
        "rate_limit_rpm": req.rate_limit_rpm,
        "basic_auth_user": req.basic_auth_user,
        "basic_auth_secret_id": req.basic_auth_secret_id,
        "websocket_extended": req.websocket_extended,
        "cache_static": req.cache_static,
    }))
}

async fn compatibility_plan(
    state: &AppState,
    credential: &crate::operations::invocation::CredentialContext,
    resource_id: &str,
    action: &str,
    input: serde_json::Value,
) -> CompatibilityResult<Response> {
    let prepared =
        operation_adoption::prepare(state, credential, resource_id, action, input).await?;
    let view = prepared.view();
    Ok(Json(serde_json::json!({
        "dry_run": true,
        "plan": view.operation,
        "policy": view.policy,
        "resource": view.resource,
    }))
    .into_response())
}

// ─── Handlers ────────────────────────────────────────────────────────────────

pub async fn nginx_setup_status(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>> {
    require_admin(&state, &jar).await?;

    let s = tokio::task::spawn_blocking(check_nginx_setup)
        .await
        .unwrap();
    let mut steps: Vec<serde_json::Value> = Vec::new();

    let mode_str = match s.mode {
        NginxMode::Docker => "docker",
        NginxMode::None => "none",
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
        if cmds.is_empty() {
            None
        } else {
            Some(cmds.join(" && \\\n"))
        }
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

    let proxies =
        sqlx::query_as::<_, ProxyConfig>("SELECT * FROM proxy_configs ORDER BY created_at")
            .fetch_all(&state.db)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

    let nginx_ok = tokio::task::spawn_blocking(|| docker_nginx_container_id().is_some())
        .await
        .unwrap();

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
    token: Option<Extension<AuthenticatedApiToken>>,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> CompatibilityResult<Response> {
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    create_with_credential(&state, &credential, &headers, req).await
}

pub(crate) async fn create_with_credential(
    state: &AppState,
    credential: &crate::operations::invocation::CredentialContext,
    headers: &HeaderMap,
    req: CreateRequest,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "proxy.rule.create";
    operation_adoption::authorize(credential, ACTION)?;
    ensure_proxy_provider_available().await?;
    let resource = resolve_proxy_service(state, credential, ACTION).await?;
    let dry_run = req.dry_run;
    let input = canonical_rule_input(&req)?;

    if dry_run {
        return compatibility_plan(state, credential, &resource.id, ACTION, input).await;
    }
    operation_adoption::submit(state, credential, &resource.id, ACTION, input, headers).await
}

pub async fn delete_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(proxy_id): Path<String>,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "proxy.rule.delete";
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, ACTION)?;
    ensure_proxy_provider_available().await?;
    let resource = observe_proxy_rule(&state, &credential, &proxy_id, ACTION).await?;
    operation_adoption::submit(
        &state,
        &credential,
        &resource.id,
        ACTION,
        serde_json::json!({}),
        &headers,
    )
    .await
}

pub async fn update_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(proxy_id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "proxy.rule.update";
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, ACTION)?;
    ensure_proxy_provider_available().await?;
    let resource = observe_proxy_rule(&state, &credential, &proxy_id, ACTION).await?;
    let dry_run = req.dry_run;
    let input = canonical_rule_input(&req)?;

    if dry_run {
        return compatibility_plan(&state, &credential, &resource.id, ACTION, input).await;
    }
    operation_adoption::submit(&state, &credential, &resource.id, ACTION, input, &headers).await
}

// ── AI auto-proxy ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AiAutoReq {
    pub upstream: String,
}

pub async fn ai_auto_proxy(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    headers: HeaderMap,
    Json(req): Json<AiAutoReq>,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "proxy.rule.create";
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, ACTION)?;
    ensure_proxy_provider_available().await?;

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
    let resource = resolve_proxy_service(&state, &credential, ACTION).await?;
    let input = serde_json::json!({
        "domain": domain,
        "upstream": req.upstream,
        "ssl": false,
        "allow_embed": true,
        "sso_protect": false,
        "custom_headers": [],
        "rate_limit_rpm": null,
        "basic_auth_user": null,
        "basic_auth_secret_id": null,
        "websocket_extended": false,
        "cache_static": false,
    });
    operation_adoption::submit(&state, &credential, &resource.id, ACTION, input, &headers).await
}

// ── nginx management ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct NginxActionReq {
    pub action: String, // start | stop | restart | reload | test
}

pub async fn nginx_action(
    State(state): State<AppState>,
    jar: CookieJar,
    token: Option<Extension<AuthenticatedApiToken>>,
    headers: HeaderMap,
    Json(req): Json<NginxActionReq>,
) -> CompatibilityResult<Response> {
    let action = req.action.as_str();
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, "proxy.nginx.reload")?;

    if action == "test" {
        let out = tokio::task::spawn_blocking(proxy_provider::test_configuration)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
        return Ok(Json(match out {
            Ok(output) => serde_json::json!({ "ok": true, "output": output }),
            Err(error) => serde_json::json!({ "ok": false, "output": error.to_string() }),
        })
        .into_response());
    }
    let action = match action {
        "start" => "proxy.nginx.start",
        "stop" => "proxy.nginx.stop",
        "restart" => "proxy.nginx.restart",
        "reload" => "proxy.nginx.reload",
        _ => {
            return Err(AppError::BadRequest(
                "action must be start|stop|restart|reload|test".into(),
            )
            .into())
        }
    };
    operation_adoption::authorize(&credential, action)?;
    ensure_proxy_provider_available().await?;
    let resource = resolve_proxy_service(&state, &credential, action).await?;
    operation_adoption::submit(
        &state,
        &credential,
        &resource.id,
        action,
        serde_json::json!({}),
        &headers,
    )
    .await
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

        (
            String::new(),
            vec!["nginx-proxy container is not running — deploy it from App Vault".to_string()],
        )
    })
    .await
    .unwrap();

    Ok(Json(
        serde_json::json!({ "path": result.0, "lines": result.1 }),
    ))
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
    token: Option<Extension<AuthenticatedApiToken>>,
    Path(proxy_id): Path<String>,
    headers: HeaderMap,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "proxy.rule.toggle";
    let credential =
        super::actions::credential(&state, &jar, token.map(|Extension(token)| token)).await?;
    operation_adoption::authorize(&credential, ACTION)?;
    ensure_proxy_provider_available().await?;
    let cfg = sqlx::query_as::<_, ProxyConfig>("SELECT * FROM proxy_configs WHERE id = ?")
        .bind(&proxy_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::BadRequest("Proxy not found".into()))?;
    let resource = observe_proxy_config(&state, &credential, &cfg, ACTION).await?;
    operation_adoption::submit(
        &state,
        &credential,
        &resource.id,
        ACTION,
        serde_json::json!({"enabled": !cfg.enabled}),
        &headers,
    )
    .await
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
    let target = cfg
        .upstream
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

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> CreateRequest {
        CreateRequest {
            domain: "app.example.test".into(),
            upstream: "http://127.0.0.1:8080".into(),
            ssl: false,
            allow_embed: false,
            sso_protect: false,
            dry_run: false,
            custom_headers: Vec::new(),
            rate_limit_rpm: None,
            basic_auth_user: Some("operator".into()),
            basic_auth_password: None,
            basic_auth_secret_id: Some("00000000-0000-4000-8000-000000000001".into()),
            websocket_extended: false,
            cache_static: false,
        }
    }

    #[test]
    fn compatibility_input_preserves_only_basic_auth_secret_references() {
        let input = canonical_rule_input(&request()).unwrap();
        assert_eq!(
            input["basic_auth_secret_id"],
            "00000000-0000-4000-8000-000000000001"
        );
        assert!(input.get("basic_auth_password").is_none());
    }

    #[test]
    fn compatibility_input_rejects_raw_basic_auth_passwords() {
        let mut request = request();
        request.basic_auth_password = Some("raw-secret".into());
        assert!(canonical_rule_input(&request).is_err());
    }
}
