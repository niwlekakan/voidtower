-- Resource-backed CMDB / Asset Registry foundation.

CREATE TABLE cmdb_classes (
    key         TEXT PRIMARY KEY,
    label       TEXT NOT NULL,
    description TEXT,
    is_builtin  INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0, 1)),
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE cmdb_types (
    key         TEXT PRIMARY KEY,
    class_key   TEXT NOT NULL REFERENCES cmdb_classes(key) ON DELETE RESTRICT,
    label       TEXT NOT NULL,
    description TEXT,
    is_builtin  INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0, 1)),
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (class_key, key)
);

CREATE INDEX idx_cmdb_types_class
    ON cmdb_types(class_key, enabled, label);

CREATE TABLE cmdb_relationship_types (
    key           TEXT PRIMARY KEY,
    label         TEXT NOT NULL,
    inverse_label TEXT NOT NULL,
    is_builtin    INTEGER NOT NULL DEFAULT 0 CHECK (is_builtin IN (0, 1)),
    enabled       INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL
);

CREATE TABLE cmdb_locations (
    id          TEXT PRIMARY KEY,
    parent_id   TEXT REFERENCES cmdb_locations(id) ON DELETE RESTRICT,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    CHECK (parent_id IS NULL OR parent_id != id),
    UNIQUE (parent_id, name)
);

CREATE INDEX idx_cmdb_locations_parent
    ON cmdb_locations(parent_id, name);
CREATE UNIQUE INDEX idx_cmdb_locations_root_name
    ON cmdb_locations(name)
    WHERE parent_id IS NULL;

CREATE TABLE cmdb_identifier_settings (
    id               TEXT PRIMARY KEY CHECK (id = 'default'),
    prefix           TEXT NOT NULL,
    template         TEXT NOT NULL,
    separator        TEXT NOT NULL,
    number_width     INTEGER NOT NULL CHECK (number_width BETWEEN 1 AND 12),
    starting_number  INTEGER NOT NULL CHECK (starting_number > 0),
    counter_scope    TEXT NOT NULL CHECK (counter_scope IN ('global', 'class', 'type', 'class_type')),
    letter_case      TEXT NOT NULL CHECK (letter_case IN ('preserve', 'lower', 'upper')),
    discovery_policy TEXT NOT NULL CHECK (discovery_policy IN ('off', 'review_first', 'trusted_providers', 'automatic')),
    updated_at       INTEGER NOT NULL
);

CREATE TABLE cmdb_identifier_counters (
    scope_key  TEXT PRIMARY KEY,
    next_number INTEGER NOT NULL CHECK (next_number > 0),
    updated_at INTEGER NOT NULL
);

CREATE TABLE cmdb_assets (
    resource_id      TEXT PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    asset_id         TEXT NOT NULL UNIQUE,
    class_key        TEXT NOT NULL,
    type_key         TEXT NOT NULL,
    subtype          TEXT,
    friendly_name    TEXT,
    description      TEXT,
    manufacturer     TEXT,
    model            TEXT,
    serial_number    TEXT,
    part_number      TEXT,
    lifecycle_status TEXT NOT NULL DEFAULT 'unknown' CHECK (lifecycle_status IN (
                         'unknown', 'new', 'inventory', 'testing', 'available', 'reserved',
                         'deployed', 'maintenance', 'degraded', 'quarantine', 'wipe_pending',
                         'wiping', 'wiped', 'retired', 'disposed', 'lost'
                     )),
    discovery_status TEXT NOT NULL DEFAULT 'manual' CHECK (discovery_status IN (
                         'online', 'offline', 'missing', 'manual', 'unmanaged', 'ignored', 'stale'
                     )),
    condition_status TEXT NOT NULL DEFAULT 'unknown' CHECK (condition_status IN (
                         'unknown', 'new', 'good', 'fair', 'poor', 'damaged', 'failed'
                     )),
    location_id      TEXT REFERENCES cmdb_locations(id) ON DELETE SET NULL,
    first_seen_at    INTEGER,
    last_seen_at     INTEGER,
    metadata_json    TEXT NOT NULL DEFAULT '{}',
    notes            TEXT NOT NULL DEFAULT '',
    revision         INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    FOREIGN KEY (class_key, type_key) REFERENCES cmdb_types(class_key, key) ON DELETE RESTRICT
);

CREATE INDEX idx_cmdb_assets_class_type
    ON cmdb_assets(class_key, type_key, lifecycle_status);
CREATE INDEX idx_cmdb_assets_discovery
    ON cmdb_assets(discovery_status, last_seen_at);
CREATE INDEX idx_cmdb_assets_location
    ON cmdb_assets(location_id, asset_id);

CREATE TABLE cmdb_asset_identities (
    id               TEXT PRIMARY KEY,
    resource_id      TEXT NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    identity_kind    TEXT NOT NULL,
    normalized_value TEXT NOT NULL,
    confidence       TEXT NOT NULL CHECK (confidence IN ('strong', 'weak')),
    source           TEXT NOT NULL,
    first_seen_at    INTEGER NOT NULL,
    last_seen_at     INTEGER NOT NULL,
    UNIQUE (resource_id, identity_kind, normalized_value, source)
);

CREATE INDEX idx_cmdb_asset_identities_lookup
    ON cmdb_asset_identities(identity_kind, normalized_value);
CREATE UNIQUE INDEX idx_cmdb_asset_identities_strong_unique
    ON cmdb_asset_identities(identity_kind, normalized_value)
    WHERE identity_kind IN ('wwn', 'nvme_uuid', 'nvme_eui', 'hardware_uuid');

CREATE TABLE cmdb_inventory_snapshots (
    id                 TEXT PRIMARY KEY,
    source_resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE RESTRICT,
    node_id            TEXT,
    snapshot_id        TEXT NOT NULL,
    schema_version     INTEGER NOT NULL CHECK (schema_version > 0),
    collector_version  TEXT NOT NULL,
    platform           TEXT NOT NULL,
    collected_at       INTEGER NOT NULL,
    received_at        INTEGER NOT NULL,
    state              TEXT NOT NULL CHECK (state IN ('processing', 'completed')),
    fingerprint        TEXT NOT NULL,
    result_json        TEXT,
    UNIQUE (source_resource_id, snapshot_id)
);

CREATE INDEX idx_cmdb_inventory_snapshots_source_time
    ON cmdb_inventory_snapshots(source_resource_id, received_at DESC);

CREATE TABLE cmdb_observations (
    id                   TEXT PRIMARY KEY,
    resource_id          TEXT REFERENCES resources(id) ON DELETE SET NULL,
    source_resource_id   TEXT NOT NULL REFERENCES resources(id) ON DELETE RESTRICT,
    snapshot_row_id      TEXT NOT NULL REFERENCES cmdb_inventory_snapshots(id) ON DELETE RESTRICT,
    provider             TEXT NOT NULL,
    scope_key            TEXT NOT NULL,
    entity_key           TEXT NOT NULL,
    entity_type          TEXT NOT NULL,
    schema_version       INTEGER NOT NULL CHECK (schema_version > 0),
    identity_json        TEXT NOT NULL,
    attributes_json      TEXT NOT NULL,
    runtime_json         TEXT NOT NULL,
    health_json          TEXT NOT NULL,
    provider_observed_at INTEGER,
    received_at          INTEGER NOT NULL,
    first_seen_at        INTEGER NOT NULL,
    last_seen_at         INTEGER NOT NULL,
    state                TEXT NOT NULL CHECK (state IN (
                             'online', 'offline', 'missing', 'ignored', 'stale', 'review'
                         )),
    fingerprint          TEXT NOT NULL,
    UNIQUE (provider, source_resource_id, scope_key, entity_key)
);

CREATE INDEX idx_cmdb_observations_asset_state
    ON cmdb_observations(resource_id, state, last_seen_at DESC);
CREATE INDEX idx_cmdb_observations_source_state
    ON cmdb_observations(source_resource_id, state, last_seen_at DESC);
CREATE INDEX idx_cmdb_observations_review
    ON cmdb_observations(state, received_at DESC)
    WHERE resource_id IS NULL;

CREATE TABLE cmdb_discovery_decisions (
    id             TEXT PRIMARY KEY,
    observation_id TEXT NOT NULL REFERENCES cmdb_observations(id) ON DELETE CASCADE,
    fingerprint    TEXT NOT NULL,
    decision       TEXT NOT NULL CHECK (decision IN ('registered', 'linked', 'ignored')),
    decided_by     TEXT,
    notes          TEXT,
    decided_at     INTEGER NOT NULL,
    UNIQUE (observation_id, fingerprint)
);

CREATE TABLE cmdb_relationships (
    id                      TEXT PRIMARY KEY,
    source_resource_id      TEXT NOT NULL REFERENCES resources(id) ON DELETE RESTRICT,
    destination_resource_id TEXT NOT NULL REFERENCES resources(id) ON DELETE RESTRICT,
    type_key                TEXT NOT NULL REFERENCES cmdb_relationship_types(key) ON DELETE RESTRICT,
    started_at              INTEGER NOT NULL,
    ended_at                INTEGER,
    active                  INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
    metadata_json           TEXT NOT NULL DEFAULT '{}',
    created_at              INTEGER NOT NULL,
    updated_at              INTEGER NOT NULL,
    CHECK (source_resource_id != destination_resource_id),
    CHECK ((active = 1 AND ended_at IS NULL) OR (active = 0 AND ended_at IS NOT NULL))
);

CREATE INDEX idx_cmdb_relationships_source
    ON cmdb_relationships(source_resource_id, active, type_key);
CREATE INDEX idx_cmdb_relationships_destination
    ON cmdb_relationships(destination_resource_id, active, type_key);
CREATE UNIQUE INDEX idx_cmdb_relationships_active_edge
    ON cmdb_relationships(source_resource_id, destination_resource_id, type_key)
    WHERE active = 1;
CREATE UNIQUE INDEX idx_cmdb_relationships_single_install
    ON cmdb_relationships(source_resource_id, type_key)
    WHERE active = 1 AND type_key = 'installed_in';
