# 2026-09-13 — C3-01 Linux collector contract handoff

Status: unit-verified
Tracked plan slice: C3-01 — platform-neutral collector contract and Linux fixtures
Commit: eb78b2c6e6488b37757f34bc4d034e0c1f657783
Branch: dev; upstream: ahead 3 / behind 0 after implementation commit; no push

## Implemented

- Added `backend/src/collector.rs`, exposed as `crate::collector` from `backend/src/main.rs`.
- Added a side-effect-free `lsblk` JSON parser producing the existing `InventorySnapshotV1` contract.
- Added explicit command field list and bounds: 256 KiB input, 128 entities, 512-byte strings, JSON depth 16.
- Filters loop, RAM, partitions, and `dm-*`; traverses nested device trees; requires serial/WWN source identity and rejects identity collisions.
- Added operator documentation at `docs/agent/linux-inventory-collector.md`.

## Verification

- `cd backend && cargo test collector::tests --all-features` — passed, 3 tests.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cd backend && cargo test --all-targets --all-features` — passed, 582 unit tests and 2 integration tests.
- `scripts/check-schema-migration-ownership.sh` — passed; the environment reports optional `rg` unavailable.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed before commit; source inventory only.
- `git diff --check` — passed.
- Independent final delegated review — passed; no security or logic errors.

## Limitations and blockers

- This is `unit-verified`, not integration- or runtime-verified: no process runner invokes `lsblk`, no Linux host probe or service package was exercised, and no controller upload/reconciliation was performed.
- C3-02 owns authenticated upload/replay/reconciliation; C3-03 owns supervision, service installation, outage recovery, upgrade, and rollback.
- Frontend, Windows, providers, migrations, release qualification, and the staged agent hardening remain out of scope.

## Preserved unrelated work

- `backend/src/agent/mod.rs` and `backend/src/agent/state.rs` remain pre-existing staged changes and were not included in commit `eb78b2c6e6488b37757f34bc4d034e0c1f657783`.

## Retrospective and reusable next action

- Learned: the existing CMDB contract already models host and observed entities, so C3-01 can remain independent of database rows and canonical resource UUIDs; source identity must be explicit and collision-checked before reconciliation.
- Verified: malformed, oversized, deep, missing-device, filtered-device, nested-device, and metadata-boundary behavior through the focused parser seam; full backend and Clippy gates remain green.
- Next dependency-ready slice: C3-02 authenticated inventory upload and CMDB reconciliation using this snapshot contract and sanitized fixtures. Add real-router enrollment/upload, replay/conflict, revocation, protected-field, ambiguity, and bounded-body tests; do not add service packaging until that boundary is green.
