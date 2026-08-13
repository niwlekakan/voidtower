-- Shared resource, capability, job, approval, and event contracts.

CREATE TABLE resources (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    node_id         TEXT,
    provider        TEXT,
    lifecycle_state TEXT NOT NULL DEFAULT 'active'
                    CHECK (lifecycle_state IN ('active', 'unavailable', 'retired')),
    revision        INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX idx_resources_kind_state
    ON resources(kind, lifecycle_state);

CREATE TABLE resource_aliases (
    resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    namespace   TEXT NOT NULL,
    scope_key   TEXT NOT NULL,
    value       TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    PRIMARY KEY (resource_id, namespace, scope_key, value),
    UNIQUE (namespace, scope_key, value)
);

CREATE INDEX idx_resource_aliases_resource
    ON resource_aliases(resource_id);

CREATE TABLE resource_capabilities (
    resource_id  TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    action       TEXT NOT NULL,
    availability TEXT NOT NULL
                 CHECK (availability IN ('available', 'unavailable', 'unknown')),
    reason_code  TEXT,
    detail       TEXT,
    schema_version INTEGER NOT NULL DEFAULT 1 CHECK (schema_version > 0),
    observed_at  INTEGER NOT NULL,
    PRIMARY KEY (resource_id, action)
);

CREATE INDEX idx_resource_capabilities_action
    ON resource_capabilities(action, availability);

CREATE TABLE jobs (
    id                  TEXT PRIMARY KEY,
    action              TEXT NOT NULL,
    resource_id         TEXT NOT NULL REFERENCES resources(id) ON DELETE RESTRICT,
    resource_revision   INTEGER NOT NULL CHECK (resource_revision >= 0),
    actor_type          TEXT NOT NULL,
    actor_id            TEXT,
    actor_source        TEXT,
    ingress             TEXT NOT NULL,
    input_json          TEXT NOT NULL,
    request_digest      TEXT NOT NULL,
    plan_json           TEXT NOT NULL,
    plan_digest         TEXT NOT NULL,
    external_fingerprint TEXT NOT NULL,
    state               TEXT NOT NULL CHECK (state IN (
                            'awaiting_approval', 'queued', 'running', 'succeeded',
                            'failed', 'cancelled', 'needs_attention', 'rejected', 'expired'
                        )),
    progress_current    INTEGER NOT NULL DEFAULT 0 CHECK (progress_current >= 0),
    progress_total      INTEGER NOT NULL DEFAULT 0 CHECK (progress_total >= 0),
    progress_message    TEXT,
    idempotency_scope   TEXT NOT NULL,
    idempotency_key     TEXT NOT NULL,
    concurrency_key     TEXT NOT NULL,
    retry_class         TEXT NOT NULL,
    recovery_class      TEXT NOT NULL,
    cancel_requested    INTEGER NOT NULL DEFAULT 0 CHECK (cancel_requested IN (0, 1)),
    lease_owner         TEXT,
    lease_expires_at    INTEGER,
    result_json         TEXT,
    error_code          TEXT,
    error_message       TEXT,
    submitted_at        INTEGER NOT NULL,
    queued_at           INTEGER,
    started_at          INTEGER,
    finished_at         INTEGER,
    updated_at          INTEGER NOT NULL,
    UNIQUE (idempotency_scope, idempotency_key)
);

CREATE INDEX idx_jobs_state_queue
    ON jobs(state, submitted_at);
CREATE INDEX idx_jobs_resource_time
    ON jobs(resource_id, submitted_at DESC);
CREATE INDEX idx_jobs_lease
    ON jobs(state, lease_expires_at);
CREATE INDEX idx_jobs_concurrency
    ON jobs(concurrency_key, state);

CREATE TABLE job_steps (
    id                  TEXT PRIMARY KEY,
    job_id              TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    position            INTEGER NOT NULL CHECK (position >= 0),
    kind                TEXT NOT NULL,
    name                TEXT NOT NULL,
    state               TEXT NOT NULL CHECK (state IN (
                            'pending', 'running', 'succeeded', 'failed',
                            'cancelled', 'needs_attention'
                        )),
    retry_class         TEXT NOT NULL,
    recovery_class      TEXT NOT NULL,
    progress_current    INTEGER NOT NULL DEFAULT 0 CHECK (progress_current >= 0),
    progress_total      INTEGER NOT NULL DEFAULT 0 CHECK (progress_total >= 0),
    external_operation_id TEXT,
    result_json         TEXT,
    error_code          TEXT,
    error_message       TEXT,
    started_at          INTEGER,
    finished_at         INTEGER,
    updated_at          INTEGER NOT NULL,
    UNIQUE (job_id, position)
);

CREATE INDEX idx_job_steps_job_state
    ON job_steps(job_id, state, position);

CREATE TABLE job_attempts (
    id              TEXT PRIMARY KEY,
    job_id          TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    step_id         TEXT NOT NULL REFERENCES job_steps(id) ON DELETE CASCADE,
    attempt_number  INTEGER NOT NULL CHECK (attempt_number > 0),
    worker_id       TEXT NOT NULL,
    lease_expires_at INTEGER NOT NULL,
    started_at      INTEGER NOT NULL,
    finished_at     INTEGER,
    outcome         TEXT,
    diagnostic_json TEXT,
    UNIQUE (step_id, attempt_number)
);

CREATE INDEX idx_job_attempts_job
    ON job_attempts(job_id, started_at);

CREATE TABLE approvals (
    id                  TEXT PRIMARY KEY,
    job_id              TEXT NOT NULL UNIQUE REFERENCES jobs(id) ON DELETE RESTRICT,
    requirement         TEXT NOT NULL,
    reason              TEXT NOT NULL,
    status              TEXT NOT NULL CHECK (status IN (
                            'pending', 'approved', 'rejected', 'expired', 'stale'
                        )),
    expires_at          INTEGER NOT NULL,
    request_digest      TEXT NOT NULL,
    plan_digest         TEXT NOT NULL,
    resource_revision   INTEGER NOT NULL CHECK (resource_revision >= 0),
    external_fingerprint TEXT NOT NULL,
    decided_by          TEXT,
    decision_comment    TEXT,
    requested_at        INTEGER NOT NULL,
    decided_at          INTEGER,
    updated_at          INTEGER NOT NULL
);

CREATE INDEX idx_approvals_status_expiry
    ON approvals(status, expires_at);

CREATE TABLE events (
    sequence        INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id        TEXT NOT NULL UNIQUE,
    schema_version  INTEGER NOT NULL DEFAULT 1 CHECK (schema_version > 0),
    event_type      TEXT NOT NULL,
    occurred_at     INTEGER NOT NULL,
    actor_type      TEXT,
    actor_id        TEXT,
    actor_source    TEXT,
    resource_id     TEXT REFERENCES resources(id) ON DELETE RESTRICT,
    job_id          TEXT REFERENCES jobs(id) ON DELETE RESTRICT,
    approval_id     TEXT REFERENCES approvals(id) ON DELETE RESTRICT,
    correlation_id  TEXT NOT NULL,
    causation_id    TEXT,
    payload_json    TEXT NOT NULL
);

CREATE INDEX idx_events_time
    ON events(occurred_at, sequence);
CREATE INDEX idx_events_resource
    ON events(resource_id, sequence);
CREATE INDEX idx_events_job
    ON events(job_id, sequence);
CREATE INDEX idx_events_type
    ON events(event_type, sequence);
