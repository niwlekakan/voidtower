#![recursion_limit = "256"]

mod agent;
mod ai;
mod alerts;
mod api;
mod audit;
mod auth;
mod automation;
mod backups;
mod cluster;
pub mod cmdb;
pub mod collector;
mod config;
mod containers;
mod db;
mod error;
mod household;
mod monitoring;
mod networking;
mod oidc;
mod operations;
mod policy;
mod security;
mod services;
mod storage;
mod terminal;
mod updates;
mod vms;
mod voidwatch;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use monitoring::{MetricsBroadcaster, MetricsCollector, MetricsSnapshot};
use sqlx::{AssertSqlSafe, SqlitePool};
use std::{collections::HashMap, io::Read, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

#[derive(Debug)]
pub struct LoginAttempts {
    pub count: u32,
    pub window_start: std::time::Instant,
    pub locked_until: Option<std::time::Instant>,
}
use tower_http::services::{ServeDir, ServeFile};

/// token_hash -> stable non-secret token/session context — avoids a DB write and scopes lookup on
/// every Bearer request without collapsing token identity into its compatibility session.
pub type TokenSessionCache = HashMap<String, api::bearer_auth::CachedTokenSession>;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<config::Config>,
    pub metrics_tx: MetricsBroadcaster,
    pub latest_metrics: Arc<RwLock<Option<MetricsSnapshot>>>,
    pub agents_tx: api::agents::AgentBroadcaster,
    pub secrets_key: Arc<[u8; 32]>,
    pub token_sessions: Arc<RwLock<TokenSessionCache>>,
    /// Serializes token cache resolution with revocation invalidation. The guard spans the
    /// database validation and cache insertion so a revoke cannot race a cache miss.
    pub token_session_lock: Arc<tokio::sync::Mutex<()>>,
    /// Serializes compatibility adoption through the durable idempotency boundary. The guard
    /// spans the pre-observation lookup and final durable submission so concurrent first requests
    /// cannot observe providers or register resources before the winning idempotency record exists.
    pub compatibility_idempotency_lock: Arc<tokio::sync::Mutex<()>>,
    pub login_limiter: Arc<std::sync::Mutex<HashMap<std::net::IpAddr, LoginAttempts>>>,
    pub deploy_registry: containers::DeployRegistry,
    pub operation_adapters: Arc<operations::adapters::AdapterRegistry>,
    pub operation_runtime: operations::runtime::RuntimeControl,
    pub service_provider: Arc<dyn operations::adapters::services::ServiceProvider>,
}

#[derive(Parser, Debug)]
#[command(
    name = "voidtower",
    version,
    about = "Self-hosted infrastructure command tower"
)]
struct Cli {
    /// Bind address
    #[arg(long, env = "VOIDTOWER_BIND")]
    bind: Option<String>,

    /// HTTP port
    #[arg(long, env = "VOIDTOWER_PORT")]
    port: Option<u16>,

    /// Config file path
    #[arg(long, short = 'c')]
    config: Option<PathBuf>,

    /// Run in agent mode (outbound heartbeat only; no controller services)
    #[arg(long)]
    agent: bool,

    /// Persistent agent state path (used with --agent)
    #[arg(long, default_value = "/var/lib/voidtower/agent/state.json")]
    agent_state: PathBuf,

    /// Disable TLS even if configured
    #[arg(long)]
    no_tls: bool,

    /// Print the bootstrap token and exit
    #[arg(long)]
    show_token: bool,

    /// Run diagnostics and exit
    #[arg(long)]
    doctor: bool,

    /// Output diagnostics as JSON (use with --doctor)
    #[arg(long)]
    json: bool,

    /// Reset this node's identity (dangerous)
    #[arg(long)]
    reset_node: bool,

    /// Export config to file (or stdout if path omitted) and exit
    #[arg(long, value_name = "OUTPUT_PATH")]
    export_config: Option<Option<String>>,

    /// Import config from file and exit
    #[arg(long, value_name = "INPUT_PATH")]
    import_config: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Manage the outbound inventory agent
    Agent {
        #[command(subcommand)]
        action: AgentCommand,
    },
    /// Manage user accounts
    User {
        #[command(subcommand)]
        action: UserCommand,
    },
    /// Manage backup jobs
    Backup {
        #[command(subcommand)]
        action: BackupCommand,
    },
    /// Submit automation mutations through the canonical operation boundary
    Automation {
        #[command(subcommand)]
        action: AutomationCommand,
    },
}

#[derive(Subcommand, Debug)]
enum AgentCommand {
    /// Enroll this outbound agent with a VoidTower controller
    Enroll {
        #[arg(long)]
        server_url: String,
        #[arg(
            long,
            allow_hyphen_values = true,
            conflicts_with = "pairing_code_stdin",
            required_unless_present = "pairing_code_stdin"
        )]
        pairing_code: Option<PairingCode>,
        /// Read the one-time pairing code from stdin instead of exposing it in argv.
        #[arg(long, conflicts_with = "pairing_code")]
        pairing_code_stdin: bool,
        #[arg(long)]
        display_name: String,
        #[arg(long, default_value = "other")]
        device_type: String,
        #[arg(long)]
        ca_path: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        provision_wireguard: bool,
        #[arg(long, default_value = "/var/lib/voidtower/agent/state.json")]
        state_path: PathBuf,
    },
    /// Validate the local agent state file without contacting the controller
    CheckState {
        #[arg(long, default_value = "/var/lib/voidtower/agent/state.json")]
        state_path: PathBuf,
    },
}

#[derive(Clone)]
struct PairingCode(String);

impl std::str::FromStr for PairingCode {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        // Credential validation happens after Clap parsing so Clap never echoes an
        // invalid pairing code as part of a value-parser diagnostic.
        Ok(Self(value.to_string()))
    }
}

impl std::fmt::Debug for PairingCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

impl PairingCode {
    fn into_inner(self) -> String {
        self.0
    }
}

const MAX_PAIRING_CODE_BYTES: usize = 512;

fn read_pairing_code_from_reader<R: Read>(reader: &mut R) -> Result<String> {
    let mut bytes = Vec::with_capacity(MAX_PAIRING_CODE_BYTES + 1);
    let mut byte = [0_u8; 1];
    loop {
        let count = reader
            .read(&mut byte)
            .context("failed to read pairing code from stdin")?;
        if count == 0 {
            anyhow::bail!("pairing code from stdin must end with a newline");
        }
        if byte[0] == b'\n' {
            break;
        }
        if byte[0] == b'\r' {
            let mut line_feed = [0_u8; 1];
            let count = reader
                .read(&mut line_feed)
                .context("failed to read pairing code from stdin")?;
            if count == 0 || line_feed[0] != b'\n' {
                anyhow::bail!("pairing code from stdin must end with a newline");
            }
            break;
        }
        bytes.push(byte[0]);
        if bytes.len() > MAX_PAIRING_CODE_BYTES {
            anyhow::bail!("pairing code from stdin exceeds {MAX_PAIRING_CODE_BYTES} bytes");
        }
    }

    let value = String::from_utf8(bytes).context("pairing code from stdin must be UTF-8")?;
    if value.contains(['\r', '\n']) {
        anyhow::bail!("pairing code from stdin must be a single line");
    }
    if value.is_empty() || value.len() > MAX_PAIRING_CODE_BYTES {
        anyhow::bail!("pairing code from stdin exceeds {MAX_PAIRING_CODE_BYTES} bytes");
    }
    Ok(value.to_string())
}

fn read_pairing_code_from_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    read_pairing_code_from_reader(&mut stdin.lock())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupPath {
    AgentEnrollment,
    AgentRuntime,
    Controller,
}

fn startup_path(cli: &Cli) -> StartupPath {
    if matches!(cli.command, Some(Command::Agent { .. })) {
        StartupPath::AgentEnrollment
    } else if cli.agent {
        StartupPath::AgentRuntime
    } else {
        StartupPath::Controller
    }
}

#[derive(Subcommand, Debug)]
enum UserCommand {
    /// List all users
    List,
    /// Create a new user
    Create {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
        /// owner | admin | operator | viewer
        #[arg(long, default_value = "operator")]
        role: String,
    },
    /// Reset a user's password (forces password change on next login)
    ResetPassword {
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// Change a user's role
    SetRole {
        #[arg(long)]
        username: String,
        /// owner | admin | operator | viewer
        #[arg(long)]
        role: String,
    },
    /// Delete a user
    Delete {
        #[arg(long)]
        username: String,
    },
}

#[derive(Subcommand, Debug)]
enum BackupCommand {
    /// List configured backup jobs
    List,
    /// Create a new backup job
    Create {
        #[arg(long)]
        name: String,
        #[arg(long)]
        source: String,
        #[arg(long)]
        repo: String,
        #[arg(long)]
        retention_days: Option<i64>,
    },
    /// Run a backup job now
    Run {
        #[arg(long)]
        name: String,
    },
    /// Verify repo integrity (restic check)
    Check {
        #[arg(long)]
        name: String,
    },
    /// Dry-run restore test
    RestoreTest {
        #[arg(long)]
        name: String,
    },
    /// Delete a backup job's config (does not delete data on disk)
    Delete {
        #[arg(long)]
        name: String,
    },
}

#[derive(Subcommand, Debug)]
enum AutomationCommand {
    /// Submit an enabled automation job for durable execution
    Run {
        /// Automation job identifier (resolved to resources.id by the server)
        #[arg(long)]
        id: String,
        /// Caller-provided stable request identifier used for idempotency
        #[arg(long)]
        request_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut cli = Cli::parse();

    match startup_path(&cli) {
        StartupPath::AgentEnrollment => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            let Some(Command::Agent { action }) = cli.command.take() else {
                unreachable!("startup path guarantees an agent command");
            };
            match action {
                AgentCommand::CheckState { state_path } => {
                    agent::check_state(&state_path)?;
                    return Ok(());
                }
                AgentCommand::Enroll {
                    server_url,
                    pairing_code,
                    pairing_code_stdin,
                    display_name,
                    device_type,
                    ca_path,
                    provision_wireguard,
                    state_path,
                } => {
                    let pairing_code = match (pairing_code, pairing_code_stdin) {
                        (Some(pairing_code), false) => pairing_code.into_inner(),
                        (None, true) => read_pairing_code_from_stdin()?,
                        _ => unreachable!("Clap enforces exactly one pairing-code source"),
                    };
                    agent::enroll(agent::EnrollmentOptions {
                        server_url,
                        pairing_code,
                        display_name,
                        device_type,
                        ca_path,
                        provision_wireguard,
                        state_path,
                    })
                    .await?;
                    return Ok(());
                }
            }
        }
        StartupPath::AgentRuntime => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            agent::run(cli.agent_state.clone()).await?;
            return Ok(());
        }
        StartupPath::Controller => {}
    }

    let mut cfg = config::Config::load(cli.config.as_deref())?;
    if let Some(b) = cli.bind {
        cfg.bind = b;
    }
    if let Some(p) = cli.port {
        cfg.port = p;
    }

    tracing_subscriber::fmt()
        .with_env_filter(&cfg.log_level)
        .init();

    if cli.doctor {
        run_doctor(&cfg, cli.json);
        return Ok(());
    }

    // --export-config / --import-config: require DB but skip web server
    if cli.export_config.is_some() || cli.import_config.is_some() {
        std::fs::create_dir_all(&cfg.data_dir)?;
        let pool = db::init_pool(&cfg.db_path()).await?;

        // Load or generate secrets encryption key only for exports; imports do
        // not resolve secret values and preserve their existing behavior.
        if let Some(out_arg) = cli.export_config {
            let secrets_key = load_or_create_secrets_key(&cfg)?;
            let path_opt = out_arg.as_deref();
            api::disaster::cli_export(&pool, &secrets_key, path_opt).await?;
        } else if let Some(input_path) = cli.import_config {
            api::disaster::cli_import(&pool, &input_path).await?;
        }
        return Ok(());
    }

    // Management subcommands (user / backup) — require DB but skip web server
    if let Some(command) = cli.command {
        std::fs::create_dir_all(&cfg.data_dir)?;
        let pool = db::init_pool(&cfg.db_path()).await?;
        run_command(&pool, &cfg, command).await?;
        return Ok(());
    }

    // Ensure directories exist
    std::fs::create_dir_all(&cfg.data_dir)?;
    std::fs::create_dir_all(cfg.apps_dir())?;
    if std::path::Path::new("/etc").exists() {
        let _ = std::fs::create_dir_all(&cfg.config_dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&cfg.config_dir, std::fs::Permissions::from_mode(0o700));
        }
    }

    // Load or generate secrets encryption key for normal controller startup.
    let secrets_key = load_or_create_secrets_key(&cfg)?;

    // Bootstrap token
    let bootstrap_result = auth::ensure_bootstrap_token(&cfg.bootstrap_token_path()).await?;
    if let Some(token) = &bootstrap_result {
        tracing::info!("╔══════════════════════════════════════════════════╗");
        tracing::info!("║          VoidTower first-run bootstrap           ║");
        tracing::info!("║                                                  ║");
        tracing::info!("║  Bootstrap token: {:<31}║", token);
        tracing::info!(
            "║  Visit http://{}:{} to complete setup   ║",
            cfg.bind,
            cfg.port
        );
        tracing::info!("╚══════════════════════════════════════════════════╝");
    }

    if cli.show_token {
        match auth::read_bootstrap_token(&cfg.bootstrap_token_path()).await? {
            Some(t) => println!("{}", t),
            None => println!("Bootstrap token not found (setup may already be complete)"),
        }
        return Ok(());
    }

    // Database
    let pool = db::init_pool(&cfg.db_path()).await?;
    api::secrets::migrate_legacy_provider_secrets(&pool, &secrets_key).await?;
    api::secrets::migrate_legacy_proxmox_token(&pool, &secrets_key).await?;
    api::secrets::migrate_legacy_ssh_session_passwords(&pool, &secrets_key).await?;
    api::secrets::migrate_legacy_webhook_secret(&pool, &secrets_key).await?;
    api::secrets::migrate_legacy_notification_credentials(&pool, &secrets_key).await?;
    // The legacy migrations delete plaintext rows inside a transaction, which
    // leaves the freed cell content in the database file. Rebuild the file once
    // before serving so no deleted credential can be read back from free pages.
    // A rebuild that cannot complete (busy checkpoint, disk pressure) is not
    // marked as done and is retried on the next start: it must not stop an
    // otherwise healthy controller from serving.
    match db::scrub_legacy_plaintext_residue(&pool, &cfg.db_path()).await {
        Ok(true) => tracing::info!("rebuilt database file to drop legacy plaintext residue"),
        Ok(false) => {}
        Err(error) => tracing::warn!(
            error = %error,
            "plaintext residue rebuild deferred to the next startup"
        ),
    }
    operations::registry::validate()?;

    // Re-log bootstrap token on every restart while setup is still pending.
    // Covers Docker deployments where the first-run log line was missed.
    if bootstrap_result.is_none() {
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool)
            .await
            .unwrap_or(0);
        if user_count == 0 {
            if let Some(token) = auth::read_bootstrap_token(&cfg.bootstrap_token_path()).await? {
                tracing::info!("╔══════════════════════════════════════════════════╗");
                tracing::info!("║        VoidTower setup not yet complete          ║");
                tracing::info!("║                                                  ║");
                tracing::info!("║  Bootstrap token: {:<31}║", token);
                tracing::info!(
                    "║  Visit http://{}:{} to complete setup   ║",
                    cfg.bind,
                    cfg.port
                );
                tracing::info!("╚══════════════════════════════════════════════════╝");
            }
        }
    }

    // Metrics broadcaster
    let (metrics_tx, _) = broadcast::channel::<MetricsSnapshot>(16);
    let latest_metrics: Arc<RwLock<Option<MetricsSnapshot>>> = Arc::new(RwLock::new(None));

    // Agent status broadcaster
    let (agents_tx, _) = broadcast::channel::<api::agents::AgentStatusUpdate>(64);

    let operation_adapters = Arc::new(operations::adapters::AdapterRegistry::staged(
        pool.clone(),
        secrets_key.clone(),
        cfg.data_dir.clone(),
    )?);
    let prepared_operation_runtime = operations::runtime::prepare(
        pool.clone(),
        operation_adapters.clone(),
        cfg.operations.clone(),
    )
    .await?;
    let state = AppState {
        db: pool.clone(),
        config: Arc::new(cfg.clone()),
        metrics_tx: metrics_tx.clone(),
        latest_metrics: latest_metrics.clone(),
        agents_tx: agents_tx.clone(),
        secrets_key,
        token_sessions: Arc::new(RwLock::new(HashMap::new())),
        token_session_lock: Arc::new(tokio::sync::Mutex::new(())),
        compatibility_idempotency_lock: Arc::new(tokio::sync::Mutex::new(())),
        login_limiter: Arc::new(std::sync::Mutex::new(HashMap::new())),
        deploy_registry: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        operation_adapters,
        operation_runtime: prepared_operation_runtime.control(),
        service_provider: Arc::new(operations::adapters::services::SystemdServiceProvider),
    };

    // Spawn agent status heartbeat loop (marks stale agents offline)
    tokio::spawn(api::agents::run_heartbeat_loop(state.clone()));

    // Spawn metrics collector
    let collector = MetricsCollector::new(metrics_tx.clone());
    let latest_clone = latest_metrics.clone();
    let mut rx_latest = metrics_tx.subscribe();
    tokio::spawn(async move {
        while let Ok(snapshot) = rx_latest.recv().await {
            *latest_clone.write().await = Some(snapshot);
        }
    });
    tokio::spawn(collector.run_loop());

    // Spawn threshold-based alert generator (checks every 60s)
    let alert_pool = pool.clone();
    let mut rx_alerts = metrics_tx.subscribe();
    tokio::spawn(async move {
        let mut last_check = std::time::Instant::now();
        while let Ok(snap) = rx_alerts.recv().await {
            if last_check.elapsed().as_secs() < 60 {
                continue;
            }
            last_check = std::time::Instant::now();

            let cpu = snap.cpu_usage;
            let ram_pct = if snap.ram_total > 0 {
                (snap.ram_used as f32 / snap.ram_total as f32) * 100.0
            } else {
                0.0
            };

            if cpu > 95.0 {
                api::alerts::create_alert(
                    &alert_pool,
                    "High CPU Usage",
                    &format!("CPU usage is {:.1}% — above 95% threshold", cpu),
                    "critical",
                    "system",
                    None,
                    None,
                )
                .await;
            } else if cpu > 85.0 {
                api::alerts::create_alert(
                    &alert_pool,
                    "Elevated CPU Usage",
                    &format!("CPU usage is {:.1}% — above 85% threshold", cpu),
                    "warning",
                    "system",
                    None,
                    None,
                )
                .await;
            }

            if ram_pct > 92.0 {
                api::alerts::create_alert(
                    &alert_pool,
                    "High Memory Usage",
                    &format!("RAM usage is {:.1}% — above 92% threshold", ram_pct),
                    "critical",
                    "system",
                    None,
                    None,
                )
                .await;
            } else if ram_pct > 80.0 {
                api::alerts::create_alert(
                    &alert_pool,
                    "Elevated Memory Usage",
                    &format!("RAM usage is {:.1}% — above 80% threshold", ram_pct),
                    "warning",
                    "system",
                    None,
                    None,
                )
                .await;
            }

            for disk in &snap.disks {
                if disk.total == 0 {
                    continue;
                }
                let pct = (disk.used as f32 / disk.total as f32) * 100.0;
                if pct > 92.0 {
                    api::alerts::create_alert(
                        &alert_pool,
                        &format!("Disk Almost Full: {}", disk.mount_point),
                        &format!("{} is {:.1}% full", disk.mount_point, pct),
                        "critical",
                        "storage",
                        Some("disk"),
                        Some(&disk.mount_point),
                    )
                    .await;
                } else if pct > 80.0 {
                    api::alerts::create_alert(
                        &alert_pool,
                        &format!("Disk Usage High: {}", disk.mount_point),
                        &format!("{} is {:.1}% full", disk.mount_point, pct),
                        "warning",
                        "storage",
                        Some("disk"),
                        Some(&disk.mount_point),
                    )
                    .await;
                }
            }
        }
    });

    // Spawn status-check scheduler
    let sc_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            let Ok(checks) = sqlx::query_as::<_, api::status::StatusCheck>(
                "SELECT id, name, type, target, interval_secs, enabled, last_checked_at, last_status, last_latency_ms, created_at FROM status_checks WHERE enabled = 1"
            ).fetch_all(&sc_pool).await else { continue };

            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            for check in checks {
                let due = check
                    .last_checked_at
                    .map(|t| now_ts - t >= check.interval_secs)
                    .unwrap_or(true);
                if due {
                    let pool2 = sc_pool.clone();
                    let c = check.clone();
                    tokio::spawn(async move { api::status::run_check(&pool2, &c).await });
                }
            }
        }
    });

    // Spawn automation job scheduler (checks every 60s)
    let auto_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            api::automation::run_scheduled_jobs(&auto_state).await;
        }
    });

    // Item #10A: scheduled restore-test runner (checks every 60s against cron expressions)
    let rt_state = state.clone();
    tokio::spawn(async move {
        const ACTION: &str = "backup.restore_test";
        let context = operations::invocation::InvocationContext::Scheduler;
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(error) = operations::backup_adoption::authorize(&context, ACTION) {
                tracing::warn!(
                    error_code = "backup_restore_test_scheduler_authorization_failed",
                    error = %error
                );
                continue;
            }
            if !backups::is_restic_available() {
                continue;
            }
            let Ok(cfgs) = sqlx::query_as::<_, backups::BackupConfig>(
                AssertSqlSafe(format!("SELECT {} FROM backup_configs WHERE enabled = 1 AND restore_test_schedule IS NOT NULL",
                    "id, name, source_path, repo_path, schedule, retention_days, enabled, last_run_at, last_status, created_at, last_check_at, last_check_status, last_restore_test_at, last_restore_test_status, restore_test_schedule"))
            ).fetch_all(&rt_state.db).await else { continue };

            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            for cfg in cfgs {
                let Some(ref sched) = cfg.restore_test_schedule else {
                    continue;
                };
                if !backups::cron_matches_now(sched) {
                    continue;
                }

                // Avoid re-running if already ran within this minute
                if cfg
                    .last_restore_test_at
                    .map(|t| now_ts - t < 60)
                    .unwrap_or(false)
                {
                    continue;
                }

                let adopted = match operations::backup_adoption::resolve_config_target(
                    &rt_state.db,
                    &context,
                    operations::backup_adoption::BackupSelector::Id(&cfg.id),
                    ACTION,
                )
                .await
                {
                    Ok(adopted) => adopted,
                    Err(error) => {
                        tracing::warn!(
                            error_code = "backup_restore_test_scheduler_target_failed",
                            config_id = %cfg.id,
                            error = %error
                        );
                        continue;
                    }
                };
                let idempotency_key = scheduled_restore_test_idempotency_key(&cfg.id, now_ts);
                match operations::invocation::submit(
                    &rt_state.db,
                    &rt_state.operation_adapters,
                    &context,
                    &adopted.resource.id,
                    ACTION,
                    serde_json::json!({}),
                    &idempotency_key,
                )
                .await
                {
                    Ok(job) => tracing::info!(
                        event_code = "backup_restore_test_scheduled",
                        config_id = %cfg.id,
                        job_id = %job.id,
                        state = job.state.as_str()
                    ),
                    Err(error) => tracing::warn!(
                        error_code = "backup_restore_test_scheduler_submit_failed",
                        config_id = %cfg.id,
                        error = %error
                    ),
                }
            }
        }
    });

    // Item #10C: daily alert for backup jobs never restore-tested (older than 7 days)
    let ntpool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(86400));
        interval.tick().await; // skip first immediate tick
        loop {
            interval.tick().await;
            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let seven_days_ago = now_ts - 7 * 86400;

            let Ok(untested) = sqlx::query_as::<_, (String, String, i64)>(
                "SELECT id, name, created_at FROM backup_configs WHERE enabled = 1 AND last_restore_test_at IS NULL AND created_at < ?"
            ).bind(seven_days_ago).fetch_all(&ntpool).await else { continue };

            for (id, name, _) in untested {
                api::alerts::create_alert(
                    &ntpool,
                    &format!("Backup '{}' never restore-tested", name),
                    &format!("Backup job '{name}' has never been restore-tested"),
                    "warning",
                    "backups",
                    Some("backup_config"),
                    Some(&id),
                )
                .await;
            }
        }
    });

    // Spawn periodic session cleanup
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let _ = auth::delete_expired_sessions(&pool_clone).await;
        }
    });

    // Proxmox VM state-change alert monitor (90s poll)
    let pmon_state = state.clone();
    tokio::spawn(api::proxmox::run_vm_state_monitor(pmon_state));

    // Member storage-quota / assigned-drive usage poll (5 min) — same
    // poll-interval shape as the backup last_check_at scheduler above.
    let member_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
        loop {
            interval.tick().await;
            api::members::poll_member_storage(member_state.clone()).await;
        }
    });

    // Build router
    let api_routes = api::router(state);

    // Serve frontend static files if directory exists
    let app = if cfg.frontend_dir.exists() {
        tracing::info!("Serving frontend from {}", cfg.frontend_dir.display());
        api_routes.fallback_service(
            ServeDir::new(&cfg.frontend_dir)
                .fallback(ServeFile::new(cfg.frontend_dir.join("index.html"))),
        )
    } else {
        tracing::warn!(
            "Frontend directory {} not found — API only mode",
            cfg.frontend_dir.display()
        );
        api_routes
    };

    let addr: SocketAddr = format!("{}:{}", cfg.bind, cfg.port).parse()?;
    let (listener, operation_runtime) =
        bind_then_start(addr, || prepared_operation_runtime.start()).await?;
    tracing::info!("VoidTower listening on http://{}", addr);
    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(operation_runtime.control()))
    .await;
    let clean_operation_shutdown = operation_runtime.shutdown().await;
    if !clean_operation_shutdown {
        tracing::warn!(error_code = "operation_runtime_unclean_shutdown");
    }
    serve_result?;

    Ok(())
}

fn load_or_create_secrets_key(cfg: &config::Config) -> Result<Arc<[u8; 32]>> {
    std::fs::create_dir_all(&cfg.config_dir)?;
    let key_path = cfg.config_dir.join("secrets.key");
    let key_bytes = if key_path.exists() {
        let raw = std::fs::read(&key_path)?;
        anyhow::ensure!(raw.len() == 32, "secrets.key must be exactly 32 bytes");
        let mut key = [0u8; 32];
        key.copy_from_slice(&raw);
        key
    } else {
        use rand::RngCore;
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        std::fs::write(&key_path, key)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::info!(
            "Generated new secrets encryption key at {}",
            key_path.display()
        );
        key
    };
    Ok(Arc::new(key_bytes))
}

async fn bind_then_start<T>(
    addr: SocketAddr,
    start: impl FnOnce() -> T,
) -> std::io::Result<(tokio::net::TcpListener, T)> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    Ok((listener, start()))
}

async fn shutdown_signal(operation_runtime: operations::runtime::RuntimeControl) {
    #[cfg(unix)]
    {
        let mut terminate =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(signal) => signal,
                Err(_) => {
                    tracing::warn!(error_code = "sigterm_handler_registration_failed");
                    let _ = tokio::signal::ctrl_c().await;
                    operation_runtime.cancel();
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
    operation_runtime.cancel();
    tracing::info!(event_code = "operation_runtime_shutdown_requested");
}

fn run_doctor(cfg: &config::Config, as_json: bool) {
    let checks = api::diagnostics::run_all_checks(cfg);

    let fail = checks
        .iter()
        .filter(|c| c.status == api::diagnostics::CheckStatus::Fail)
        .count();
    let warn = checks
        .iter()
        .filter(|c| c.status == api::diagnostics::CheckStatus::Warn)
        .count();
    let overall = if fail > 0 {
        "fail"
    } else if warn > 0 {
        "warn"
    } else {
        "pass"
    };

    if as_json {
        let json_checks: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| {
                let status = match c.status {
                    api::diagnostics::CheckStatus::Pass => "pass",
                    api::diagnostics::CheckStatus::Warn => "warn",
                    api::diagnostics::CheckStatus::Fail => "fail",
                    api::diagnostics::CheckStatus::Info => "info",
                };
                let mut obj = serde_json::json!({
                    "name": c.name,
                    "status": status,
                    "message": c.message,
                });
                if let Some(detail) = &c.detail {
                    obj["detail"] = serde_json::Value::String(detail.clone());
                }
                obj
            })
            .collect();
        let output = serde_json::json!({
            "checks": json_checks,
            "overall": overall,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        println!("VoidTower Doctor");
        println!("================");
        println!("  Config dir:   {}", cfg.config_dir.display());
        println!("  Data dir:     {}", cfg.data_dir.display());
        println!("  Frontend dir: {}", cfg.frontend_dir.display());
        println!("  Catalog dir:  {}", cfg.catalog_dir.display());
        println!("  Bind:         {}:{}", cfg.bind, cfg.port);
        println!();

        let mut last_cat = "";
        for c in &checks {
            if c.category != last_cat {
                println!("  --- {} ---", c.category);
                last_cat = c.category;
            }
            let icon = match c.status {
                api::diagnostics::CheckStatus::Pass => "✓",
                api::diagnostics::CheckStatus::Warn => "⚠",
                api::diagnostics::CheckStatus::Fail => "✗",
                api::diagnostics::CheckStatus::Info => "·",
            };
            println!("  [{}] {}  {}", icon, c.name, c.message);
            if let Some(detail) = &c.detail {
                println!("        → {}", detail);
            }
        }

        println!();
        let pass = checks
            .iter()
            .filter(|c| c.status == api::diagnostics::CheckStatus::Pass)
            .count();
        println!("  Result: {} pass  {} warn  {} fail", pass, warn, fail);
    }

    if fail > 0 {
        std::process::exit(1);
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn scheduled_restore_test_idempotency_key(config_id: &str, now: i64) -> String {
    format!("scheduled-restore-test:{config_id}:{}", now / 60)
}

fn is_valid_role(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "operator" | "viewer")
}

async fn run_command(pool: &SqlitePool, cfg: &config::Config, command: Command) -> Result<()> {
    match command {
        Command::Agent { .. } => {
            anyhow::bail!("agent commands run before controller initialization")
        }
        Command::User { action } => run_user_command(pool, action).await,
        Command::Backup { action } => run_backup_command(pool, cfg, action).await,
        Command::Automation { action } => run_automation_command(pool, cfg, action).await,
    }
}

async fn run_user_command(pool: &SqlitePool, action: UserCommand) -> Result<()> {
    match action {
        UserCommand::List => {
            let users = sqlx::query_as::<_, auth::User>(
                "SELECT id, username, password_hash, role, force_password_change, \
                 totp_enabled, totp_secret, created_at, updated_at FROM users ORDER BY created_at",
            )
            .fetch_all(pool)
            .await?;

            if users.is_empty() {
                println!("No users found.");
            } else {
                println!(
                    "{:<36}  {:<20}  {:<9}  {:<5}  {:<14}",
                    "ID", "USERNAME", "ROLE", "2FA", "MUST CHANGE PW"
                );
                for u in users {
                    println!(
                        "{:<36}  {:<20}  {:<9}  {:<5}  {:<14}",
                        u.id,
                        u.username,
                        u.role,
                        if u.totp_enabled { "yes" } else { "no" },
                        if u.force_password_change { "yes" } else { "no" },
                    );
                }
            }
        }
        UserCommand::Create {
            username,
            password,
            role,
        } => {
            if !is_valid_role(&role) {
                anyhow::bail!(
                    "Invalid role '{role}' — must be one of: owner, admin, operator, viewer"
                );
            }
            if username.len() < 3 || password.len() < 8 {
                anyhow::bail!("Username must be ≥3 chars, password ≥8 chars");
            }
            let user = auth::create_user(pool, &username, &password, &role, true).await?;
            println!(
                "Created user '{}' ({}) with role '{}'",
                user.username, user.id, user.role
            );
        }
        UserCommand::ResetPassword { username, password } => {
            if password.len() < 8 {
                anyhow::bail!("Password must be ≥8 characters");
            }
            let user = auth::find_user_by_username(pool, &username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("User '{username}' not found"))?;
            let hash = auth::hash_password(&password)?;
            sqlx::query(
                "UPDATE users SET password_hash = ?, force_password_change = 1, updated_at = ? WHERE id = ?",
            )
            .bind(&hash)
            .bind(unix_now())
            .bind(&user.id)
            .execute(pool)
            .await?;
            println!("Password reset for '{username}' — they will be required to change it on next login.");
        }
        UserCommand::SetRole { username, role } => {
            if !is_valid_role(&role) {
                anyhow::bail!(
                    "Invalid role '{role}' — must be one of: owner, admin, operator, viewer"
                );
            }
            let user = auth::find_user_by_username(pool, &username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("User '{username}' not found"))?;
            auth::update_user_role(pool, &user.id, &role).await?;
            println!("Set role of '{username}' to '{role}'");
        }
        UserCommand::Delete { username } => {
            let user = auth::find_user_by_username(pool, &username)
                .await?
                .ok_or_else(|| anyhow::anyhow!("User '{username}' not found"))?;
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user.id)
                .execute(pool)
                .await?;
            println!("Deleted user '{username}'");
        }
    }
    Ok(())
}

async fn run_automation_command(
    pool: &SqlitePool,
    cfg: &config::Config,
    action: AutomationCommand,
) -> Result<()> {
    match action {
        AutomationCommand::Run { id, request_id } => {
            run_automation_mutation(pool, cfg, &id, &request_id).await
        }
    }
}

fn cli_automation_idempotency_key(request_id: &str) -> Result<String> {
    operations::invocation::validate_idempotency_key(request_id).map_err(|_| {
        anyhow::anyhow!("request-id must use the bounded ASCII idempotency grammar")
    })?;
    let key = format!("cli-automation:{request_id}");
    operations::invocation::validate_idempotency_key(&key).map_err(|_| {
        anyhow::anyhow!("request-id is too long for the bounded cli-automation idempotency key")
    })?;
    Ok(key)
}

async fn run_automation_mutation(
    pool: &SqlitePool,
    cfg: &config::Config,
    automation_id: &str,
    request_id: &str,
) -> Result<()> {
    let idempotency_key = cli_automation_idempotency_key(request_id)?;
    operations::registry::validate()?;
    let secrets_key = load_or_create_secrets_key(cfg)?;
    let adapters = Arc::new(operations::adapters::AdapterRegistry::staged(
        pool.clone(),
        secrets_key,
        cfg.data_dir.clone(),
    )?);
    let prepared =
        operations::runtime::prepare(pool.clone(), adapters.clone(), cfg.operations.clone())
            .await?;
    let runtime = prepared.start();
    let result = async {
        let resource =
            resolve_cli_automation_resource(pool, automation_id, &idempotency_key).await?;
        let context = operations::invocation::InvocationContext::LocalCli;
        let job = operations::invocation::submit(
            pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            &idempotency_key,
        )
        .await
        .map_err(cli_submission_error)?;
        println!("Submitted (job {})", job.id);
        wait_for_durable_job(pool, &job.id, "automation").await
    }
    .await;
    if !runtime.shutdown().await {
        tracing::warn!(error_code = "operation_runtime_unclean_shutdown");
    }
    result
}

async fn resolve_cli_automation_resource(
    pool: &SqlitePool,
    automation_id: &str,
    correlation_id: &str,
) -> Result<operations::contracts::ResourceRef> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM automation_jobs WHERE id = ?")
        .bind(automation_id)
        .fetch_optional(pool)
        .await?;
    let name = name.ok_or_else(|| anyhow::anyhow!("automation job not found"))?;
    let context = operations::invocation::InvocationContext::LocalCli;
    let actor = Some(context.actor());
    let resource =
        match operations::resources::resolve_alias(pool, "automation.job", "local", automation_id)
            .await?
        {
            Some(resource) if resource.kind == "automation_job" => resource,
            Some(_) => anyhow::bail!("automation alias resolves to the wrong resource kind"),
            None => {
                operations::resources::observe(
                    pool,
                    operations::resources::ObserveResource {
                        kind: "automation_job",
                        display_name: &name,
                        node_id: None,
                        provider: Some("local"),
                        namespace: "automation.job",
                        scope_key: "local",
                        alias: automation_id,
                    },
                    actor.clone(),
                    correlation_id,
                )
                .await?
            }
        };
    if operations::resources::capability(pool, &resource.id, "automation.run")
        .await?
        .is_none_or(|capability| capability.availability != "available")
    {
        operations::resources::set_capability_with_actor(
            pool,
            &resource.id,
            "automation.run",
            operations::contracts::CapabilityAvailability::Available,
            None,
            Some("local automation job is available"),
            actor,
            correlation_id,
        )
        .await?;
    }
    Ok(resource)
}

const BACKUP_SELECT_COLS: &str =
    "id, name, source_path, repo_path, schedule, retention_days, enabled, \
     last_run_at, last_status, created_at, last_check_at, last_check_status, \
     last_restore_test_at, last_restore_test_status, restore_test_schedule";

async fn run_backup_command(
    pool: &SqlitePool,
    cfg: &config::Config,
    action: BackupCommand,
) -> Result<()> {
    match action {
        BackupCommand::List => {
            let configs = sqlx::query_as::<_, backups::BackupConfig>(AssertSqlSafe(format!(
                "SELECT {BACKUP_SELECT_COLS} FROM backup_configs ORDER BY created_at DESC"
            )))
            .fetch_all(pool)
            .await?;

            if configs.is_empty() {
                println!("No backup jobs configured.");
            } else {
                println!(
                    "{:<20}  {:<10}  {:<8}  {:<10}  {:<10}",
                    "NAME", "ENABLED", "STATUS", "LAST RUN", "CONFIDENCE"
                );
                for c in &configs {
                    let last_run = c
                        .last_run_at
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "never".into());
                    println!(
                        "{:<20}  {:<10}  {:<8}  {:<10}  {:<10}",
                        c.name,
                        if c.enabled { "yes" } else { "no" },
                        c.last_status.clone().unwrap_or_else(|| "-".into()),
                        last_run,
                        backups::confidence(c),
                    );
                }
            }
            if !backups::is_restic_available() {
                println!("\nWarning: restic is not installed — run/check/restore-test will fail.");
            }
            Ok(())
        }
        mutation => run_backup_mutation(pool, cfg, mutation).await,
    }
}

async fn run_backup_mutation(
    pool: &SqlitePool,
    cfg: &config::Config,
    command: BackupCommand,
) -> Result<()> {
    operations::registry::validate()?;
    let secrets_key = load_or_create_secrets_key(cfg)?;
    let adapters = Arc::new(operations::adapters::AdapterRegistry::staged(
        pool.clone(),
        secrets_key,
        cfg.data_dir.clone(),
    )?);
    let prepared =
        operations::runtime::prepare(pool.clone(), adapters.clone(), cfg.operations.clone())
            .await?;
    let runtime = prepared.start();
    let result = submit_and_wait_for_backup_job(pool, &adapters, command).await;
    if !runtime.shutdown().await {
        tracing::warn!(error_code = "operation_runtime_unclean_shutdown");
    }
    result
}

async fn submit_and_wait_for_backup_job(
    pool: &SqlitePool,
    adapters: &operations::adapters::AdapterRegistry,
    command: BackupCommand,
) -> Result<()> {
    use operations::{
        backup_adoption::{self, BackupSelector},
        invocation::{self, InvocationContext},
    };

    let context = InvocationContext::LocalCli;
    let (resource, action, input) = match command {
        BackupCommand::Create {
            name,
            source,
            repo,
            retention_days,
        } => {
            let resource = backup_adoption::resolve_create_target(pool, &context).await?;
            let input = backups::BackupConfigInput {
                name,
                source_path: source,
                repo_path: repo,
                schedule: None,
                retention_days: retention_days.unwrap_or(30),
                restore_test_schedule: None,
            };
            input.validate()?;
            (
                resource,
                "backup.config.create",
                serde_json::to_value(input)?,
            )
        }
        BackupCommand::Run { name } => {
            let adopted = backup_adoption::resolve_config_target(
                pool,
                &context,
                BackupSelector::Name(&name),
                "backup.run",
            )
            .await?;
            (adopted.resource, "backup.run", serde_json::json!({}))
        }
        BackupCommand::Check { name } => {
            let adopted = backup_adoption::resolve_config_target(
                pool,
                &context,
                BackupSelector::Name(&name),
                "backup.check",
            )
            .await?;
            (adopted.resource, "backup.check", serde_json::json!({}))
        }
        BackupCommand::RestoreTest { name } => {
            let adopted = backup_adoption::resolve_config_target(
                pool,
                &context,
                BackupSelector::Name(&name),
                "backup.restore_test",
            )
            .await?;
            (
                adopted.resource,
                "backup.restore_test",
                serde_json::json!({}),
            )
        }
        BackupCommand::Delete { name } => {
            let adopted = backup_adoption::resolve_config_target(
                pool,
                &context,
                BackupSelector::Name(&name),
                "backup.config.delete",
            )
            .await?;
            (
                adopted.resource,
                "backup.config.delete",
                serde_json::json!({}),
            )
        }
        BackupCommand::List => anyhow::bail!("backup list is not a mutation"),
    };
    let idempotency_key = format!("cli-{}", Uuid::new_v4());
    let job = invocation::submit(
        pool,
        adapters,
        &context,
        &resource.id,
        action,
        input,
        &idempotency_key,
    )
    .await
    .map_err(cli_submission_error)?;
    println!("Submitted (job {})", job.id);
    wait_for_backup_job(pool, &job.id).await
}

async fn wait_for_backup_job(pool: &SqlitePool, job_id: &str) -> Result<()> {
    wait_for_durable_job(pool, job_id, "backup").await
}

async fn wait_for_durable_job(pool: &SqlitePool, job_id: &str, kind: &str) -> Result<()> {
    use operations::contracts::JobState;

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(30 * 60));
    tokio::pin!(timeout);
    let interrupted = tokio::signal::ctrl_c();
    tokio::pin!(interrupted);
    loop {
        let job = operations::jobs::get(pool, job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("submitted {kind} job disappeared"))?;
        match job.state {
            JobState::Queued | JobState::Running => {}
            JobState::AwaitingApproval => {
                println!("State: awaiting_approval");
                if let Some(approval_id) = job.approval_id {
                    println!("Approval: {approval_id}");
                    if let Some(approval) = operations::approvals::get(pool, &approval_id).await? {
                        println!("Requirement: {}", bounded_cli_text(&approval.requirement));
                        println!("Reason: {}", bounded_cli_text(&approval.reason));
                    }
                }
                return Ok(());
            }
            JobState::Succeeded => {
                println!("State: succeeded");
                if let Some(result) = job.result {
                    println!("Result: {}", bounded_cli_json(&result));
                }
                return Ok(());
            }
            JobState::NeedsAttention => {
                println!("State: needs_attention");
                if let Some(error) = job.error {
                    println!("Diagnostic: {}", bounded_cli_text(&error.message));
                }
                anyhow::bail!("{kind} job {job_id} needs operator attention");
            }
            state => {
                println!("State: {}", state.as_str());
                if let Some(error) = job.error {
                    println!("Diagnostic: {}", bounded_cli_text(&error.message));
                }
                anyhow::bail!("{kind} job {job_id} ended in state {}", state.as_str());
            }
        }

        tokio::select! {
            result = &mut interrupted => {
                result?;
                anyhow::bail!("stopped waiting for {kind} job {job_id}; the durable job remains recoverable");
            }
            () = &mut timeout => {
                anyhow::bail!("timed out waiting for {kind} job {job_id}; the durable job remains recoverable");
            }
            () = tokio::time::sleep(tokio::time::Duration::from_millis(250)) => {}
        }
    }
}

fn bounded_cli_json(value: &serde_json::Value) -> String {
    bounded_cli_text(&serde_json::to_string(value).unwrap_or_else(|_| "null".into()))
}

fn bounded_cli_text(value: &str) -> String {
    api::mcp::redact::redact_patterns(value)
        .chars()
        .take(4 * 1024)
        .collect()
}

/// Renders a canonical submission failure as a bounded CLI error. The CLI is the operator's
/// recovery surface, so the unresolved-operation refusal names the stable reason and the recovery
/// path instead of a Debug variant, and never names the blocking job (not an existence oracle).
fn cli_submission_error(error: operations::invocation::InvocationError) -> anyhow::Error {
    use operations::invocation::InvocationError;
    match error {
        InvocationError::UnresolvedOperation => anyhow::anyhow!(
            "refused: unresolved_operation; this resource has an operation whose outcome is \
             still uncertain - reconcile that job before submitting a new intent"
        ),
        other => other.into(),
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    type CanonicalJobFields = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    );

    #[test]
    fn automation_run_cli_requires_canonical_id_and_request_id() {
        let cli = Cli::try_parse_from([
            "voidtower",
            "automation",
            "run",
            "--id",
            "automation-1",
            "--request-id",
            "request-1",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Some(Command::Automation {
                action: AutomationCommand::Run { id, request_id }
            }) if id == "automation-1" && request_id == "request-1"
        ));
    }

    #[test]
    fn automation_cli_request_id_uses_shared_idempotency_grammar() {
        assert_eq!(
            cli_automation_idempotency_key("request-1").unwrap(),
            "cli-automation:request-1"
        );
        for invalid in ["", "contains space", "contains/slash", "é"] {
            assert!(
                cli_automation_idempotency_key(invalid).is_err(),
                "{invalid:?}"
            );
        }
        assert!(cli_automation_idempotency_key(&"a".repeat(114)).is_err());
    }

    #[test]
    fn agent_enroll_cli_defaults_to_no_wireguard_and_uses_state_override() {
        let cli = Cli::try_parse_from([
            "voidtower",
            "agent",
            "enroll",
            "--server-url",
            "https://controller.example.test",
            "--pairing-code",
            "temporary-pairing-credential-123",
            "--display-name",
            "lab-node",
            "--state-path",
            "/tmp/voidtower-agent-state.json",
        ])
        .unwrap();

        let debug = format!("{cli:?}");
        assert!(!debug.contains("temporary-pairing-credential-123"));
        assert!(debug.contains("[REDACTED]"));
        assert_eq!(startup_path(&cli), StartupPath::AgentEnrollment);
        let Some(Command::Agent {
            action:
                AgentCommand::Enroll {
                    provision_wireguard,
                    state_path,
                    ..
                },
        }) = cli.command
        else {
            panic!("agent enroll command not parsed");
        };
        assert!(!provision_wireguard);
        assert_eq!(state_path, PathBuf::from("/tmp/voidtower-agent-state.json"));
    }

    #[test]
    fn agent_check_state_cli_uses_the_agent_startup_path() {
        let cli = Cli::try_parse_from([
            "voidtower",
            "agent",
            "check-state",
            "--state-path",
            "/tmp/voidtower-agent-state.json",
        ])
        .unwrap();

        assert_eq!(startup_path(&cli), StartupPath::AgentEnrollment);
        let Some(Command::Agent {
            action: AgentCommand::CheckState { state_path },
        }) = cli.command
        else {
            panic!("agent check-state command not parsed");
        };
        assert_eq!(state_path, PathBuf::from("/tmp/voidtower-agent-state.json"));
    }

    #[test]
    fn agent_check_state_defaults_to_the_packaged_state_path() {
        let cli = Cli::try_parse_from(["voidtower", "agent", "check-state"]).unwrap();

        let Some(Command::Agent {
            action: AgentCommand::CheckState { state_path },
        }) = cli.command
        else {
            panic!("agent check-state command not parsed");
        };
        assert_eq!(
            state_path,
            PathBuf::from("/var/lib/voidtower/agent/state.json")
        );
    }

    #[test]
    fn oversized_pairing_code_is_never_echoed_by_cli_diagnostics() {
        let credential = "Q".repeat(4097);
        let cli = Cli::try_parse_from([
            "voidtower",
            "agent",
            "enroll",
            "--server-url",
            "https://controller.example.test",
            "--pairing-code",
            credential.as_str(),
            "--display-name",
            "lab-node",
        ])
        .unwrap();
        let debug = format!("{cli:?}");
        assert!(!debug.contains(&credential));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn hyphen_prefixed_pairing_code_is_parsed_as_a_redacted_value() {
        let credential = "--PAIRING_SECRET_VALUE";
        let cli = Cli::try_parse_from([
            "voidtower",
            "agent",
            "enroll",
            "--server-url",
            "https://controller.example.test",
            "--pairing-code",
            credential,
            "--display-name",
            "lab-node",
        ])
        .unwrap();
        let debug = format!("{cli:?}");
        assert!(!debug.contains(credential));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn agent_enroll_cli_accepts_stdin_pairing_code_without_an_argv_credential() {
        let cli = Cli::try_parse_from([
            "voidtower",
            "agent",
            "enroll",
            "--server-url",
            "https://controller.example.test",
            "--pairing-code-stdin",
            "--display-name",
            "lab-node",
        ])
        .unwrap();

        let debug = format!("{cli:?}");
        assert!(debug.contains("pairing_code_stdin: true"));
        assert!(!debug.contains("pairing-code"));
    }

    #[test]
    fn agent_enroll_cli_requires_exactly_one_pairing_code_source() {
        let missing = Cli::try_parse_from([
            "voidtower",
            "agent",
            "enroll",
            "--server-url",
            "https://controller.example.test",
            "--display-name",
            "lab-node",
        ]);
        assert!(missing.is_err());

        let conflicting = Cli::try_parse_from([
            "voidtower",
            "agent",
            "enroll",
            "--server-url",
            "https://controller.example.test",
            "--pairing-code",
            "compatibility-code",
            "--pairing-code-stdin",
            "--display-name",
            "lab-node",
        ]);
        assert!(conflicting.is_err());
    }

    #[test]
    fn stdin_pairing_code_reader_stops_at_line_ending() {
        let mut input = std::io::Cursor::new(b"pairing-code-value\r\ntrailing".to_vec());

        let value = read_pairing_code_from_reader(&mut input).unwrap();

        assert_eq!(value, "pairing-code-value");
    }

    #[test]
    fn stdin_pairing_code_reader_accepts_the_exact_limit_with_crlf() {
        let mut input =
            std::io::Cursor::new([vec![b'Q'; MAX_PAIRING_CODE_BYTES], b"\r\n".to_vec()].concat());

        let value = read_pairing_code_from_reader(&mut input).unwrap();

        assert_eq!(value.len(), MAX_PAIRING_CODE_BYTES);
    }

    #[test]
    fn stdin_pairing_code_reader_rejects_empty_and_invalid_utf8_without_echoing() {
        for input in [vec![b'\n'], vec![0xff, b'\n']] {
            let mut input = std::io::Cursor::new(input);
            let error = read_pairing_code_from_reader(&mut input).unwrap_err();
            assert!(!error.to_string().contains("0xff"));
        }
    }

    #[test]
    fn stdin_pairing_code_reader_does_not_wait_for_eof_after_a_line() {
        struct OpenAfterLine {
            bytes: &'static [u8],
            offset: usize,
        }

        impl std::io::Read for OpenAfterLine {
            fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
                if self.offset == self.bytes.len() {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    return Ok(0);
                }
                target[0] = self.bytes[self.offset];
                self.offset += 1;
                Ok(1)
            }
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut input = OpenAfterLine {
                bytes: b"pairing-code-value\n",
                offset: 0,
            };
            sender
                .send(read_pairing_code_from_reader(&mut input))
                .unwrap();
        });

        let value = receiver
            .recv_timeout(std::time::Duration::from_millis(250))
            .expect("line-bounded stdin reads must not wait for EOF")
            .unwrap();
        assert_eq!(value, "pairing-code-value");
    }

    #[test]
    fn stdin_pairing_code_reader_rejects_an_oversized_open_line_without_waiting() {
        struct OpenAfterBytes {
            bytes: &'static [u8],
            offset: usize,
        }

        impl std::io::Read for OpenAfterBytes {
            fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
                if self.offset == self.bytes.len() {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                    return Ok(0);
                }
                target[0] = self.bytes[self.offset];
                self.offset += 1;
                Ok(1)
            }
        }

        let bytes = Box::leak(vec![b'Q'; MAX_PAIRING_CODE_BYTES + 1].into_boxed_slice());
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut input = OpenAfterBytes { bytes, offset: 0 };
            sender
                .send(read_pairing_code_from_reader(&mut input))
                .unwrap();
        });

        let error = receiver
            .recv_timeout(std::time::Duration::from_millis(250))
            .expect("oversized stdin must fail without waiting for EOF")
            .unwrap_err();
        assert!(error.to_string().contains("exceeds 512 bytes"));
    }

    #[test]
    fn agent_flag_selects_runtime_before_controller_startup() {
        let cli = Cli::try_parse_from([
            "voidtower",
            "--agent",
            "--agent-state",
            "/tmp/agent-state.json",
        ])
        .unwrap();
        assert_eq!(startup_path(&cli), StartupPath::AgentRuntime);
    }

    #[tokio::test]
    async fn listener_binding_failure_does_not_start_operation_runtime() {
        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = occupied.local_addr().unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let started_by_closure = started.clone();

        let result = bind_then_start(addr, move || {
            started_by_closure.store(true, Ordering::SeqCst);
        })
        .await;

        assert!(result.is_err());
        assert!(!started.load(Ordering::SeqCst));
    }

    #[test]
    fn scheduled_restore_test_keys_are_stable_per_utc_minute() {
        let first = scheduled_restore_test_idempotency_key("config-1", 120);
        assert_eq!(
            first,
            scheduled_restore_test_idempotency_key("config-1", 179)
        );
        assert_ne!(
            first,
            scheduled_restore_test_idempotency_key("config-1", 180)
        );
        assert_ne!(
            first,
            scheduled_restore_test_idempotency_key("config-2", 120)
        );
    }

    #[test]
    fn cli_diagnostics_are_bounded_and_redacted() {
        let output = bounded_cli_text(&format!("api_key=known-secret-value {}", "x".repeat(5000)));
        assert!(!output.contains("known-secret-value"));
        assert!(output.chars().count() <= 4 * 1024);
    }

    #[tokio::test]
    async fn automation_cli_resolves_canonical_resource_and_replays_durable_job() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at)
             VALUES ('cli-automation', 'CLI automation', 'printf cli', 1, 30, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at)
             VALUES ('cli-automation-other', 'Other automation', 'printf other', 1, 30, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let cfg = config::Config::default();
        let adapters = Arc::new(
            operations::adapters::AdapterRegistry::staged(
                pool.clone(),
                Arc::new([0u8; 32]),
                cfg.data_dir.clone(),
            )
            .unwrap(),
        );
        let prepared = operations::runtime::prepare(pool.clone(), adapters.clone(), cfg.operations)
            .await
            .unwrap();
        let runtime = prepared.start();
        let resource =
            resolve_cli_automation_resource(&pool, "cli-automation", "cli-automation:request-1")
                .await
                .unwrap();
        let context = operations::invocation::InvocationContext::LocalCli;
        let first = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-automation:request-1",
        )
        .await
        .unwrap();
        let repeat = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-automation:request-1",
        )
        .await
        .unwrap();

        assert_eq!(first.id, repeat.id);
        assert_eq!(first.action, "automation.run");
        assert_eq!(first.resource.id, resource.id);
        assert_eq!(
            first.actor.actor_type,
            operations::contracts::ActorType::System
        );
        assert_eq!(first.ingress, "local_cli");

        let other = resolve_cli_automation_resource(
            &pool,
            "cli-automation-other",
            "cli-automation:request-1",
        )
        .await
        .unwrap();
        assert!(matches!(
            operations::invocation::submit(
                &pool,
                &adapters,
                &context,
                &other.id,
                "automation.run",
                serde_json::json!({}),
                "cli-automation:request-1",
            )
            .await,
            Err(operations::invocation::InvocationError::IdempotencyConflict)
        ));

        sqlx::query("UPDATE automation_jobs SET enabled = 0 WHERE id = 'cli-automation'")
            .execute(&pool)
            .await
            .unwrap();
        let disabled_resource =
            resolve_cli_automation_resource(&pool, "cli-automation", "cli-automation:request-1")
                .await
                .unwrap();
        let disabled_replay = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &disabled_resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-automation:request-1",
        )
        .await
        .unwrap();
        assert_eq!(disabled_replay.id, first.id);

        let capability_actor: String = sqlx::query_scalar(
            "SELECT actor_type FROM events WHERE event_type = 'capability.changed.v1'
             AND correlation_id = 'cli-automation:request-1' LIMIT 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(capability_actor, "system");
        let audit_action: String = sqlx::query_scalar(
            "SELECT action FROM audit_log WHERE request_id = ? ORDER BY rowid DESC LIMIT 1",
        )
        .bind(&first.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit_action, "automation.run");
        let submitted_event: String = sqlx::query_scalar(
            "SELECT event_type FROM events WHERE job_id = ? AND event_type = 'job.submitted.v1'",
        )
        .bind(&first.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(submitted_event, "job.submitted.v1");
        assert!(runtime.shutdown().await);
    }

    /// M1-03 — the local CLI is machine ingress and must obey the same admission rule as the
    /// HTTP, MCP, Studio, webhook, and automation-scheduler ingresses.
    ///
    /// While the resolved automation resource holds a `needs_attention` job, a new CLI intent is
    /// refused before planning and writes no job, approval, audit record, or automation run. The
    /// refusal is rendered as the stable `unresolved_operation` reason with no blocking job id, so
    /// an operator reconciles instead of replaying a possibly applied mutation. A same-key replay
    /// still returns the blocked job, and a reconciled resource admits the deferred intent.
    #[tokio::test]
    async fn automation_cli_refuses_new_intent_while_its_resource_operation_is_unresolved() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO automation_jobs (id, name, command, enabled, timeout_secs, created_at, updated_at)
             VALUES ('cli-guard', 'Guarded automation', 'printf guarded', 1, 30, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let cfg = config::Config::default();
        let adapters = Arc::new(
            operations::adapters::AdapterRegistry::staged(
                pool.clone(),
                Arc::new([0u8; 32]),
                cfg.data_dir.clone(),
            )
            .unwrap(),
        );
        let prepared = operations::runtime::prepare(pool.clone(), adapters.clone(), cfg.operations)
            .await
            .unwrap();
        let runtime = prepared.start();
        let context = operations::invocation::InvocationContext::LocalCli;
        let resource = resolve_cli_automation_resource(&pool, "cli-guard", "cli-guard:first")
            .await
            .unwrap();
        let first = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-guard:first",
        )
        .await
        .unwrap();

        // The provider outcome for that job becomes uncertain.
        sqlx::query(
            "UPDATE jobs SET state = 'needs_attention', error_code = 'provider_unavailable',
             error_message = 'outcome unresolved' WHERE id = ?",
        )
        .bind(&first.id)
        .execute(&pool)
        .await
        .unwrap();
        let count = |query: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(query)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        let jobs_before = count("SELECT COUNT(*) FROM jobs").await;
        let approvals_before = count("SELECT COUNT(*) FROM approvals").await;
        let audits_before = count("SELECT COUNT(*) FROM audit_log").await;
        let runs_before = count("SELECT COUNT(*) FROM automation_runs").await;

        let refused = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-guard:second",
        )
        .await;
        let Err(error) = refused else {
            panic!("a new CLI intent must be refused while the resource is unresolved");
        };
        assert!(
            matches!(
                error,
                operations::invocation::InvocationError::UnresolvedOperation
            ),
            "{error:?}"
        );
        let rendered = cli_submission_error(error).to_string();
        assert!(rendered.contains("unresolved_operation"), "{rendered}");
        assert!(
            !rendered.contains(&first.id),
            "the refusal must not disclose the blocking job id: {rendered}"
        );
        assert!(rendered.chars().count() <= 4 * 1024);
        assert_eq!(count("SELECT COUNT(*) FROM jobs").await, jobs_before);
        assert_eq!(
            count("SELECT COUNT(*) FROM approvals").await,
            approvals_before
        );
        assert_eq!(count("SELECT COUNT(*) FROM audit_log").await, audits_before);
        assert_eq!(
            count("SELECT COUNT(*) FROM automation_runs").await,
            runs_before
        );

        // Recovery by query: the original key still returns the blocked job unchanged.
        let replay = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-guard:first",
        )
        .await
        .unwrap();
        assert_eq!(replay.id, first.id);
        assert_eq!(
            replay.state,
            operations::contracts::JobState::NeedsAttention
        );

        // Reconciling the uncertain outcome to a terminal state admits the deferred intent.
        sqlx::query(
            "UPDATE jobs SET state = 'failed', error_code = 'provider_rejected',
             error_message = 'reconciled' WHERE id = ?",
        )
        .bind(&first.id)
        .execute(&pool)
        .await
        .unwrap();
        let admitted = operations::invocation::submit(
            &pool,
            &adapters,
            &context,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-guard:third",
        )
        .await
        .unwrap();
        assert_eq!(admitted.ingress, "local_cli");
        assert_ne!(admitted.id, first.id);
        assert!(runtime.shutdown().await);
    }

    #[tokio::test]
    async fn automation_scheduler_and_local_cli_share_canonical_plan_semantics() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        sqlx::query(
            "INSERT INTO automation_jobs (id, name, command, schedule, enabled, timeout_secs, created_at, updated_at)
             VALUES ('equivalent-automation', 'Equivalent automation', 'printf equivalent', '@minutely', 1, 30, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = crate::api::mcp::test_support::build(pool.clone());

        api::automation::run_scheduled_jobs(&state).await;
        let scheduler_job_id: String =
            sqlx::query_scalar("SELECT id FROM jobs WHERE actor_id = 'automation_scheduler'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let scheduler_job: CanonicalJobFields = sqlx::query_as(
            "SELECT action, resource_id, input_json, plan_json, plan_digest,
                    external_fingerprint, retry_class, recovery_class, actor_type,
                    actor_id, actor_source, ingress, idempotency_scope
             FROM jobs WHERE actor_id = 'automation_scheduler'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let resource =
            resolve_cli_automation_resource(&pool, "equivalent-automation", "cli-equivalence")
                .await
                .unwrap();
        let cli_job = operations::invocation::submit(
            &pool,
            &state.operation_adapters,
            &operations::invocation::InvocationContext::LocalCli,
            &resource.id,
            "automation.run",
            serde_json::json!({}),
            "cli-automation:equivalence",
        )
        .await
        .unwrap();
        let cli_fields: CanonicalJobFields = sqlx::query_as(
            "SELECT action, resource_id, input_json, plan_json, plan_digest,
                    external_fingerprint, retry_class, recovery_class, actor_type,
                    actor_id, actor_source, ingress, idempotency_scope
             FROM jobs WHERE id = ?",
        )
        .bind(&cli_job.id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_ne!(cli_job.id, scheduler_job_id, "each ingress has its own job");
        assert_eq!(cli_fields.0, scheduler_job.0);
        assert_eq!(
            cli_fields.1, scheduler_job.1,
            "resource identity is server-owned"
        );
        assert_eq!(cli_fields.2, scheduler_job.2);
        assert_eq!(
            cli_fields.3, scheduler_job.3,
            "plans must be immutable and equivalent"
        );
        assert_eq!(cli_fields.4, scheduler_job.4);
        assert_eq!(cli_fields.5, scheduler_job.5);
        assert_eq!(cli_fields.6, scheduler_job.6);
        assert_eq!(cli_fields.7, scheduler_job.7);
        assert_eq!(scheduler_job.8, "system");
        assert_eq!(scheduler_job.9, "automation_scheduler");
        assert_eq!(scheduler_job.10, "scheduler");
        assert_eq!(scheduler_job.11, "scheduler");
        assert_eq!(cli_fields.8, "system");
        assert_eq!(cli_fields.9, "voidtower_cli");
        assert_eq!(cli_fields.10, "local_cli");
        assert_eq!(cli_fields.11, "local_cli");
        assert_ne!(cli_fields.12, scheduler_job.12);

        let queued_runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM automation_runs")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(queued_runs, 0, "both ingress paths must queue only");

        for job_id in [&scheduler_job_id, &cli_job.id] {
            let audit_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_log
                 WHERE request_id = ? AND action = 'automation.run' AND outcome = 'queued'",
            )
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(audit_count, 1);
            let event_count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM events
                 WHERE job_id = ? AND event_type = 'job.submitted.v1' AND correlation_id = ?",
            )
            .bind(job_id)
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(event_count, 1);
        }
    }

    #[tokio::test]
    async fn backup_create_cli_submits_and_waits_through_the_durable_runtime() {
        let pool = crate::api::mcp::test_support::setup_db().await;
        operations::resources::observe(
            &pool,
            operations::resources::ObserveResource {
                kind: "system",
                display_name: "This VoidTower",
                node_id: None,
                provider: Some("local"),
                namespace: "voidtower.singleton",
                scope_key: "local",
                alias: "system",
            },
            None,
            "seed",
        )
        .await
        .unwrap();
        let cfg = config::Config::default();
        let adapters = Arc::new(
            operations::adapters::AdapterRegistry::staged(
                pool.clone(),
                Arc::new([0u8; 32]),
                cfg.data_dir.clone(),
            )
            .unwrap(),
        );
        let prepared = operations::runtime::prepare(pool.clone(), adapters.clone(), cfg.operations)
            .await
            .unwrap();
        let runtime = prepared.start();
        submit_and_wait_for_backup_job(
            &pool,
            &adapters,
            BackupCommand::Create {
                name: "CLI backup".into(),
                source: "/srv/data".into(),
                repo: "/srv/restic".into(),
                retention_days: Some(30),
            },
        )
        .await
        .unwrap();
        assert!(runtime.shutdown().await);

        let config: Option<String> =
            sqlx::query_scalar("SELECT id FROM backup_configs WHERE name = 'CLI backup'")
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(config.is_some());
        let job = operations::jobs::list(&pool, 1)
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(job.ingress, "local_cli");
        assert_eq!(
            job.actor.actor_type,
            operations::contracts::ActorType::System
        );
        assert_eq!(job.state, operations::contracts::JobState::Succeeded);
    }
}
