-- Canonical household membership and least-privilege resource grants.
-- resources.id remains the sole canonical operational/CMDB identity.
CREATE TABLE households (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK (length(name) BETWEEN 1 AND 128)
);

CREATE TABLE household_memberships (
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'guest', 'device')),
    status       TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'revoked')),
    expires_at   INTEGER,
    revoked_at   INTEGER,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (household_id, user_id)
);

CREATE INDEX idx_household_memberships_user_status
    ON household_memberships(user_id, status, expires_at);

CREATE TABLE resource_grants (
    household_id TEXT NOT NULL REFERENCES households(id) ON DELETE CASCADE,
    resource_id  TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    permission   TEXT NOT NULL CHECK (permission IN ('read', 'operate', 'admin')),
    granted_by   TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    expires_at   INTEGER,
    revoked_at   INTEGER,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    PRIMARY KEY (household_id, resource_id, user_id),
    CHECK (expires_at IS NULL OR expires_at > created_at)
);

CREATE INDEX idx_resource_grants_user_resource
    ON resource_grants(user_id, resource_id, revoked_at, expires_at);
CREATE INDEX idx_resource_grants_household
    ON resource_grants(household_id, resource_id, user_id);
