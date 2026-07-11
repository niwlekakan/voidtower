# Task P1-04: Full schema golden-file dump + v0.9.0 upgrade test

## Status: Ready
**ADR:** ADR-008
Depends-On: none
Requires-Path: backend/src/db/mod.rs, backend/tests/schema_golden.sql

## Source

- `docs/gap-analysis.md` §3, P1 table row 6: "Schema | Fresh-DB schema dump golden file +
  upgrade test from a seeded v0.9.0 database | Prerequisite for P2." §4 (P2): "migration
  `0000_baseline.sql` = exact current v0.9.0 schema (generated from the golden dump)... The
  upgrade test from P1 guards the conversion."
- `docs/adr/ADR-008-p1-schema-golden-file.md` — **authoritative** for the grant, the
  precedent this generalizes (ADR-002's two-table version), and the constraint that this is a
  documentation task, not a schema-change task.
- `docs/codebase-map.md` §3 (DB table → creation site map) — enumerates every table as of the
  map's last verification; **re-derive the live list from `backend/src/db/mod.rs` directly**,
  don't transcribe the map (it can be stale, and new tables/columns may have landed since).
- `backend/src/db/mod.rs`'s existing `mod tests` block (`schema_golden_file_matches_live_schema_after_migration`,
  `pre_existing_tables_byte_identical_after_upgrade`) — the pattern to generalize, not
  duplicate wholesale.

## Scope

Two deliverables:

1. **Generalize the golden-file test to the whole schema.** `backend/tests/schema_golden.sql`
   currently covers only `voidwatch_default_allowlist`/`voidwatch_mode_settings` (P0-02's
   addition). Extend it to every table `run_migrations()` + `init_pool()` create as of current
   `main` — generated from a live `sqlite_master` dump after a fresh `init_pool()` run, not
   hand-typed (ADR-008 constraint 2), normalized the same way the existing fixture is.
   Generalize (or add a sibling to) `schema_golden_file_matches_live_schema_after_migration`
   to check the full dump.
2. **A v0.9.0 upgrade test.** First, check whether a literal `v0.9.0` git tag exists
   (`git tag -l 'v0.9.0'` or similar) and whether it's reachable from this checkout. If it
   is: check it out (in a scratch location, not touching this task's working tree) and dump
   its `db/mod.rs`-produced schema the same way. If it isn't reachable, reconstruct the
   v0.9.0-era schema from `docs/gap-analysis.md`'s basis-commit citation and `git log`'s
   earliest relevant commits touching `db/mod.rs`, and **say explicitly in the PR description**
   that this is a reconstruction, not a verified historical dump (ADR-008 constraint 1 — don't
   present a guess as verified history). Write the result to
   `backend/tests/schema_v0_9_0_seed.sql`. Add a test that seeds a database from this fixture,
   runs today's `init_pool()` against it, and asserts every v0.9.0-era table/column is
   byte-identical afterward while every post-v0.9.0 addition (per `docs/gap-analysis.md` §1's
   feature list — tiered accounts, fleet node enrollment, member hub, voidwatch tables, etc.)
   is present.

This is explicitly a read-and-document task (ADR-008's own framing) — if reconciling the
live schema against the v0.9.0 reconstruction reveals something that looks like it needs an
actual schema change (a column that should exist and doesn't, a type mismatch), do not fix
it under this ADR. Escalate with the specific finding.

## Contract (verbatim, `docs/gap-analysis.md` §3 and §4)

> Schema | Fresh-DB schema dump golden file + upgrade test from a seeded v0.9.0 database |
> Prerequisite for P2.
>
> P2 — Schema discipline: Convert `db/mod.rs` accretion to sqlx migration files: migration
> `0000_baseline.sql` = exact current v0.9.0 schema (generated from the golden dump), all
> future changes as numbered migrations... Upgrade test from P1 guards the conversion.

## Architecture decision already made (ADR-008 — do not re-litigate)

- Additive-only, and narrower than even ADR-002's precedent: this task adds no new tables,
  only documents existing ones. If the live schema and the v0.9.0 reconstruction can't be
  reconciled without a change, escalate rather than alter anything to make the test pass.
- Golden dump is machine-generated from a live `init_pool()` run, not hand-typed.
- Migration-file conversion itself is P2 scope, explicitly not this task's.

## Files to touch

- `backend/src/db/mod.rs` — **forbidden zone, ADR-008, test code only** (the `mod tests`
  block; no `CREATE TABLE`/`ALTER TABLE` statement in `init_pool()`/`run_migrations()` may
  change).
- `backend/tests/schema_golden.sql` — **forbidden zone, ADR-008**, extended to full schema.
- New: `backend/tests/schema_v0_9_0_seed.sql` — **forbidden zone, ADR-008**.

## Explicitly not to touch

- Any `CREATE TABLE`, `ALTER TABLE`, or connection-setup code in `db/mod.rs` outside
  `mod tests`.
- `backend/migrations/` — does not exist yet; do not create it, that's P2.
- Everything else in CLAUDE.md's forbidden list.

## Acceptance tests (name before implementing)

- `schema_golden_file_matches_live_schema_after_migration` (generalized to full schema, or a
  new sibling test if generalizing proves awkward — document the choice) — fails the build on
  any drift between live `sqlite_master` and the fixture, in either direction (new
  undocumented table, or a stale fixture entry for a removed one).
- `pre_existing_tables_byte_identical_after_upgrade_from_v0_9_0_seed` — seeds from
  `schema_v0_9_0_seed.sql`, runs `init_pool()`, asserts every v0.9.0-era table/column survives
  unchanged.
- `post_v0_9_0_additions_present_after_upgrade_from_v0_9_0_seed` — the inverse: every table
  gap-analysis §1 names as added since v0.9.0 (tiered accounts, node enrollment, member hub,
  voidwatch tables, etc.) exists after running `init_pool()` against the seeded database.
- `v0_9_0_seed_fixture_provenance_is_documented` — not a runtime assertion so much as a
  reviewable artifact: the PR description (or a comment at the top of the fixture file) states
  plainly whether the fixture is a verified historical dump or a reconstruction, and why.

## Forbidden zones for this task

`backend/src/db/mod.rs` (ADR-008, test-only), `backend/tests/schema_golden.sql`,
`backend/tests/schema_v0_9_0_seed.sql` (ADR-008).

## Review tier

Full line review (EDD §15.5 — schema/migrations are explicitly full-line-review tier, and
this is the prerequisite for P2 opening `db/mod.rs` more broadly).
