-- 0010: read-only NPM (nginx-proxy-manager) adapter for the app vault.
--
-- Additive only: existing rows keep their data and default to kind='docker',
-- visible (hidden=0) and with no external link. hostname/url are reconciled
-- from NPM proxy hosts by POST /api/npm/sync, never written back to NPM.
ALTER TABLE deployed_apps ADD COLUMN url TEXT;
ALTER TABLE deployed_apps ADD COLUMN hostname TEXT;
ALTER TABLE deployed_apps ADD COLUMN kind TEXT NOT NULL DEFAULT 'docker';
ALTER TABLE deployed_apps ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;
