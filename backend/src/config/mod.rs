use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_agent_port")]
    pub agent_port: u16,
    #[serde(default = "default_status_port")]
    pub status_port: u16,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Host filesystem path that `data_dir` is bind-mounted from, as seen by
    /// the Docker daemon VoidTower talks to over `/var/run/docker.sock`.
    ///
    /// On bare-metal installs VoidTower runs directly on the host, so this is
    /// the same as `data_dir`. When VoidTower itself runs containerized
    /// (e.g. the TrueNAS SCALE Custom App, where `data_dir` is
    /// `/var/lib/voidtower` inside the container but the real data lives at
    /// `/mnt/<pool>/voidtower/data` on the host), compose files VoidTower
    /// writes must use *this* path for bind-mount sources under `data_dir` —
    /// otherwise the host daemon resolves `/var/lib/voidtower/...` against
    /// its own root filesystem instead of the mounted dataset. See
    /// `rewrite_host_bind_mounts` in `api/apps.rs`.
    #[serde(default = "default_data_dir")]
    pub host_data_dir: PathBuf,
    #[serde(default = "default_config_dir")]
    pub config_dir: PathBuf,
    #[serde(default = "default_frontend_dir")]
    pub frontend_dir: PathBuf,
    #[serde(default = "default_catalog_dir")]
    pub catalog_dir: PathBuf,
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub agent_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub operations: OperationRuntimeConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_file: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationRuntimeConfig {
    #[serde(default = "default_operation_worker_count")]
    pub worker_count: usize,
    #[serde(default = "default_operation_lease_seconds")]
    pub lease_seconds: u64,
    #[serde(default = "default_operation_idle_poll_millis")]
    pub idle_poll_millis: u64,
    #[serde(default = "default_operation_reconciliation_poll_seconds")]
    pub reconciliation_poll_seconds: u64,
    #[serde(default = "default_operation_error_backoff_max_seconds")]
    pub error_backoff_max_seconds: u64,
    #[serde(default = "default_operation_shutdown_timeout_seconds")]
    pub shutdown_timeout_seconds: u64,
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8743
}
fn default_agent_port() -> u16 {
    8744
}
fn default_status_port() -> u16 {
    8745
}
fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/voidtower")
}
fn default_config_dir() -> PathBuf {
    PathBuf::from("/etc/voidtower")
}
fn default_frontend_dir() -> PathBuf {
    PathBuf::from("/usr/share/voidtower/frontend")
}
fn default_catalog_dir() -> PathBuf {
    PathBuf::from("/usr/share/voidtower/apps")
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_operation_worker_count() -> usize {
    2
}
fn default_operation_lease_seconds() -> u64 {
    30
}
fn default_operation_idle_poll_millis() -> u64 {
    500
}
fn default_operation_reconciliation_poll_seconds() -> u64 {
    30
}
fn default_operation_error_backoff_max_seconds() -> u64 {
    5
}
fn default_operation_shutdown_timeout_seconds() -> u64 {
    60
}

impl OperationRuntimeConfig {
    pub fn validate(&self) -> Result<()> {
        validate_range("operations.worker_count", self.worker_count, 1, 32)?;
        validate_range("operations.lease_seconds", self.lease_seconds, 6, 3600)?;
        validate_range(
            "operations.idle_poll_millis",
            self.idle_poll_millis,
            50,
            60_000,
        )?;
        validate_range(
            "operations.reconciliation_poll_seconds",
            self.reconciliation_poll_seconds,
            1,
            3600,
        )?;
        validate_range(
            "operations.error_backoff_max_seconds",
            self.error_backoff_max_seconds,
            1,
            300,
        )?;
        validate_range(
            "operations.shutdown_timeout_seconds",
            self.shutdown_timeout_seconds,
            1,
            3600,
        )?;
        let maximum_backoff_millis = self
            .error_backoff_max_seconds
            .checked_mul(1000)
            .context("operations.error_backoff_max_seconds overflows milliseconds")?;
        if maximum_backoff_millis < self.idle_poll_millis {
            bail!("operations.error_backoff_max_seconds must cover operations.idle_poll_millis");
        }
        Ok(())
    }

    pub fn lease_renew_interval(&self) -> Duration {
        Duration::from_secs((self.lease_seconds / 3).max(1))
    }

    pub fn idle_poll_interval(&self) -> Duration {
        Duration::from_millis(self.idle_poll_millis)
    }

    pub fn reconciliation_poll_interval(&self) -> Duration {
        Duration::from_secs(self.reconciliation_poll_seconds)
    }

    pub fn maximum_error_backoff(&self) -> Duration {
        Duration::from_secs(self.error_backoff_max_seconds)
    }

    pub fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_seconds)
    }
}

impl Default for OperationRuntimeConfig {
    fn default() -> Self {
        Self {
            worker_count: default_operation_worker_count(),
            lease_seconds: default_operation_lease_seconds(),
            idle_poll_millis: default_operation_idle_poll_millis(),
            reconciliation_poll_seconds: default_operation_reconciliation_poll_seconds(),
            error_backoff_max_seconds: default_operation_error_backoff_max_seconds(),
            shutdown_timeout_seconds: default_operation_shutdown_timeout_seconds(),
        }
    }
}

fn validate_range<T>(field: &str, value: T, minimum: T, maximum: T) -> Result<()>
where
    T: Copy + PartialOrd + std::fmt::Display,
{
    if value < minimum || value > maximum {
        bail!("{field} must be between {minimum} and {maximum}, got {value}");
    }
    Ok(())
}

fn environment_value(name: &str) -> Result<Option<String>> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("{name} must contain valid Unicode")
        }
    }
}

fn parse_environment_value<T>(name: &str, value: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value
        .parse()
        .map_err(|error| anyhow::anyhow!("{name} has an invalid value: {error}"))
}

fn apply_operation_environment(config: &mut OperationRuntimeConfig) -> Result<()> {
    apply_operation_environment_with(config, environment_value)
}

fn apply_operation_environment_with(
    config: &mut OperationRuntimeConfig,
    mut lookup: impl FnMut(&str) -> Result<Option<String>>,
) -> Result<()> {
    if let Some(value) = lookup("VOIDTOWER_OPERATIONS_WORKER_COUNT")? {
        config.worker_count = parse_environment_value("VOIDTOWER_OPERATIONS_WORKER_COUNT", &value)?;
    }
    if let Some(value) = lookup("VOIDTOWER_OPERATIONS_LEASE_SECONDS")? {
        config.lease_seconds =
            parse_environment_value("VOIDTOWER_OPERATIONS_LEASE_SECONDS", &value)?;
    }
    if let Some(value) = lookup("VOIDTOWER_OPERATIONS_IDLE_POLL_MILLIS")? {
        config.idle_poll_millis =
            parse_environment_value("VOIDTOWER_OPERATIONS_IDLE_POLL_MILLIS", &value)?;
    }
    if let Some(value) = lookup("VOIDTOWER_OPERATIONS_RECONCILIATION_POLL_SECONDS")? {
        config.reconciliation_poll_seconds =
            parse_environment_value("VOIDTOWER_OPERATIONS_RECONCILIATION_POLL_SECONDS", &value)?;
    }
    if let Some(value) = lookup("VOIDTOWER_OPERATIONS_ERROR_BACKOFF_MAX_SECONDS")? {
        config.error_backoff_max_seconds =
            parse_environment_value("VOIDTOWER_OPERATIONS_ERROR_BACKOFF_MAX_SECONDS", &value)?;
    }
    if let Some(value) = lookup("VOIDTOWER_OPERATIONS_SHUTDOWN_TIMEOUT_SECONDS")? {
        config.shutdown_timeout_seconds =
            parse_environment_value("VOIDTOWER_OPERATIONS_SHUTDOWN_TIMEOUT_SECONDS", &value)?;
    }
    Ok(())
}

impl Config {
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let mut config = if let Some(path) = config_path {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read config: {}", path.display()))?;
                toml::from_str(&content).with_context(|| "Failed to parse config TOML")?
            } else {
                Config::default()
            }
        } else {
            let default_path = default_config_dir().join("config.toml");
            if default_path.exists() {
                let content = std::fs::read_to_string(&default_path)?;
                toml::from_str(&content)?
            } else {
                Config::default()
            }
        };

        // Environment overrides
        if let Ok(v) = std::env::var("VOIDTOWER_BIND") {
            config.bind = v;
        }
        if let Ok(v) = std::env::var("VOIDTOWER_PORT") {
            if let Ok(p) = v.parse() {
                config.port = p;
            }
        }
        if let Ok(v) = std::env::var("VOIDTOWER_DATA_DIR") {
            config.data_dir = PathBuf::from(v);
        }
        let host_data_dir_override = std::env::var("VOIDTOWER_HOST_DATA_DIR")
            .ok()
            .map(PathBuf::from);
        if let Ok(v) = std::env::var("VOIDTOWER_CONFIG_DIR") {
            config.config_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("VOIDTOWER_CATALOG_DIR") {
            config.catalog_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("VOIDTOWER_FRONTEND_DIR") {
            config.frontend_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("RUST_LOG") {
            config.log_level = v;
        }
        apply_operation_environment(&mut config.operations)?;
        config.operations.validate()?;

        // Resolve relative paths to absolute so stored paths (compose files, etc.)
        // work regardless of where the user's shell is when they run a command.
        let cwd = std::env::current_dir().unwrap_or_default();
        if config.data_dir.is_relative() {
            config.data_dir = cwd.join(&config.data_dir);
        }
        if config.config_dir.is_relative() {
            config.config_dir = cwd.join(&config.config_dir);
        }
        if config.catalog_dir.is_relative() {
            config.catalog_dir = cwd.join(&config.catalog_dir);
        }
        if config.frontend_dir.is_relative() {
            config.frontend_dir = cwd.join(&config.frontend_dir);
        }

        // host_data_dir defaults to the (now-resolved) data_dir for bare-metal
        // installs, where VoidTower runs directly on the host and the two are
        // identical. VOIDTOWER_HOST_DATA_DIR overrides this for containerized
        // installs (e.g. TrueNAS SCALE) — see the field doc comment.
        config.host_data_dir = match host_data_dir_override {
            Some(mut p) => {
                if p.is_relative() {
                    p = cwd.join(&p);
                }
                p
            }
            None => config.data_dir.clone(),
        };

        Ok(config)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("voidtower.db")
    }
    pub fn bootstrap_token_path(&self) -> PathBuf {
        self.config_dir.join("bootstrap-token")
    }
    pub fn apps_dir(&self) -> PathBuf {
        self.data_dir.join("apps")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            port: default_port(),
            agent_port: default_agent_port(),
            status_port: default_status_port(),
            data_dir: default_data_dir(),
            host_data_dir: default_data_dir(),
            config_dir: default_config_dir(),
            frontend_dir: default_frontend_dir(),
            catalog_dir: default_catalog_dir(),
            tls: TlsConfig::default(),
            agent_mode: false,
            log_level: default_log_level(),
            operations: OperationRuntimeConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_runtime_defaults_are_safe_and_valid() {
        let config = OperationRuntimeConfig::default();
        config.validate().unwrap();
        assert_eq!(config.worker_count, 2);
        assert_eq!(config.lease_renew_interval(), Duration::from_secs(10));
        assert_eq!(config.shutdown_timeout(), Duration::from_secs(60));
    }

    #[test]
    fn toml_can_override_operation_runtime_settings() {
        let config: Config = toml::from_str(
            r#"
                [operations]
                worker_count = 4
                lease_seconds = 60
                idle_poll_millis = 250
                reconciliation_poll_seconds = 15
                error_backoff_max_seconds = 3
                shutdown_timeout_seconds = 90
            "#,
        )
        .unwrap();
        config.operations.validate().unwrap();
        assert_eq!(
            config.operations,
            OperationRuntimeConfig {
                worker_count: 4,
                lease_seconds: 60,
                idle_poll_millis: 250,
                reconciliation_poll_seconds: 15,
                error_backoff_max_seconds: 3,
                shutdown_timeout_seconds: 90,
            }
        );
    }

    #[test]
    fn environment_values_are_strictly_parsed() {
        assert_eq!(
            parse_environment_value::<usize>("VOIDTOWER_OPERATIONS_WORKER_COUNT", "7").unwrap(),
            7
        );
        let error = parse_environment_value::<usize>("VOIDTOWER_OPERATIONS_WORKER_COUNT", "many")
            .unwrap_err()
            .to_string();
        assert!(error.contains("VOIDTOWER_OPERATIONS_WORKER_COUNT"));
    }

    #[test]
    fn every_operation_environment_override_is_applied() {
        let values = std::collections::HashMap::from([
            ("VOIDTOWER_OPERATIONS_WORKER_COUNT", "4"),
            ("VOIDTOWER_OPERATIONS_LEASE_SECONDS", "60"),
            ("VOIDTOWER_OPERATIONS_IDLE_POLL_MILLIS", "250"),
            ("VOIDTOWER_OPERATIONS_RECONCILIATION_POLL_SECONDS", "15"),
            ("VOIDTOWER_OPERATIONS_ERROR_BACKOFF_MAX_SECONDS", "3"),
            ("VOIDTOWER_OPERATIONS_SHUTDOWN_TIMEOUT_SECONDS", "90"),
        ]);
        let mut config = OperationRuntimeConfig::default();
        apply_operation_environment_with(&mut config, |name| {
            Ok(values.get(name).map(|value| (*value).to_string()))
        })
        .unwrap();
        config.validate().unwrap();
        assert_eq!(
            config,
            OperationRuntimeConfig {
                worker_count: 4,
                lease_seconds: 60,
                idle_poll_millis: 250,
                reconciliation_poll_seconds: 15,
                error_backoff_max_seconds: 3,
                shutdown_timeout_seconds: 90,
            }
        );
    }

    #[test]
    fn operation_runtime_rejects_out_of_range_values() {
        for (field, config) in [
            (
                "operations.worker_count",
                OperationRuntimeConfig {
                    worker_count: 0,
                    ..OperationRuntimeConfig::default()
                },
            ),
            (
                "operations.lease_seconds",
                OperationRuntimeConfig {
                    lease_seconds: 5,
                    ..OperationRuntimeConfig::default()
                },
            ),
            (
                "operations.reconciliation_poll_seconds",
                OperationRuntimeConfig {
                    reconciliation_poll_seconds: 0,
                    ..OperationRuntimeConfig::default()
                },
            ),
            (
                "operations.shutdown_timeout_seconds",
                OperationRuntimeConfig {
                    shutdown_timeout_seconds: 3601,
                    ..OperationRuntimeConfig::default()
                },
            ),
        ] {
            let error = config.validate().unwrap_err().to_string();
            assert!(error.contains(field), "unexpected error: {error}");
        }
    }

    #[test]
    fn maximum_error_backoff_must_cover_idle_interval() {
        let config = OperationRuntimeConfig {
            idle_poll_millis: 2_000,
            error_backoff_max_seconds: 1,
            ..OperationRuntimeConfig::default()
        };
        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("operations.error_backoff_max_seconds"));
        assert!(error.contains("operations.idle_poll_millis"));
    }
}
