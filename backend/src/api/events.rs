use crate::{
    auth,
    error::{AppError, Result},
    operations::events::{self, sse_frame_fits, EventBounds},
    AppState,
};
use axum::{
    extract::{Extension, Query, State},
    http::{header, HeaderMap},
    response::sse::{Event, KeepAlive, Sse},
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, time::Duration};
use tokio_stream::wrappers::ReceiverStream;

const DELIVERY_BATCH_SIZE: i64 = 100;
const DELIVERY_CHANNEL_SIZE: usize = 64;
const FOLLOW_INTERVAL: Duration = Duration::from_millis(500);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

#[derive(Deserialize)]
pub struct StreamQuery {
    pub after: Option<String>,
    pub token: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    #[serde(default)]
    pub after: i64,
    #[serde(default = "default_history_limit")]
    pub limit: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedCursor {
    value: i64,
    supplied: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorGapReason {
    BehindRetention,
    FutureCursor,
    Discontinuity,
}

impl CursorGapReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::BehindRetention => "behind_retention",
            Self::FutureCursor => "future_cursor",
            Self::Discontinuity => "discontinuity",
        }
    }
}

#[derive(Serialize)]
struct ReadyPayload {
    cursor: i64,
    high_water: i64,
}

#[derive(Serialize)]
struct GapPayload {
    reason: &'static str,
    requested_after: i64,
    earliest_available: Option<i64>,
    latest_available: i64,
}

fn default_history_limit() -> i64 {
    100
}

pub async fn history_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<HistoryQuery>,
) -> Result<axum::Json<serde_json::Value>> {
    let session_id = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
        .ok_or(AppError::Unauthorized)?;
    let user = auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    super::role_guard::require_operator(&user)?;
    let events = events::list_after(&state.db, query.after, query.limit)
        .await
        .map_err(AppError::Internal)?;
    let next_cursor = events
        .last()
        .map(|event| event.sequence)
        .unwrap_or(query.after);
    Ok(axum::Json(serde_json::json!({
        "events": events,
        "next_cursor": next_cursor,
    })))
}

/// Cursor-resumable durable event delivery. `GET /api/integrations/events` is mounted to this
/// same handler so both public durable URLs share authentication, cursor, and frame behavior.
pub async fn stream_handler(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<StreamQuery>,
    headers: HeaderMap,
    token_context: Option<Extension<super::bearer_auth::AuthenticatedApiToken>>,
) -> Result<Sse<impl futures_util::Stream<Item = std::result::Result<Event, Infallible>>>> {
    authorize_stream(&state, &jar, &headers, token_context.is_some()).await?;
    if query.token.is_some() {
        return Err(AppError::BadRequest(
            "query-string API tokens are not supported; use Authorization: Bearer".into(),
        ));
    }

    let bounds = events::bounds(&state.db)
        .await
        .map_err(AppError::Internal)?;
    let last_event_id = headers
        .get("last-event-id")
        .map(|value| {
            value.to_str().map_err(|_| {
                AppError::BadRequest("Last-Event-ID must be a non-negative event sequence".into())
            })
        })
        .transpose()?;
    let cursor = select_cursor(query.after.as_deref(), last_event_id, bounds.latest)?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Event>(DELIVERY_CHANNEL_SIZE);
    let db = state.db.clone();
    tokio::spawn(async move {
        deliver(db, tx, cursor, bounds).await;
    });

    let stream = ReceiverStream::new(rx).map(Ok);
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(KEEPALIVE_INTERVAL)
            .text("keepalive"),
    ))
}

async fn authorize_stream(
    state: &AppState,
    jar: &CookieJar,
    headers: &HeaderMap,
    bearer_context: bool,
) -> Result<()> {
    let header_token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty());

    if bearer_context {
        let raw = header_token.ok_or(AppError::Unauthorized)?;
        auth::validate_api_token(&state.db, raw, "alerts:read")
            .await
            .map_err(|_| AppError::Unauthorized)?;
        if integration_emergency_disabled(state).await? {
            return Err(AppError::FeatureUnavailable(
                "AI access is emergency-disabled".into(),
            ));
        }
        return Ok(());
    }

    if let Some(session_id) = jar
        .get("vt_session")
        .map(|cookie| cookie.value().to_owned())
    {
        let user = auth::validate_session(&state.db, &session_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or(AppError::Unauthorized)?;
        return super::role_guard::require_operator(&user);
    }

    if let Some(raw) = header_token {
        auth::validate_api_token(&state.db, raw, "alerts:read")
            .await
            .map_err(|_| AppError::Unauthorized)?;
        if integration_emergency_disabled(state).await? {
            return Err(AppError::FeatureUnavailable(
                "AI access is emergency-disabled".into(),
            ));
        }
        return Ok(());
    }

    Err(AppError::Unauthorized)
}

async fn integration_emergency_disabled(state: &AppState) -> Result<bool> {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM settings WHERE key = 'odysseus.emergency_disabled'",
    )
    .fetch_optional(&state.db)
    .await
    .map(|value| value.as_deref() == Some("true"))
    .map_err(|error| AppError::Internal(error.into()))
}

fn parse_cursor(value: &str, source: &str) -> Result<i64> {
    let parsed = value.parse::<i64>().map_err(|_| {
        AppError::BadRequest(format!("{source} must be a non-negative event sequence"))
    })?;
    if parsed < 0 {
        return Err(AppError::BadRequest(format!(
            "{source} must be a non-negative event sequence"
        )));
    }
    Ok(parsed)
}

fn select_cursor(
    after: Option<&str>,
    last_event_id: Option<&str>,
    high_water: i64,
) -> Result<SelectedCursor> {
    let after = after
        .map(|value| parse_cursor(value, "after"))
        .transpose()?;
    let last_event_id = last_event_id
        .map(|value| parse_cursor(value, "Last-Event-ID"))
        .transpose()?;
    let supplied = after.is_some() || last_event_id.is_some();
    Ok(SelectedCursor {
        value: after
            .into_iter()
            .chain(last_event_id)
            .max()
            .unwrap_or(high_water),
        supplied,
    })
}

fn initial_gap(cursor: SelectedCursor, bounds: EventBounds) -> Option<CursorGapReason> {
    if !cursor.supplied {
        return None;
    }
    match bounds.earliest {
        None if cursor.value > 0 => Some(CursorGapReason::FutureCursor),
        None => None,
        Some(earliest) if cursor.value < earliest.saturating_sub(1) => {
            Some(CursorGapReason::BehindRetention)
        }
        Some(_) if cursor.value > bounds.latest => Some(CursorGapReason::FutureCursor),
        Some(_) => None,
    }
}

fn ready_event(cursor: i64, high_water: i64) -> Option<Event> {
    let data = serde_json::to_string(&ReadyPayload { cursor, high_water }).ok()?;
    sse_frame_fits("stream.ready", data.len(), None)
        .then(|| Event::default().event("stream.ready").data(data))
}

fn gap_event(reason: CursorGapReason, cursor: i64, bounds: EventBounds) -> Option<Event> {
    let data = serde_json::to_string(&GapPayload {
        reason: reason.as_str(),
        requested_after: cursor,
        earliest_available: bounds.earliest,
        latest_available: bounds.latest,
    })
    .ok()?;
    sse_frame_fits("stream.gap", data.len(), None)
        .then(|| Event::default().event("stream.gap").data(data))
}

fn durable_event(envelope: &crate::operations::contracts::EventEnvelopeV1) -> Option<Event> {
    let data = serde_json::to_string(envelope).ok()?;
    if !sse_frame_fits("durable_event", data.len(), Some(envelope.sequence)) {
        return None;
    }
    Some({
        Event::default()
            .id(envelope.sequence.to_string())
            .event("durable_event")
            .data(data)
    })
}

async fn deliver(
    db: sqlx::SqlitePool,
    tx: tokio::sync::mpsc::Sender<Event>,
    selected: SelectedCursor,
    initial_bounds: EventBounds,
) {
    if let Some(reason) = initial_gap(selected, initial_bounds) {
        if let Some(event) = gap_event(reason, selected.value, initial_bounds) {
            let _ = tx.send(event).await;
        }
        return;
    }

    let Some(ready) = ready_event(selected.value, initial_bounds.latest) else {
        return;
    };
    if tx.send(ready).await.is_err() {
        return;
    }

    let mut cursor = selected.value;
    loop {
        let batch = match events::list_after(&db, cursor, DELIVERY_BATCH_SIZE).await {
            Ok(batch) => batch,
            Err(error) => {
                tracing::warn!(error = %error, cursor, "durable SSE event read failed");
                return;
            }
        };
        let batch_was_full = batch.len() == DELIVERY_BATCH_SIZE as usize;

        for envelope in batch {
            if envelope.sequence != cursor.saturating_add(1) {
                let bounds = events::bounds(&db).await.unwrap_or(EventBounds {
                    earliest: None,
                    latest: cursor,
                });
                if let Some(event) = gap_event(CursorGapReason::Discontinuity, cursor, bounds) {
                    let _ = tx.send(event).await;
                }
                return;
            }
            let sequence = envelope.sequence;
            let Some(event) = durable_event(&envelope) else {
                let bounds = events::bounds(&db).await.unwrap_or(EventBounds {
                    earliest: None,
                    latest: cursor,
                });
                if let Some(event) = gap_event(CursorGapReason::Discontinuity, cursor, bounds) {
                    let _ = tx.send(event).await;
                }
                return;
            };
            if tx.send(event).await.is_err() {
                return;
            }
            cursor = sequence;
        }

        if !batch_was_full {
            tokio::select! {
                _ = tx.closed() => return,
                _ = tokio::time::sleep(FOLLOW_INTERVAL) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::events::PendingEvent;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn cursor_selection_is_live_by_default_and_never_rewinds() {
        assert_eq!(
            select_cursor(None, None, 42).unwrap(),
            SelectedCursor {
                value: 42,
                supplied: false,
            }
        );
        assert_eq!(
            select_cursor(Some("7"), Some("11"), 42).unwrap(),
            SelectedCursor {
                value: 11,
                supplied: true,
            }
        );
        assert_eq!(select_cursor(Some("15"), Some("11"), 42).unwrap().value, 15);
    }

    #[test]
    fn invalid_cursor_inputs_fail_closed() {
        for value in ["", "-1", "not-a-number", "999999999999999999999999"] {
            assert!(select_cursor(Some(value), None, 0).is_err(), "{value:?}");
            assert!(select_cursor(None, Some(value), 0).is_err(), "{value:?}");
        }
    }

    #[test]
    fn initial_gap_classifies_retention_and_future_cursors() {
        let bounds = EventBounds {
            earliest: Some(5),
            latest: 10,
        };
        assert_eq!(
            initial_gap(
                SelectedCursor {
                    value: 3,
                    supplied: true,
                },
                bounds,
            ),
            Some(CursorGapReason::BehindRetention)
        );
        assert_eq!(
            initial_gap(
                SelectedCursor {
                    value: 4,
                    supplied: true,
                },
                bounds,
            ),
            None
        );
        assert_eq!(
            initial_gap(
                SelectedCursor {
                    value: 11,
                    supplied: true,
                },
                bounds,
            ),
            Some(CursorGapReason::FutureCursor)
        );
        assert_eq!(
            initial_gap(
                SelectedCursor {
                    value: 99,
                    supplied: false,
                },
                bounds,
            ),
            None
        );
    }

    #[tokio::test]
    async fn delivery_crosses_bounded_batches_and_stops_when_the_receiver_closes() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::db::run_migrations(&db).await.unwrap();
        let mut transaction = db.begin().await.unwrap();
        for number in 1..=101 {
            events::append(
                &mut transaction,
                PendingEvent {
                    event_type: "test.delivery.v1".into(),
                    actor: None,
                    resource_id: None,
                    job_id: None,
                    approval_id: None,
                    correlation_id: "delivery-test".into(),
                    causation_id: None,
                    payload: serde_json::json!({"number": number}),
                },
            )
            .await
            .unwrap();
        }
        transaction.commit().await.unwrap();
        let bounds = events::bounds(&db).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let task = tokio::spawn(deliver(
            db,
            tx,
            SelectedCursor {
                value: 0,
                supplied: true,
            },
            bounds,
        ));

        for _ in 0..102 {
            let _event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("delivery must not stall")
                .expect("ready plus 101 events must be delivered");
        }
        drop(rx);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .expect("receiver closure must stop the producer")
            .unwrap();
    }
}
