-- Provenance (ADR-008 constraint 1): this is a machine-generated dump, not a
-- hand-transcription — but it is NOT a dump of a tagged `v0.9.0` release, because
-- no such tag exists. Verified directly: `git tag -l 'v0.9.0'` and
-- `git ls-remote --tags origin` both return nothing, in this checkout or on the
-- remote.
--
-- What does exist and is reachable from `main`: commit
-- eb5526d0fd00b86610429062a0db972b3a207bd5 ("chore: version 0.9.0 — CHANGELOG,
-- ROADMAP update, version bump", 2026-06-08), which is the commit that bumped
-- `backend/Cargo.toml`'s `version` field from 0.1.0 to 0.9.0 — i.e. the exact
-- point in history where the repo became "v0.9.0" per gap-analysis.md's own
-- basis line ("main (v0.9.0, cloned 2026-07-08)"), which cites no separate
-- commit hash of its own.
--
-- This fixture was produced by checking out eb5526d0 into a scratch git
-- worktree (not this task's working tree), building its actual
-- `backend/src/db/mod.rs` at that commit, running its real `init_pool()`
-- against a fresh on-disk database, and dumping `sqlite_master` the same way
-- `schema_golden.sql` is generated for HEAD (see that file's test for the
-- method) — not by reading diffs and re-typing CREATE TABLE statements by
-- hand. `sqlite_sequence` (SQLite's own internal autoincrement bookkeeping
-- table) is excluded, same as the HEAD dump.
--
-- Net effect: every table/column below is a verified historical schema, just
-- not verified against a git tag (none was ever created for this release).
CREATE TABLE alerts (
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
CREATE TABLE api_tokens (
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
CREATE TABLE audit_log (
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
CREATE TABLE automation_jobs (
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
CREATE TABLE automation_runs (
            id          TEXT PRIMARY KEY,
            job_id      TEXT NOT NULL REFERENCES automation_jobs(id) ON DELETE CASCADE,
            started_at  INTEGER NOT NULL,
            finished_at INTEGER,
            status      TEXT NOT NULL DEFAULT 'running',
            exit_code   INTEGER,
            output      TEXT NOT NULL DEFAULT ''
        )
;
CREATE TABLE backup_configs (
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
CREATE TABLE deployed_apps (
            id          TEXT PRIMARY KEY,
            app_id      TEXT NOT NULL,
            app_name    TEXT NOT NULL,
            project_name TEXT NOT NULL UNIQUE,
            status      TEXT NOT NULL DEFAULT 'running',
            deployed_at INTEGER NOT NULL,
            compose_path TEXT NOT NULL
        , primary_port INTEGER, origin TEXT NOT NULL DEFAULT 'voidtower')
;
CREATE TABLE local_sessions (
            id         TEXT PRIMARY KEY,
            label      TEXT NOT NULL,
            category   TEXT,
            created_at INTEGER NOT NULL DEFAULT (unixepoch()),
            last_used  INTEGER
        )
;
CREATE TABLE node_registry (
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
CREATE TABLE plugins (
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
CREATE TABLE policy_rules (
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
CREATE TABLE proxmox_hosts (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            node        TEXT NOT NULL DEFAULT 'pve',
            fingerprint TEXT,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        )
;
CREATE TABLE proxy_configs (
            id          TEXT PRIMARY KEY,
            domain      TEXT NOT NULL UNIQUE,
            upstream    TEXT NOT NULL,
            ssl         INTEGER NOT NULL DEFAULT 0,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        , allow_embed INTEGER NOT NULL DEFAULT 0, embed_port INTEGER)
;
CREATE TABLE resource_tags (
            resource_type TEXT NOT NULL,
            resource_id   TEXT NOT NULL,
            tag_id        TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
            PRIMARY KEY (resource_type, resource_id, tag_id)
        )
;
CREATE TABLE secrets (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            description TEXT,
            value_enc   TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL,
            last_used_at INTEGER
        , version INTEGER NOT NULL DEFAULT 0)
;
CREATE TABLE sessions (
            id          TEXT PRIMARY KEY,
            user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at  INTEGER NOT NULL,
            created_at  INTEGER NOT NULL,
            ip_address  TEXT,
            user_agent  TEXT
        )
;
CREATE TABLE settings (
            key         TEXT PRIMARY KEY,
            value       TEXT NOT NULL,
            updated_at  INTEGER NOT NULL
        )
;
CREATE TABLE ssh_sessions (
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
CREATE TABLE status_checks (
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
CREATE TABLE tags (
            id         TEXT PRIMARY KEY,
            name       TEXT NOT NULL UNIQUE,
            color      TEXT NOT NULL DEFAULT '#6366f1',
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        )
;
CREATE TABLE themes (
            id          TEXT PRIMARY KEY,
            name        TEXT NOT NULL UNIQUE,
            is_builtin  INTEGER NOT NULL DEFAULT 0,
            is_default  INTEGER NOT NULL DEFAULT 0,
            data        TEXT NOT NULL,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        )
;
CREATE TABLE users (
            id          TEXT PRIMARY KEY,
            username    TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role        TEXT NOT NULL DEFAULT 'viewer',
            force_password_change INTEGER NOT NULL DEFAULT 0,
            created_at  INTEGER NOT NULL,
            updated_at  INTEGER NOT NULL
        , totp_secret TEXT, totp_enabled INTEGER NOT NULL DEFAULT 0)
;
CREATE TABLE webhook_configs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL,
            url         TEXT NOT NULL,
            type        TEXT NOT NULL DEFAULT 'generic',
            events      TEXT NOT NULL DEFAULT '["alert.created"]',
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  INTEGER NOT NULL
        )
;
CREATE TABLE wireguard_peers (
            id          TEXT PRIMARY KEY,
            interface   TEXT NOT NULL DEFAULT 'wg0',
            name        TEXT NOT NULL,
            public_key  TEXT NOT NULL UNIQUE,
            allocated_ip TEXT NOT NULL,
            created_at  INTEGER NOT NULL
        )
;
