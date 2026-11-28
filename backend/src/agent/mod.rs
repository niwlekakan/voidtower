pub mod state;
pub mod supervision;
pub mod transport;

use anyhow::{bail, Context, Result};
use state::{reject_symlink_chain, AgentSchedule, AgentState, PreparedStateWrite};
use std::{fs::OpenOptions, io::Read, io::Write, path::PathBuf};
use supervision::Cancellation;
use transport::{AgentTransport, EnrollmentRequest};

pub struct EnrollmentOptions {
    pub server_url: String,
    pub pairing_code: String,
    pub display_name: String,
    pub device_type: String,
    pub ca_path: Option<PathBuf>,
    pub provision_wireguard: bool,
    pub state_path: PathBuf,
}

pub async fn enroll(options: EnrollmentOptions) -> Result<()> {
    let ca_certificate_pem = options
        .ca_path
        .as_deref()
        .map(read_ca_certificate)
        .transpose()?;
    let prepared_state = PreparedStateWrite::prepare(&options.state_path)?;
    let transport = AgentTransport::new(&options.server_url, ca_certificate_pem.as_deref())?;
    let enrolled = transport
        .enroll(&EnrollmentRequest::new(
            options.pairing_code,
            options.display_name,
            options.device_type,
            options.provision_wireguard,
        ))
        .await?;
    let wireguard_returned = enrolled.wireguard_client_config.is_some();
    let warning_count = enrolled.warnings.len();
    let state = AgentState {
        server_url: options.server_url,
        node_id: enrolled.node_id,
        heartbeat_token: enrolled.heartbeat_token,
        ca_certificate_pem: ca_certificate_pem
            .map(String::from_utf8)
            .transpose()
            .context("CA certificate must be UTF-8 PEM")?,
        wireguard_client_config: enrolled.wireguard_client_config,
        schedule: AgentSchedule::default(),
    };
    prepared_state.commit(&state)?;

    println!("Enrolled agent node {}", state.node_id);
    println!("State saved to {}", options.state_path.display());
    if warning_count > 0 {
        println!("Controller returned {warning_count} enrollment warning(s)");
    }
    if wireguard_returned {
        println!(
            "Controller provisioned WireGuard; configuration was saved but not applied by this runtime"
        );
    }
    Ok(())
}

/// Validate the local agent state file and print a redacted summary.
///
/// This is the offline compatibility probe used before rolling an installed
/// agent binary back: it proves the restored binary can still read the state a
/// newer release left behind, without contacting the controller or exposing the
/// node token, CA material, or WireGuard configuration.
pub fn check_state(state_path: &std::path::Path) -> Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    check_state_into(state_path, &mut handle)
}

pub fn check_state_into(state_path: &std::path::Path, out: &mut impl Write) -> Result<()> {
    let state = AgentState::load(state_path)?;
    write_state_summary(&state, out)
}

pub fn write_state_summary(state: &AgentState, out: &mut impl Write) -> Result<()> {
    state.validate()?;
    let url = reqwest::Url::parse(&state.server_url).context("server URL is invalid")?;
    writeln!(out, "state: ok")?;
    writeln!(out, "node_id: {}", state.node_id)?;
    writeln!(out, "server: {}://{}", url.scheme(), url.authority())?;
    writeln!(
        out,
        "heartbeat_interval_seconds: {}",
        state.schedule.heartbeat_interval_seconds
    )?;
    writeln!(
        out,
        "inventory_interval_seconds: {}",
        state.schedule.inventory_interval_seconds
    )?;
    writeln!(
        out,
        "max_backoff_seconds: {}",
        state.schedule.max_backoff_seconds
    )?;
    writeln!(
        out,
        "ca_certificate: {}",
        presence(state.ca_certificate_pem.is_some())
    )?;
    writeln!(
        out,
        "wireguard_config: {}",
        presence(state.wireguard_client_config.is_some())
    )?;
    out.flush()?;
    Ok(())
}

fn presence(is_present: bool) -> &'static str {
    if is_present {
        "configured"
    } else {
        "none"
    }
}

pub async fn run(state_path: PathBuf) -> Result<()> {
    let state = AgentState::load(&state_path)?;
    let transport = AgentTransport::new(
        &state.server_url,
        state.ca_certificate_pem.as_deref().map(str::as_bytes),
    )?;
    let cancellation = Cancellation::new();
    let supervision =
        supervision::run_with_state_path(state, transport, cancellation.clone(), Some(state_path));
    tokio::pin!(supervision);

    tokio::select! {
        () = &mut supervision => Ok(()),
        () = shutdown_signal() => {
            cancellation.cancel();
            supervision.await;
            Ok(())
        }
    }
}

fn read_ca_certificate(path: &std::path::Path) -> Result<Vec<u8>> {
    reject_symlink_chain(path, "CA certificate path")?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .with_context(|| format!("failed to open CA certificate {}", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("failed to inspect CA certificate {}", path.display()))?;
    if !metadata.is_file() {
        bail!("CA certificate path must be a regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o600 {
            bail!("CA certificate permissions must be 0600");
        }
    }
    if metadata.len() == 0 || metadata.len() > 64 * 1024 {
        bail!("CA certificate must be between 1 and 65536 bytes");
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(64 * 1024 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read CA certificate {}", path.display()))?;
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        bail!("CA certificate must be between 1 and 65536 bytes");
    }
    Ok(bytes)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(_) => {
                    let _ = tokio::signal::ctrl_c().await;
                    return;
                }
            };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::agent::state::HeartbeatToken;
    use std::os::unix::fs::symlink;
    use uuid::Uuid;

    fn protected_state_fixture() -> AgentState {
        AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: Uuid::parse_str("6f1c5f9e-3b6c-4f6e-9a1f-2f5f4f4f4f4f").unwrap(),
            heartbeat_token: HeartbeatToken::new("node-token-value-1234567890".into()).unwrap(),
            ca_certificate_pem: Some(
                "-----BEGIN CERTIFICATE-----\nCAVALUE\n-----END CERTIFICATE-----\n".into(),
            ),
            wireguard_client_config: Some("PRIVATEKEYMATERIAL".into()),
            schedule: AgentSchedule::default(),
        }
    }

    #[test]
    fn state_summary_reports_identity_without_credentials() {
        let state = protected_state_fixture();
        let mut summary = Vec::new();

        write_state_summary(&state, &mut summary).unwrap();
        let text = String::from_utf8(summary).unwrap();

        assert!(text.contains("state: ok"));
        assert!(text.contains(&state.node_id.to_string()));
        assert!(text.contains("server: https://controller.example.test"));
        assert!(text.contains("heartbeat_interval_seconds: 30"));
        assert!(text.contains("inventory_interval_seconds: 900"));
        assert!(text.contains("max_backoff_seconds: 300"));
        assert!(text.contains("ca_certificate: configured"));
        assert!(text.contains("wireguard_config: configured"));
        assert!(!text.contains("node-token-value-1234567890"));
        assert!(!text.contains("CAVALUE"));
        assert!(!text.contains("PRIVATEKEYMATERIAL"));
    }

    #[test]
    fn state_summary_keeps_the_server_authority_only() {
        let mut state = protected_state_fixture();
        state.server_url = "https://controller.example.test:8443".into();
        let mut summary = Vec::new();

        write_state_summary(&state, &mut summary).unwrap();
        let text = String::from_utf8(summary).unwrap();

        assert!(text.contains("server: https://controller.example.test:8443"));
    }

    #[test]
    fn check_state_accepts_a_protected_state_file() {
        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-check-state-ok-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("state.json");
        protected_state_fixture().save(&path).unwrap();

        let mut summary = Vec::new();
        check_state_into(&path, &mut summary).unwrap();

        assert!(String::from_utf8(summary).unwrap().contains("state: ok"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn check_state_rejects_a_group_readable_state_file() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-check-state-mode-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("state.json");
        protected_state_fixture().save(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let error = check_state_into(&path, &mut Vec::new()).unwrap_err();

        assert!(error.to_string().contains("0600"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn check_state_rejects_a_symlinked_state_file() {
        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-check-state-link-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target.json");
        protected_state_fixture().save(&target).unwrap();
        let link = root.join("state.json");
        symlink(&target, &link).unwrap();

        let error = check_state_into(&link, &mut Vec::new()).unwrap_err();

        assert!(error.to_string().contains("symlink"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn check_state_rejects_corrupt_state_without_echoing_content() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-check-state-corrupt-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("state.json");
        std::fs::write(&path, "{\"heartbeat_token\":\"LEAKEDTOKENVALUE\"").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        let error = check_state_into(&path, &mut Vec::new()).unwrap_err();

        let text = format!("{error:#}");
        assert!(!text.contains("LEAKEDTOKENVALUE"), "{text}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn invalid_state_destination_fails_before_enrollment_request() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-preflight-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("target.json");
        std::fs::write(&target, "unchanged").unwrap();
        let state_path = root.join("state.json");
        symlink(&target, &state_path).unwrap();

        let result = enroll(EnrollmentOptions {
            server_url: format!("https://{}", listener.local_addr().unwrap()),
            pairing_code: "pairing-code-value".into(),
            display_name: "preflight".into(),
            device_type: "other".into(),
            ca_path: None,
            provision_wireguard: false,
            state_path,
        })
        .await;

        assert!(result.unwrap_err().to_string().contains("symlink"));
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(50), listener.accept())
                .await
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ca_read_rejects_symlinked_parent_chain() {
        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-ca-parent-link-{}",
            uuid::Uuid::new_v4()
        ));
        let real = root.join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("ca.pem"), "test certificate").unwrap();
        let linked_parent = root.join("linked-parent");
        symlink(&real, &linked_parent).unwrap();

        let error = read_ca_certificate(&linked_parent.join("ca.pem")).unwrap_err();

        assert!(error.to_string().contains("symlink"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ca_read_rejects_group_or_world_readable_files() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-ca-permissions-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("ca.pem");
        std::fs::write(&path, "test certificate").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let error = read_ca_certificate(&path).unwrap_err();

        assert!(error.to_string().contains("0600"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ca_read_rejects_a_fifo_without_blocking() {
        use std::{ffi::CString, os::unix::ffi::OsStrExt, sync::mpsc, thread, time::Duration};

        let root =
            std::env::temp_dir().join(format!("voidtower-agent-ca-fifo-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("ca.pem");
        let name = CString::new(path.as_os_str().as_bytes()).unwrap();
        assert_eq!(unsafe { nix::libc::mkfifo(name.as_ptr(), 0o600) }, 0);

        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            sender.send(read_ca_certificate(&path).map(|_| ())).unwrap();
        });
        let result = receiver
            .recv_timeout(Duration::from_millis(250))
            .expect("CA FIFO validation must not block");

        assert!(result.unwrap_err().to_string().contains("regular file"));
        worker.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
}
