use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::Path;

pub async fn init_pool(db_path: &Path) -> Result<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let url = format!("sqlite://{}?mode=rwc", db_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect(&url)
        .await?;

    // Enable WAL mode and foreign keys
    sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;
    sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await?;

    run_migrations(&pool).await?;

    // Webhook configs table (added post-initial schema)
    let _ = sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS webhook_configs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            type        TEXT NOT NULL DEFAULT 'generic',
            events      TEXT NOT NULL DEFAULT '["alert.created"]',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        )"#,
    )
    .execute(&pool)
    .await;

    // Add columns introduced after initial schema — safe to ignore if already present
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN force_password_change INTEGER NOT NULL DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN allow_embed INTEGER NOT NULL DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE backup_configs ADD COLUMN last_check_at INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE backup_configs ADD COLUMN last_check_status TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE backup_configs ADD COLUMN last_restore_test_at INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE backup_configs ADD COLUMN last_restore_test_status TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE deployed_apps ADD COLUMN primary_port INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN totp_secret TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE audit_log ADD COLUMN source TEXT").execute(&pool).await;

    // Item #7A: secret rotation version counter
    let _ = sqlx::query("ALTER TABLE secrets ADD COLUMN version INTEGER NOT NULL DEFAULT 0").execute(&pool).await;
    // Item #7B: per-token secret scope restriction (JSON array of secret IDs, NULL = unrestricted)
    let _ = sqlx::query("ALTER TABLE api_tokens ADD COLUMN secret_ids TEXT").execute(&pool).await;
    // Item #10A: scheduled restore-test cron expression
    let _ = sqlx::query("ALTER TABLE backup_configs ADD COLUMN restore_test_schedule TEXT").execute(&pool).await;
    // Disaster recovery import uses ON CONFLICT(name) — needs a unique index
    let _ = sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_automation_jobs_name ON automation_jobs(name)").execute(&pool).await;

    // App embed: dedicated LAN port per app (port-based nginx, no DNS needed)
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN embed_port INTEGER").execute(&pool).await;

    // SSH session encrypted password storage
    let _ = sqlx::query("ALTER TABLE ssh_sessions ADD COLUMN password_enc TEXT").execute(&pool).await;

    // External / adopted app origin tracking
    let _ = sqlx::query("ALTER TABLE deployed_apps ADD COLUMN origin TEXT NOT NULL DEFAULT 'voidtower'").execute(&pool).await;

    // Proxmox multi-host management (added post-initial schema)
    let _ = sqlx::query(r#"CREATE TABLE IF NOT EXISTS proxmox_hosts (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL,
        url         TEXT NOT NULL,
        node        TEXT NOT NULL DEFAULT 'pve',
        fingerprint TEXT,
        created_at  TEXT NOT NULL DEFAULT (datetime('now'))
    )"#).execute(&pool).await;

    // Policy engine — rules governing what automated actors (API tokens, automations) may do
    let _ = sqlx::query(r#"CREATE TABLE IF NOT EXISTS policy_rules (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        actor_type    TEXT NOT NULL DEFAULT 'api_token',
        action        TEXT NOT NULL DEFAULT '*',
        resource_type TEXT NOT NULL DEFAULT '*',
        resource_tag  TEXT,
        effect        TEXT NOT NULL DEFAULT 'deny',
        priority      INTEGER NOT NULL DEFAULT 100,
        enabled       INTEGER NOT NULL DEFAULT 1,
        created_at    INTEGER NOT NULL
    )"#).execute(&pool).await;

    // Seed three default deny rules on first install (skip if any rules already exist)
    let rule_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_rules")
        .fetch_one(&pool).await.unwrap_or(0);
    if rule_count == 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let seeds = [
            (uuid::Uuid::new_v4().to_string(), "Block AI access to ai-no-touch resources",   "api_token", "*",      "*",      Some("ai-no-touch"), "deny", 10i64),
            (uuid::Uuid::new_v4().to_string(), "Block AI access to critical resources",       "api_token", "*",      "*",      Some("critical"),    "deny", 20i64),
            (uuid::Uuid::new_v4().to_string(), "Block API tokens from deleting anything",     "api_token", "remove", "*",      None,                "deny", 30i64),
        ];
        for (id, name, actor_type, action, resource_type, resource_tag, effect, priority) in seeds {
            let _ = sqlx::query(
                "INSERT INTO policy_rules (id,name,actor_type,action,resource_type,resource_tag,effect,priority,enabled,created_at)
                 VALUES (?,?,?,?,?,?,?,?,1,?)"
            )
            .bind(id).bind(name).bind(actor_type).bind(action)
            .bind(resource_type).bind(resource_tag).bind(effect).bind(priority).bind(now)
            .execute(&pool).await;
        }
    }

    // Plugin registry
    let _ = sqlx::query(r#"CREATE TABLE IF NOT EXISTS plugins (
        id           TEXT PRIMARY KEY,
        name         TEXT NOT NULL,
        description  TEXT NOT NULL DEFAULT '',
        version      TEXT NOT NULL DEFAULT '1.0.0',
        author       TEXT,
        entry        TEXT NOT NULL DEFAULT 'index.html',
        icon         TEXT,
        nav_group    TEXT,
        enabled      INTEGER NOT NULL DEFAULT 1,
        installed_at INTEGER NOT NULL
    )"#).execute(&pool).await;

    // Authentik / OIDC SSO
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN auth_source TEXT NOT NULL DEFAULT 'local'").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN oidc_subject TEXT").execute(&pool).await;
    let _ = sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_users_oidc_subject ON users(oidc_subject) WHERE oidc_subject IS NOT NULL").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN sso_protect INTEGER NOT NULL DEFAULT 0").execute(&pool).await;

    let _ = sqlx::query(r#"CREATE TABLE IF NOT EXISTS oidc_config (
        id               TEXT PRIMARY KEY DEFAULT 'default',
        enabled          INTEGER NOT NULL DEFAULT 0,
        issuer_url       TEXT,
        client_id        TEXT,
        client_secret_id TEXT,
        redirect_url     TEXT,
        scopes           TEXT NOT NULL DEFAULT 'openid profile email groups',
        role_claim       TEXT NOT NULL DEFAULT 'groups',
        role_map         TEXT NOT NULL DEFAULT '{}',
        default_role     TEXT NOT NULL DEFAULT 'viewer',
        auto_provision   INTEGER NOT NULL DEFAULT 1,
        updated_at       INTEGER NOT NULL DEFAULT 0
    )"#).execute(&pool).await;

    // Proxy: full edit form, presets, health dashboard
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN custom_headers TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN rate_limit_rpm INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN basic_auth_user TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN basic_auth_pass_hash TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN websocket_extended INTEGER NOT NULL DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN cache_static INTEGER NOT NULL DEFAULT 0").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN health_status TEXT").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN health_checked_at INTEGER").execute(&pool).await;
    let _ = sqlx::query("ALTER TABLE proxy_configs ADD COLUMN health_latency_ms INTEGER").execute(&pool).await;

    // Tiered accounts: guest accounts expire automatically (role = 'guest'),
    // demo accounts are sandboxed to read-only (role = 'demo') — reuses the
    // existing role-string ladder rather than a separate flag column.
    let _ = sqlx::query("ALTER TABLE users ADD COLUMN expires_at INTEGER").execute(&pool).await;

    // AI provider abstraction layer
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS ai_providers (
            id          TEXT PRIMARY KEY,
            kind        TEXT NOT NULL,
            name        TEXT NOT NULL,
            enabled     INTEGER NOT NULL DEFAULT 1,
            base_url    TEXT,
            api_key_ref TEXT,
            model       TEXT,
            priority    INTEGER NOT NULL DEFAULT 50,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await;

    // Fleet node enrollment: pairing codes (single-use, short-lived) mint a node
    // identity that owns exactly one WireGuard peer + one heartbeat bearer token.
    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS node_pairing_codes (
            id          TEXT PRIMARY KEY,
            token_hash  TEXT NOT NULL UNIQUE,
            created_by  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at  INTEGER NOT NULL,
            used_at     INTEGER,
            created_at  INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await;

    let _ = sqlx::query(
        "CREATE TABLE IF NOT EXISTS nodes (
            id              TEXT PRIMARY KEY,
            display_name    TEXT NOT NULL,
            device_type     TEXT NOT NULL DEFAULT 'other',
            owner_user_id   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            wg_peer_id      TEXT,
            wg_public_key   TEXT NOT NULL DEFAULT '',
            token_hash      TEXT NOT NULL,
            last_seen       INTEGER,
            last_telemetry  TEXT,
            agent_capable   INTEGER NOT NULL DEFAULT 0,
            approved        INTEGER NOT NULL DEFAULT 1,
            created_at      INTEGER NOT NULL
        )",
    )
    .execute(&pool)
    .await;

    Ok(pool)
}

pub(crate) async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id          TEXT PRIMARY KEY,
            username    TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'viewer',
            force_password_change INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at  INTEGER NOT NULL,
            created_at  INTEGER NOT NULL,
            ip_address  TEXT,
            user_agent  TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
        CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

        CREATE TABLE IF NOT EXISTS api_tokens (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name        TEXT NOT NULL,
            token_hash  TEXT NOT NULL UNIQUE,
            scopes      TEXT NOT NULL DEFAULT '[]',
            last_used_at INTEGER,
            expires_at  INTEGER,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id          TEXT PRIMARY KEY,
            timestamp   INTEGER NOT NULL,
            user_id     TEXT,
            actor_type  TEXT NOT NULL DEFAULT 'human',
            action      TEXT NOT NULL,
            resource_type TEXT,
            resource_id TEXT,
            outcome     TEXT NOT NULL DEFAULT 'success',
            ip_address  TEXT,
            request_id  TEXT,
            details     TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_log(user_id);

        CREATE TABLE IF NOT EXISTS settings (
            key         TEXT PRIMARY KEY,
            value       TEXT NOT NULL,
            updated_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS themes (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            is_builtin  INTEGER NOT NULL DEFAULT 0,
            is_default  INTEGER NOT NULL DEFAULT 0,
            data        TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS alerts (
            id          TEXT PRIMARY KEY,
            title       TEXT NOT NULL,
            message     TEXT NOT NULL,
            severity    TEXT NOT NULL DEFAULT 'info',
            category    TEXT NOT NULL DEFAULT 'general',
            node_id     TEXT,
            resource_type TEXT,
            resource_id TEXT,
            state       TEXT NOT NULL DEFAULT 'active',
            acknowledged_by TEXT,
            acknowledged_at INTEGER,
            resolved_at INTEGER,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_alerts_state ON alerts(state);
        CREATE INDEX IF NOT EXISTS idx_alerts_severity ON alerts(severity);

        CREATE TABLE IF NOT EXISTS status_checks (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            type        TEXT NOT NULL,
            target      TEXT NOT NULL,
            interval_secs INTEGER NOT NULL DEFAULT 60,
            enabled     INTEGER NOT NULL DEFAULT 1,
            last_checked_at INTEGER,
            last_status TEXT,
            last_latency_ms INTEGER,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS backup_configs (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            source_path     TEXT NOT NULL,
            repo_path       TEXT NOT NULL,
            schedule        TEXT,
            retention_days  INTEGER NOT NULL DEFAULT 30,
            enabled         INTEGER NOT NULL DEFAULT 1,
            last_run_at     INTEGER,
            last_status     TEXT,
            created_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS deployed_apps (
            id          TEXT PRIMARY KEY,
            app_id      TEXT NOT NULL,
            app_name    TEXT NOT NULL,
            project_name TEXT NOT NULL UNIQUE,
            status      TEXT NOT NULL DEFAULT 'running',
            deployed_at INTEGER NOT NULL,
            compose_path TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS proxy_configs (
            id          TEXT PRIMARY KEY,
            domain      TEXT NOT NULL UNIQUE,
            upstream    TEXT NOT NULL,
            ssl         INTEGER NOT NULL DEFAULT 0,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS node_registry (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            address     TEXT NOT NULL,
            agent_port  INTEGER NOT NULL DEFAULT 8744,
            join_token  TEXT,
            state       TEXT NOT NULL DEFAULT 'connected',
            last_seen_at INTEGER,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS wireguard_peers (
            id          TEXT PRIMARY KEY,
            interface   TEXT NOT NULL DEFAULT 'wg0',
            name        TEXT NOT NULL,
            public_key  TEXT NOT NULL UNIQUE,
            allocated_ip TEXT NOT NULL,
            created_at  INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_wg_peers_iface ON wireguard_peers(interface);

        CREATE TABLE IF NOT EXISTS secrets (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            description TEXT,
            value_enc   TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL,
            last_used_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS automation_jobs (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            description     TEXT,
            command         TEXT NOT NULL,
            schedule        TEXT,
            enabled         INTEGER NOT NULL DEFAULT 1,
            timeout_secs    INTEGER NOT NULL DEFAULT 300,
            last_run_at     INTEGER,
            last_status     TEXT,
            last_exit_code  INTEGER,
            created_at      INTEGER NOT NULL,
            updated_at      INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS automation_runs (
            id          TEXT PRIMARY KEY,
            job_id      TEXT NOT NULL REFERENCES automation_jobs(id) ON DELETE CASCADE,
            started_at  INTEGER NOT NULL,
            finished_at INTEGER,
            status      TEXT NOT NULL DEFAULT 'running',
            exit_code   INTEGER,
            output      TEXT NOT NULL DEFAULT ''
        );

        CREATE INDEX IF NOT EXISTS idx_automation_runs_job_id ON automation_runs(job_id);
        CREATE INDEX IF NOT EXISTS idx_automation_runs_started_at ON automation_runs(started_at);

        CREATE TABLE IF NOT EXISTS tags (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL UNIQUE,
            color      TEXT NOT NULL DEFAULT '#6366f1',
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS resource_tags (
            resource_type TEXT NOT NULL,
            resource_id   TEXT NOT NULL,
            tag_id        TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (resource_type, resource_id, tag_id)
        );

        CREATE INDEX IF NOT EXISTS idx_resource_tags_type_id ON resource_tags(resource_type, resource_id);
        CREATE INDEX IF NOT EXISTS idx_resource_tags_tag_id  ON resource_tags(tag_id);

        CREATE TABLE IF NOT EXISTS ssh_sessions (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            host       TEXT NOT NULL,
            port       INTEGER NOT NULL DEFAULT 22,
            username   TEXT NOT NULL,
            key_path   TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used  INTEGER
        );

        CREATE TABLE IF NOT EXISTS local_sessions (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            category   TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used  INTEGER
        );

        CREATE TABLE IF NOT EXISTS proxmox_hosts (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            node        TEXT NOT NULL DEFAULT 'pve',
            fingerprint TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS agent_registry (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            source      TEXT NOT NULL,
            icon        TEXT,
            color       TEXT,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS agent_status (
            agent_id    TEXT PRIMARY KEY REFERENCES agent_registry(id) ON DELETE CASCADE,
            state       TEXT NOT NULL DEFAULT 'offline',
            activity    TEXT,
            task_id     TEXT,
            updated_at  INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS custom_tabs (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            title       TEXT NOT NULL,
            icon        TEXT,
            kind        TEXT NOT NULL,
            config      TEXT NOT NULL DEFAULT '{}',
            sort_order  INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_custom_tabs_user ON custom_tabs(user_id);

        CREATE TABLE IF NOT EXISTS user_nav_config (
            user_id     TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            items       TEXT NOT NULL,
            nav_groups  TEXT NOT NULL,
            updated_at  INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
