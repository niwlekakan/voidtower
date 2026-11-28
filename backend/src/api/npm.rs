//! Read-only NPM (nginx-proxy-manager) HTTP surface.
//!
//! These handlers expose the hostname/certificate inventory and a reconciliation action
//! that copies matched hostnames into `deployed_apps`. They never write to NPM and never
//! expose NPM credentials; all credential handling stays in `networking::npm`.

use crate::{
    auth,
    error::{AppError, Result},
    networking::npm::{self, NpmSettings},
    AppState,
};
use axum::{extract::State, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::Serialize;
use time::OffsetDateTime;

async fn require_user(state: &AppState, jar: &CookieJar) -> Result<auth::User> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)
}

fn configured_client(state: &AppState) -> Result<npm::NpmClient> {
    let settings = NpmSettings::load(&state.config.config_dir)
        .map_err(AppError::Internal)?
        .ok_or_else(|| {
            AppError::FeatureUnavailable(
                "NPM is not configured; set VOIDTOWER_NPM_TOKEN or write a credential to \
                 <config-dir>/npm-token"
                    .into(),
            )
        })?;
    npm::NpmClient::new(settings).map_err(AppError::Internal)
}

#[derive(Debug, Serialize)]
pub struct NpmHostsResponse {
    pub available: bool,
    pub stale: bool,
    pub fetched_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub hosts: Vec<npm::NpmHostView>,
}

/// `GET /api/npm/hosts`: every NPM proxy host joined with per-hostname certificate expiry.
pub async fn hosts(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<NpmHostsResponse>> {
    let _user = require_user(&state, &jar).await?;
    let client = configured_client(&state)?;
    let fetch = npm::fetch_snapshot(&client)
        .await
        .map_err(|error| AppError::FeatureUnavailable(format!("NPM is unreachable: {error}")))?;

    let hosts = npm::join_hosts_with_certificates(
        &fetch.snapshot.hosts,
        &fetch.snapshot.certificates,
        OffsetDateTime::now_utc(),
    );

    Ok(Json(NpmHostsResponse {
        available: true,
        stale: fetch.stale,
        fetched_at: fetch.snapshot.fetched_at,
        warning: fetch.warning,
        hosts,
    }))
}

#[derive(Debug, Serialize)]
pub struct NpmCertificatesResponse {
    pub available: bool,
    pub stale: bool,
    pub fetched_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    pub certificates: Vec<npm::NpmCertificateView>,
}

/// `GET /api/npm/certificates`: certificate inventory with days remaining.
pub async fn certificates(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<NpmCertificatesResponse>> {
    let _user = require_user(&state, &jar).await?;
    let client = configured_client(&state)?;
    let fetch = npm::fetch_snapshot(&client)
        .await
        .map_err(|error| AppError::FeatureUnavailable(format!("NPM is unreachable: {error}")))?;

    let certificates =
        npm::certificate_views(&fetch.snapshot.certificates, OffsetDateTime::now_utc());

    Ok(Json(NpmCertificatesResponse {
        available: true,
        stale: fetch.stale,
        fetched_at: fetch.snapshot.fetched_at,
        warning: fetch.warning,
        certificates,
    }))
}

#[derive(Debug, Serialize)]
pub struct NpmSyncResponse {
    pub available: bool,
    pub fetched_at: i64,
    pub scanned_apps: usize,
    pub matched: usize,
    pub updated: usize,
    pub cleared: usize,
    pub unmatched: Vec<String>,
}

/// `deployed_apps` row this endpoint reconciles:
/// `(id, app_id, app_name, project_name, hostname, url)`.
type DeployedAppRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

/// `POST /api/npm/sync`: reconcile `deployed_apps.hostname`/`url` from NPM proxy hosts.
///
/// Reads are open to every authenticated role; this write is operator-and-above. A stale
/// snapshot is refused so a reconciliation never overwrites links from data that may no
/// longer exist in NPM.
pub async fn sync(State(state): State<AppState>, jar: CookieJar) -> Result<Json<NpmSyncResponse>> {
    let user = require_user(&state, &jar).await?;
    super::role_guard::require_operator(&user)?;

    let client = configured_client(&state)?;
    let fetch = npm::fetch_snapshot(&client)
        .await
        .map_err(|error| AppError::FeatureUnavailable(format!("NPM is unreachable: {error}")))?;
    if fetch.stale {
        return Err(AppError::FeatureUnavailable(
            "NPM is unreachable; refusing to reconcile links from a stale snapshot".into(),
        ));
    }

    let rows: Vec<DeployedAppRow> = sqlx::query_as(
        "SELECT id, app_id, app_name, project_name, hostname, url FROM deployed_apps",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    let apps: Vec<npm::DeployedAppLink> = rows
        .into_iter()
        .map(
            |(row_id, app_id, app_name, project_name, hostname, url)| npm::DeployedAppLink {
                row_id,
                app_id,
                app_name,
                project_name,
                hostname,
                url,
            },
        )
        .collect();

    let scanned_apps = apps.len();
    let plan = npm::plan_sync(&apps, &fetch.snapshot.hosts);

    if !plan.updates.is_empty() || !plan.clears.is_empty() {
        let mut transaction = state.db.begin().await.map_err(AppError::Database)?;

        for update in &plan.updates {
            sqlx::query("UPDATE deployed_apps SET hostname = ?, url = ? WHERE id = ?")
                .bind(&update.hostname)
                .bind(&update.url)
                .bind(&update.row_id)
                .execute(&mut *transaction)
                .await
                .map_err(AppError::Database)?;
        }

        for clear in &plan.clears {
            sqlx::query("UPDATE deployed_apps SET hostname = NULL, url = NULL WHERE id = ?")
                .bind(&clear.row_id)
                .execute(&mut *transaction)
                .await
                .map_err(AppError::Database)?;
        }

        transaction.commit().await.map_err(AppError::Database)?;
    }

    Ok(Json(NpmSyncResponse {
        available: true,
        fetched_at: fetch.snapshot.fetched_at,
        scanned_apps,
        matched: plan.matched,
        updated: plan.updates.len(),
        cleared: plan.clears.len(),
        unmatched: plan.unmatched,
    }))
}
