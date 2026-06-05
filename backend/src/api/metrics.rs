use crate::{
    auth,
    error::{AppError, Result},
    monitoring::MetricsSnapshot,
    AppState,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

pub async fn get_current(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<MetricsSnapshot>> {
    require_session(&state, &jar).await?;
    let snapshot = state.latest_metrics.read().await.clone();
    match snapshot {
        Some(s) => Ok(Json(s)),
        None => Err(AppError::FeatureUnavailable("Metrics not yet collected".to_string())),
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Response> {
    require_session(&state, &jar).await?;
    let rx = state.metrics_tx.subscribe();
    Ok(ws.on_upgrade(move |socket| handle_ws(socket, rx)))
}

async fn handle_ws(socket: WebSocket, mut rx: broadcast::Receiver<MetricsSnapshot>) {
    let (mut sink, mut stream) = socket.split();

    loop {
        tokio::select! {
            Ok(snapshot) = rx.recv() => {
                let json = match serde_json::to_string(&snapshot) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if sink.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            msg = stream.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(d))) => {
                        let _ = sink.send(Message::Pong(d)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn require_session(state: &AppState, jar: &CookieJar) -> Result<()> {
    let session_id = jar
        .get("vt_session")
        .map(|c| c.value().to_string())
        .ok_or(AppError::Unauthorized)?;
    auth::validate_session(&state.db, &session_id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    Ok(())
}
