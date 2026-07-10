CREATE TABLE voidwatch_default_allowlist (
            id            TEXT PRIMARY KEY,
            actor_type    TEXT NOT NULL,
            action        TEXT NOT NULL,
            resource_type TEXT NOT NULL,
            created_at    INTEGER NOT NULL
        )
;
CREATE TABLE voidwatch_mode_settings (
            scope         TEXT PRIMARY KEY,
            mode          TEXT NOT NULL DEFAULT 'observer',
            updated_at    INTEGER NOT NULL,
            updated_by    TEXT
        )
;
