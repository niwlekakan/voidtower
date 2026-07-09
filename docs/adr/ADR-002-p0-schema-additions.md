# ADR-002 — Narrow schema-addition grant for Phase P0

**Status:** Proposed
**Date:** 2026-07-09
**Expires:** at P0 phase exit (`adr.sh revoke ADR-002`)

## Context

ADR-001 grants the policy engine and AI-ingress files but explicitly withholds
`backend/src/db/mod.rs`, routing schema needs through escalation. Two P0 tasks have now hit
that wall for legitimate reasons:

- **P0-02** (default-deny): needs storage for the generated allowlist.
- **P0-03** (mode ladder): needs per-scope mode storage (global default + per-resource override)
  and, later, the approvals queue.

Schema drift is why `db/mod.rs` is a forbidden zone at all: it is a 466-line accretion of
`CREATE TABLE IF NOT EXISTS` with no migration files, and an unreviewed `ALTER` against a
production database is the most plausible data-loss incident this project can suffer
(gap-analysis §4). The risk is *alteration*, not *addition*.

## Decision

Task specs numbered `P0-*` citing **ADR-002** may add new tables and their creation calls to:

```granted-paths
backend/src/db/mod.rs
backend/src/voidwatch.rs
backend/src/voidwatch/**
```

## Explicitly NOT granted

- `ALTER TABLE`, `DROP TABLE`, column removal, type changes, or renames on **any** existing table
- Any modification to existing `CREATE TABLE` statements, including adding columns
- Data migration or backfill logic
- Everything else in CLAUDE.md's forbidden list (auth, crypto, CI, harness, plan docs)

## Constraints

1. **Additive only.** New tables via `CREATE TABLE IF NOT EXISTS` only. If a task believes it
   needs to alter an existing table, it escalates — that decision is P2 (migration files) work
   and is not delegated.
2. Every new table is created in the same `init` path as existing tables, following the
   file's current conventions exactly.
3. New tables carry a schema-golden-file test (gap-analysis §3, P1 row "Schema"): the PR adds
   the table's definition to `tests/schema_golden.sql` (creating that fixture if absent) and a
   test asserting the live schema matches it. This is the seed of the P2 baseline migration.
4. An upgrade test: a database seeded before the change must open, create the new tables, and
   leave all pre-existing tables byte-identical.
5. Table names are prefixed `voidwatch_` for policy-owned state, so P2's baseline migration
   can identify P0's additions unambiguously.

## Consequences

P0-02 and P0-03 unblock without opening `db/mod.rs` to alteration. The golden-file requirement
means P0 leaves behind the fixture that P2 needs, converting a grant into a down payment on
the migration work. At P0 exit this ADR is revoked and `db/mod.rs` closes again.
