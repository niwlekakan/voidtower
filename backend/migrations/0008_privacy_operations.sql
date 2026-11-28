-- Household consent/purpose records and durable privacy operation status.
-- Sensitive exports remain local by default; cloud export requires explicit consent.
CREATE TABLE household_consents (
    id               TEXT PRIMARY KEY,
    schema_version   INTEGER NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    household_id     TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose          TEXT NOT NULL CHECK (purpose IN ('privacy_export', 'ai_assistance', 'cloud_export', 'support')),
    domain           TEXT NOT NULL CHECK (domain IN ('identity', 'inventory', 'telemetry', 'configuration')),
    resource_id      TEXT REFERENCES resources(id) ON DELETE CASCADE,
    destination      TEXT NOT NULL DEFAULT 'local' CHECK (destination IN ('local', 'cloud')),
    ai_cloud_export  INTEGER NOT NULL DEFAULT 0 CHECK (ai_cloud_export IN (0, 1)),
    status           TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'withdrawn', 'expired')),
    expires_at       INTEGER,
    retention_until  INTEGER,
    withdrawn_at     INTEGER,
    created_by       TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    CHECK (expires_at IS NULL OR expires_at > created_at),
    CHECK (retention_until IS NULL OR retention_until >= created_at),
    CHECK (destination = 'local' OR ai_cloud_export = 1),
    CHECK (withdrawn_at IS NULL OR status = 'withdrawn')
);

CREATE INDEX idx_household_consents_scope
    ON household_consents(household_id, user_id, status, purpose, domain, resource_id);

CREATE TABLE privacy_requests (
    id               TEXT PRIMARY KEY,
    schema_version   INTEGER NOT NULL DEFAULT 1 CHECK (schema_version = 1),
    household_id     TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind             TEXT NOT NULL CHECK (kind IN ('export', 'deletion')),
    purpose          TEXT NOT NULL CHECK (purpose IN ('privacy_export', 'privacy_deletion')),
    domain           TEXT NOT NULL CHECK (domain IN ('identity', 'inventory', 'telemetry', 'configuration')),
    resource_id      TEXT REFERENCES resources(id) ON DELETE RESTRICT,
    destination      TEXT NOT NULL DEFAULT 'local' CHECK (destination IN ('local', 'cloud')),
    minimization     TEXT NOT NULL CHECK (minimization IN ('metadata', 'records')),
    idempotency_key  TEXT NOT NULL CHECK (length(idempotency_key) BETWEEN 1 AND 128),
    state            TEXT NOT NULL DEFAULT 'queued' CHECK (state IN ('queued', 'processing', 'succeeded', 'failed', 'cancelled')),
    failure_code     TEXT CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 64),
    failure_message  TEXT CHECK (failure_message IS NULL OR length(failure_message) <= 256),
    correlation_id   TEXT NOT NULL,
    requested_by     TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    completed_at     INTEGER,
    UNIQUE (household_id, user_id, kind, idempotency_key)
);

CREATE INDEX idx_privacy_requests_household_status
    ON privacy_requests(household_id, state, created_at DESC);
CREATE INDEX idx_privacy_requests_user_time
    ON privacy_requests(user_id, created_at DESC);