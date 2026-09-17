use crate::{
    agent::state::{AgentState, HeartbeatToken},
    cmdb::contracts::{InventorySnapshotResultV1, InventorySnapshotV1},
};
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{fmt, io, net::IpAddr, time::Duration};
use uuid::Uuid;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_INVENTORY_REQUEST_BYTES: usize = 256 * 1024;

struct BoundedJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}

impl BoundedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit),
            limit,
            overflowed: false,
        }
    }
}

impl io::Write for BoundedJsonWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            self.overflowed = true;
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "serialized JSON exceeds configured bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct AgentTransport {
    client: reqwest::Client,
    base_url: reqwest::Url,
}

#[derive(Serialize)]
pub struct EnrollmentRequest {
    pairing_code: String,
    display_name: String,
    device_type: String,
    agent_capable: bool,
    provision_wireguard: bool,
}

impl EnrollmentRequest {
    pub fn new(
        pairing_code: String,
        display_name: String,
        device_type: String,
        provision_wireguard: bool,
    ) -> Self {
        Self {
            pairing_code,
            display_name,
            device_type,
            agent_capable: true,
            provision_wireguard,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.pairing_code.is_empty() || self.pairing_code.len() > 512 {
            bail!("pairing code must be between 1 and 512 bytes");
        }
        if self.display_name.trim().is_empty()
            || self.display_name.len() > 128
            || self.display_name.chars().any(char::is_control)
        {
            bail!("display name must be between 1 and 128 bytes");
        }
        if !matches!(
            self.device_type.as_str(),
            "phone" | "tablet" | "pi" | "other"
        ) {
            bail!("device type must be phone, tablet, pi, or other");
        }
        Ok(())
    }
}

pub struct EnrollmentResult {
    pub node_id: Uuid,
    pub heartbeat_token: HeartbeatToken,
    pub wireguard_client_config: Option<String>,
    pub warnings: Vec<String>,
}

impl fmt::Debug for EnrollmentResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnrollmentResult")
            .field("node_id", &self.node_id)
            .field("heartbeat_token", &self.heartbeat_token)
            .field(
                "wireguard_client_config",
                &self.wireguard_client_config.as_ref().map(|_| "[REDACTED]"),
            )
            .field("warning_count", &self.warnings.len())
            .finish()
    }
}

#[derive(Deserialize)]
struct EnrollmentWireResponse {
    node_id: String,
    heartbeat_token: String,
    #[serde(default)]
    wg_client_config: String,
    #[serde(default)]
    warnings: Vec<String>,
}

impl AgentTransport {
    pub fn new(server_url: &str, ca_certificate_pem: Option<&[u8]>) -> Result<Self> {
        Self::build(server_url, ca_certificate_pem, false)
    }

    #[cfg(test)]
    pub fn new_loopback_test(server_url: &str, ca_certificate_pem: Option<&[u8]>) -> Result<Self> {
        Self::build(server_url, ca_certificate_pem, true)
    }

    fn build(
        server_url: &str,
        ca_certificate_pem: Option<&[u8]>,
        allow_loopback_http: bool,
    ) -> Result<Self> {
        let mut base_url = reqwest::Url::parse(server_url).context("server URL is invalid")?;
        if base_url.host_str().is_none() || base_url.cannot_be_a_base() {
            bail!("server URL must be absolute");
        }
        if base_url.username() != ""
            || base_url.password().is_some()
            || base_url.query().is_some()
            || base_url.fragment().is_some()
        {
            bail!("server URL must not contain credentials, query, or fragment");
        }
        let secure = base_url.scheme() == "https";
        let test_loopback = allow_loopback_http
            && base_url.scheme() == "http"
            && base_url
                .host_str()
                .and_then(|host| host.parse::<IpAddr>().ok())
                .is_some_and(|address| address.is_loopback());
        if !secure && !test_loopback {
            bail!("server URL must use HTTPS");
        }
        if ca_certificate_pem.is_some() && !secure {
            bail!("a custom CA may only be configured for HTTPS");
        }
        let path = base_url.path().trim_end_matches('/').to_string();
        base_url.set_path(&format!("{path}/"));

        let mut builder = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(pem) = ca_certificate_pem {
            if pem.is_empty() || pem.len() > 64 * 1024 {
                bail!("CA certificate must be between 1 and 65536 bytes");
            }
            let pem_text = std::str::from_utf8(pem).context("CA certificate must be UTF-8 PEM")?;
            if !pem_text.contains("-----BEGIN CERTIFICATE-----")
                || !pem_text.contains("-----END CERTIFICATE-----")
            {
                bail!("CA certificate is not valid PEM");
            }
            let certificate =
                reqwest::Certificate::from_pem(pem).context("CA certificate is not valid PEM")?;
            builder = builder.add_root_certificate(certificate);
        }
        let client = builder
            .build()
            .context("failed to build agent HTTP client")?;
        Ok(Self { client, base_url })
    }

    fn endpoint(&self, relative_path: &str) -> Result<reqwest::Url> {
        self.base_url
            .join(relative_path)
            .context("failed to construct agent endpoint URL")
    }

    pub async fn enroll(&self, request: &EnrollmentRequest) -> Result<EnrollmentResult> {
        request.validate()?;
        let response = self
            .client
            .post(self.endpoint("api/nodes/enroll")?)
            .json(request)
            .send()
            .await
            .context("enrollment request failed")?;
        let response: EnrollmentWireResponse = read_json_response(response).await?;
        let node_id =
            Uuid::parse_str(&response.node_id).context("server returned an invalid node ID")?;
        let heartbeat_token = HeartbeatToken::new(response.heartbeat_token)
            .context("server returned an invalid heartbeat token")?;
        if response.wg_client_config.len() > MAX_RESPONSE_BYTES {
            bail!("server returned an oversized WireGuard configuration");
        }
        if response.warnings.len() > 32
            || response.warnings.iter().any(|warning| warning.len() > 1024)
        {
            bail!("server returned oversized enrollment warnings");
        }
        Ok(EnrollmentResult {
            node_id,
            heartbeat_token,
            wireguard_client_config: (!response.wg_client_config.is_empty())
                .then_some(response.wg_client_config),
            warnings: response.warnings,
        })
    }

    pub async fn heartbeat(&self, state: &AgentState) -> Result<()> {
        let endpoint = self.endpoint(&format!("api/nodes/{}/heartbeat", state.node_id))?;
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(state.heartbeat_token.expose())
            .json(&serde_json::json!({
                "battery": null,
                "storage_free_bytes": null,
                "online": true
            }))
            .send()
            .await
            .context("heartbeat request failed")?;
        let response: HeartbeatResponse = read_json_response(response).await?;
        if !response.ok {
            bail!("agent server rejected heartbeat");
        }
        Ok(())
    }

    pub async fn upload_inventory(
        &self,
        state: &AgentState,
        snapshot: &InventorySnapshotV1,
    ) -> Result<InventorySnapshotResultV1> {
        let mut writer = BoundedJsonWriter::new(MAX_INVENTORY_REQUEST_BYTES);
        if let Err(error) = serde_json::to_writer(&mut writer, snapshot) {
            if writer.overflowed {
                bail!(
                    "inventory snapshot exceeds {MAX_INVENTORY_REQUEST_BYTES} bytes"
                );
            }
            return Err(error).context("failed to serialize inventory snapshot");
        }
        let body = writer.bytes;
        let endpoint = self.endpoint(&format!("api/nodes/{}/inventory", state.node_id))?;
        let response = self
            .client
            .post(endpoint)
            .bearer_auth(state.heartbeat_token.expose())
            .body(body)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .send()
            .await
            .context("inventory upload request failed")?;
        let result: InventorySnapshotResultV1 = read_json_response(response).await?;
        if result.snapshot_id != snapshot.snapshot_id {
            bail!("agent server acknowledged a different inventory snapshot");
        }
        Ok(result)
    }

    #[cfg(test)]
    fn connect_timeout(&self) -> Duration {
        CONNECT_TIMEOUT
    }

    #[cfg(test)]
    fn request_timeout(&self) -> Duration {
        REQUEST_TIMEOUT
    }
}

#[derive(Debug, Deserialize)]
struct HeartbeatResponse {
    ok: bool,
}

async fn read_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        bail!("agent server returned HTTP {status}");
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        bail!("agent server response exceeds {MAX_RESPONSE_BYTES} bytes");
    }
    let mut body = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.context("failed to read agent server response")?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            bail!("agent server response exceeds {MAX_RESPONSE_BYTES} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).context("agent server returned invalid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn serve_once(
        response_status: &str,
        response_body: String,
    ) -> (String, tokio::sync::oneshot::Receiver<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (request_tx, request_rx) = tokio::sync::oneshot::channel();
        let status = response_status.to_string();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let read = stream.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end + 4]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .map(str::to_string)
                    })
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            let _ = request_tx.send(String::from_utf8(request).unwrap());
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
                response_body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        (format!("http://{address}"), request_rx)
    }

    #[test]
    fn enrollment_result_debug_redacts_credentials() {
        let result = EnrollmentResult {
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("debug-heartbeat-token".into()).unwrap(),
            wireguard_client_config: Some("PrivateKey = wireguard-secret".into()),
            warnings: vec!["bounded warning".into()],
        };

        let debug = format!("{result:?}");
        assert!(!debug.contains("debug-heartbeat-token"));
        assert!(!debug.contains("wireguard-secret"));
        assert!(!debug.contains("bounded warning"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn production_transport_requires_https_and_valid_ca() {
        assert!(AgentTransport::new("http://127.0.0.1:8743", None).is_err());
        assert!(AgentTransport::new(
            "https://controller.example.test",
            Some(b"not a certificate")
        )
        .is_err());
        assert!(AgentTransport::new(
            "https://controller.example.test",
            Some(b"-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n")
        )
        .is_err());
        assert!(AgentTransport::new("https://controller.example.test", None).is_ok());
    }

    #[test]
    fn configured_transport_timeouts_are_bounded() {
        let transport = AgentTransport::new("https://controller.example.test", None).unwrap();
        assert_eq!(
            transport.connect_timeout(),
            std::time::Duration::from_secs(5)
        );
        assert_eq!(
            transport.request_timeout(),
            std::time::Duration::from_secs(15)
        );
    }

    #[tokio::test]
    async fn heartbeat_uses_scoped_token_and_node_path() {
        let (server_url, request_rx) = serve_once("200 OK", "{\"ok\":true}".into()).await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let node_id = uuid::Uuid::new_v4();
        let state = AgentState {
            server_url,
            node_id,
            heartbeat_token: HeartbeatToken::new("scoped-heartbeat-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };

        transport.heartbeat(&state).await.unwrap();

        let request = request_rx.await.unwrap();
        assert!(request.starts_with(&format!("POST /api/nodes/{node_id}/heartbeat HTTP/1.1")));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer scoped-heartbeat-token"));
        assert!(!request.contains("/inventory"));
    }

    #[tokio::test]
    async fn heartbeat_rejects_a_success_response_that_is_not_acknowledged() {
        let (server_url, _request_rx) = serve_once("200 OK", "{\"ok\":false}".into()).await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("heartbeat-contract-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };

        let error = transport.heartbeat(&state).await.unwrap_err();
        assert!(error.to_string().contains("rejected heartbeat"));
    }

    #[tokio::test]
    async fn inventory_upload_uses_node_path_and_scoped_token() {
        let (server_url, request_rx) = serve_once(
            "200 OK",
            "{\"snapshot_id\":\"snapshot-1\",\"replayed\":false,\"linked\":1,\"registered\":0,\"review_required\":0,\"missing\":0}".into(),
        )
        .await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url,
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("inventory-heartbeat-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };
        let snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "snapshot_id": "snapshot-1", "collector_version": "test",
            "platform": "linux", "collected_at": 0,
            "host": {"entity_key":"host","identities":[],"attributes":{},"runtime":{}},
            "entities": []
        })).unwrap();
        let result = transport.upload_inventory(&state, &snapshot).await.unwrap();
        assert!(!result.replayed);
        let request = request_rx.await.unwrap();
        assert!(request.starts_with(&format!("POST /api/nodes/{}/inventory HTTP/1.1", state.node_id)));
        assert!(request.to_ascii_lowercase().contains("authorization"));
        assert!(request.contains("snapshot-1"));
    }

    #[tokio::test]
    async fn inventory_upload_rejects_oversized_snapshot_before_network_request() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_url = format!("http://{}", listener.local_addr().unwrap());
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url,
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("inventory-heartbeat-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };
        let mut snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "snapshot_id": "snapshot-large", "collector_version": "test",
            "platform": "linux", "collected_at": 0,
            "host": {"entity_key":"host","identities":[],"attributes":{},"runtime":{}},
            "entities": []
        })).unwrap();
        snapshot.host.runtime = serde_json::Value::String("x".repeat(MAX_INVENTORY_REQUEST_BYTES));

        let error = transport.upload_inventory(&state, &snapshot).await.unwrap_err();

        assert!(error.to_string().contains("inventory snapshot exceeds"));
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(25),
            listener.accept()
        ).await.is_err());
    }

    #[tokio::test]
    async fn response_status_and_size_are_bounded_without_echoing_body() {
        let secret_body = "server accidentally echoed returned-heartbeat-token".to_string();
        let (server_url, _) = serve_once("500 Internal Server Error", secret_body.clone()).await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let error = transport
            .enroll(&EnrollmentRequest::new(
                "pairing-code".into(),
                "test-node".into(),
                "pi".into(),
                false,
            ))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("500"));
        assert!(!error.contains(&secret_body));

        let (server_url, _) = serve_once("200 OK", "x".repeat(MAX_RESPONSE_BYTES + 1)).await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let error = transport
            .enroll(&EnrollmentRequest::new(
                "pairing-code".into(),
                "test-node".into(),
                "pi".into(),
                false,
            ))
            .await
            .unwrap_err()
            .to_string();
        assert!(error.contains("exceeds"));
    }

    #[tokio::test]
    async fn heartbeat_rejects_malformed_success_response() {
        let (server_url, _request_rx) = serve_once("200 OK", "{".into()).await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("heartbeat-contract-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };

        let error = transport.heartbeat(&state).await.unwrap_err();
        assert!(error.to_string().contains("invalid JSON"));
    }

    #[tokio::test]
    async fn inventory_upload_rejects_incomplete_success_response() {
        let (server_url, _request_rx) = serve_once(
            "200 OK",
            "{\"snapshot_id\":\"snapshot-1\",\"replayed\":false}".into(),
        )
        .await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("inventory-contract-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };
        let snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "snapshot_id": "snapshot-1", "collector_version": "test",
            "platform": "linux", "collected_at": 0,
            "host": {"entity_key":"host","identities":[],"attributes":{},"runtime":{}},
            "entities": []
        })).unwrap();

        let error = transport.upload_inventory(&state, &snapshot).await.unwrap_err();
        assert!(format!("{error:#}").contains("missing field `linked`"));
    }

    #[tokio::test]
    async fn inventory_upload_rejects_success_for_a_different_snapshot() {
        let (server_url, _request_rx) = serve_once(
            "200 OK",
            "{\"snapshot_id\":\"different-snapshot\",\"replayed\":false,\"linked\":1,\"registered\":0,\"review_required\":0,\"missing\":0}".into(),
        )
        .await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();
        let state = AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("inventory-contract-token".into()).unwrap(),
            ca_certificate_pem: None,
            wireguard_client_config: None,
            schedule: crate::agent::state::AgentSchedule::default(),
        };
        let snapshot: InventorySnapshotV1 = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "snapshot_id": "snapshot-1", "collector_version": "test",
            "platform": "linux", "collected_at": 0,
            "host": {"entity_key":"host","identities":[],"attributes":{},"runtime":{}},
            "entities": []
        })).unwrap();

        let error = transport.upload_inventory(&state, &snapshot).await.unwrap_err();
        assert!(error.to_string().contains("different inventory snapshot"));
    }

    #[test]
    fn enrollment_validation_matches_controller_pairing_code_limit() {
        let request = EnrollmentRequest::new("x".repeat(513), "test-node".into(), "pi".into(), false);
        let error = request.validate().unwrap_err();
        assert!(error.to_string().contains("512"));
    }

    #[test]
    fn enrollment_validation_rejects_control_characters_in_display_name() {
        let request = EnrollmentRequest::new(
            "pairing-code".into(),
            "test-node\n".into(),
            "pi".into(),
            false,
        );
        assert!(request.validate().is_err());
    }

    #[tokio::test]
    async fn enrollment_sends_wireguard_false_by_default() {
        let node_id = uuid::Uuid::new_v4();
        let response = serde_json::json!({
            "node_id": node_id,
            "heartbeat_token": "returned-heartbeat-token",
            "wg_client_config": "",
            "warnings": []
        })
        .to_string();
        let (server_url, request_rx) = serve_once("200 OK", response).await;
        let transport = AgentTransport::new_loopback_test(&server_url, None).unwrap();

        let enrolled = transport
            .enroll(&EnrollmentRequest::new(
                "pairing-code".into(),
                "test-node".into(),
                "pi".into(),
                false,
            ))
            .await
            .unwrap();

        assert_eq!(enrolled.node_id, node_id);
        assert_eq!(
            enrolled.heartbeat_token.expose(),
            "returned-heartbeat-token"
        );
        let request = request_rx.await.unwrap();
        assert!(request.starts_with("POST /api/nodes/enroll HTTP/1.1"));
        let body = request.split("\r\n\r\n").nth(1).unwrap();
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(body["provision_wireguard"], false);
        assert_eq!(body["agent_capable"], true);
    }
}
