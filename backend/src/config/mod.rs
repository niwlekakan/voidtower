use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_file: Option<PathBuf>,
    pub key_file: Option<PathBuf>,
}

fn default_bind() -> String { "127.0.0.1".to_string() }
fn default_port() -> u16 { 8743 }
fn default_agent_port() -> u16 { 8744 }
fn default_status_port() -> u16 { 8745 }
fn default_data_dir() -> PathBuf { PathBuf::from("/var/lib/voidtower") }
fn default_config_dir() -> PathBuf { PathBuf::from("/etc/voidtower") }
fn default_frontend_dir() -> PathBuf { PathBuf::from("/usr/share/voidtower/frontend") }
fn default_catalog_dir() -> PathBuf { PathBuf::from("/usr/share/voidtower/apps") }
fn default_log_level() -> String { "info".to_string() }

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
        if let Ok(v) = std::env::var("VOIDTOWER_BIND") { config.bind = v; }
        if let Ok(v) = std::env::var("VOIDTOWER_PORT") {
            if let Ok(p) = v.parse() { config.port = p; }
        }
        if let Ok(v) = std::env::var("VOIDTOWER_DATA_DIR") { config.data_dir = PathBuf::from(v); }
        let host_data_dir_override = std::env::var("VOIDTOWER_HOST_DATA_DIR").ok().map(PathBuf::from);
        if let Ok(v) = std::env::var("VOIDTOWER_CONFIG_DIR") { config.config_dir = PathBuf::from(v); }
        if let Ok(v) = std::env::var("VOIDTOWER_CATALOG_DIR") { config.catalog_dir = PathBuf::from(v); }
        if let Ok(v) = std::env::var("VOIDTOWER_FRONTEND_DIR") { config.frontend_dir = PathBuf::from(v); }
        if let Ok(v) = std::env::var("RUST_LOG") { config.log_level = v; }

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

    pub fn db_path(&self) -> PathBuf { self.data_dir.join("voidtower.db") }
    pub fn bootstrap_token_path(&self) -> PathBuf { self.config_dir.join("bootstrap-token") }
    pub fn apps_dir(&self) -> PathBuf { self.data_dir.join("apps") }
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
        }
    }
}
