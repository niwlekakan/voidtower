use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use crate::{
    auth,
    error::{AppError, Result},
    networking::firewall::{self as firewall_provider, FirewallSnapshot},
    operations::canonical_json,
    AppState,
};

use super::operation_adoption::{self, CompatibilityResource, CompatibilityResult};

#[derive(Serialize, Clone)]
pub struct FirewallRule {
    pub num: u32,
    pub to: String,
    pub action: String,
    pub from: String,
    pub ipv6: bool,
    pub comment: Option<String>,
}

#[derive(Serialize)]
pub struct FirewallStatus {
    pub backend: String,
    pub enabled: bool,
    pub rules: Vec<FirewallRule>,
    pub logging: Option<String>,
    pub error: Option<String>,
}

fn parse_ufw_status(output: &str) -> (bool, Vec<FirewallRule>, Option<String>) {
    let mut enabled = false;
    let mut rules = Vec::new();
    let mut logging = None;

    for line in output.lines() {
        let l = line.trim();
        if l.starts_with("Status: active") {
            enabled = true;
        }
        if l.starts_with("Status: inactive") {
            enabled = false;
        }
        if l.starts_with("Logging:") {
            logging = l.split_once(':').map(|x| x.1).map(|s| s.trim().to_string());
        }

        // Parse numbered rules: "[ 1] 22/tcp    ALLOW IN    Anywhere"
        if l.starts_with('[') {
            if let Some(end) = l.find(']') {
                let num_str = l[1..end].trim();
                let Ok(num) = num_str.parse::<u32>() else {
                    continue;
                };
                let rest = l[end + 1..].trim();

                // Detect IPv6 entries (contain "(v6)")
                let ipv6 = rest.contains("(v6)");
                let rest_clean = rest.replace("(v6)", "").replace("  ", " ");

                // Split on double-space or column-aligned whitespace
                let parts: Vec<&str> = rest_clean.split_whitespace().collect();
                if parts.len() < 3 {
                    continue;
                }

                // Format: TO  ACTION  FROM  or  TO  ACTION  IN/OUT  FROM
                // We need to find the action word (ALLOW/DENY/LIMIT/REJECT)
                let mut to = String::new();
                let mut action = String::new();
                let mut from = String::new();

                for (i, &p) in parts.iter().enumerate() {
                    let pu = p.to_uppercase();
                    if pu == "ALLOW" || pu == "DENY" || pu == "LIMIT" || pu == "REJECT" {
                        to = parts[..i].join(" ");
                        // skip IN/OUT direction word if present
                        let next = i + 1;
                        let skip = if parts
                            .get(next)
                            .map(|s| s.eq_ignore_ascii_case("IN") || s.eq_ignore_ascii_case("OUT"))
                            .unwrap_or(false)
                        {
                            next + 1
                        } else {
                            next
                        };
                        let dir = if parts
                            .get(i + 1)
                            .map(|s| s.eq_ignore_ascii_case("OUT"))
                            .unwrap_or(false)
                        {
                            " OUT"
                        } else {
                            ""
                        };
                        action = format!("{}{}", p, dir);
                        from = parts[skip..].join(" ");
                        break;
                    }
                }

                if action.is_empty() {
                    continue;
                }

                rules.push(FirewallRule {
                    num,
                    to,
                    action,
                    from,
                    ipv6,
                    comment: None,
                });
            }
        }
    }

    (enabled, rules, logging)
}

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn ufw_available() -> bool {
    firewall_provider::available()
}

pub async fn get_status(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<FirewallStatus>> {
    require_user(&state, &jar).await?;

    if !ufw_available() {
        return Ok(Json(FirewallStatus {
            backend: "none".into(),
            enabled: false,
            rules: vec![],
            logging: None,
            error: Some("No supported firewall backend found (ufw not installed)".into()),
        }));
    }

    let text = match firewall_provider::status_text().await {
        Ok(text) => text,
        Err(error) => {
            let message = error.to_string();
            let permission_error = ["permission", "sudo", "root"]
                .iter()
                .any(|needle| message.to_ascii_lowercase().contains(needle));
            return Ok(Json(FirewallStatus {
                backend: "ufw".into(),
                enabled: false,
                rules: vec![],
                logging: None,
                error: Some(if permission_error {
                    "VoidTower needs elevated privileges to read firewall rules (run as root or add sudo permission for ufw)".into()
                } else {
                    message
                }),
            }));
        }
    };

    let (enabled, rules, logging) = parse_ufw_status(&text);

    Ok(Json(FirewallStatus {
        backend: "ufw".into(),
        enabled,
        rules,
        logging,
        error: None,
    }))
}

#[derive(Deserialize)]
pub struct AddRuleRequest {
    pub action: String, // "allow" | "deny" | "limit"
    pub port: Option<String>,
    pub proto: Option<String>,     // "tcp" | "udp" | "any"
    pub from: Option<String>,      // IP or "Anywhere"
    pub direction: Option<String>, // "in" | "out"
    pub comment: Option<String>,
    #[serde(default)]
    pub dry_run: bool,
}

pub async fn add_rule(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<AddRuleRequest>,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "firewall.rule.add";
    let credential = super::actions::credential(&state, &jar, None).await?;
    operation_adoption::authorize(&credential, ACTION)?;
    ensure_ufw_available()?;
    let resource = resolve_firewall(&state, &credential, ACTION).await?;
    let input = serde_json::json!({
        "action": req.action,
        "port": req.port,
        "proto": req.proto,
        "from": req.from,
        "direction": req.direction,
        "comment": req.comment,
    });

    if req.dry_run {
        return compatibility_plan(&state, &credential, &resource.id, ACTION, input).await;
    }
    operation_adoption::submit(&state, &credential, &resource.id, ACTION, input, &headers).await
}

#[derive(Deserialize)]
pub struct DeleteRuleRequest {
    pub num: u32,
}

pub async fn delete_rule(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<DeleteRuleRequest>,
) -> CompatibilityResult<Response> {
    const ACTION: &str = "firewall.rule.delete";
    let credential = super::actions::credential(&state, &jar, None).await?;
    operation_adoption::authorize(&credential, ACTION)?;
    ensure_ufw_available()?;
    let snapshot = firewall_provider::snapshot()
        .await
        .map_err(|error| AppError::FeatureUnavailable(error.to_string()))?;
    let (display_name, alias) = observed_rule(&snapshot, req.num)?;
    let resource = operation_adoption::observe_available(
        &state,
        &credential,
        CompatibilityResource {
            kind: "firewall_rule",
            display_name,
            node_id: None,
            provider: Some("ufw"),
            namespace: "ufw.rule",
            scope_key: "local",
            alias: &alias,
        },
        &[ACTION],
    )
    .await?;

    operation_adoption::submit(
        &state,
        &credential,
        &resource.id,
        ACTION,
        serde_json::json!({"num": req.num}),
        &headers,
    )
    .await
}

#[derive(Deserialize)]
pub struct FirewallActionRequest {
    pub action: String, // "enable" | "disable" | "reload" | "reset"
}

pub async fn firewall_action(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
    Json(req): Json<FirewallActionRequest>,
) -> CompatibilityResult<Response> {
    let credential = super::actions::credential(&state, &jar, None).await?;
    operation_adoption::authorize(&credential, "firewall.enable")?;
    let action = match req.action.as_str() {
        "enable" => "firewall.enable",
        "disable" => "firewall.disable",
        "reload" => "firewall.reload",
        "reset" => "firewall.reset",
        _ => return Err(AppError::BadRequest("unknown action".into()).into()),
    };
    operation_adoption::authorize(&credential, action)?;
    ensure_ufw_available()?;
    let resource = resolve_firewall(&state, &credential, action).await?;
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

fn ensure_ufw_available() -> Result<()> {
    ufw_available()
        .then_some(())
        .ok_or_else(|| AppError::FeatureUnavailable("UFW is not available".into()))
}

async fn resolve_firewall(
    state: &AppState,
    credential: &crate::operations::invocation::CredentialContext,
    action: &str,
) -> CompatibilityResult<crate::operations::contracts::ResourceRef> {
    operation_adoption::resolve_available(
        state,
        credential,
        "firewall",
        "voidtower.singleton",
        "local",
        "firewall",
        &[action],
    )
    .await
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

fn observed_rule(snapshot: &FirewallSnapshot, number: u32) -> Result<(&str, String)> {
    let index = usize::try_from(number)
        .ok()
        .and_then(|number| number.checked_sub(1))
        .ok_or_else(|| AppError::BadRequest("firewall rule number must be positive".into()))?;
    let rule = snapshot
        .rules
        .get(index)
        .ok_or_else(|| AppError::BadRequest("firewall rule does not exist".into()))?;
    let occurrence = snapshot.rules[..index]
        .iter()
        .filter(|candidate| *candidate == rule)
        .count();
    let alias = canonical_json::digest(&(rule, occurrence)).map_err(AppError::Internal)?;
    Ok((rule, alias))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_identity_comes_from_normalized_provider_state() {
        let snapshot = FirewallSnapshot {
            backend: "ufw",
            enabled: true,
            rules: vec![
                "22/tcp ALLOW IN Anywhere".into(),
                "80/tcp DENY IN Anywhere".into(),
            ],
        };
        let (display_name, alias) = observed_rule(&snapshot, 2).unwrap();
        assert_eq!(display_name, "80/tcp DENY IN Anywhere");
        assert_eq!(alias, canonical_json::digest(&(display_name, 0)).unwrap());
        assert!(observed_rule(&snapshot, 0).is_err());
        assert!(observed_rule(&snapshot, 3).is_err());
    }
}
