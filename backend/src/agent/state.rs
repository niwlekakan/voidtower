use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

const MAX_STATE_BYTES: u64 = 256 * 1024;
const MAX_CA_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct HeartbeatToken(String);

impl HeartbeatToken {
    pub fn new(value: String) -> Result<Self> {
        let token = Self(value);
        token.validate()?;
        Ok(token)
    }

    fn validate(&self) -> Result<()> {
        if self.0.len() < 16 || self.0.len() > 4096 || self.0.trim() != self.0 {
            bail!("heartbeat token is invalid");
        }
        Ok(())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HeartbeatToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSchedule {
    #[serde(default = "default_heartbeat_interval_seconds")]
    pub heartbeat_interval_seconds: u64,
    #[serde(default = "default_inventory_interval_seconds")]
    pub inventory_interval_seconds: u64,
    #[serde(default = "default_max_backoff_seconds")]
    pub max_backoff_seconds: u64,
}

impl Default for AgentSchedule {
    fn default() -> Self {
        Self {
            heartbeat_interval_seconds: default_heartbeat_interval_seconds(),
            inventory_interval_seconds: default_inventory_interval_seconds(),
            max_backoff_seconds: default_max_backoff_seconds(),
        }
    }
}

fn default_heartbeat_interval_seconds() -> u64 {
    30
}

fn default_inventory_interval_seconds() -> u64 {
    15 * 60
}

fn default_max_backoff_seconds() -> u64 {
    5 * 60
}

#[derive(Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub server_url: String,
    pub node_id: Uuid,
    pub heartbeat_token: HeartbeatToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_certificate_pem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wireguard_client_config: Option<String>,
    #[serde(default)]
    pub schedule: AgentSchedule,
}

impl fmt::Debug for AgentState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AgentState")
            .field("server_url", &self.server_url)
            .field("node_id", &self.node_id)
            .field("heartbeat_token", &self.heartbeat_token)
            .field(
                "ca_certificate_pem",
                &self.ca_certificate_pem.as_ref().map(|_| "configured"),
            )
            .field(
                "wireguard_client_config",
                &self.wireguard_client_config.as_ref().map(|_| "[REDACTED]"),
            )
            .field("schedule", &self.schedule)
            .finish()
    }
}

impl AgentState {
    pub fn validate(&self) -> Result<()> {
        self.heartbeat_token.validate()?;
        let url = reqwest::Url::parse(&self.server_url).context("server URL is invalid")?;
        if url.scheme() != "https" || url.host_str().is_none() || url.cannot_be_a_base() {
            bail!("server URL must be an absolute HTTPS URL");
        }
        if url.username() != ""
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("server URL must not contain credentials, query, or fragment");
        }
        if self
            .ca_certificate_pem
            .as_ref()
            .is_some_and(|pem| pem.is_empty() || pem.len() > MAX_CA_BYTES)
        {
            bail!("CA certificate must be between 1 and {MAX_CA_BYTES} bytes");
        }
        if self
            .wireguard_client_config
            .as_ref()
            .is_some_and(|config| config.is_empty() || config.len() > MAX_CA_BYTES)
        {
            bail!("WireGuard client configuration must be between 1 and {MAX_CA_BYTES} bytes");
        }
        if !(5..=3600).contains(&self.schedule.heartbeat_interval_seconds) {
            bail!("heartbeat interval must be between 5 and 3600 seconds");
        }
        if !(60..=86_400).contains(&self.schedule.inventory_interval_seconds) {
            bail!("inventory interval must be between 60 and 86400 seconds");
        }
        if !(1..=3600).contains(&self.schedule.max_backoff_seconds) {
            bail!("maximum backoff must be between 1 and 3600 seconds");
        }
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        if fs::symlink_metadata(path)
            .with_context(|| format!("failed to inspect agent state {}", path.display()))?
            .file_type()
            .is_symlink()
        {
            bail!("agent state path must not be a symlink");
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(nix::libc::O_NOFOLLOW);
        }
        let file = options
            .open(path)
            .with_context(|| format!("failed to open agent state {}", path.display()))?;
        let metadata = file
            .metadata()
            .with_context(|| format!("failed to inspect agent state {}", path.display()))?;
        if !metadata.is_file() {
            bail!("agent state path must be a regular file");
        }
        #[cfg(windows)]
        secure_windows_path(path)?;
        if metadata.len() > MAX_STATE_BYTES {
            bail!("agent state exceeds {MAX_STATE_BYTES} bytes");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o777 != 0o600 {
                bail!("agent state permissions must be 0600");
            }
        }
        let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        file.take(MAX_STATE_BYTES + 1)
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read agent state {}", path.display()))?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            bail!("agent state exceeds {MAX_STATE_BYTES} bytes");
        }
        let state: Self =
            serde_json::from_slice(&bytes).context("failed to parse agent state JSON")?;
        state.validate()?;
        Ok(state)
    }

    #[cfg(test)]
    pub fn save(&self, path: &Path) -> Result<()> {
        PreparedStateWrite::prepare(path)?.commit(self)
    }
}

pub struct PreparedStateWrite {
    path: PathBuf,
    parent: PathBuf,
    temp_path: PathBuf,
    file: Option<File>,
    committed: bool,
}

impl PreparedStateWrite {
    pub fn prepare(path: &Path) -> Result<Self> {
        let parent = state_parent(path)?.to_path_buf();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("agent state path must have a UTF-8 file name")?;
        reject_symlink(path, "agent state path")?;
        prepare_parent(&parent)?;
        reject_symlink(path, "agent state path")?;

        let temp_path = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&temp_path)
            .with_context(|| format!("failed to reserve {}", temp_path.display()))?;
        #[cfg(windows)]
        secure_windows_path(&temp_path)?;
        Ok(Self {
            path: path.to_path_buf(),
            parent,
            temp_path,
            file: Some(file),
            committed: false,
        })
    }

    pub fn commit(mut self, state: &AgentState) -> Result<()> {
        state.validate()?;
        let bytes = serde_json::to_vec_pretty(state).context("failed to serialize agent state")?;
        if bytes.len() as u64 > MAX_STATE_BYTES {
            bail!("agent state exceeds {MAX_STATE_BYTES} bytes");
        }
        let mut file = self
            .file
            .take()
            .context("agent state write was already consumed")?;
        file.write_all(&bytes)
            .context("failed to write agent state")?;
        file.sync_all().context("failed to sync agent state")?;
        drop(file);
        reject_symlink(&self.path, "agent state path")?;
        atomic_replace(&self.temp_path, &self.path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
            fs::File::open(&self.parent)?.sync_all()?;
        }
        self.committed = true;
        Ok(())
    }
}

impl Drop for PreparedStateWrite {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.file.take();
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

fn state_parent(path: &Path) -> Result<&Path> {
    path.file_name()
        .context("agent state path must name a file")?;
    Ok(path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new(".")))
}

fn reject_symlink(path: &Path, label: &str) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => bail!("{label} must not be a symlink"),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {label}")),
    }
}

fn prepare_parent(parent: &Path) -> Result<()> {
    #[cfg(windows)]
    let parent_was_missing = !parent.exists();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        let absolute = if parent.is_absolute() {
            parent.to_path_buf()
        } else {
            std::env::current_dir()?.join(parent)
        };
        for ancestor in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
            match fs::symlink_metadata(ancestor) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() {
                        bail!("agent state parent chain must not contain symlinks");
                    }
                    if !metadata.is_dir() {
                        bail!("agent state parent chain must contain only directories");
                    }
                    let mode = metadata.permissions().mode();
                    if mode & 0o022 != 0 && mode & 0o1000 == 0 {
                        bail!("agent state parent chain contains an unsafe writable directory");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => {
                    return Err(error).context("failed to inspect agent state parent chain")
                }
            }
        }
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(parent).with_context(|| {
            format!(
                "failed to create agent state directory {}",
                parent.display()
            )
        })?;
        for ancestor in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
            let metadata = fs::symlink_metadata(ancestor)?;
            if metadata.file_type().is_symlink() {
                bail!("agent state parent chain must not contain symlinks");
            }
        }
    }
    #[cfg(not(unix))]
    fs::create_dir_all(parent).with_context(|| {
        format!(
            "failed to create agent state directory {}",
            parent.display()
        )
    })?;
    #[cfg(windows)]
    if parent_was_missing {
        secure_windows_path(parent)?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(temp_path: &Path, path: &Path) -> Result<()> {
    fs::rename(temp_path, path).with_context(|| {
        format!(
            "failed to atomically replace agent state {}",
            path.display()
        )
    })
}

#[cfg(windows)]
fn atomic_replace(temp_path: &Path, path: &Path) -> Result<()> {
    use std::{io, os::windows::ffi::OsStrExt};

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let existing: Vec<u16> = temp_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let new: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let replaced = unsafe {
        MoveFileExW(
            existing.as_ptr(),
            new.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        return Err(io::Error::last_os_error()).with_context(|| {
            format!(
                "failed to atomically replace agent state {}",
                path.display()
            )
        });
    }
    Ok(())
}

#[cfg(windows)]
fn secure_windows_path(path: &Path) -> Result<()> {
    let username =
        std::env::var("USERNAME").context("USERNAME is required to secure agent state")?;
    if username.trim().is_empty() {
        bail!("USERNAME is required to secure agent state");
    }
    let principal = match std::env::var("USERDOMAIN") {
        Ok(domain) if !domain.trim().is_empty() => format!("{domain}\\{username}"),
        _ => username,
    };
    let grant = format!("{principal}:(F)");
    let status = std::process::Command::new("icacls.exe")
        .arg(path)
        .args(["/inheritance:r", "/grant:r"])
        .arg(grant)
        .status()
        .context("failed to invoke icacls.exe for agent state")?;
    if !status.success() {
        bail!("failed to restrict agent state ACL");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_state_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("voidtower-agent-{name}-{}", uuid::Uuid::new_v4()))
            .join("state.json")
    }

    fn state() -> AgentState {
        AgentState {
            server_url: "https://controller.example.test".into(),
            node_id: uuid::Uuid::new_v4(),
            heartbeat_token: HeartbeatToken::new("heartbeat-secret".into()).unwrap(),
            ca_certificate_pem: Some(
                "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n".into(),
            ),
            wireguard_client_config: Some("PrivateKey = wireguard-secret".into()),
            schedule: AgentSchedule::default(),
        }
    }

    #[test]
    fn bare_state_file_uses_current_directory_as_parent() {
        assert_eq!(
            state_parent(Path::new("state.json")).unwrap(),
            Path::new(".")
        );
    }

    #[test]
    fn dropping_prepared_write_removes_reserved_temp_file() {
        let path = temp_state_path("prepared-cleanup");
        let parent = path.parent().unwrap();
        let prepared = PreparedStateWrite::prepare(&path).unwrap();

        assert_eq!(fs::read_dir(parent).unwrap().count(), 1);
        drop(prepared);
        assert_eq!(fs::read_dir(parent).unwrap().count(), 0);
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn state_round_trip_preserves_runtime_configuration() {
        let path = temp_state_path("round-trip");
        let expected = state();

        expected.save(&path).unwrap();
        let actual = AgentState::load(&path).unwrap();

        assert_eq!(actual.server_url, expected.server_url);
        assert_eq!(actual.node_id, expected.node_id);
        assert_eq!(actual.heartbeat_token.expose(), "heartbeat-secret");
        assert_eq!(actual.ca_certificate_pem, expected.ca_certificate_pem);
        assert_eq!(
            actual.wireguard_client_config,
            expected.wireguard_client_config
        );
        assert_eq!(actual.schedule, expected.schedule);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn saved_state_and_parent_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_state_path("permissions");
        state().save(&path).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn nested_new_state_directories_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("voidtower-agent-nested-{}", Uuid::new_v4()));
        let child = root.join("child");
        state().save(&child.join("state.json")).unwrap();
        assert_eq!(
            fs::metadata(&root).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&child).unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn save_rejects_symlinked_target_and_parent() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("voidtower-agent-links-{}", Uuid::new_v4()));
        let real = root.join("real");
        fs::create_dir_all(&real).unwrap();
        let target = real.join("target.json");
        fs::write(&target, "unchanged").unwrap();
        let linked_target = real.join("state.json");
        symlink(&target, &linked_target).unwrap();
        assert!(state()
            .save(&linked_target)
            .unwrap_err()
            .to_string()
            .contains("symlink"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
        let linked_parent = root.join("linked-parent");
        symlink(&real, &linked_parent).unwrap();
        assert!(state()
            .save(&linked_parent.join("new-state.json"))
            .unwrap_err()
            .to_string()
            .contains("symlink"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn load_rejects_state_without_exact_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = temp_state_path("insecure-permissions");
        state().save(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        assert!(AgentState::load(&path)
            .unwrap_err()
            .to_string()
            .contains("0600"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn load_rejects_symlinked_state() {
        use std::os::unix::fs::symlink;

        let target = temp_state_path("symlink-target");
        state().save(&target).unwrap();
        let link = target.parent().unwrap().join("linked-state.json");
        symlink(&target, &link).unwrap();

        assert!(AgentState::load(&link)
            .unwrap_err()
            .to_string()
            .contains("symlink"));
        fs::remove_dir_all(target.parent().unwrap()).unwrap();
    }

    #[test]
    fn saving_again_atomically_replaces_existing_state() {
        let path = temp_state_path("atomic-replace");
        let first = state();
        first.save(&path).unwrap();
        let mut second = state();
        second.server_url = "https://replacement.example.test".into();

        second.save(&path).unwrap();

        let loaded = AgentState::load(&path).unwrap();
        assert_eq!(loaded.server_url, "https://replacement.example.test");
        assert_eq!(loaded.node_id, second.node_id);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn debug_output_redacts_heartbeat_token() {
        let state = state();
        let state_debug = format!("{state:?}");
        let token_debug = format!("{:?}", state.heartbeat_token);

        assert!(!state_debug.contains("heartbeat-secret"));
        assert!(!state_debug.contains("wireguard-secret"));
        assert!(!token_debug.contains("heartbeat-secret"));
        assert!(state_debug.contains("[REDACTED]"));
    }

    #[test]
    fn load_rejects_invalid_persisted_token() {
        let path = temp_state_path("invalid-token");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            serde_json::json!({
                "server_url": "https://controller.example.test",
                "node_id": uuid::Uuid::new_v4(),
                "heartbeat_token": "short",
                "schedule": AgentSchedule::default()
            })
            .to_string(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        assert!(AgentState::load(&path)
            .unwrap_err()
            .to_string()
            .contains("heartbeat token"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn schedule_validation_rejects_unbounded_intervals() {
        let mut invalid_heartbeat = state();
        invalid_heartbeat.schedule.heartbeat_interval_seconds = 0;
        assert!(invalid_heartbeat
            .validate()
            .unwrap_err()
            .to_string()
            .contains("heartbeat"));

        let mut invalid_inventory = state();
        invalid_inventory.schedule.inventory_interval_seconds = 86_401;
        assert!(invalid_inventory
            .validate()
            .unwrap_err()
            .to_string()
            .contains("inventory"));
    }
}
