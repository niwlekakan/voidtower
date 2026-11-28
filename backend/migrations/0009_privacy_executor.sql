-- Durable local privacy executor state. Request rows are retained as bounded tombstones;
-- artifacts never store provider credentials or raw diagnostics.
ALTER TABLE privacy_requests ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0);
ALTER TABLE privacy_requests ADD COLUMN lease_owner TEXT;
ALTER TABLE privacy_requests ADD COLUMN lease_expires_at INTEGER;
ALTER TABLE privacy_requests ADD COLUMN cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1));
ALTER TABLE privacy_requests ADD COLUMN artifact_id TEXT;

CREATE INDEX idx_privacy_requests_executor
    ON privacy_requests(state, lease_expires_at, created_at);

CREATE TABLE privacy_artifacts (
    id           TEXT PRIMARY KEY,
    request_id   TEXT NOT NULL UNIQUE REFERENCES privacy_requests(id) ON DELETE RESTRICT,
    relative_path TEXT NOT NULL CHECK (length(relative_path) BETWEEN 1 AND 256),
    byte_length  INTEGER NOT NULL CHECK (byte_length >= 0),
    sha256       TEXT NOT NULL CHECK (length(sha256) = 64),
    created_at   INTEGER NOT NULL
);

CREATE TABLE privacy_holds (
    id           TEXT PRIMARY KEY,
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    domain       TEXT NOT NULL CHECK (domain IN ('identity', 'inventory', 'telemetry', 'configuration')),
    resource_id  TEXT REFERENCES resources(id) ON DELETE RESTRICT,
    reason_code  TEXT NOT NULL CHECK (length(reason_code) BETWEEN 1 AND 64),
    created_at   INTEGER NOT NULL
);

CREATE INDEX idx_privacy_holds_scope
    ON privacy_holds(household_id, user_id, domain, resource_id);
