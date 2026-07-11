# ADR-008 — Full schema golden-file and upgrade-test grant for Phase P1

**Status:** Accepted (signed by operator niwlekakan on 2026-07-11)
**Date:** 2026-07-11
**Expires:** at P1 phase exit (`scripts/devteam/adr.sh revoke ADR-008`)

## Context

Gap-analysis §3's P1 table, row 6: "Schema | Fresh-DB schema dump golden file + upgrade test
from a seeded v0.9.0 database | Prerequisite for P2." P2 (schema discipline, converting
`db/mod.rs`'s accretion into sqlx migration files) depends on this existing first — the
golden dump becomes `0000_baseline.sql`'s acceptance criterion, per gap-analysis §4.

ADR-002 (P0, now expired) already established the narrower precedent: additive-only
`CREATE TABLE` grants to `db/mod.rs`, plus a golden-file/upgrade-test pair scoped to the two
new `voidwatch_*` tables it introduced (`backend/tests/schema_golden.sql`,
`schema_golden_file_matches_live_schema_after_migration`,
`pre_existing_tables_byte_identical_after_upgrade` — all verified present in
`backend/src/db/mod.rs` at ADR-drafting time). This ADR generalizes that pattern to **every**
table in the schema, not just the two P0 added, and adds the second half gap-analysis asks
for: an upgrade test from a fixture representing the actual v0.9.0 tag's schema (not just
"whatever `run_migrations` produces today," which has already drifted from v0.9.0 — see
gap-analysis §1's table noting the current repo is past v0.9.0).

## Decision

Task specs numbered `P1-*` that cite **ADR-008** are granted modification rights to exactly:

```granted-paths
backend/src/db/mod.rs
backend/tests/schema_golden.sql
backend/tests/schema_v0_9_0_seed.sql
backend/tests/**
```

Scoped to:

- Extending `backend/tests/schema_golden.sql` from its current two-table scope to a full
  dump of every table `run_migrations()` + `init_pool()` create (§3 of
  `docs/codebase-map.md` enumerates them: baseline tables, `webhook_configs`,
  `policy_rules`, `plugins`, `oidc_config`, `ai_providers`, `node_pairing_codes`, `nodes`,
  `member_app_access`, `member_storage`, `member_drives`, `member_settings`,
  `voidwatch_default_allowlist`, `voidwatch_mode_settings`, and every `ALTER TABLE ADD
  COLUMN` — the map's §3 caveat about needing to verify against source, not the map itself,
  applies; re-derive the live list from `db/mod.rs` directly, don't transcribe the map).
- A **test-only** addition to `backend/src/db/mod.rs`'s existing `mod tests` block:
  generalizing `schema_golden_file_matches_live_schema_after_migration` to the full dump
  above (or adding a sibling test if generalizing the existing one is awkward — implementer's
  call, document the choice).
- A new fixture, `backend/tests/schema_v0_9_0_seed.sql`: a `CREATE TABLE`-only reconstruction
  of the schema as it existed at the `v0.9.0` tag (gap-analysis §1's basis commit), and a new
  upgrade test opening a database seeded from that fixture, running `init_pool()` against it,
  and asserting every v0.9.0-era table/column survives byte-identical while every
  post-v0.9.0 addition appears — the general form of `pre_existing_tables_byte_identical_after_upgrade`
  gap-analysis is asking P1 to deliver for the whole schema, not just P0's two tables.

## Explicitly NOT granted

- Any `ALTER TABLE`, `DROP TABLE`, column removal, type change, or rename to an existing
  table — this ADR is strictly narrower than even ADR-002's already-additive-only precedent:
  no new tables either, since this task's job is to *document* the current schema, not
  extend it. If the task discovers the live schema and the v0.9.0 reconstruction can't be
  reconciled without a schema change, it escalates rather than altering anything to make the
  test pass.
- `sqlx` migration files (`backend/migrations/`) — that conversion is explicitly P2 scope
  (gap-analysis §4), not this ADR's.
- Everything else in CLAUDE.md's forbidden list, including the rest of `db/mod.rs`
  (`init_pool()`'s actual `CREATE TABLE`/`ALTER TABLE` statements, connection setup) outside
  the `mod tests` block.

## Constraints

1. **Read-and-document, not change.** If `git tag` / `git log` doesn't have a literal
   `v0.9.0` ref to check out and diff against (verify first — don't assume), reconstruct the
   fixture from `docs/gap-analysis.md`'s own basis-commit citation and the earliest commits
   touching `db/mod.rs` in `git log`, and say so explicitly in the PR description rather than
   presenting a guess as verified history.
2. The full golden dump must be generated *from the live schema*, not hand-typed — same
   method as ADR-002's original two-table version (dump `sqlite_master` after a fresh
   `init_pool()` run, normalize whitespace, write to the fixture), so the fixture and the
   test that checks it can never silently diverge from how it was produced.
3. This is a read-heavy, mechanical task by design — the risk isn't creativity, it's
   transcription error against a 888-line file. The acceptance tests below exist specifically
   to catch that.

## Consequences

Delivers P2's prerequisite (gap-analysis §4) without opening `db/mod.rs` to any actual schema
change. At P1 exit this ADR is revoked; P2's own ADR (drafted at that phase's kickoff) will
need a materially different, broader grant to do the migration-file conversion itself.
