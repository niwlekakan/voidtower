use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata, CoreResponseType},
    reqwest::async_http_client,
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Admin-configured OIDC settings, loaded fresh on every login attempt so config
/// changes take effect immediately without a restart.
pub struct OidcSettings {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub scopes: Vec<String>,
    pub role_claim: String,
    pub role_map: HashMap<String, String>,
    pub default_role: String,
    pub auto_provision: bool,
}

#[allow(clippy::type_complexity)]
pub async fn load_settings(
    db: &SqlitePool,
    secrets_key: &[u8; 32],
) -> Result<Option<OidcSettings>> {
    let row: Option<(
        bool,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        bool,
    )> = sqlx::query_as(
        "SELECT enabled, issuer_url, client_id, client_secret_id, redirect_url, scopes, role_claim, role_map, default_role, auto_provision FROM oidc_config WHERE id = 'default'"
    )
    .fetch_optional(db)
    .await?;

    let Some((
        enabled,
        issuer_url,
        client_id,
        client_secret_id,
        redirect_url,
        scopes,
        role_claim,
        role_map,
        default_role,
        auto_provision,
    )) = row
    else {
        return Ok(None);
    };

    if !enabled {
        return Ok(None);
    }

    let (issuer_url, client_id, secret_id, redirect_url) =
        match (issuer_url, client_id, client_secret_id, redirect_url) {
            (Some(i), Some(c), Some(s), Some(r)) => (i, c, s, r),
            _ => return Ok(None),
        };

    let client_secret =
        match crate::api::secrets::resolve(db, secrets_key, &secret_id, "oidc_client").await {
            Ok(value) => value,
            Err(crate::api::secrets::ResolveError::Database) => {
                return Err(anyhow!("OIDC client secret unavailable"));
            }
            Err(_) => return Ok(None),
        };

    Ok(Some(OidcSettings {
        issuer_url,
        client_id,
        client_secret,
        redirect_url,
        scopes: scopes.split_whitespace().map(String::from).collect(),
        role_claim,
        role_map: serde_json::from_str(&role_map).unwrap_or_default(),
        default_role,
        auto_provision,
    }))
}

pub async fn build_client(settings: &OidcSettings) -> Result<CoreClient> {
    let issuer = IssuerUrl::new(settings.issuer_url.clone())
        .map_err(|e| anyhow!("invalid issuer_url: {e}"))?;
    let metadata = CoreProviderMetadata::discover_async(issuer, async_http_client)
        .await
        .map_err(|e| anyhow!("OIDC discovery failed: {e}"))?;

    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(settings.client_id.clone()),
        Some(ClientSecret::new(settings.client_secret.clone())),
    )
    .set_redirect_uri(
        RedirectUrl::new(settings.redirect_url.clone())
            .map_err(|e| anyhow!("invalid redirect_url: {e}"))?,
    );

    Ok(client)
}

/// Separately exposes the userinfo endpoint URL discovered for a client's issuer,
/// since `CoreClient` doesn't expose it directly after construction.
pub async fn discover_userinfo_endpoint(issuer_url: &str) -> Result<Option<String>> {
    let issuer =
        IssuerUrl::new(issuer_url.to_string()).map_err(|e| anyhow!("invalid issuer_url: {e}"))?;
    let metadata = CoreProviderMetadata::discover_async(issuer, async_http_client)
        .await
        .map_err(|e| anyhow!("OIDC discovery failed: {e}"))?;
    Ok(metadata
        .userinfo_endpoint()
        .as_ref()
        .map(|u| u.url().to_string()))
}

pub struct AuthFlowStart {
    pub authorize_url: String,
    pub flow_state: FlowState,
}

/// Opaque CSRF/nonce/PKCE state carried in a short-lived cookie across the redirect
/// to Authentik and back, instead of server-side session storage.
#[derive(Serialize, Deserialize)]
pub struct FlowState {
    pub csrf_state: String,
    pub nonce: String,
    pub pkce_verifier: String,
}

impl FlowState {
    pub fn encode(&self) -> String {
        let json = serde_json::to_string(self).unwrap_or_default();
        base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, json)
    }

    pub fn decode(s: &str) -> Result<Self> {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, s)
            .map_err(|e| anyhow!("invalid flow state encoding: {e}"))?;
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("invalid flow state contents: {e}"))
    }
}

pub fn start_authorization(client: &CoreClient, scopes: &[String]) -> AuthFlowStart {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut req = client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .set_pkce_challenge(pkce_challenge);

    for scope in scopes {
        if scope != "openid" {
            req = req.add_scope(Scope::new(scope.clone()));
        }
    }

    let (url, csrf_state, nonce) = req.url();

    AuthFlowStart {
        authorize_url: url.to_string(),
        flow_state: FlowState {
            csrf_state: csrf_state.secret().clone(),
            nonce: nonce.secret().clone(),
            pkce_verifier: pkce_verifier.secret().clone(),
        },
    }
}

pub struct OidcIdentity {
    pub subject: String,
    pub email: Option<String>,
    pub preferred_username: Option<String>,
    pub access_token: String,
}

pub async fn exchange_and_verify(
    client: &CoreClient,
    code: String,
    pkce_verifier: String,
    expected_nonce: &str,
) -> Result<OidcIdentity> {
    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(PkceCodeVerifier::new(pkce_verifier))
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow!("token exchange failed: {e}"))?;

    let id_token = token_response
        .extra_fields()
        .id_token()
        .ok_or_else(|| anyhow!("provider did not return an ID token"))?;

    let nonce = Nonce::new(expected_nonce.to_string());
    let verifier = client.id_token_verifier();
    let claims = id_token
        .claims(&verifier, &nonce)
        .map_err(|e| anyhow!("ID token verification failed: {e}"))?;

    Ok(OidcIdentity {
        subject: claims.subject().to_string(),
        email: claims.email().map(|e| e.to_string()),
        preferred_username: claims.preferred_username().map(|u| u.to_string()),
        access_token: token_response.access_token().secret().clone(),
    })
}

/// Authentik (and most OIDC providers) expose group membership via the userinfo
/// endpoint rather than the ID token, so role mapping fetches it directly.
pub async fn fetch_role_claim_values(
    userinfo_url: &str,
    access_token: &str,
    claim: &str,
) -> Vec<String> {
    let client = reqwest::Client::new();
    let Ok(resp) = client
        .get(userinfo_url)
        .bearer_auth(access_token)
        .send()
        .await
    else {
        return Vec::new();
    };
    let Ok(body) = resp.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    match body.get(claim) {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

/// Maps a set of Authentik group names to a VoidTower role using the admin-configured
/// role_map, preferring the most-privileged matching role when a user belongs to
/// multiple mapped groups.
pub fn map_role(
    groups: &[String],
    role_map: &HashMap<String, String>,
    default_role: &str,
) -> String {
    const PRIORITY: [&str; 4] = ["owner", "admin", "operator", "viewer"];
    let matched: HashSet<&str> = groups
        .iter()
        .filter_map(|g| role_map.get(g).map(|r| r.as_str()))
        .collect();
    PRIORITY
        .iter()
        .copied()
        .find(|role| matched.contains(*role))
        .map(|role| role.to_string())
        .unwrap_or_else(|| default_role.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_db() -> SqlitePool {
        let path =
            std::env::temp_dir().join(format!("voidtower-oidc-secret-{}.db", uuid::Uuid::new_v4()));
        crate::db::init_pool(&path).await.unwrap()
    }

    async fn insert_oidc_config(db: &SqlitePool, secret_id: &str) {
        sqlx::query(
            "INSERT INTO oidc_config (id, enabled, issuer_url, client_id, client_secret_id, redirect_url) \
             VALUES ('default', 1, 'https://issuer.example.test', 'client', ?, 'https://tower.example.test/callback')",
        )
        .bind(secret_id)
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn oidc_settings_resolve_through_secret_manager_and_record_last_use() {
        let db = setup_db().await;
        let key = [7_u8; 32];
        let secret_id = uuid::Uuid::new_v4().to_string();
        let encrypted = crate::api::secrets::encrypt(&key, "oidc-client-secret").unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, created_at, updated_at) VALUES (?, 'oidc-client', ?, 1, 1)",
        )
        .bind(&secret_id)
        .bind(encrypted)
        .execute(&db)
        .await
        .unwrap();
        insert_oidc_config(&db, &secret_id).await;

        let settings = load_settings(&db, &key).await.unwrap().unwrap();

        assert_eq!(settings.client_secret, "oidc-client-secret");
        let last_used_at: Option<i64> =
            sqlx::query_scalar("SELECT last_used_at FROM secrets WHERE id = ?")
                .bind(&secret_id)
                .fetch_one(&db)
                .await
                .unwrap();
        assert!(last_used_at.is_some());
        db.close().await;
    }

    #[tokio::test]
    async fn disabled_oidc_secret_fails_closed_without_returning_client_secret() {
        let db = setup_db().await;
        let key = [7_u8; 32];
        let secret_id = uuid::Uuid::new_v4().to_string();
        let encrypted = crate::api::secrets::encrypt(&key, "disabled-oidc-secret").unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, value_enc, disabled, created_at, updated_at) VALUES (?, 'disabled-oidc-client', ?, 1, 1, 1)",
        )
        .bind(&secret_id)
        .bind(encrypted)
        .execute(&db)
        .await
        .unwrap();
        insert_oidc_config(&db, &secret_id).await;

        assert!(load_settings(&db, &key).await.unwrap().is_none());
        db.close().await;
    }
}
