use crate::agent::{state::AgentState, transport::AgentTransport};
use rand::Rng;
use std::time::Duration;
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
    let heartbeat_interval = Duration::from_secs(state.schedule.heartbeat_interval_seconds);
    let mut backoff = Backoff::new(
        Duration::from_secs(1),
        Duration::from_secs(state.schedule.max_backoff_seconds),
    );

    // Inventory intentionally has no task until a collector can produce a complete snapshot.
    // Sending an empty full snapshot would incorrectly mark existing observations missing.
    loop {
        if cancellation.is_cancelled() {
            return;
        }
        let heartbeat = transport.heartbeat(&state);
        tokio::pin!(heartbeat);
        let heartbeat_result = tokio::select! {
            result = &mut heartbeat => result,
            _ = cancellation.cancelled() => return,
        };
        let delay = match heartbeat_result {
            Ok(()) => {
                backoff.reset();
                heartbeat_interval
            }
            Err(_) => {
                tracing::warn!(
                    event_code = "agent_heartbeat_failed",
                    node_id = %state.node_id
                );
                backoff.next_delay()
            }
        };
        if wait_or_cancel(&cancellation, delay).await {
            return;
        }
    }
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
            server_url,
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
}
