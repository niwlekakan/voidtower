use crate::{
    auth,
    error::{AppError, Result},
    services,
    AppState,
};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::StreamExt;
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};
use tokio_stream::wrappers::ReceiverStream;

#[derive(Deserialize)]
pub struct StreamQuery {
    pub token: Option<String>,
}

/// `GET /api/events/stream`
///
/// SSE endpoint that polls every 15 s and emits:
///
/// - `high_cpu`         — CPU > 90 %
/// - `high_memory`      — RAM > 90 %
/// - `disk_nearly_full` — any disk > 85 %
/// - `service_failed`   — a systemd service entered the `failed` state
///   (deduplicated per service until it recovers)
///
/// A `: keepalive` SSE comment is sent every 30 s automatically.
///
/// Auth: session cookie OR `Authorization: Bearer <token>` OR `?token=` param.
pub async fn stream_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<
    Sse<impl futures_util::Stream<Item = std::result::Result<Event, std::convert::Infallible>>>,
> {
    // -------------------------------------------------------------------------
    // Auth — session cookie, Bearer header, or ?token= query param
    // (EventSource in browsers cannot set headers, hence the query param path)
    // -------------------------------------------------------------------------
    let authed = if let Some(raw) = q.token {
        auth::validate_api_token(&state.db, &raw, "alerts:read")
            .await
            .is_ok()
    } else if let Some(hdr) = headers.get("Authorization") {
        let raw = hdr
            .to_str()
            .unwrap_or("")
            .trim_start_matches("Bearer ")
            .to_string();
        auth::validate_api_token(&state.db, &raw, "alerts:read")
            .await
            .is_ok()
    } else if let Some(sid) = jar.get("vt_session").map(|c| c.value().to_string()) {
        auth::validate_session(&state.db, &sid)
            .await
            .map(|u| u.is_some())
            .unwrap_or(false)
    } else {
        false
    };

    if !authed {
        return Err(AppError::Unauthorized);
    }

    // -------------------------------------------------------------------------
    // Spawn background polling task
    // -------------------------------------------------------------------------
    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(32);
    let latest_metrics = state.latest_metrics.clone();
    // Hold a broadcast receiver so the sender is never considered dead while
    // at least one stream is open.
    let _metrics_rx = state.metrics_tx.subscribe();

    tokio::spawn(async move {
        // Deduplication: track which services are currently in `failed` state
        // so we only emit `service_failed` once per incident, not every poll.
        let mut failed_seen: HashSet<String> = HashSet::new();

        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Consume the immediate first tick so we start after one full interval.
        interval.tick().await;

        loop {
            interval.tick().await;

            // -----------------------------------------------------------------
            // Metrics thresholds
            // -----------------------------------------------------------------
            if let Some(snap) = latest_metrics.read().await.clone() {
                // CPU > 90 %
                if snap.cpu_usage > 90.0 {
                    let data = serde_json::json!({ "cpu_percent": snap.cpu_usage }).to_string();
                    if tx.send(Event::default().event("high_cpu").data(data)).await.is_err() {
                        return;
                    }
                }

                // RAM > 90 %
                if snap.ram_total > 0 {
                    let mem_pct = (snap.ram_used as f64 / snap.ram_total as f64) * 100.0;
                    if mem_pct > 90.0 {
                        let data =
                            serde_json::json!({ "memory_percent": mem_pct }).to_string();
                        if tx.send(Event::default().event("high_memory").data(data)).await.is_err() {
                            return;
                        }
                    }
                }

                // Disk > 85 %
                for disk in &snap.disks {
                    if disk.total == 0 {
                        continue;
                    }
                    let disk_pct = (disk.used as f64 / disk.total as f64) * 100.0;
                    if disk_pct > 85.0 {
                        let data = serde_json::json!({
                            "path": disk.mount_point,
                            "percent": disk_pct,
                        })
                        .to_string();
                        if tx
                            .send(Event::default().event("disk_nearly_full").data(data))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }

            // -----------------------------------------------------------------
            // Service failures
            // -----------------------------------------------------------------
            if services::is_systemd_available() {
                let svcs = services::list_services().unwrap_or_default();

                let now_failed: HashSet<String> = svcs
                    .iter()
                    .filter(|s| s.active_state == "failed")
                    .map(|s| s.name.clone())
                    .collect();

                // Emit only for services newly entering `failed`.
                for name in &now_failed {
                    if !failed_seen.contains(name) {
                        let data = serde_json::json!({ "name": name }).to_string();
                        if tx
                            .send(Event::default().event("service_failed").data(data))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }

                // Forget services that have recovered so they can fire again.
                failed_seen.retain(|n| now_failed.contains(n));
                for name in now_failed {
                    failed_seen.insert(name);
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(Ok);
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keepalive"),
    ))
}
