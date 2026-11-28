-- Secret-manager convergence: allow operators to disable a secret without
-- deleting its metadata or encrypted value.
ALTER TABLE secrets
    ADD COLUMN disabled INTEGER NOT NULL DEFAULT 0
    CHECK (disabled IN (0, 1));
