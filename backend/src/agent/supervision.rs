use crate::{agent::{state::{AgentState, PendingSnapshotStore}, transport::AgentTransport}, collector};
use rand::Rng;
use std::{time::{Duration, SystemTime, UNIX_EPOCH}};
use uuid::Uuid;
use tokio::sync::watch;

#[derive(Clone)]
pub struct Cancellation {
    sender: watch::Sender<bool>,
}

impl Cancellation {
    pub fn new() -> Self {
        let (sender, _) = watch::channel(false);
        Self { sender }
    }

    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    async fn cancelled(&self) {
        let mut receiver = self.sender.subscribe();
        if *receiver.borrow() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow() {
                return;
            }
        }
    }
}

pub struct Backoff {
    initial: Duration,
    maximum: Duration,
    current: Duration,
}

impl Backoff {
    pub fn new(initial: Duration, maximum: Duration) -> Self {
        Self {
            initial,
            maximum,
            current: initial,
        }
    }

    pub fn reset(&mut self) {
        self.current = self.initial;
    }

    pub fn next_delay(&mut self) -> Duration {
        let capped = self.current.min(self.maximum);
        let lower_millis = (capped.as_millis() / 2).max(1);
        let upper_millis = capped.as_millis().max(1);
        let jittered_millis = rand::thread_rng().gen_range(lower_millis..=upper_millis);
        self.current = self.current.saturating_mul(2).min(self.maximum);
        Duration::from_millis(u64::try_from(jittered_millis).unwrap_or(u64::MAX))
    }
}

pub async fn run(state: AgentState, transport: AgentTransport, cancellation: Cancellation) {
    run_with_state_path(state, transport, cancellation, None).await;
}

pub async fn run_with_state_path(
    state: AgentState,
    transport: AgentTransport,
    cancellation: Cancellation,
    state_path: Option<std::path::PathBuf>,
) {
    if state.validate().is_err() {
        tracing::warn!(
            event_code = "agent_state_invalid",
            error_code = "invalid_agent_state"
        );
        return;
    }
    let heartbeat_state = state.clone();
    let heartbeat_transport = transport.clone();
    let heartbeat_cancel = cancellation.clone();
    let heartbeat = tokio::spawn(async move {
        run_heartbeat(heartbeat_state, heartbeat_transport, heartbeat_cancel).await;
    });
    let inventory_state = state;
    let inventory_transport = transport;
    let inventory_cancel = cancellation.clone();
    let inventory = tokio::spawn(async move {
        run_inventory(inventory_state, inventory_transport, inventory_cancel, state_path).await;
    });
    let _ = tokio::join!(heartbeat, inventory);
}

async fn run_heartbeat(state: AgentState, transport: AgentTransport, cancellation: Cancellation) {
    let interval = Duration::from_secs(state.schedule.heartbeat_interval_seconds);
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(state.schedule.max_backoff_seconds));
    loop {
        if cancellation.is_cancelled() { return; }
        let result = tokio::select! {
            result = transport.heartbeat(&state) => result,
            _ = cancellation.cancelled() => return,
        };
        let delay = match result {
            Ok(()) => { backoff.reset(); interval }
            Err(_) => {
                tracing::warn!(event_code = "agent_heartbeat_failed", node_id = %state.node_id);
                backoff.next_delay()
            }
        };
        if wait_or_cancel(&cancellation, delay).await { return; }
    }
}

async fn run_inventory(
    state: AgentState,
    transport: AgentTransport,
    cancellation: Cancellation,
    state_path: Option<std::path::PathBuf>,
) {
    let interval = Duration::from_secs(state.schedule.inventory_interval_seconds);
    let mut backoff = Backoff::new(
        Duration::from_secs(1),
        Duration::from_secs(state.schedule.max_backoff_seconds),
    );
    let host_key = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "linux-host".into());
    let pending_store = match state_path.as_deref() {
        Some(path) => match PendingSnapshotStore::for_state_path(path) {
            Ok(store) => match store.load() {
                Ok(snapshot) => Some((store, snapshot)),
                Err(error) => {
                    tracing::warn!(
                        event_code = "agent_inventory_persistence_load_failed",
                        node_id = %state.node_id,
                        error_code = %inventory_error_code(&error),
                    );
                    return;
                }
            },
            Err(error) => {
                tracing::warn!(
                    event_code = "agent_inventory_persistence_path_failed",
                    node_id = %state.node_id,
                    error_code = %inventory_error_code(&error),
                );
                return;
            }
        },
        None => None,
    };
    let mut pending_snapshot = pending_store.as_ref().and_then(|(_, snapshot)| snapshot.clone());
    loop {
        if cancellation.is_cancelled() {
            return;
        }
        if pending_snapshot.is_none() {
            let snapshot_id = Uuid::new_v4().to_string();
            let collected_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|value| value.as_secs() as i64)
                .unwrap_or(0);
            let collection = tokio::select! {
                result = collector::collect_linux_command(&snapshot_id, collected_at, &host_key) => result,
                _ = cancellation.cancelled() => return,
            };
            match collection {
                Ok(snapshot) => {
                    if let Some((store, _)) = pending_store.as_ref() {
                        if let Err(error) = store.save(&snapshot) {
                            tracing::warn!(
                                event_code = "agent_inventory_persistence_failed",
                                node_id = %state.node_id,
                                error_code = %inventory_error_code(&error),
                            );
                            if wait_or_cancel(&cancellation, backoff.next_delay()).await {
                                return;
                            }
                            continue;
                        }
                    }
                    pending_snapshot = Some(snapshot);
                }
                Err(error) => {
                    tracing::warn!(
                        event_code = "agent_inventory_collection_failed",
                        node_id = %state.node_id,
                        error_code = %inventory_error_code(&anyhow::anyhow!(error)),
                    );
                    if wait_or_cancel(&cancellation, backoff.next_delay()).await {
                        return;
                    }
                    continue;
                }
            }
        }

        let snapshot = pending_snapshot.as_ref().expect("pending snapshot was set");
        let result = tokio::select! {
            result = transport.upload_inventory(&state, snapshot) => result,
            _ = cancellation.cancelled() => return,
        };
        match result {
            Ok(_) => {
                let mut cleared = true;
                if let Some((store, _)) = pending_store.as_ref() {
                    if let Err(error) = store.clear() {
                        cleared = false;
                        tracing::warn!(
                            event_code = "agent_inventory_persistence_clear_failed",
                            node_id = %state.node_id,
                            error_code = %inventory_error_code(&error),
                        );
                    }
                }
                if cleared {
                    pending_snapshot = None;
                    backoff.reset();
                }
                let delay = if cleared { interval } else { backoff.next_delay() };
                if wait_or_cancel(&cancellation, delay).await {
                    return;
                }
            }
            Err(error) => {
                tracing::warn!(
                    event_code = "agent_inventory_upload_failed",
                    node_id = %state.node_id,
                    error_code = %inventory_error_code(&error),
                );
                if wait_or_cancel(&cancellation, backoff.next_delay()).await {
                    return;
                }
            }
        }
    }
}

fn inventory_error_code(error: &anyhow::Error) -> &'static str {
    let _ = error;
    "bounded_collection_or_upload_failure"
}

pub async fn wait_or_cancel(cancellation: &Cancellation, duration: Duration) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(duration) => false,
        _ = cancellation.cancelled() => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_backoff_is_jittered_and_bounded() {
        let mut backoff = Backoff::new(
            std::time::Duration::from_secs(1),
            std::time::Duration::from_secs(8),
        );
        let delays: Vec<_> = (0..20).map(|_| backoff.next_delay()).collect();

        assert!(delays.iter().all(|delay| !delay.is_zero()));
        assert!(delays
            .iter()
            .all(|delay| *delay <= std::time::Duration::from_secs(8)));
        assert!(delays.iter().skip(4).all(|delay| {
            *delay >= std::time::Duration::from_secs(4)
                && *delay <= std::time::Duration::from_secs(8)
        }));
        backoff.reset();
        assert!(backoff.next_delay() <= std::time::Duration::from_secs(1));
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_heartbeat() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_url = format!("http://{}", listener.local_addr().unwrap());
        let (accepted_tx, accepted_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            let _ = accepted_tx.send(());
            std::future::pending::<()>().await;
        });
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: crate::agent::state::HeartbeatToken::new(
                "supervision-heartbeat-token".into(),
            )
            .unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };
        let cancellation = Cancellation::new();
        let runtime = tokio::spawn(run(state, transport, cancellation.clone()));
        accepted_rx.await.unwrap();

        cancellation.cancel();
        tokio::time::timeout(std::time::Duration::from_millis(100), runtime)
            .await
            .unwrap()
            .unwrap();
        server.abort();
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_scheduled_wait() {
        let cancellation = Cancellation::new();
        let waiting = tokio::spawn({
            let cancellation = cancellation.clone();
            async move { wait_or_cancel(&cancellation, std::time::Duration::from_secs(3600)).await }
        });

        cancellation.cancel();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), waiting)
                .await
                .unwrap()
                .unwrap()
        );
    }

    #[tokio::test]
    async fn invalid_state_fails_closed_before_starting_loops() {
        let state = crate::agent::state::AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: crate::agent::state::HeartbeatToken::new(
                "supervision-invalid-state-token".into(),
            )
            .unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule {
                heartbeat_interval_seconds: 0,
                ..Default::default()
            },
        };
        let transport = AgentTransport::new_loopback_test("http://127.0.0.1:1", None).unwrap();
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            run(state, transport, Cancellation::new()),
        )
        .await
        .expect("invalid state should stop without starting network loops");
    }
}
