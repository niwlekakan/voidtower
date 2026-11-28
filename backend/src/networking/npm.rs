//! Read-only nginx-proxy-manager (NPM) adapter.
//!
//! NPM owns the public hostname inventory and TLS certificates for the homelab. This
//! module is deliberately read-only: it lists proxy hosts and certificates, joins hosts
//! to the nearest matching certificate expiry, and plans how `deployed_apps` links should
//! be reconciled. It never creates, updates, or deletes anything inside NPM.
//!
//! Credentials are resolved from the process environment or from
//! `<config_dir>/npm-token` and are never logged, serialized, or included in any error
//! message or response produced here.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::Mutex;

/// Default NPM API base URL. NPM listens on port 81 for its admin API.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:81";
/// File name (inside the VoidTower config directory) holding the NPM credential.
pub const TOKEN_FILE_NAME: &str = "npm-token";
/// The dashboard reads through a short server-side cache; NPM is polled at most this often.
pub const CACHE_TTL_SECONDS: i64 = 5;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
const TOKEN_EXPIRY_SKEW_SECONDS: i64 = 30;
const DEFAULT_TOKEN_LIFETIME_SECONDS: i64 = 300;

fn unix_now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

// ---------------------------------------------------------------------------
// Settings and credentials
// ---------------------------------------------------------------------------

/// Resolved NPM connection settings.
///
/// Deliberately does not implement `Debug`/`Display`: the credential must never be
/// reachable through formatting, tracing, or a serialized error.
#[derive(Clone)]
pub struct NpmSettings {
    base_url: String,
    credential: Credential,
}

#[derive(Clone)]
enum Credential {
    /// A pre-issued bearer token.
    Bearer(String),
    /// NPM identity + password, exchanged for a short-lived token via `/api/tokens`.
    Login { identity: String, password: String },
}

impl NpmSettings {
    /// Load settings from the environment, falling back to `<config_dir>/npm-token`.
    ///
    /// Returns `Ok(None)` when no credential is configured; the caller decides whether
    /// that is a hard error or simply "integration disabled".
    pub fn load(config_dir: &Path) -> Result<Option<Self>> {
        Self::load_with(config_dir, |name| std::env::var(name).ok())
    }

    fn load_with(config_dir: &Path, env: impl Fn(&str) -> Option<String>) -> Result<Option<Self>> {
        let base_url = env("VOIDTOWER_NPM_URL")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let base_url = base_url.trim_end_matches('/').to_string();

        let credential = if let Some(token) = non_empty(env("VOIDTOWER_NPM_TOKEN")) {
            Credential::Bearer(token)
        } else if let (Some(identity), Some(password)) = (
            non_empty(env("VOIDTOWER_NPM_IDENTITY")),
            non_empty(env("VOIDTOWER_NPM_PASSWORD")),
        ) {
            Credential::Login { identity, password }
        } else {
            match read_credential_file(&config_dir.join(TOKEN_FILE_NAME))? {
                Some(credential) => credential,
                None => return Ok(None),
            }
        };

        Ok(Some(Self {
            base_url,
            credential,
        }))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Parse the credential file. `identity:password` means a login exchange; anything else
/// is treated as a ready-made bearer token.
fn read_credential_file(path: &Path) -> Result<Option<Credential>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read NPM credential file {}", path.display()))
        }
    };

    let Some(line) = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
    else {
        return Ok(None);
    };

    if let Some((identity, password)) = line.split_once(':') {
        if !identity.trim().is_empty() && !password.is_empty() {
            return Ok(Some(Credential::Login {
                identity: identity.trim().to_string(),
                password: password.to_string(),
            }));
        }
    }

    Ok(Some(Credential::Bearer(line.to_string())))
}

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

/// Read-only NPM HTTP client.
///
/// Does not implement `Debug`: the underlying settings carry credentials.
pub struct NpmClient {
    http: reqwest::Client,
    settings: NpmSettings,
}

impl NpmClient {
    pub fn new(settings: NpmSettings) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .user_agent("voidtower-npm-adapter")
            .build()
            .context("failed to build the NPM HTTP client")?;
        Ok(Self { http, settings })
    }

    pub fn base_url(&self) -> &str {
        self.settings.base_url()
    }

    async fn bearer_token(&self) -> Result<String> {
        match &self.settings.credential {
            Credential::Bearer(token) => Ok(token.clone()),
            Credential::Login { identity, password } => {
                login_token(&self.http, &self.settings.base_url, identity, password).await
            }
        }
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let token = self.bearer_token().await?;
        let url = format!("{}{}", self.settings.base_url, path);
        let response = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("NPM request failed: GET {path}"))?;
        decode_json(response, path).await
    }

    /// List every NPM proxy host (enabled and disabled).
    pub async fn list_hosts(&self) -> Result<Vec<NpmHost>> {
        self.get_json("/api/nginx/proxy-hosts").await
    }

    /// List every NPM certificate.
    pub async fn list_certificates(&self) -> Result<Vec<NpmCertificate>> {
        self.get_json("/api/nginx/certificates").await
    }
}

struct CachedToken {
    cache_key: String,
    token: String,
    expires_at: i64,
}

static LOGIN_CACHE: OnceLock<Mutex<Option<CachedToken>>> = OnceLock::new();

async fn login_token(
    http: &reqwest::Client,
    base_url: &str,
    identity: &str,
    password: &str,
) -> Result<String> {
    let cache_key = format!("{base_url}\u{0}{identity}");
    let cache = LOGIN_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().await;
    let now = unix_now();

    if let Some(cached) = guard.as_ref() {
        if cached.cache_key == cache_key && cached.expires_at > now + TOKEN_EXPIRY_SKEW_SECONDS {
            return Ok(cached.token.clone());
        }
    }

    let response = http
        .post(format!("{base_url}/api/tokens"))
        .json(&serde_json::json!({ "identity": identity, "secret": password }))
        .send()
        .await
        .context("NPM authentication request failed")?;
    if !response.status().is_success() {
        bail!("NPM authentication failed with HTTP {}", response.status());
    }

    #[derive(Deserialize)]
    struct TokenResponse {
        token: String,
        #[serde(default)]
        expires: Option<String>,
    }

    let payload: TokenResponse = response
        .json()
        .await
        .context("NPM authentication returned an unexpected payload")?;
    let expires_at = payload
        .expires
        .as_deref()
        .and_then(parse_expiry)
        .map(|expires| expires.unix_timestamp())
        .unwrap_or(now + DEFAULT_TOKEN_LIFETIME_SECONDS);

    *guard = Some(CachedToken {
        cache_key,
        token: payload.token.clone(),
        expires_at,
    });
    Ok(payload.token)
}

async fn decode_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    path: &str,
) -> Result<T> {
    if !response.status().is_success() {
        bail!("NPM API returned HTTP {} for {path}", response.status());
    }
    response
        .json::<T>()
        .await
        .with_context(|| format!("NPM API returned an unexpected payload for {path}"))
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct NpmHostMeta {
    #[serde(default)]
    pub nginx_err: Option<String>,
}

/// One NPM proxy host, reduced to the fields VoidTower consumes.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NpmHost {
    pub id: i64,
    #[serde(default)]
    pub domain_names: Vec<String>,
    #[serde(default)]
    pub forward_scheme: String,
    #[serde(default)]
    pub forward_host: String,
    #[serde(default)]
    pub forward_port: i64,
    /// NPM reports `0` when no certificate is attached.
    #[serde(default)]
    pub certificate_id: i64,
    #[serde(default)]
    pub ssl_forced: bool,
    #[serde(default)]
    pub enabled: bool,
    /// NPM reports `0` when no access list is attached.
    #[serde(default)]
    pub access_list_id: i64,
    #[serde(default)]
    pub meta: Option<NpmHostMeta>,
}

impl NpmHost {
    pub fn certificate_id(&self) -> Option<i64> {
        (self.certificate_id > 0).then_some(self.certificate_id)
    }

    pub fn access_list_id(&self) -> Option<i64> {
        (self.access_list_id > 0).then_some(self.access_list_id)
    }

    pub fn nginx_err(&self) -> Option<&str> {
        self.meta
            .as_ref()
            .and_then(|meta| meta.nginx_err.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct NpmCertificate {
    pub id: i64,
    #[serde(default)]
    pub domain_names: Vec<String>,
    #[serde(default)]
    pub expires_on: Option<String>,
    #[serde(default)]
    pub provider: String,
}

// ---------------------------------------------------------------------------
// Views (safe to serialize)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HostDomainTls {
    pub hostname: String,
    pub certificate_id: Option<i64>,
    pub expires_on: Option<String>,
    pub days_to_expiry: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NpmHostView {
    pub id: i64,
    pub domain_names: Vec<String>,
    pub domains: Vec<HostDomainTls>,
    pub forward_scheme: String,
    pub forward_host: String,
    pub forward_port: i64,
    pub certificate_id: Option<i64>,
    pub ssl_forced: bool,
    pub enabled: bool,
    pub access_list_id: Option<i64>,
    pub nginx_err: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NpmCertificateView {
    pub id: i64,
    pub domain_names: Vec<String>,
    pub expires_on: Option<String>,
    pub provider: String,
    pub days_to_expiry: Option<i64>,
}

// ---------------------------------------------------------------------------
// Expiry math
// ---------------------------------------------------------------------------

fn parse_expiry(value: &str) -> Option<OffsetDateTime> {
    OffsetDateTime::parse(value.trim(), &Rfc3339)
        .ok()
        .or_else(|| {
            // NPM returns "YYYY-MM-DD HH:MM:SS" (space separated, UTC).
            time::PrimitiveDateTime::parse(
                value.trim(),
                time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"),
            )
            .ok()
            .map(|dt| dt.assume_utc())
        })
        .or_else(|| {
            time::Date::parse(
                value.trim(),
                time::macros::format_description!("[year]-[month]-[day]"),
            )
            .ok()
            .map(|date| date.midnight().assume_utc())
        })
}

/// Whole days until `expires_on`, floored toward negative infinity.
///
/// A certificate that expired an hour ago yields `-1`, not `0`, so an expired cert can
/// never be mistaken for one that expires today. Unparseable timestamps yield `None`.
pub fn days_until_expiry(expires_on: &str, now: OffsetDateTime) -> Option<i64> {
    let expires = parse_expiry(expires_on)?;
    Some((expires.unix_timestamp() - now.unix_timestamp()).div_euclid(86_400))
}

pub fn certificate_views(
    certificates: &[NpmCertificate],
    now: OffsetDateTime,
) -> Vec<NpmCertificateView> {
    certificates
        .iter()
        .map(|certificate| NpmCertificateView {
            id: certificate.id,
            domain_names: certificate.domain_names.clone(),
            expires_on: certificate.expires_on.clone(),
            provider: certificate.provider.clone(),
            days_to_expiry: certificate
                .expires_on
                .as_deref()
                .and_then(|expires_on| days_until_expiry(expires_on, now)),
        })
        .collect()
}

fn certificate_covers(certificate: &NpmCertificate, hostname: &str) -> bool {
    let hostname = normalize_hostname(hostname);
    certificate.domain_names.iter().any(|name| {
        let name = normalize_hostname(name);
        if name == hostname {
            return true;
        }
        let Some(suffix) = name.strip_prefix("*.") else {
            return false;
        };
        hostname
            .strip_suffix(suffix)
            .and_then(|prefix| prefix.strip_suffix('.'))
            .map(|label| !label.is_empty() && !label.contains('.'))
            .unwrap_or(false)
    })
}

/// Attach the nearest certificate expiry to every hostname of every proxy host.
///
/// A certificate explicitly attached to the host and certificates whose names cover the
/// hostname are both considered; the one that expires soonest wins.
pub fn join_hosts_with_certificates(
    hosts: &[NpmHost],
    certificates: &[NpmCertificate],
    now: OffsetDateTime,
) -> Vec<NpmHostView> {
    hosts
        .iter()
        .map(|host| {
            let attached = host
                .certificate_id()
                .and_then(|id| certificates.iter().find(|certificate| certificate.id == id));
            let domains = host
                .domain_names
                .iter()
                .map(|domain| {
                    let mut candidates: Vec<&NpmCertificate> = Vec::new();
                    if let Some(certificate) = attached {
                        candidates.push(certificate);
                    }
                    for certificate in certificates {
                        if certificate_covers(certificate, domain)
                            && !candidates
                                .iter()
                                .any(|candidate| candidate.id == certificate.id)
                        {
                            candidates.push(certificate);
                        }
                    }
                    let nearest = candidates.into_iter().min_by_key(|certificate| {
                        certificate
                            .expires_on
                            .as_deref()
                            .and_then(parse_expiry)
                            .map(|expiry| expiry.unix_timestamp())
                            .unwrap_or(i64::MAX)
                    });
                    let expires_on = nearest.and_then(|certificate| certificate.expires_on.clone());
                    HostDomainTls {
                        hostname: domain.clone(),
                        certificate_id: nearest
                            .map(|certificate| certificate.id)
                            .or_else(|| host.certificate_id()),
                        days_to_expiry: expires_on
                            .as_deref()
                            .and_then(|expires_on| days_until_expiry(expires_on, now)),
                        expires_on,
                    }
                })
                .collect();

            NpmHostView {
                id: host.id,
                domain_names: host.domain_names.clone(),
                domains,
                forward_scheme: host.forward_scheme.clone(),
                forward_host: host.forward_host.clone(),
                forward_port: host.forward_port,
                certificate_id: host.certificate_id(),
                ssl_forced: host.ssl_forced,
                enabled: host.enabled,
                access_list_id: host.access_list_id(),
                nginx_err: host.nginx_err().map(str::to_string),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Sync planning
// ---------------------------------------------------------------------------

/// A `deployed_apps` row as far as NPM reconciliation is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployedAppLink {
    /// `deployed_apps.id` primary key.
    pub row_id: String,
    pub app_id: String,
    pub app_name: String,
    pub project_name: String,
    pub hostname: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLinkUpdate {
    pub row_id: String,
    pub hostname: String,
    pub url: String,
    /// Which app field produced the match (`project_name` or `app_id`).
    pub matched_on: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLinkClear {
    pub row_id: String,
    pub previous_hostname: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncPlan {
    /// Rows whose hostname or URL must change.
    pub updates: Vec<AppLinkUpdate>,
    /// Rows whose stored hostname was an NPM domain that no longer maps to the app.
    pub clears: Vec<AppLinkClear>,
    /// Project names with no enabled NPM host.
    pub unmatched: Vec<String>,
    /// Number of apps that matched an enabled host, including already-current rows.
    pub matched: usize,
}

#[derive(Debug, Clone)]
struct DomainCandidate {
    domain: String,
    /// Leftmost hostname label, e.g. `cloud` for `cloud.example.test`.
    label: String,
    /// Normalized NPM `forward_host`, usually the compose service or container name.
    upstream: String,
    host_id: i64,
}

fn normalize_hostname(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn normalize_domain(value: &str) -> String {
    normalize_hostname(value)
        .trim_start_matches("*.")
        .to_string()
}

/// Normalize an app identifier for hostname-label comparison: lowercase, anything that is
/// not alphanumeric becomes a hyphen, repeated hyphens collapse, edges trimmed.
fn normalize_token(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_hyphen = false;
    for character in value.trim().to_ascii_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            out.push(character);
            previous_hyphen = false;
        } else if !previous_hyphen {
            out.push('-');
            previous_hyphen = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Score how strongly one app matches one NPM hostname.
///
/// The NPM `forward_host` is the strongest signal because it is the actual upstream the
/// proxy talks to (the compose service or container name), so `cloud.heimdallr.cv` ->
/// `nextcloud` links the `nextcloud` app even though the hostname label is `cloud`.
fn match_score(app: &DeployedAppLink, label: &str, upstream: &str) -> Option<(&'static str, u8)> {
    let project_name = normalize_token(&app.project_name);
    let app_id = normalize_token(&app.app_id);

    if !upstream.is_empty()
        && ((!project_name.is_empty() && upstream == project_name)
            || (!app_id.is_empty() && upstream == app_id))
    {
        return Some(("forward_host", 4));
    }
    if !project_name.is_empty() && label == project_name {
        return Some(("project_name", 3));
    }
    if !app_id.is_empty() && label == app_id {
        return Some(("app_id", 2));
    }
    // Abbreviations people actually use: vault.heimdallr.cv -> vaultwarden,
    // auth -> authentik, omni -> omniroute. Requires a real label, not a stub.
    if !project_name.is_empty() && label.len() >= 3 && project_name.starts_with(label) {
        return Some(("project_name", 2));
    }
    // Token match: chat.skully.boo -> skully-chat.
    if label.len() >= 3
        && project_name
            .split(['-', '_', '.'])
            .any(|token| !token.is_empty() && token == label)
    {
        return Some(("project_name", 1));
    }
    for prefix in ["voidtower-", "vt-"] {
        if let Some(stripped) = project_name.strip_prefix(prefix) {
            if !stripped.is_empty() && label == stripped {
                return Some(("project_name", 1));
            }
        }
    }
    None
}

/// Plan how `deployed_apps.hostname`/`url` should be reconciled from NPM.
///
/// Matching is restricted to enabled proxy hosts and considers, in order of strength, the
/// host's `forward_host` against the app id or project name, the leftmost hostname label
/// against the project name, then against the app id, then against a `voidtower-`/`vt-`
/// prefixed project name. A hostname is linked to at most one app: the strongest match
/// wins, and ties break deterministically on hostname then row id. Rows pointing at an NPM
/// domain that no longer maps to the app are cleared; rows pointing anywhere else (a manual
/// link, an external URL) are left untouched.
pub fn plan_sync(apps: &[DeployedAppLink], hosts: &[NpmHost]) -> SyncPlan {
    let mut candidates: Vec<DomainCandidate> = Vec::new();
    let mut known_domains: HashSet<String> = HashSet::new();

    for host in hosts.iter().filter(|host| host.enabled) {
        let upstream = normalize_token(&host.forward_host);
        for domain in &host.domain_names {
            let normalized = normalize_domain(domain);
            if normalized.is_empty() {
                continue;
            }
            let label = normalized.split('.').next().unwrap_or_default().to_string();
            candidates.push(DomainCandidate {
                domain: normalized,
                label,
                upstream: upstream.clone(),
                host_id: host.id,
            });
        }
    }
    for host in hosts {
        for domain in &host.domain_names {
            let normalized = normalize_domain(domain);
            if !normalized.is_empty() {
                known_domains.insert(normalized);
            }
        }
    }

    // Strongest claim per app: (app index, candidate index, matched field, score).
    let mut claims: Vec<(usize, usize, &'static str, u8)> = Vec::new();
    for (app_index, app) in apps.iter().enumerate() {
        let mut best: Option<(usize, &'static str, u8)> = None;
        for (candidate_index, candidate) in candidates.iter().enumerate() {
            let Some((matched_on, score)) = match_score(app, &candidate.label, &candidate.upstream)
            else {
                continue;
            };
            let better = match best {
                None => true,
                Some((current_index, _, current_score)) => {
                    let current = &candidates[current_index];
                    score > current_score
                        || (score == current_score
                            && (candidate.domain.len(), candidate.host_id, &candidate.domain)
                                < (current.domain.len(), current.host_id, &current.domain))
                }
            };
            if better {
                best = Some((candidate_index, matched_on, score));
            }
        }
        if let Some((candidate_index, matched_on, score)) = best {
            claims.push((app_index, candidate_index, matched_on, score));
        }
    }

    claims.sort_by(|a, b| {
        b.3.cmp(&a.3)
            .then_with(|| candidates[a.1].domain.cmp(&candidates[b.1].domain))
            .then_with(|| apps[a.0].row_id.cmp(&apps[b.0].row_id))
    });

    let mut claimed_domains: HashSet<&str> = HashSet::new();
    let mut assignments: HashMap<usize, (usize, &'static str)> = HashMap::new();
    for (app_index, candidate_index, matched_on, _) in claims {
        let domain = candidates[candidate_index].domain.as_str();
        if !claimed_domains.insert(domain) {
            continue;
        }
        assignments.insert(app_index, (candidate_index, matched_on));
    }

    let mut plan = SyncPlan::default();
    for (app_index, app) in apps.iter().enumerate() {
        match assignments.get(&app_index) {
            Some((candidate_index, matched_on)) => {
                let candidate = &candidates[*candidate_index];
                plan.matched += 1;
                let url = format!("https://{}", candidate.domain);
                let current_hostname = app.hostname.as_deref().map(normalize_domain);
                if current_hostname.as_deref() != Some(candidate.domain.as_str())
                    || app.url.as_deref() != Some(url.as_str())
                {
                    plan.updates.push(AppLinkUpdate {
                        row_id: app.row_id.clone(),
                        hostname: candidate.domain.clone(),
                        url,
                        matched_on,
                    });
                }
            }
            None => {
                plan.unmatched.push(app.project_name.clone());
                if let Some(existing) = app.hostname.as_deref() {
                    let normalized = normalize_domain(existing);
                    if known_domains.contains(&normalized) {
                        plan.clears.push(AppLinkClear {
                            row_id: app.row_id.clone(),
                            previous_hostname: normalized,
                        });
                    }
                }
            }
        }
    }

    plan
}

// ---------------------------------------------------------------------------
// Snapshot cache
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct NpmSnapshot {
    pub fetched_at: i64,
    pub hosts: Vec<NpmHost>,
    pub certificates: Vec<NpmCertificate>,
}

#[derive(Debug, Clone)]
pub struct SnapshotFetch {
    pub snapshot: NpmSnapshot,
    /// True when NPM could not be reached and the last-known snapshot is being served.
    pub stale: bool,
    pub warning: Option<String>,
}

struct CachedSnapshot {
    base_url: String,
    snapshot: NpmSnapshot,
}

static SNAPSHOT_CACHE: OnceLock<Mutex<Option<CachedSnapshot>>> = OnceLock::new();

async fn load_snapshot(client: &NpmClient) -> Result<NpmSnapshot> {
    let hosts = client.list_hosts().await?;
    let certificates = client.list_certificates().await?;
    Ok(NpmSnapshot {
        fetched_at: unix_now(),
        hosts,
        certificates,
    })
}

/// Fetch hosts + certificates, reusing a snapshot younger than [`CACHE_TTL_SECONDS`].
///
/// If NPM is unreachable, the last-known snapshot for the same base URL is returned with
/// `stale = true` so the dashboard can degrade honestly instead of blanking out.
pub async fn fetch_snapshot(client: &NpmClient) -> Result<SnapshotFetch> {
    let cache = SNAPSHOT_CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().await;
    let now = unix_now();

    if let Some(cached) = guard.as_ref() {
        if cached.base_url == client.base_url()
            && now.saturating_sub(cached.snapshot.fetched_at) < CACHE_TTL_SECONDS
        {
            return Ok(SnapshotFetch {
                snapshot: cached.snapshot.clone(),
                stale: false,
                warning: None,
            });
        }
    }

    match load_snapshot(client).await {
        Ok(snapshot) => {
            *guard = Some(CachedSnapshot {
                base_url: client.base_url().to_string(),
                snapshot: snapshot.clone(),
            });
            Ok(SnapshotFetch {
                snapshot,
                stale: false,
                warning: None,
            })
        }
        Err(error) => match guard
            .as_ref()
            .filter(|cached| cached.base_url == client.base_url())
        {
            Some(cached) => Ok(SnapshotFetch {
                snapshot: cached.snapshot.clone(),
                stale: true,
                warning: Some(format!("NPM unreachable: {error}")),
            }),
            None => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn matches_common_abbreviations_and_tokens() {
        let app = |project: &str| DeployedAppLink {
            app_id: project.to_string(),
            app_name: project.to_string(),
            project_name: project.to_string(),
            row_id: "row-1".to_string(),
            hostname: None,
            url: None,
        };
        // Abbreviation: the NPM label is a prefix of the project name.
        assert_eq!(
            match_score(&app("vaultwarden"), "vault", ""),
            Some(("project_name", 2))
        );
        assert_eq!(
            match_score(&app("authentik"), "auth", ""),
            Some(("project_name", 2))
        );
        // Token in a hyphenated project name.
        assert_eq!(
            match_score(&app("skully-chat"), "chat", ""),
            Some(("project_name", 1))
        );
        // Two-letter stubs must not match, or half the zone links wrongly.
        assert_eq!(match_score(&app("immich"), "im", ""), None);
        // Exact matches still outrank abbreviations.
        assert_eq!(
            match_score(&app("vault"), "vault", ""),
            Some(("project_name", 3))
        );
    }

    #[test]
    fn parses_npm_space_separated_expiry() {
        // NPM reports "YYYY-MM-DD HH:MM:SS"; this used to yield None and hide TLS expiry.
        let now = time::macros::datetime!(2026-11-23 00:00:00 UTC);
        assert_eq!(days_until_expiry("2026-12-01 12:00:00", now), Some(8));
        assert_eq!(days_until_expiry("2026-11-01 00:00:00", now), Some(-22));
    }
    use super::*;

    /// Captured shape of `GET /api/nginx/proxy-hosts` from NPM 2.17.1. Token-bearing
    /// fields are absent by construction: NPM never returns credentials on this endpoint.
    const HOSTS_PAYLOAD: &str = r#"[
      {
        "id": 4,
        "created_on": "2026-01-11T09:12:44.000Z",
        "modified_on": "2026-10-02T18:03:11.000Z",
        "owner_user_id": 1,
        "domain_names": ["cloud.heimdallr.cv"],
        "forward_scheme": "http",
        "forward_host": "nextcloud",
        "forward_port": 80,
        "certificate_id": 5,
        "ssl_forced": true,
        "hsts_enabled": false,
        "hsts_subdomains": false,
        "http2_support": false,
        "block_exploit": false,
        "caching_enabled": false,
        "allow_websocket_upgrade": true,
        "access_list_id": 0,
        "ipv6": false,
        "enabled": true,
        "meta": {"letsencrypt_agree": false, "dns_challenge": false, "nginx_err": null},
        "advanced_config": "",
        "locations": [],
        "use_default_location": true
      },
      {
        "id": 9,
        "owner_user_id": 1,
        "domain_names": ["immich.heimdallr.cv", "photos.heimdallr.cv"],
        "forward_scheme": "http",
        "forward_host": "outpost",
        "forward_port": 9000,
        "certificate_id": 0,
        "ssl_forced": true,
        "access_list_id": 1,
        "enabled": true,
        "meta": {"letsencrypt_agree": true, "dns_challenge": false, "nginx_err": "upstream timed out"}
      },
      {
        "id": 15,
        "owner_user_id": 1,
        "domain_names": ["paperless.heimdallr.cv"],
        "forward_scheme": "http",
        "forward_host": "paperless",
        "forward_port": 8000,
        "certificate_id": 5,
        "ssl_forced": true,
        "access_list_id": 0,
        "enabled": true,
        "meta": {"letsencrypt_agree": false, "dns_challenge": false}
      },
      {
        "id": 12,
        "owner_user_id": 1,
        "domain_names": ["old.heimdallr.cv"],
        "forward_scheme": "https",
        "forward_host": "192.168.50.194",
        "forward_port": 21114,
        "certificate_id": 7,
        "ssl_forced": false,
        "access_list_id": 0,
        "enabled": false,
        "meta": null
      }
    ]"#;

    const CERTIFICATES_PAYLOAD: &str = r#"[
      {
        "id": 5,
        "created_on": "2026-09-01T00:00:00.000Z",
        "modified_on": "2026-09-01T00:00:00.000Z",
        "owner_user_id": 1,
        "provider": "letsencrypt",
        "nice_name": "heimdallr-wildcard",
        "domain_names": ["*.heimdallr.cv"],
        "expires_on": "2026-12-01T03:04:05.000Z",
        "meta": {"letsencrypt_agree": true, "dns_challenge": false}
      },
      {
        "id": 7,
        "provider": "other",
        "domain_names": ["old.heimdallr.cv"],
        "expires_on": "2026-11-01T00:00:00.000Z"
      }
    ]"#;

    fn now() -> OffsetDateTime {
        OffsetDateTime::parse("2026-11-22T12:00:00Z", &Rfc3339).unwrap()
    }

    fn parsed_hosts() -> Vec<NpmHost> {
        serde_json::from_str(HOSTS_PAYLOAD).expect("captured hosts payload must parse")
    }

    fn parsed_certificates() -> Vec<NpmCertificate> {
        serde_json::from_str(CERTIFICATES_PAYLOAD)
            .expect("captured certificates payload must parse")
    }

    #[test]
    fn parses_captured_npm_hosts_payload() {
        let hosts = parsed_hosts();
        assert_eq!(hosts.len(), 4);

        let cloud = &hosts[0];
        assert_eq!(cloud.id, 4);
        assert_eq!(cloud.domain_names, vec!["cloud.heimdallr.cv"]);
        assert_eq!(cloud.forward_scheme, "http");
        assert_eq!(cloud.forward_host, "nextcloud");
        assert_eq!(cloud.forward_port, 80);
        assert_eq!(cloud.certificate_id(), Some(5));
        assert!(cloud.ssl_forced);
        assert!(cloud.enabled);
        assert_eq!(cloud.access_list_id(), None);
        assert_eq!(cloud.nginx_err(), None);

        let immich = &hosts[1];
        assert_eq!(immich.certificate_id(), None);
        assert_eq!(immich.access_list_id(), Some(1));
        assert_eq!(immich.nginx_err(), Some("upstream timed out"));

        let old = &hosts[3];
        assert!(!old.enabled);
        assert!(!old.ssl_forced);
        assert_eq!(old.forward_scheme, "https");
    }

    #[test]
    fn parses_captured_npm_certificates_payload() {
        let certificates = parsed_certificates();
        assert_eq!(certificates.len(), 2);
        assert_eq!(certificates[0].provider, "letsencrypt");
        assert_eq!(certificates[0].domain_names, vec!["*.heimdallr.cv"]);
        assert_eq!(
            certificates[0].expires_on.as_deref(),
            Some("2026-12-01T03:04:05.000Z")
        );
        assert_eq!(certificates[1].provider, "other");
    }

    #[test]
    fn expiry_math_counts_whole_days_and_expired_certificates_go_negative() {
        assert_eq!(days_until_expiry("2026-12-22T12:00:00Z", now()), Some(30));
        assert_eq!(days_until_expiry("2026-11-22T12:00:00Z", now()), Some(0));
        // 12 hours from now is still "today", not tomorrow.
        assert_eq!(days_until_expiry("2026-11-23T00:00:00Z", now()), Some(0));
        // One hour in the past must be -1, never 0.
        assert_eq!(days_until_expiry("2026-11-22T11:00:00Z", now()), Some(-1));
        assert_eq!(days_until_expiry("2026-11-01T00:00:00Z", now()), Some(-22));
        assert_eq!(days_until_expiry("not-a-date", now()), None);
    }

    #[test]
    fn certificates_join_uses_nearest_expiry_per_hostname() {
        let hosts = parsed_hosts();
        let certificates = parsed_certificates();
        let views = join_hosts_with_certificates(&hosts, &certificates, now());

        let cloud = &views[0];
        assert_eq!(cloud.certificate_id, Some(5));
        assert_eq!(cloud.domains.len(), 1);
        // 2026-12-01T03:04:05 minus 2026-11-22T12:00:00 = 8 whole days.
        assert_eq!(cloud.domains[0].days_to_expiry, Some(8));
        assert_eq!(cloud.domains[0].certificate_id, Some(5));

        let immich = &views[1];
        // No attached cert, but the wildcard covers both hostnames.
        assert_eq!(immich.domains.len(), 2);
        for domain in &immich.domains {
            assert_eq!(domain.certificate_id, Some(5));
            assert_eq!(domain.days_to_expiry, Some(8));
        }
        assert_eq!(immich.access_list_id, Some(1));
        assert_eq!(immich.nginx_err.as_deref(), Some("upstream timed out"));

        let old = &views[3];
        assert_eq!(old.certificate_id, Some(7));
        assert_eq!(old.domains[0].days_to_expiry, Some(-22));
    }

    #[test]
    fn expired_certificate_is_negative_not_zero() {
        let certificate = NpmCertificate {
            id: 1,
            domain_names: vec!["example.test".into()],
            expires_on: Some("2026-11-22T11:59:59Z".into()),
            provider: "other".into(),
        };
        let views = certificate_views(&[certificate], now());
        assert_eq!(views[0].days_to_expiry, Some(-1));
    }

    fn app(row_id: &str, app_id: &str, project_name: &str) -> DeployedAppLink {
        DeployedAppLink {
            row_id: row_id.into(),
            app_id: app_id.into(),
            app_name: project_name.into(),
            project_name: project_name.into(),
            hostname: None,
            url: None,
        }
    }

    #[test]
    fn sync_matches_project_name_app_id_and_voidtower_prefix() {
        let hosts = parsed_hosts();
        let apps = vec![
            app("row-1", "nextcloud", "nextcloud-stack"),
            app("row-2", "immich-app", "immich"),
            app("row-3", "paperless-ngx", "voidtower-paperless"),
            app("row-4", "unknown", "unmatched-stack"),
        ];

        let plan = plan_sync(&apps, &hosts);
        assert_eq!(plan.matched, 3);
        assert_eq!(plan.unmatched, vec!["unmatched-stack".to_string()]);
        assert_eq!(plan.clears.len(), 0);

        let nextcloud = plan
            .updates
            .iter()
            .find(|update| update.row_id == "row-1")
            .expect("nextcloud must be linked");
        assert_eq!(nextcloud.hostname, "cloud.heimdallr.cv");
        assert_eq!(nextcloud.url, "https://cloud.heimdallr.cv");
        // The hostname label is `cloud`; the link is proven by NPM's `forward_host` (nextcloud).
        assert_eq!(nextcloud.matched_on, "forward_host");

        let immich = plan
            .updates
            .iter()
            .find(|update| update.row_id == "row-2")
            .expect("immich must be linked");
        // Only `immich.heimdallr.cv` matches; `photos.…` does not.
        assert_eq!(immich.hostname, "immich.heimdallr.cv");
        assert_eq!(immich.matched_on, "project_name");
        let paperless = plan
            .updates
            .iter()
            .find(|update| update.row_id == "row-3")
            .expect("prefixed project must be linked");
        assert_eq!(paperless.hostname, "paperless.heimdallr.cv");
    }

    #[test]
    fn sync_ignores_disabled_hosts_but_clears_their_stale_links() {
        let mut hosts = parsed_hosts();
        // Give the disabled host a domain that matches an app.
        hosts[3].domain_names = vec!["old.heimdallr.cv".into()];
        let mut old_app = app("row-5", "old", "old");
        old_app.hostname = Some("old.heimdallr.cv".into());
        old_app.url = Some("https://old.heimdallr.cv".into());

        let plan = plan_sync(&[old_app], &hosts);
        assert_eq!(plan.matched, 0);
        assert_eq!(plan.updates.len(), 0);
        assert_eq!(plan.clears.len(), 1);
        assert_eq!(plan.clears[0].row_id, "row-5");
        assert_eq!(plan.clears[0].previous_hostname, "old.heimdallr.cv");
    }

    #[test]
    fn sync_resolves_hostname_collisions_in_favour_of_the_strongest_link() {
        let hosts = parsed_hosts();
        let apps = vec![
            // `forward_host = outpost` is a stronger link than a bare hostname label.
            app("row-b", "outpost", "outpost-gate"),
            app("row-a", "immich-app", "immich"),
        ];

        let plan = plan_sync(&apps, &hosts);
        assert_eq!(plan.matched, 1, "one hostname must not link two apps");
        assert_eq!(plan.updates.len(), 1);
        assert_eq!(plan.updates[0].row_id, "row-b");
        assert_eq!(plan.updates[0].hostname, "immich.heimdallr.cv");
        assert_eq!(plan.updates[0].matched_on, "forward_host");
        assert_eq!(plan.unmatched, vec!["immich".to_string()]);
        assert_eq!(plan.clears.len(), 0);
    }

    #[test]
    fn sync_leaves_manual_links_alone_and_does_not_rewrite_current_rows() {
        let hosts = parsed_hosts();
        let mut current = app("row-1", "nextcloud", "nextcloud");
        current.hostname = Some("cloud.heimdallr.cv".into());
        current.url = Some("https://cloud.heimdallr.cv".into());
        let mut manual = app("row-6", "vault", "vault");
        manual.hostname = Some("vault.example.net".into());
        manual.url = Some("https://vault.example.net".into());

        let plan = plan_sync(&[current, manual], &hosts);
        assert_eq!(plan.matched, 1);
        assert_eq!(plan.updates.len(), 0, "already-current row must not churn");
        assert_eq!(plan.clears.len(), 0, "manual non-NPM link must survive");
        assert_eq!(plan.unmatched, vec!["vault".to_string()]);
    }

    #[test]
    fn credential_file_accepts_bearer_tokens_and_identity_passwords() {
        let dir = std::env::temp_dir().join(format!("vt-npm-cred-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join(TOKEN_FILE_NAME), "opaque-token-value\n").unwrap();
        let bearer = NpmSettings::load_with(&dir, |_| None).unwrap().unwrap();
        assert!(matches!(bearer.credential, Credential::Bearer(_)));
        assert_eq!(bearer.base_url(), DEFAULT_BASE_URL);

        std::fs::write(dir.join(TOKEN_FILE_NAME), "admin@example.test:s3cret\n").unwrap();
        let login = NpmSettings::load_with(&dir, |_| None).unwrap().unwrap();
        assert!(matches!(login.credential, Credential::Login { .. }));

        std::fs::remove_file(dir.join(TOKEN_FILE_NAME)).unwrap();
        assert!(NpmSettings::load_with(&dir, |_| None).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_base_url_env_override_is_trimmed() {
        let settings = NpmSettings::load_with(Path::new("/nonexistent"), |name| match name {
            "VOIDTOWER_NPM_URL" => Some("http://npm.internal:81/".to_string()),
            "VOIDTOWER_NPM_TOKEN" => Some("token".to_string()),
            _ => None,
        })
        .unwrap()
        .unwrap();
        assert_eq!(settings.base_url(), "http://npm.internal:81");
    }

    #[test]
    fn env_token_takes_precedence_over_the_credential_file() {
        let dir = std::env::temp_dir().join(format!("vt-npm-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(TOKEN_FILE_NAME), "file-token\n").unwrap();

        let settings = NpmSettings::load_with(&dir, |name| match name {
            "VOIDTOWER_NPM_TOKEN" => Some("  env-token  ".to_string()),
            _ => None,
        })
        .unwrap()
        .unwrap();
        match settings.credential {
            Credential::Bearer(token) => assert_eq!(token, "env-token"),
            Credential::Login { .. } => panic!("an env token must win over the credential file"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
