-- Canonical VoidTower schema baseline. Keep immutable after release.
-- Legacy conditional column additions are handled before SQLx records this migration.

CREATE TABLE IF NOT EXISTS agent_registry (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            source      TEXT NOT NULL,
            icon        TEXT,
            color       TEXT,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS agent_status (
            agent_id    TEXT PRIMARY KEY REFERENCES agent_registry(id) ON DELETE CASCADE,
            state       TEXT NOT NULL DEFAULT 'offline',
            activity    TEXT,
            task_id     TEXT,
            updated_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS ai_providers (
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
        )
;
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
        )
;
CREATE TABLE IF NOT EXISTS api_tokens (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name        TEXT NOT NULL,
            token_hash  TEXT NOT NULL UNIQUE,
            scopes      TEXT NOT NULL DEFAULT '[]',
            last_used_at INTEGER,
            expires_at  INTEGER,
            created_at  INTEGER NOT NULL
        , secret_ids TEXT)
;
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
        , source TEXT)
;
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
        )
;
CREATE TABLE IF NOT EXISTS automation_runs (
            id          TEXT PRIMARY KEY,
            job_id      TEXT NOT NULL REFERENCES automation_jobs(id) ON DELETE CASCADE,
            started_at  INTEGER NOT NULL,
            finished_at INTEGER,
            status      TEXT NOT NULL DEFAULT 'running',
            exit_code   INTEGER,
            output      TEXT NOT NULL DEFAULT ''
        )
;
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
        , last_check_at INTEGER, last_check_status TEXT, last_restore_test_at INTEGER, last_restore_test_status TEXT, restore_test_schedule TEXT)
;
CREATE TABLE IF NOT EXISTS custom_tabs (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            title       TEXT NOT NULL,
            icon        TEXT,
            kind        TEXT NOT NULL,
            config      TEXT NOT NULL DEFAULT '{}',
            sort_order  INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS deployed_apps (
            id          TEXT PRIMARY KEY,
            app_id      TEXT NOT NULL,
            app_name    TEXT NOT NULL,
            project_name TEXT NOT NULL UNIQUE,
            status      TEXT NOT NULL DEFAULT 'running',
            deployed_at INTEGER NOT NULL,
            compose_path TEXT NOT NULL
        , primary_port INTEGER, origin TEXT NOT NULL DEFAULT 'voidtower', owner_user_id TEXT, storage_root TEXT, target_node_id TEXT)
;
CREATE TABLE IF NOT EXISTS local_sessions (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            category   TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used  INTEGER
        )
;
CREATE TABLE IF NOT EXISTS member_app_access (
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            app_id      TEXT NOT NULL,
            granted_at  INTEGER NOT NULL,
            PRIMARY KEY (user_id, app_id)
        )
;
CREATE TABLE IF NOT EXISTS member_drives (
            id            TEXT PRIMARY KEY,
            user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            label         TEXT NOT NULL,
            host_path     TEXT NOT NULL,
            total_bytes   INTEGER,
            free_bytes    INTEGER,
            last_check_at INTEGER,
            created_at    INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS member_settings (
            user_id           TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            can_deploy_custom INTEGER NOT NULL DEFAULT 0,
            updated_at        INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS member_storage (
            user_id       TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            quota_bytes   INTEGER NOT NULL DEFAULT 5368709120,
            max_apps      INTEGER NOT NULL DEFAULT 5,
            used_bytes    INTEGER NOT NULL DEFAULT 0,
            last_check_at INTEGER
        )
;
CREATE TABLE IF NOT EXISTS node_pairing_codes (
            id          TEXT PRIMARY KEY,
            token_hash  TEXT NOT NULL UNIQUE,
            created_by  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at  INTEGER NOT NULL,
            used_at     INTEGER,
            created_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS node_registry (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            address     TEXT NOT NULL,
            agent_port  INTEGER NOT NULL DEFAULT 8744,
            join_token  TEXT,
            state       TEXT NOT NULL DEFAULT 'connected',
            last_seen_at INTEGER,
            created_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS nodes (
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
        )
;
CREATE TABLE IF NOT EXISTS oidc_config (
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
    )
;
CREATE TABLE IF NOT EXISTS plugins (
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
    )
;
CREATE TABLE IF NOT EXISTS policy_rules (
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
    )
;
CREATE TABLE IF NOT EXISTS proxmox_hosts (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            node        TEXT NOT NULL DEFAULT 'pve',
            fingerprint TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )
;
CREATE TABLE IF NOT EXISTS proxy_configs (
            id          TEXT PRIMARY KEY,
            domain      TEXT NOT NULL UNIQUE,
            upstream    TEXT NOT NULL,
            ssl         INTEGER NOT NULL DEFAULT 0,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        , allow_embed INTEGER NOT NULL DEFAULT 0, embed_port INTEGER, sso_protect INTEGER NOT NULL DEFAULT 0, custom_headers TEXT, rate_limit_rpm INTEGER, basic_auth_user TEXT, basic_auth_pass_hash TEXT, websocket_extended INTEGER NOT NULL DEFAULT 0, cache_static INTEGER NOT NULL DEFAULT 0, health_status TEXT, health_checked_at INTEGER, health_latency_ms INTEGER)
;
CREATE TABLE IF NOT EXISTS resource_tags (
            resource_type TEXT NOT NULL,
            resource_id   TEXT NOT NULL,
            tag_id        TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (resource_type, resource_id, tag_id)
        )
;
CREATE TABLE IF NOT EXISTS secrets (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            description TEXT,
            value_enc   TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL,
            last_used_at INTEGER
        , version INTEGER NOT NULL DEFAULT 0)
;
CREATE TABLE IF NOT EXISTS sessions (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at  INTEGER NOT NULL,
            created_at  INTEGER NOT NULL,
            ip_address  TEXT,
            user_agent  TEXT
        )
;
CREATE TABLE IF NOT EXISTS settings (
            key         TEXT PRIMARY KEY,
            value       TEXT NOT NULL,
            updated_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS ssh_sessions (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            host       TEXT NOT NULL,
            port       INTEGER NOT NULL DEFAULT 22,
            username   TEXT NOT NULL,
            key_path   TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used  INTEGER
        , password_enc TEXT)
;
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
        )
;
CREATE TABLE IF NOT EXISTS tags (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL UNIQUE,
            color      TEXT NOT NULL DEFAULT '#6366f1',
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        )
;
CREATE TABLE IF NOT EXISTS themes (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            is_builtin  INTEGER NOT NULL DEFAULT 0,
            is_default  INTEGER NOT NULL DEFAULT 0,
            data        TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS user_nav_config (
            user_id     TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
            items       TEXT NOT NULL,
            nav_groups  TEXT NOT NULL,
            updated_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS users (
            id          TEXT PRIMARY KEY,
            username    TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'viewer',
            force_password_change INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        , totp_secret TEXT, totp_enabled INTEGER NOT NULL DEFAULT 0, auth_source TEXT NOT NULL DEFAULT 'local', oidc_subject TEXT, expires_at INTEGER)
;
CREATE TABLE IF NOT EXISTS voidwatch_default_allowlist (
            id            TEXT PRIMARY KEY,
            actor_type    TEXT NOT NULL,
            action        TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            created_at    INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS voidwatch_mode_settings (
            scope         TEXT PRIMARY KEY,
            mode          TEXT NOT NULL DEFAULT 'observer',
            updated_at    INTEGER NOT NULL,
            updated_by    TEXT
        )
;
CREATE TABLE IF NOT EXISTS webhook_configs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            type        TEXT NOT NULL DEFAULT 'generic',
            events      TEXT NOT NULL DEFAULT '["alert.created"]',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        )
;
CREATE TABLE IF NOT EXISTS wireguard_peers (
            id          TEXT PRIMARY KEY,
            interface   TEXT NOT NULL DEFAULT 'wg0',
            name        TEXT NOT NULL,
            public_key  TEXT NOT NULL UNIQUE,
            allocated_ip TEXT NOT NULL,
            created_at  INTEGER NOT NULL
        )
;

CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_audit_user_id ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_alerts_state ON alerts(state);
CREATE INDEX IF NOT EXISTS idx_alerts_severity ON alerts(severity);
CREATE UNIQUE INDEX IF NOT EXISTS idx_automation_jobs_name ON automation_jobs(name);
CREATE INDEX IF NOT EXISTS idx_automation_runs_job_id ON automation_runs(job_id);
CREATE INDEX IF NOT EXISTS idx_automation_runs_started_at ON automation_runs(started_at);
CREATE INDEX IF NOT EXISTS idx_resource_tags_type_id ON resource_tags(resource_type, resource_id);
CREATE INDEX IF NOT EXISTS idx_resource_tags_tag_id ON resource_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_wg_peers_iface ON wireguard_peers(interface);
CREATE INDEX IF NOT EXISTS idx_custom_tabs_user ON custom_tabs(user_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_oidc_subject ON users(oidc_subject) WHERE oidc_subject IS NOT NULL;
