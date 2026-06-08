use sqlx::SqlitePool;

#[derive(Debug, sqlx::FromRow, serde::Serialize, serde::Deserialize, Clone)]
pub struct PolicyRule {
    pub id: String,
    pub name: String,
    /// "api_token" | "automation" | "*"
    pub actor_type: String,
    /// "restart" | "stop" | "remove" | "deploy" | "run" | "*"
    pub action: String,
    /// "container" | "service" | "app" | "backup" | "vm" | "*"
    pub resource_type: String,
    /// If set, rule only applies when the resource has this tag name
    pub resource_tag: Option<String>,
    /// "allow" | "deny"
    pub effect: String,
    pub priority: i64,
    pub enabled: bool,
    pub created_at: i64,
}

#[derive(Debug)]
pub enum PolicyVerdict {
    Allow,
    Deny(String),
}

/// Check policy rules for an automated actor performing an action on a resource.
/// Returns `Allow` if no deny rule matches (default-allow after scope check).
/// Returns `Deny(reason)` if a matching deny rule fires.
///
/// `actor_type`: "api_token" | "automation"
/// `action`:     "restart" | "stop" | "remove" | "deploy" | "run" | etc.
/// `resource_type`: "container" | "service" | "app" | "backup" | "vm"
/// `resource_id`: used to look up the resource's tags
pub async fn check(
    db: &SqlitePool,
    actor_type: &str,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) -> PolicyVerdict {
    // Fetch tag names for this resource
    let tag_names: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM tags t
         JOIN resource_tags rt ON rt.tag_id = t.id
         WHERE rt.resource_type = ? AND rt.resource_id = ?",
    )
    .bind(resource_type)
    .bind(resource_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    // Fetch all enabled rules in priority order (lowest number = highest priority)
    let rules: Vec<PolicyRule> = sqlx::query_as(
        "SELECT id, name, actor_type, action, resource_type, resource_tag,
                effect, priority, enabled, created_at
         FROM policy_rules WHERE enabled = 1 ORDER BY priority ASC",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    for rule in &rules {
        if !matches_actor(&rule.actor_type, actor_type) { continue; }
        if !matches_field(&rule.action, action)          { continue; }
        if !matches_field(&rule.resource_type, resource_type) { continue; }
        if let Some(required_tag) = &rule.resource_tag {
            if !tag_names.iter().any(|t| t == required_tag) { continue; }
        }

        return if rule.effect == "deny" {
            PolicyVerdict::Deny(format!("Blocked by policy rule \"{}\"", rule.name))
        } else {
            PolicyVerdict::Allow
        };
    }

    PolicyVerdict::Allow
}

fn matches_actor(rule_actor: &str, request_actor: &str) -> bool {
    rule_actor == "*" || rule_actor == request_actor
}

fn matches_field(rule_val: &str, request_val: &str) -> bool {
    rule_val == "*" || rule_val == request_val
}

/// Marker extension injected by bearer_auth middleware when a request arrives
/// via API token rather than a browser session cookie.
#[derive(Clone)]
pub struct ApiTokenActor;

/// Axum extractor that reads `true` when the request came via API token.
/// Returns `false` (never fails) for normal session-cookie requests.
pub struct MaybeTokenActor(pub bool);

#[async_trait::async_trait]
impl<S> axum::extract::FromRequestParts<S> for MaybeTokenActor
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(MaybeTokenActor(parts.extensions.get::<ApiTokenActor>().is_some()))
    }
}
