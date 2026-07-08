mod ai;
mod alerts;
mod api;
mod audit;
mod auth;
mod policy;
mod automation;
mod backups;
mod cluster;
mod config;
mod containers;
mod db;
mod error;
mod monitoring;
mod networking;
mod oidc;
mod security;
mod services;
mod storage;
mod terminal;
mod vms;

use anyhow::Result;
use clap::{Parser, Subcommand};
use monitoring::{MetricsBroadcaster, MetricsCollector, MetricsSnapshot};
use sqlx::SqlitePool;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

#[derive(Debug)]
pub struct LoginAttempts {
    pub count: u32,
    pub window_start: std::time::Instant,
    pub locked_until: Option<std::time::Instant>,
}
use tower_http::services::{ServeDir, ServeFile};

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Arc<config::Config>,
    pub metrics_tx: MetricsBroadcaster,
    pub latest_metrics: Arc<RwLock<Option<MetricsSnapshot>>>,
    pub agents_tx: api::agents::AgentBroadcaster,
    pub secrets_key: Arc<[u8; 32]>,
    // token_hash -> (session_id, expires_at_unix) — avoids a DB write on every Bearer request
    pub token_sessions: Arc<RwLock<HashMap<String, (String, i64)>>>,
    pub login_limiter: Arc<std::sync::Mutex<HashMap<std::net::IpAddr, LoginAttempts>>>,
    pub deploy_registry: containers::DeployRegistry,
}

#[derive(Parser, Debug)]
#[command(name = "voidtower", version, about = "Self-hosted infrastructure command tower")]
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

    /// Run in agent mode (no UI, exposes agent API only)
    #[arg(long)]
    agent: bool,

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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut cfg = config::Config::load(cli.config.as_deref())?;
    if let Some(b) = cli.bind { cfg.bind = b; }
    if let Some(p) = cli.port { cfg.port = p; }
    if cli.agent { cfg.agent_mode = true; }

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

        if let Some(out_arg) = cli.export_config {
            let path_opt = out_arg.as_deref();
            api::disaster::cli_export(&pool, path_opt).await?;
        } else if let Some(input_path) = cli.import_config {
            api::disaster::cli_import(&pool, &input_path).await?;
        }
        return Ok(());
    }

    // Management subcommands (user / backup) — require DB but skip web server
    if let Some(command) = cli.command {
        std::fs::create_dir_all(&cfg.data_dir)?;
        let pool = db::init_pool(&cfg.db_path()).await?;
        run_command(&pool, command).await?;
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
            let _ = std::fs::set_permissions(&cfg.config_dir, std::fs::Permissions::from_mode(0o700));
        }
    }

    // Load or generate secrets encryption key
    let secrets_key: Arc<[u8; 32]> = {
        let key_path = cfg.config_dir.join("secrets.key");
        let key_bytes = if key_path.exists() {
            let raw = std::fs::read(&key_path)?;
            anyhow::ensure!(raw.len() == 32, "secrets.key must be exactly 32 bytes");
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&raw);
            arr
        } else {
            use rand::RngCore;
            let mut arr = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut arr);
            std::fs::write(&key_path, arr)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
            tracing::info!("Generated new secrets encryption key at {}", key_path.display());
            arr
        };
        Arc::new(key_bytes)
    };

    // Bootstrap token
    let bootstrap_result = auth::ensure_bootstrap_token(&cfg.bootstrap_token_path()).await?;
    if let Some(token) = &bootstrap_result {
        tracing::info!("╔══════════════════════════════════════════════════╗");
        tracing::info!("║          VoidTower first-run bootstrap           ║");
        tracing::info!("║                                                  ║");
        tracing::info!("║  Bootstrap token: {:<31}║", token);
        tracing::info!("║  Visit http://{}:{} to complete setup   ║", cfg.bind, cfg.port);
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

    // Re-log bootstrap token on every restart while setup is still pending.
    // Covers Docker deployments where the first-run log line was missed.
    if bootstrap_result.is_none() {
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&pool).await.unwrap_or(0);
        if user_count == 0 {
            if let Some(token) = auth::read_bootstrap_token(&cfg.bootstrap_token_path()).await? {
                tracing::info!("╔══════════════════════════════════════════════════╗");
                tracing::info!("║        VoidTower setup not yet complete          ║");
                tracing::info!("║                                                  ║");
                tracing::info!("║  Bootstrap token: {:<31}║", token);
                tracing::info!("║  Visit http://{}:{} to complete setup   ║", cfg.bind, cfg.port);
                tracing::info!("╚══════════════════════════════════════════════════╝");
            }
        }
    }

    // Metrics broadcaster
    let (metrics_tx, _) = broadcast::channel::<MetricsSnapshot>(16);
    let latest_metrics: Arc<RwLock<Option<MetricsSnapshot>>> = Arc::new(RwLock::new(None));

    // Agent status broadcaster
    let (agents_tx, _) = broadcast::channel::<api::agents::AgentStatusUpdate>(64);

    let state = AppState {
        db: pool.clone(),
        config: Arc::new(cfg.clone()),
        metrics_tx: metrics_tx.clone(),
        latest_metrics: latest_metrics.clone(),
        agents_tx: agents_tx.clone(),
        secrets_key,
        token_sessions: Arc::new(RwLock::new(HashMap::new())),
        login_limiter: Arc::new(std::sync::Mutex::new(HashMap::new())),
        deploy_registry: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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
            if last_check.elapsed().as_secs() < 60 { continue; }
            last_check = std::time::Instant::now();

            let cpu = snap.cpu_usage;
            let ram_pct = if snap.ram_total > 0 {
                (snap.ram_used as f32 / snap.ram_total as f32) * 100.0
            } else { 0.0 };

            if cpu > 95.0 {
                api::alerts::create_alert(&alert_pool, "High CPU Usage",
                    &format!("CPU usage is {:.1}% — above 95% threshold", cpu),
                    "critical", "system", None, None).await;
            } else if cpu > 85.0 {
                api::alerts::create_alert(&alert_pool, "Elevated CPU Usage",
                    &format!("CPU usage is {:.1}% — above 85% threshold", cpu),
                    "warning", "system", None, None).await;
            }

            if ram_pct > 92.0 {
                api::alerts::create_alert(&alert_pool, "High Memory Usage",
                    &format!("RAM usage is {:.1}% — above 92% threshold", ram_pct),
                    "critical", "system", None, None).await;
            } else if ram_pct > 80.0 {
                api::alerts::create_alert(&alert_pool, "Elevated Memory Usage",
                    &format!("RAM usage is {:.1}% — above 80% threshold", ram_pct),
                    "warning", "system", None, None).await;
            }

            for disk in &snap.disks {
                if disk.total == 0 { continue; }
                let pct = (disk.used as f32 / disk.total as f32) * 100.0;
                if pct > 92.0 {
                    api::alerts::create_alert(&alert_pool,
                        &format!("Disk Almost Full: {}", disk.mount_point),
                        &format!("{} is {:.1}% full", disk.mount_point, pct),
                        "critical", "storage", Some("disk"), Some(&disk.mount_point)).await;
                } else if pct > 80.0 {
                    api::alerts::create_alert(&alert_pool,
                        &format!("Disk Usage High: {}", disk.mount_point),
                        &format!("{} is {:.1}% full", disk.mount_point, pct),
                        "warning", "storage", Some("disk"), Some(&disk.mount_point)).await;
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
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

            for check in checks {
                let due = check.last_checked_at
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
    let auto_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            api::automation::run_scheduled_jobs(&auto_pool).await;
        }
    });

    // Item #10A: scheduled restore-test runner (checks every 60s against cron expressions)
    let rt_pool = pool.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            if !backups::is_restic_available() { continue; }
            let Ok(cfgs) = sqlx::query_as::<_, backups::BackupConfig>(
                &format!("SELECT {} FROM backup_configs WHERE enabled = 1 AND restore_test_schedule IS NOT NULL",
                    "id, name, source_path, repo_path, schedule, retention_days, enabled, last_run_at, last_status, created_at, last_check_at, last_check_status, last_restore_test_at, last_restore_test_status, restore_test_schedule")
            ).fetch_all(&rt_pool).await else { continue };

            let now_ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

            for cfg in cfgs {
                let Some(ref sched) = cfg.restore_test_schedule else { continue };
                if !backups::cron_matches_now(sched) { continue; }

                // Avoid re-running if already ran within this minute
                if cfg.last_restore_test_at.map(|t| now_ts - t < 60).unwrap_or(false) { continue; }

                let pool2 = rt_pool.clone();
                let cfg2 = cfg.clone();
                let password = std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| "changeme".into());
                tokio::spawn(async move {
                    let (status, _) = match backups::run_restore_test(&cfg2.repo_path, &password).await {
                        Ok(s) => (s, None::<String>),
                        Err(e) => ("failed".to_string(), Some(e.to_string())),
                    };
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
                    let _ = sqlx::query(
                        "UPDATE backup_configs SET last_restore_test_at = ?, last_restore_test_status = ? WHERE id = ?"
                    ).bind(ts).bind(&status).bind(&cfg2.id).execute(&pool2).await;
                    tracing::info!("Scheduled restore test for '{}': {}", cfg2.name, status);
                });
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
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
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
                ).await;
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
    tracing::info!("VoidTower listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

fn run_doctor(cfg: &config::Config, as_json: bool) {
    let checks = api::diagnostics::run_all_checks(cfg);

    let fail = checks.iter().filter(|c| c.status == api::diagnostics::CheckStatus::Fail).count();
    let warn = checks.iter().filter(|c| c.status == api::diagnostics::CheckStatus::Warn).count();
    let overall = if fail > 0 { "fail" } else if warn > 0 { "warn" } else { "pass" };

    if as_json {
        let json_checks: Vec<serde_json::Value> = checks.iter().map(|c| {
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
        }).collect();
        let output = serde_json::json!({
            "checks": json_checks,
            "overall": overall,
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
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
        let pass = checks.iter().filter(|c| c.status == api::diagnostics::CheckStatus::Pass).count();
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

fn is_valid_role(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "operator" | "viewer")
}

async fn run_command(pool: &SqlitePool, command: Command) -> Result<()> {
    match command {
        Command::User { action } => run_user_command(pool, action).await,
        Command::Backup { action } => run_backup_command(pool, action).await,
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
                println!("{:<36}  {:<20}  {:<9}  {:<5}  {:<14}", "ID", "USERNAME", "ROLE", "2FA", "MUST CHANGE PW");
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
        UserCommand::Create { username, password, role } => {
            if !is_valid_role(&role) {
                anyhow::bail!("Invalid role '{role}' — must be one of: owner, admin, operator, viewer");
            }
            if username.len() < 3 || password.len() < 8 {
                anyhow::bail!("Username must be ≥3 chars, password ≥8 chars");
            }
            let user = auth::create_user(pool, &username, &password, &role, true).await?;
            println!("Created user '{}' ({}) with role '{}'", user.username, user.id, user.role);
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
                anyhow::bail!("Invalid role '{role}' — must be one of: owner, admin, operator, viewer");
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

const BACKUP_SELECT_COLS: &str = "id, name, source_path, repo_path, schedule, retention_days, enabled, \
     last_run_at, last_status, created_at, last_check_at, last_check_status, \
     last_restore_test_at, last_restore_test_status, restore_test_schedule";

async fn find_backup_by_name(pool: &SqlitePool, name: &str) -> Result<backups::BackupConfig> {
    sqlx::query_as::<_, backups::BackupConfig>(
        &format!("SELECT {BACKUP_SELECT_COLS} FROM backup_configs WHERE name = ?"),
    )
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow::anyhow!("Backup job '{name}' not found"))
}

async fn run_backup_command(pool: &SqlitePool, action: BackupCommand) -> Result<()> {
    match action {
        BackupCommand::List => {
            let configs = sqlx::query_as::<_, backups::BackupConfig>(
                &format!("SELECT {BACKUP_SELECT_COLS} FROM backup_configs ORDER BY created_at DESC"),
            )
            .fetch_all(pool)
            .await?;

            if configs.is_empty() {
                println!("No backup jobs configured.");
            } else {
                println!("{:<20}  {:<10}  {:<8}  {:<10}  {:<10}", "NAME", "ENABLED", "STATUS", "LAST RUN", "CONFIDENCE");
                for c in &configs {
                    let last_run = c.last_run_at.map(|t| t.to_string()).unwrap_or_else(|| "never".into());
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
        }
        BackupCommand::Create { name, source, repo, retention_days } => {
            let id = Uuid::new_v4().to_string();
            let retention = retention_days.unwrap_or(30);
            sqlx::query(
                "INSERT INTO backup_configs (id, name, source_path, repo_path, retention_days, enabled, created_at) \
                 VALUES (?, ?, ?, ?, ?, 1, ?)",
            )
            .bind(&id)
            .bind(&name)
            .bind(&source)
            .bind(&repo)
            .bind(retention)
            .bind(unix_now())
            .execute(pool)
            .await?;
            println!("Created backup job '{name}' ({id})");
        }
        BackupCommand::Run { name } => {
            if !backups::is_restic_available() {
                anyhow::bail!("restic is not installed");
            }
            let cfg = find_backup_by_name(pool, &name).await?;
            let password = std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| "changeme".into());
            backups::init_repo(&cfg.repo_path, &password).await?;
            let run = backups::run_backup(&cfg, &password).await?;
            sqlx::query("UPDATE backup_configs SET last_run_at = ?, last_status = ? WHERE id = ?")
                .bind(run.finished_at.unwrap_or_else(unix_now))
                .bind(&run.status)
                .bind(&cfg.id)
                .execute(pool)
                .await?;
            println!("Backup '{name}': {}", run.status);
            if let Some(snap) = run.snapshot_id {
                println!("Snapshot: {snap}");
            }
        }
        BackupCommand::Check { name } => {
            if !backups::is_restic_available() {
                anyhow::bail!("restic is not installed");
            }
            let cfg = find_backup_by_name(pool, &name).await?;
            let password = std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| "changeme".into());
            let (status, message) = match backups::run_check(&cfg.repo_path, &password).await {
                Ok(s) => (s, None),
                Err(e) => ("failed".to_string(), Some(e.to_string())),
            };
            sqlx::query("UPDATE backup_configs SET last_check_at = ?, last_check_status = ? WHERE id = ?")
                .bind(unix_now())
                .bind(&status)
                .bind(&cfg.id)
                .execute(pool)
                .await?;
            println!("Check '{name}': {status}");
            if let Some(m) = message {
                println!("{m}");
            }
        }
        BackupCommand::RestoreTest { name } => {
            if !backups::is_restic_available() {
                anyhow::bail!("restic is not installed");
            }
            let cfg = find_backup_by_name(pool, &name).await?;
            let password = std::env::var("RESTIC_PASSWORD").unwrap_or_else(|_| "changeme".into());
            let (status, message) = match backups::run_restore_test(&cfg.repo_path, &password).await {
                Ok(s) => (s, None),
                Err(e) => ("failed".to_string(), Some(e.to_string())),
            };
            sqlx::query("UPDATE backup_configs SET last_restore_test_at = ?, last_restore_test_status = ? WHERE id = ?")
                .bind(unix_now())
                .bind(&status)
                .bind(&cfg.id)
                .execute(pool)
                .await?;
            println!("Restore test '{name}': {status}");
            if let Some(m) = message {
                println!("{m}");
            }
        }
        BackupCommand::Delete { name } => {
            let cfg = find_backup_by_name(pool, &name).await?;
            sqlx::query("DELETE FROM backup_configs WHERE id = ?")
                .bind(&cfg.id)
                .execute(pool)
                .await?;
            println!("Deleted backup job '{name}' (config only — data on disk is untouched)");
        }
    }
    Ok(())
}
