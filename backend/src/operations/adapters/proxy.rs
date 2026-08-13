//! Durable reverse-proxy rule and nginx lifecycle adapter.

use super::{OperationAdapter, PlanRequest, ReconcileOutcome, StepOutcome, StepRequest};
use crate::{
    api::{
        mcp::action_registry::{self, RiskClass},
        proxy::{self as proxy_api, CreateRequest, CustomHeader, ProxyConfig},
    },
    networking::proxy::{self as nginx_provider, NginxAction, NginxSnapshot},
    operations::{
        canonical_json,
        contracts::{OperationPlanV1, PlanChange, PlannedStepV1},
        resources::{self, ObserveResource},
    },
};
use anyhow::{bail, ensure, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;
use std::sync::Arc;

const ACTIONS: &[&str] = &[
    "proxy.rule.create",
    "proxy.rule.update",
    "proxy.rule.delete",
    "proxy.rule.toggle",
    "proxy.nginx.start",
    "proxy.nginx.stop",
    "proxy.nginx.restart",
    "proxy.nginx.reload",
];

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleInput {
    domain: String,
    upstream: String,
    #[serde(default)]
    ssl: bool,
    #[serde(default)]
    allow_embed: bool,
    #[serde(default)]
    sso_protect: bool,
    #[serde(default)]
    custom_headers: Vec<HeaderInput>,
    #[serde(default)]
    rate_limit_rpm: Option<i64>,
    #[serde(default)]
    basic_auth_user: Option<String>,
    #[serde(default)]
    basic_auth_secret_id: Option<String>,
    #[serde(default)]
    websocket_extended: bool,
    #[serde(default)]
    cache_static: bool,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderInput {
    name: String,
    value: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToggleInput {
    enabled: bool,
}

#[derive(Clone)]
enum ProxyMutation {
    Create { id: String, input: RuleInput },
    Update { id: String, input: RuleInput },
    Delete { id: String },
    SetEnabled { id: String, enabled: bool },
    Nginx(NginxAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProxyRuleSnapshot {
    id: String,
    domain: String,
    upstream: String,
    ssl: bool,
    enabled: bool,
    allow_embed: bool,
    sso_protect: bool,
    custom_headers: Option<String>,
    rate_limit_rpm: Option<i64>,
    basic_auth_user: Option<String>,
    basic_auth_configured: bool,
    websocket_extended: bool,
    cache_static: bool,
}

impl From<ProxyConfig> for ProxyRuleSnapshot {
    fn from(config: ProxyConfig) -> Self {
        Self {
            id: config.id,
            domain: config.domain,
            upstream: config.upstream,
            ssl: config.ssl,
            enabled: config.enabled,
            allow_embed: config.allow_embed,
            sso_protect: config.sso_protect,
            custom_headers: config.custom_headers,
            rate_limit_rpm: config.rate_limit_rpm,
            basic_auth_user: config.basic_auth_user,
            basic_auth_configured: config.basic_auth_pass_hash.is_some(),
            websocket_extended: config.websocket_extended,
            cache_static: config.cache_static,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ProxySnapshot {
    nginx: NginxSnapshot,
    oidc_enabled: bool,
    rules: Vec<ProxyRuleSnapshot>,
    secrets: Vec<SecretReferenceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SecretReferenceSnapshot {
    id: String,
    version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyMutationResult {
    message: String,
    proxy_id: Option<String>,
    resource_id: Option<String>,
}

#[async_trait]
trait ProxyProvider: Send + Sync {
    async fn snapshot(
        &self,
        proxy_id: Option<&str>,
        secret_ids: &[String],
    ) -> Result<ProxySnapshot>;
    async fn execute(
        &self,
        mutation: ProxyMutation,
        correlation_id: &str,
    ) -> Result<ProxyMutationResult>;
    async fn contains_known_secret(&self, candidates: &[String]) -> Result<bool>;
}

struct LocalProxyProvider {
    pool: SqlitePool,
    secrets_key: Arc<[u8; 32]>,
}

impl LocalProxyProvider {
    fn new(pool: SqlitePool, secrets_key: Arc<[u8; 32]>) -> Self {
        Self { pool, secrets_key }
    }

    async fn config(&self, id: &str) -> Result<ProxyConfig> {
        sqlx::query_as("SELECT * FROM proxy_configs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .context("proxy rule is not present")
    }

    async fn oidc_enabled(&self) -> bool {
        sqlx::query_scalar::<_, bool>("SELECT enabled FROM oidc_config WHERE id = 'default'")
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    async fn resolve_secret(&self, secret_id: Option<&str>) -> Result<Option<String>> {
        let Some(secret_id) = secret_id else {
            return Ok(None);
        };
        let encrypted: String = sqlx::query_scalar("SELECT value_enc FROM secrets WHERE id = ?")
            .bind(secret_id)
            .fetch_optional(&self.pool)
            .await?
            .context("referenced basic-auth secret is not present")?;
        Ok(Some(crate::api::secrets::decrypt(
            &self.secrets_key,
            &encrypted,
        )?))
    }

    async fn create(
        &self,
        id: &str,
        input: &RuleInput,
        correlation_id: &str,
    ) -> Result<ProxyMutationResult> {
        ensure!(
            !input.sso_protect || self.oidc_enabled().await,
            "Authentik SSO is not configured"
        );
        let password = self
            .resolve_secret(input.basic_auth_secret_id.as_deref())
            .await?;
        let config = build_config(
            id.into(),
            input,
            password,
            crate::operations::unix_now(),
            None,
        )?;
        sqlx::query(
            "INSERT INTO proxy_configs \
             (id, domain, upstream, ssl, enabled, allow_embed, sso_protect, created_at, \
              custom_headers, rate_limit_rpm, basic_auth_user, basic_auth_pass_hash, \
              websocket_extended, cache_static) VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(&config.domain)
        .bind(&config.upstream)
        .bind(config.ssl)
        .bind(config.allow_embed)
        .bind(config.sso_protect)
        .bind(config.created_at)
        .bind(&config.custom_headers)
        .bind(config.rate_limit_rpm)
        .bind(&config.basic_auth_user)
        .bind(&config.basic_auth_pass_hash)
        .bind(config.websocket_extended)
        .bind(config.cache_static)
        .execute(&self.pool)
        .await?;
        apply_enabled_config(config.clone()).await?;
        let resource = resources::observe(
            &self.pool,
            ObserveResource {
                kind: "proxy_rule",
                display_name: &config.domain,
                node_id: None,
                provider: Some("nginx"),
                namespace: "voidtower.proxy_config",
                scope_key: "local",
                alias: id,
            },
            None,
            correlation_id,
        )
        .await?;
        Ok(ProxyMutationResult {
            message: "proxy rule created and nginx reloaded".into(),
            proxy_id: Some(id.into()),
            resource_id: Some(resource.id),
        })
    }

    async fn update(
        &self,
        id: &str,
        input: &RuleInput,
        correlation_id: &str,
    ) -> Result<ProxyMutationResult> {
        ensure!(
            !input.sso_protect || self.oidc_enabled().await,
            "Authentik SSO is not configured"
        );
        let existing = self.config(id).await?;
        let old_domain = existing.domain.clone();
        let password = self
            .resolve_secret(input.basic_auth_secret_id.as_deref())
            .await?;
        let config = build_config(
            id.into(),
            input,
            password,
            existing.created_at,
            Some(&existing),
        )?;
        sqlx::query(
            "UPDATE proxy_configs SET domain = ?, upstream = ?, ssl = ?, allow_embed = ?, \
             sso_protect = ?, custom_headers = ?, rate_limit_rpm = ?, basic_auth_user = ?, \
             basic_auth_pass_hash = ?, websocket_extended = ?, cache_static = ? WHERE id = ?",
        )
        .bind(&config.domain)
        .bind(&config.upstream)
        .bind(config.ssl)
        .bind(config.allow_embed)
        .bind(config.sso_protect)
        .bind(&config.custom_headers)
        .bind(config.rate_limit_rpm)
        .bind(&config.basic_auth_user)
        .bind(&config.basic_auth_pass_hash)
        .bind(config.websocket_extended)
        .bind(config.cache_static)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if old_domain != config.domain {
            tokio::task::spawn_blocking(move || proxy_api::remove_nginx_conf_checked(&old_domain))
                .await?
                .map_err(anyhow::Error::new)?;
        }
        if config.enabled {
            apply_enabled_config(config.clone()).await?;
        }
        let resource = resources::observe(
            &self.pool,
            ObserveResource {
                kind: "proxy_rule",
                display_name: &config.domain,
                node_id: None,
                provider: Some("nginx"),
                namespace: "voidtower.proxy_config",
                scope_key: "local",
                alias: id,
            },
            None,
            correlation_id,
        )
        .await?;
        Ok(ProxyMutationResult {
            message: if config.enabled {
                "proxy rule updated and nginx reloaded".into()
            } else {
                "disabled proxy rule updated".into()
            },
            proxy_id: Some(id.into()),
            resource_id: Some(resource.id),
        })
    }

    async fn delete(&self, id: &str) -> Result<ProxyMutationResult> {
        let config = self.config(id).await?;
        sqlx::query("DELETE FROM proxy_configs WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let domain = config.domain;
        tokio::task::spawn_blocking(move || -> Result<()> {
            proxy_api::remove_nginx_conf_checked(&domain).map_err(anyhow::Error::new)?;
            proxy_api::reload_nginx().map_err(anyhow::Error::msg)?;
            Ok(())
        })
        .await??;
        Ok(ProxyMutationResult {
            message: "proxy rule deleted and nginx reloaded".into(),
            proxy_id: Some(id.into()),
            resource_id: None,
        })
    }

    async fn set_enabled(&self, id: &str, enabled: bool) -> Result<ProxyMutationResult> {
        let mut config = self.config(id).await?;
        sqlx::query("UPDATE proxy_configs SET enabled = ? WHERE id = ?")
            .bind(enabled)
            .bind(id)
            .execute(&self.pool)
            .await?;
        config.enabled = enabled;
        if enabled {
            apply_enabled_config(config).await?;
        } else {
            let domain = config.domain;
            tokio::task::spawn_blocking(move || -> Result<()> {
                proxy_api::remove_nginx_conf_checked(&domain).map_err(anyhow::Error::new)?;
                proxy_api::reload_nginx().map_err(anyhow::Error::msg)?;
                Ok(())
            })
            .await??;
        }
        Ok(ProxyMutationResult {
            message: format!(
                "proxy rule {}",
                if enabled { "enabled" } else { "disabled" }
            ),
            proxy_id: Some(id.into()),
            resource_id: None,
        })
    }
}

#[async_trait]
impl ProxyProvider for LocalProxyProvider {
    async fn snapshot(
        &self,
        proxy_id: Option<&str>,
        secret_ids: &[String],
    ) -> Result<ProxySnapshot> {
        let mut rules: Vec<ProxyConfig> = if let Some(id) = proxy_id {
            sqlx::query_as("SELECT * FROM proxy_configs WHERE id = ?")
                .bind(id)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query_as("SELECT * FROM proxy_configs ORDER BY id")
                .fetch_all(&self.pool)
                .await?
        };
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        let mut secrets = Vec::new();
        for secret_id in secret_ids {
            if let Some((id, version)) =
                sqlx::query_as::<_, (String, i64)>("SELECT id, version FROM secrets WHERE id = ?")
                    .bind(secret_id)
                    .fetch_optional(&self.pool)
                    .await?
            {
                secrets.push(SecretReferenceSnapshot { id, version });
            }
        }
        secrets.sort_by(|left, right| left.id.cmp(&right.id));
        let nginx = tokio::task::spawn_blocking(nginx_provider::snapshot).await??;
        Ok(ProxySnapshot {
            nginx,
            oidc_enabled: self.oidc_enabled().await,
            rules: rules.into_iter().map(ProxyRuleSnapshot::from).collect(),
            secrets,
        })
    }

    async fn execute(
        &self,
        mutation: ProxyMutation,
        correlation_id: &str,
    ) -> Result<ProxyMutationResult> {
        match mutation {
            ProxyMutation::Create { id, input } => self.create(&id, &input, correlation_id).await,
            ProxyMutation::Update { id, input } => self.update(&id, &input, correlation_id).await,
            ProxyMutation::Delete { id } => self.delete(&id).await,
            ProxyMutation::SetEnabled { id, enabled } => self.set_enabled(&id, enabled).await,
            ProxyMutation::Nginx(action) => {
                let result =
                    tokio::task::spawn_blocking(move || nginx_provider::execute(action)).await??;
                Ok(ProxyMutationResult {
                    message: result.message,
                    proxy_id: None,
                    resource_id: None,
                })
            }
        }
        .with_context(|| format!("proxy mutation {correlation_id} failed"))
    }

    async fn contains_known_secret(&self, candidates: &[String]) -> Result<bool> {
        let encrypted: Vec<String> = sqlx::query_scalar("SELECT value_enc FROM secrets")
            .fetch_all(&self.pool)
            .await?;
        let mut known = Vec::new();
        for value in encrypted {
            let decrypted = crate::api::secrets::decrypt(&self.secrets_key, &value)
                .context("cannot validate proxy input against the secret vault")?;
            if decrypted.chars().count() >= 4 {
                known.push(decrypted);
            }
        }
        Ok(candidates
            .iter()
            .any(|candidate| known.iter().any(|secret| candidate.contains(secret))))
    }
}

async fn apply_enabled_config(config: ProxyConfig) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        proxy_api::write_nginx_conf(&config).map_err(anyhow::Error::new)?;
        proxy_api::reload_nginx().map_err(anyhow::Error::msg)?;
        Ok(())
    })
    .await??;
    Ok(())
}

pub struct ProxyAdapter {
    pool: SqlitePool,
    provider: Arc<dyn ProxyProvider>,
}

impl ProxyAdapter {
    pub fn new(pool: SqlitePool, secrets_key: Arc<[u8; 32]>) -> Self {
        Self {
            provider: Arc::new(LocalProxyProvider::new(pool.clone(), secrets_key)),
            pool,
        }
    }

    #[cfg(test)]
    fn with_provider(pool: SqlitePool, provider: Arc<dyn ProxyProvider>) -> Self {
        Self { pool, provider }
    }

    async fn native_id(&self, resource_id: &str) -> Result<String> {
        let aliases: Vec<String> = sqlx::query_scalar(
            "SELECT value FROM resource_aliases WHERE resource_id = ? \
             AND namespace = 'voidtower.proxy_config' ORDER BY scope_key, value",
        )
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await?;
        ensure!(
            aliases.len() == 1,
            "proxy rule resource must have exactly one voidtower.proxy_config alias"
        );
        Ok(aliases.into_iter().next().expect("length checked"))
    }

    async fn selector(&self, action: &str, resource_id: &str) -> Result<Option<String>> {
        if action.starts_with("proxy.rule.") && action != "proxy.rule.create" {
            Ok(Some(self.native_id(resource_id).await?))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl OperationAdapter for ProxyAdapter {
    fn key(&self) -> &'static str {
        "proxy"
    }

    fn actions(&self) -> &[&'static str] {
        ACTIONS
    }

    async fn plan(&self, request: PlanRequest) -> Result<OperationPlanV1> {
        validate_target(&request.action, &request.resource.kind)?;
        let parsed = parse_input(&request.action, &request.input)?;
        let literals = literal_candidates(&parsed);
        if !literals.is_empty() {
            ensure!(
                !self.provider.contains_known_secret(&literals).await?,
                "proxy input contains a stored secret value; use a secret reference"
            );
        }
        let selector = self.selector(&request.action, &request.resource.id).await?;
        let secret_ids = secret_ids(&parsed);
        let snapshot = self
            .provider
            .snapshot(selector.as_deref(), &secret_ids)
            .await?;
        validate_against_snapshot(&request.action, &parsed, &snapshot)?;
        let metadata = action_registry::action(&request.action)
            .context("proxy action is absent from the action registry")?;
        Ok(OperationPlanV1 {
            schema_version: 1,
            title: plan_title(&request.action)?.into(),
            risk: risk_name(metadata.risk).into(),
            changes: plan_changes(&request.action, &parsed, &snapshot)?,
            preview: None,
            external_fingerprint: canonical_json::digest(&snapshot)?,
            steps: vec![PlannedStepV1 {
                kind: "execute".into(),
                name: request.action,
                retry_class: metadata
                    .retry
                    .context("proxy action has no retry metadata")?
                    .class
                    .as_str()
                    .into(),
                recovery_class: metadata
                    .recovery
                    .context("proxy action has no recovery metadata")?
                    .as_str()
                    .into(),
            }],
        })
    }

    async fn external_fingerprint(&self, request: &PlanRequest) -> Result<String> {
        validate_target(&request.action, &request.resource.kind)?;
        let parsed = parse_input(&request.action, &request.input)?;
        let selector = self.selector(&request.action, &request.resource.id).await?;
        let secret_ids = secret_ids(&parsed);
        canonical_json::digest(
            &self
                .provider
                .snapshot(selector.as_deref(), &secret_ids)
                .await?,
        )
    }

    async fn execute_step(&self, request: StepRequest) -> Result<StepOutcome> {
        validate_target(&request.action, &request.resource.kind)?;
        ensure!(
            request.step.kind == "execute",
            "unsupported proxy step kind"
        );
        ensure!(
            request.step.name == request.action,
            "proxy step/action mismatch"
        );
        let parsed = parse_input(&request.action, &request.input)?;
        let native_id = self.selector(&request.action, &request.resource.id).await?;
        let mutation = mutation_for(&request.action, parsed, native_id, &request.job_id)?;
        match self.provider.execute(mutation, &request.job_id).await {
            Ok(result) => Ok(StepOutcome::Succeeded {
                result: serde_json::json!({
                    "action": request.action,
                    "message": safe_text(&result.message),
                    "proxy_id": result.proxy_id,
                    "resource_id": result.resource_id,
                }),
                external_operation_id: None,
            }),
            Err(error) => Ok(StepOutcome::Uncertain {
                code: "proxy_execution_uncertain".into(),
                message: safe_text(&format!(
                    "The reverse-proxy provider did not report a conclusive outcome: {error}"
                )),
                external_operation_id: None,
                diagnostic: None,
            }),
        }
    }

    async fn reconcile(&self, request: StepRequest) -> Result<ReconcileOutcome> {
        validate_target(&request.action, &request.resource.kind)?;
        let parsed = parse_input(&request.action, &request.input)?;
        let selector = self.selector(&request.action, &request.resource.id).await?;
        let secret_ids = secret_ids(&parsed);
        let snapshot = self
            .provider
            .snapshot(selector.as_deref(), &secret_ids)
            .await?;
        match request.action.as_str() {
            "proxy.nginx.start" if snapshot.nginx.active => Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({"action": request.action, "verified": true}),
            }),
            "proxy.nginx.stop" if !snapshot.nginx.active => Ok(ReconcileOutcome::Succeeded {
                result: serde_json::json!({"action": request.action, "verified": true}),
            }),
            "proxy.nginx.start" | "proxy.nginx.stop" => Ok(ReconcileOutcome::Failed {
                code: "proxy_nginx_state_mismatch".into(),
                message: "Observed nginx-proxy state does not match the requested outcome".into(),
            }),
            "proxy.rule.create"
            | "proxy.rule.update"
            | "proxy.rule.delete"
            | "proxy.rule.toggle"
            | "proxy.nginx.restart"
            | "proxy.nginx.reload" => Ok(ReconcileOutcome::StillUncertain {
                message: "Current proxy state cannot prove that nginx accepted this operation"
                    .into(),
            }),
            _ => bail!("unsupported proxy action"),
        }
    }
}

enum ParsedInput {
    Rule(RuleInput),
    Toggle(bool),
    Unit,
}

fn secret_ids(parsed: &ParsedInput) -> Vec<String> {
    let ParsedInput::Rule(input) = parsed else {
        return Vec::new();
    };
    let mut ids: Vec<String> = input.basic_auth_secret_id.iter().cloned().collect();
    ids.sort();
    ids.dedup();
    ids
}

fn literal_candidates(parsed: &ParsedInput) -> Vec<String> {
    let ParsedInput::Rule(input) = parsed else {
        return Vec::new();
    };
    std::iter::once(input.upstream.clone())
        .chain(
            input
                .custom_headers
                .iter()
                .map(|header| header.value.clone()),
        )
        .collect()
}

fn parse_input(action: &str, input: &Value) -> Result<ParsedInput> {
    match action {
        "proxy.rule.create" | "proxy.rule.update" => {
            let input: RuleInput = serde_json::from_value(input.clone())?;
            validate_rule_input(&input)?;
            Ok(ParsedInput::Rule(input))
        }
        "proxy.rule.toggle" => {
            let input: ToggleInput = serde_json::from_value(input.clone())?;
            Ok(ParsedInput::Toggle(input.enabled))
        }
        "proxy.rule.delete"
        | "proxy.nginx.start"
        | "proxy.nginx.stop"
        | "proxy.nginx.restart"
        | "proxy.nginx.reload" => {
            ensure!(
                input.as_object().is_some_and(serde_json::Map::is_empty),
                "proxy action input must be an empty object"
            );
            Ok(ParsedInput::Unit)
        }
        _ => bail!("unsupported proxy action"),
    }
}

fn validate_rule_input(input: &RuleInput) -> Result<()> {
    ensure!(
        !input.domain.is_empty() && input.domain.len() <= 253,
        "invalid proxy domain length"
    );
    let domain = input.domain.strip_prefix("*.").unwrap_or(&input.domain);
    ensure!(
        domain
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
            && !domain.starts_with('.')
            && !domain.ends_with('.')
            && !domain.contains(".."),
        "proxy domain contains invalid characters"
    );
    let upstream = reqwest::Url::parse(&input.upstream).context("invalid proxy upstream URL")?;
    ensure!(
        matches!(upstream.scheme(), "http" | "https") && upstream.host().is_some(),
        "proxy upstream must be an HTTP(S) URL with a host"
    );
    ensure!(
        upstream.username().is_empty() && upstream.password().is_none(),
        "proxy upstream URL must not contain credentials"
    );
    ensure!(
        upstream.query().is_none() && upstream.fragment().is_none(),
        "proxy upstream URL must not contain a query or fragment"
    );
    let host = upstream.host_str().unwrap_or_default().to_ascii_lowercase();
    ensure!(
        !host.starts_with("169.254.") && host != "0.0.0.0" && host != "::",
        "proxy upstream address is not permitted"
    );
    if let Some(rate) = input.rate_limit_rpm {
        ensure!(rate > 0, "proxy rate limit must be positive");
    }
    for header in &input.custom_headers {
        ensure!(
            !header.name.is_empty()
                && header
                    .name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-'),
            "proxy header name is invalid"
        );
        ensure!(!header.value.is_empty(), "proxy header value is empty");
        ensure!(
            !header.value.contains(['\n', '\r']),
            "proxy header value contains a control character"
        );
        ensure!(
            !sensitive_header(&header.name),
            "sensitive proxy headers are not accepted as literal durable input"
        );
        ensure!(
            crate::api::mcp::redact::redact_patterns(&header.value) == header.value,
            "proxy header value resembles a credential"
        );
    }
    let user = input
        .basic_auth_user
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let secret_id = input
        .basic_auth_secret_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    ensure!(
        user.is_some() || secret_id.is_none(),
        "basic-auth secret requires a username"
    );
    if let Some(secret_id) = secret_id {
        ensure!(
            uuid::Uuid::parse_str(secret_id).is_ok(),
            "basic-auth secret ID must be a UUID"
        );
    }
    if let Some(user) = user {
        ensure!(
            !user.contains([':', '\n', '\r']),
            "basic-auth username contains an invalid character"
        );
    }
    Ok(())
}

fn validate_against_snapshot(
    action: &str,
    parsed: &ParsedInput,
    snapshot: &ProxySnapshot,
) -> Result<()> {
    match (action, parsed) {
        ("proxy.rule.create", ParsedInput::Rule(input)) => {
            ensure!(
                !snapshot
                    .rules
                    .iter()
                    .any(|rule| rule.domain == input.domain),
                "proxy domain already exists"
            );
            validate_secret_references(input, snapshot)?;
            validate_sso_and_auth(input, None, snapshot.oidc_enabled)
        }
        ("proxy.rule.update", ParsedInput::Rule(input)) => {
            let existing = snapshot
                .rules
                .first()
                .context("proxy rule is not present")?;
            validate_secret_references(input, snapshot)?;
            validate_sso_and_auth(input, Some(existing), snapshot.oidc_enabled)
        }
        ("proxy.rule.delete" | "proxy.rule.toggle", _) => {
            ensure!(snapshot.rules.len() == 1, "proxy rule is not present");
            Ok(())
        }
        (action, ParsedInput::Unit) if action.starts_with("proxy.nginx.") => Ok(()),
        _ => bail!("proxy input/action mismatch"),
    }
}

fn validate_sso_and_auth(
    input: &RuleInput,
    existing: Option<&ProxyRuleSnapshot>,
    oidc_enabled: bool,
) -> Result<()> {
    ensure!(
        !input.sso_protect || oidc_enabled,
        "Authentik SSO is not configured"
    );
    let Some(user) = input
        .basic_auth_user
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let has_new_secret = input
        .basic_auth_secret_id
        .as_deref()
        .is_some_and(|secret_id| !secret_id.trim().is_empty());
    let can_keep_existing = existing.is_some_and(|rule| {
        rule.basic_auth_user.as_deref() == Some(user) && rule.basic_auth_configured
    });
    ensure!(
        has_new_secret || can_keep_existing,
        "basic-auth secret reference is required"
    );
    Ok(())
}

fn validate_secret_references(input: &RuleInput, snapshot: &ProxySnapshot) -> Result<()> {
    let parsed = ParsedInput::Rule(input.clone());
    for secret_id in secret_ids(&parsed) {
        ensure!(
            snapshot.secrets.iter().any(|secret| secret.id == secret_id),
            "referenced proxy secret is not present"
        );
    }
    Ok(())
}

fn sensitive_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "x-auth-token"
    ) || normalized.contains("secret")
        || normalized.contains("token")
        || normalized.ends_with("-key")
}

fn mutation_for(
    action: &str,
    parsed: ParsedInput,
    native_id: Option<String>,
    job_id: &str,
) -> Result<ProxyMutation> {
    Ok(match (action, parsed) {
        ("proxy.rule.create", ParsedInput::Rule(input)) => ProxyMutation::Create {
            id: job_id.into(),
            input,
        },
        ("proxy.rule.update", ParsedInput::Rule(input)) => ProxyMutation::Update {
            id: native_id.context("proxy rule alias is missing")?,
            input,
        },
        ("proxy.rule.delete", ParsedInput::Unit) => ProxyMutation::Delete {
            id: native_id.context("proxy rule alias is missing")?,
        },
        ("proxy.rule.toggle", ParsedInput::Toggle(enabled)) => ProxyMutation::SetEnabled {
            id: native_id.context("proxy rule alias is missing")?,
            enabled,
        },
        ("proxy.nginx.start", ParsedInput::Unit) => ProxyMutation::Nginx(NginxAction::Start),
        ("proxy.nginx.stop", ParsedInput::Unit) => ProxyMutation::Nginx(NginxAction::Stop),
        ("proxy.nginx.restart", ParsedInput::Unit) => ProxyMutation::Nginx(NginxAction::Restart),
        ("proxy.nginx.reload", ParsedInput::Unit) => ProxyMutation::Nginx(NginxAction::Reload),
        _ => bail!("proxy input/action mismatch"),
    })
}

fn plan_changes(
    action: &str,
    parsed: &ParsedInput,
    snapshot: &ProxySnapshot,
) -> Result<Vec<PlanChange>> {
    Ok(match (action, parsed) {
        ("proxy.rule.create", ParsedInput::Rule(input)) => rule_changes("Create", input),
        ("proxy.rule.update", ParsedInput::Rule(input)) => {
            let current = snapshot
                .rules
                .first()
                .context("proxy rule is not present")?;
            let mut changes = rule_changes("Update", input);
            changes.push(PlanChange {
                label: "Current domain".into(),
                value: current.domain.clone(),
            });
            changes
        }
        ("proxy.rule.delete", ParsedInput::Unit) => vec![PlanChange {
            label: "Rule".into(),
            value: snapshot
                .rules
                .first()
                .context("proxy rule is not present")?
                .domain
                .clone(),
        }],
        ("proxy.rule.toggle", ParsedInput::Toggle(enabled)) => vec![PlanChange {
            label: "Desired state".into(),
            value: if *enabled { "Enabled" } else { "Disabled" }.into(),
        }],
        (action, ParsedInput::Unit) if action.starts_with("proxy.nginx.") => vec![PlanChange {
            label: "Current nginx state".into(),
            value: snapshot.nginx.state.clone(),
        }],
        _ => bail!("proxy input/action mismatch"),
    })
}

fn rule_changes(verb: &str, input: &RuleInput) -> Vec<PlanChange> {
    vec![
        PlanChange {
            label: "Action".into(),
            value: format!("{verb} proxy rule"),
        },
        PlanChange {
            label: "Domain".into(),
            value: input.domain.clone(),
        },
        PlanChange {
            label: "Upstream".into(),
            value: input.upstream.clone(),
        },
        PlanChange {
            label: "TLS".into(),
            value: if input.ssl { "Enabled" } else { "Disabled" }.into(),
        },
    ]
}

fn build_config(
    id: String,
    input: &RuleInput,
    basic_auth_password: Option<String>,
    created_at: i64,
    existing: Option<&ProxyConfig>,
) -> Result<ProxyConfig> {
    let request = CreateRequest {
        domain: input.domain.clone(),
        upstream: input.upstream.clone(),
        ssl: input.ssl,
        allow_embed: input.allow_embed,
        sso_protect: input.sso_protect,
        dry_run: false,
        custom_headers: input
            .custom_headers
            .iter()
            .map(|header| CustomHeader {
                name: header.name.clone(),
                value: header.value.clone(),
            })
            .collect(),
        rate_limit_rpm: input.rate_limit_rpm,
        basic_auth_user: input.basic_auth_user.clone(),
        basic_auth_password,
        websocket_extended: input.websocket_extended,
        cache_static: input.cache_static,
    };
    proxy_api::build_proxy_config(id, input.domain.clone(), &request, created_at, existing)
        .map_err(anyhow::Error::new)
}

fn validate_target(action: &str, resource_kind: &str) -> Result<()> {
    ensure!(ACTIONS.contains(&action), "unsupported proxy action");
    let expected = action_registry::action(action)
        .context("proxy action is absent from the action registry")?
        .resource_kind
        .context("proxy action has no resource kind")?;
    ensure!(
        resource_kind == expected,
        "proxy action {action} requires resource kind {expected}, not {resource_kind}"
    );
    Ok(())
}

fn plan_title(action: &str) -> Result<&'static str> {
    Ok(match action {
        "proxy.rule.create" => "Create reverse-proxy rule",
        "proxy.rule.update" => "Update reverse-proxy rule",
        "proxy.rule.delete" => "Delete reverse-proxy rule",
        "proxy.rule.toggle" => "Change reverse-proxy rule state",
        "proxy.nginx.start" => "Start nginx-proxy",
        "proxy.nginx.stop" => "Stop nginx-proxy",
        "proxy.nginx.restart" => "Restart nginx-proxy",
        "proxy.nginx.reload" => "Reload nginx-proxy",
        _ => bail!("unsupported proxy action"),
    })
}

fn risk_name(risk: RiskClass) -> &'static str {
    match risk {
        RiskClass::Read => "read",
        RiskClass::Mutate => "mutate",
        RiskClass::Destructive => "destructive",
        RiskClass::Irreversible => "irreversible",
    }
}

fn safe_text(text: &str) -> String {
    crate::api::mcp::redact::redact_patterns(text)
        .chars()
        .take(4 * 1024)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::contracts::ResourceRef;
    use std::sync::Mutex;

    struct FakeProvider {
        snapshot: Mutex<ProxySnapshot>,
        executions: Mutex<Vec<String>>,
        fail_execution: bool,
    }

    #[async_trait]
    impl ProxyProvider for FakeProvider {
        async fn snapshot(
            &self,
            _proxy_id: Option<&str>,
            _secret_ids: &[String],
        ) -> Result<ProxySnapshot> {
            Ok(self.snapshot.lock().unwrap().clone())
        }

        async fn execute(
            &self,
            mutation: ProxyMutation,
            _correlation_id: &str,
        ) -> Result<ProxyMutationResult> {
            let name = match mutation {
                ProxyMutation::Create { input, .. } => {
                    assert_eq!(
                        input.basic_auth_secret_id.as_deref(),
                        Some("8af2045f-01f5-4765-b158-54ca917d59e3")
                    );
                    "create"
                }
                ProxyMutation::Update { .. } => "update",
                ProxyMutation::Delete { .. } => "delete",
                ProxyMutation::SetEnabled { enabled, .. } => {
                    if enabled {
                        "enable"
                    } else {
                        "disable"
                    }
                }
                ProxyMutation::Nginx(NginxAction::Start) => "nginx-start",
                ProxyMutation::Nginx(NginxAction::Stop) => "nginx-stop",
                ProxyMutation::Nginx(NginxAction::Restart) => "nginx-restart",
                ProxyMutation::Nginx(NginxAction::Reload) => "nginx-reload",
            };
            self.executions.lock().unwrap().push(name.into());
            if self.fail_execution {
                bail!("provider timeout")
            }
            Ok(ProxyMutationResult {
                message: "updated".into(),
                proxy_id: Some("proxy-id".into()),
                resource_id: Some("resource-id".into()),
            })
        }

        async fn contains_known_secret(&self, _candidates: &[String]) -> Result<bool> {
            Ok(false)
        }
    }

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&pool).await.unwrap();
        pool
    }

    fn service_resource() -> ResourceRef {
        ResourceRef {
            id: "proxy-service".into(),
            kind: "reverse_proxy_service".into(),
            display_name: "Local Reverse Proxy".into(),
            revision: 0,
        }
    }

    fn snapshot(active: bool) -> ProxySnapshot {
        ProxySnapshot {
            nginx: NginxSnapshot {
                container_id: Some("container".into()),
                state: if active { "running" } else { "exited" }.into(),
                active,
            },
            oidc_enabled: true,
            rules: vec![],
            secrets: vec![SecretReferenceSnapshot {
                id: "8af2045f-01f5-4765-b158-54ca917d59e3".into(),
                version: 3,
            }],
        }
    }

    #[tokio::test]
    async fn create_plan_persists_only_a_secret_reference_and_executes_typed_mutation() {
        let pool = pool().await;
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(snapshot(true)),
            executions: Mutex::new(vec![]),
            fail_execution: false,
        });
        let adapter = ProxyAdapter::with_provider(pool, provider.clone());
        let request = PlanRequest {
            action: "proxy.rule.create".into(),
            resource: service_resource(),
            input: serde_json::json!({
                "domain": "app.example.test",
                "upstream": "http://127.0.0.1:8080",
                "basic_auth_user": "operator",
                "basic_auth_secret_id": "8af2045f-01f5-4765-b158-54ca917d59e3"
            }),
        };
        let plan = adapter.plan(request.clone()).await.unwrap();
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains("known-secret-value"));
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job-id".into(),
                action: request.action,
                resource: request.resource,
                input: request.input,
                step: plan.steps[0].clone(),
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Succeeded { .. }));
        assert_eq!(provider.executions.lock().unwrap().as_slice(), &["create"]);
    }

    #[tokio::test]
    async fn secret_rotation_changes_the_external_fingerprint() {
        let pool = pool().await;
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(snapshot(true)),
            executions: Mutex::new(vec![]),
            fail_execution: false,
        });
        let adapter = ProxyAdapter::with_provider(pool, provider.clone());
        let request = PlanRequest {
            action: "proxy.rule.create".into(),
            resource: service_resource(),
            input: serde_json::json!({
                "domain": "app.example.test",
                "upstream": "http://127.0.0.1:8080",
                "basic_auth_user": "operator",
                "basic_auth_secret_id": "8af2045f-01f5-4765-b158-54ca917d59e3"
            }),
        };
        let before = adapter.external_fingerprint(&request).await.unwrap();
        provider
            .snapshot
            .lock()
            .unwrap()
            .secrets
            .first_mut()
            .unwrap()
            .version += 1;
        let after = adapter.external_fingerprint(&request).await.unwrap();
        assert_ne!(before, after);
    }

    #[tokio::test]
    async fn production_provider_resolves_encrypted_secret_only_at_execution_boundary() {
        let pool = pool().await;
        let key = Arc::new([7u8; 32]);
        let encrypted = crate::api::secrets::encrypt(&key, "known-secret-value").unwrap();
        let secret_id = "8af2045f-01f5-4765-b158-54ca917d59e3";
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) \
             VALUES (?, 'proxy-basic-auth', ?, 0, 0)",
        )
        .bind(secret_id)
        .bind(encrypted)
        .execute(&pool)
        .await
        .unwrap();
        let provider = LocalProxyProvider::new(pool, key);
        assert_eq!(
            provider.resolve_secret(Some(secret_id)).await.unwrap(),
            Some("known-secret-value".into())
        );
        assert!(provider
            .contains_known_secret(&["Bearer known-secret-value".into()])
            .await
            .unwrap());
        assert!(!provider
            .contains_known_secret(&["SAMEORIGIN".into()])
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn nginx_start_can_be_reconciled_but_reload_cannot() {
        let pool = pool().await;
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(snapshot(true)),
            executions: Mutex::new(vec![]),
            fail_execution: false,
        });
        let adapter = ProxyAdapter::with_provider(pool, provider);
        let request = |action: &str| StepRequest {
            job_id: "job".into(),
            action: action.into(),
            resource: service_resource(),
            input: serde_json::json!({}),
            step: PlannedStepV1 {
                kind: "execute".into(),
                name: action.into(),
                retry_class: "never".into(),
                recovery_class: "reconcile".into(),
            },
            attempt: 2,
            external_operation_id: None,
        };
        assert!(matches!(
            adapter
                .reconcile(request("proxy.nginx.start"))
                .await
                .unwrap(),
            ReconcileOutcome::Succeeded { .. }
        ));
        assert!(matches!(
            adapter
                .reconcile(request("proxy.nginx.reload"))
                .await
                .unwrap(),
            ReconcileOutcome::StillUncertain { .. }
        ));
    }

    #[test]
    fn invalid_or_credentialed_upstreams_fail_closed() {
        let metadata = serde_json::json!({
            "domain": "app.example.test",
            "upstream": "http://user:secret@example.test"
        });
        assert!(parse_input("proxy.rule.create", &metadata).is_err());
        let invalid_domain = serde_json::json!({
            "domain": "../../etc/passwd",
            "upstream": "http://example.test"
        });
        assert!(parse_input("proxy.rule.create", &invalid_domain).is_err());
        let query_secret = serde_json::json!({
            "domain": "app.example.test",
            "upstream": "http://example.test/api?token=known-secret-value"
        });
        assert!(parse_input("proxy.rule.create", &query_secret).is_err());
        let raw_authorization = serde_json::json!({
            "domain": "app.example.test",
            "upstream": "http://example.test",
            "custom_headers": [{"name": "Authorization", "value": "Bearer known-secret-value"}]
        });
        assert!(parse_input("proxy.rule.create", &raw_authorization).is_err());
        let safe_literal_header = serde_json::json!({
            "domain": "app.example.test",
            "upstream": "http://example.test",
            "custom_headers": [{
                "name": "X-Frame-Options",
                "value": "SAMEORIGIN"
            }]
        });
        assert!(parse_input("proxy.rule.create", &safe_literal_header).is_ok());
    }

    #[tokio::test]
    async fn provider_failure_becomes_uncertain() {
        let pool = pool().await;
        let provider = Arc::new(FakeProvider {
            snapshot: Mutex::new(snapshot(true)),
            executions: Mutex::new(vec![]),
            fail_execution: true,
        });
        let adapter = ProxyAdapter::with_provider(pool, provider);
        let outcome = adapter
            .execute_step(StepRequest {
                job_id: "job".into(),
                action: "proxy.nginx.stop".into(),
                resource: service_resource(),
                input: serde_json::json!({}),
                step: PlannedStepV1 {
                    kind: "execute".into(),
                    name: "proxy.nginx.stop".into(),
                    retry_class: "never".into(),
                    recovery_class: "reconcile".into(),
                },
                attempt: 1,
                external_operation_id: None,
            })
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Uncertain { .. }));
    }
}
