pub mod state;
pub mod supervision;
pub mod transport;

use anyhow::{bail, Context, Result};
use state::{reject_symlink_chain, AgentSchedule, AgentState, PreparedStateWrite};
use std::{fs::OpenOptions, io::Read, path::PathBuf};
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

pub async fn run(state_path: PathBuf) -> Result<()> {
    let state = AgentState::load(&state_path)?;
    let transport = AgentTransport::new(
        &state.server_url,
        state.ca_certificate_pem.as_deref().map(str::as_bytes),
    )?;
    let cancellation = Cancellation::new();
    let supervision = supervision::run_with_state_path(
        state,
        transport,
        cancellation.clone(),
        Some(state_path),
    );
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
    use std::os::unix::fs::symlink;

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
        use std::{
            ffi::CString,
            os::unix::ffi::OsStrExt,
            sync::mpsc,
            thread,
            time::Duration,
        };

        let root = std::env::temp_dir().join(format!(
            "voidtower-agent-ca-fifo-{}",
            uuid::Uuid::new_v4()
        ));
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
