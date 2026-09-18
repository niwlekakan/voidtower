# 2026-09-18 — C3-03 collector/CMDB capacity vocabulary parity

Status: integration-verified for the Linux collector-to-router/database contract; supported-host runtime and release qualification remain blocked
Tracked slice: C3-03 Linux collector physical-disk capacity contract parity
Milestone implementation commit: 79078ac (`fix(agent): align disk capacity contract`)
Branch: dev
Session timestamp: 2026-09-18T15:37:37Z

Implemented

- `backend/src/collector.rs` maps lsblk `SIZE` to the CMDB canonical `attributes.capacity_bytes` and no longer emits the unrelated `size_bytes` alias.
- Physical-disk collection fails closed with `CollectorError::InvalidField("size")` when SIZE is missing, null, zero, negative, or a string; positive JSON integers remain valid.
- `backend/src/api/cmdb/tests.rs` proves positive capacity persistence through the authenticated Axum router and SQLite reconciliation boundary, and proves zero `capacity_bytes` returns `400 bad_request` without creating an inventory snapshot.
- `docs/agent/linux-inventory-collector.md` and `docs/agent/inventory-upload.md` publish the producer and route contracts.
- The living system map and documentation backlog append the verified decision, evidence, limitations, and reusable checks.

Verification evidence

- RED: before implementation, `cargo test collector::tests::linux_fixture_produces_snapshot_and_filters_ephemeral_devices --all-features` and `cargo test api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features` failed because the collector emitted `size_bytes` and persisted capacity was absent.
- `cd backend && cargo test collector::tests --all-features` — passed, 7 tests, including missing/null/zero/negative/string SIZE rejection.
- `cd backend && cargo test api::cmdb::tests::inventory_upload_rejects_semantically_invalid_snapshots --all-features` — passed.
- `cd backend && cargo test api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features` — passed; authenticated real-router/database boundary registered one physical disk and persisted `capacity_bytes: 100` with classification and identity evidence.
- `cd backend && cargo test cmdb::observations::tests:: --all-features` — passed, 11 tests.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, 15 tests.
- `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` — passed, 630 unit tests, 2 workflow-contract integration tests, and the example target.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_release_gate -v` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `git diff --cached --check` and `git diff --check` — passed before commit.

Independent review

- First independent staged-diff review failed closed on the producer-boundary gap for malformed/missing SIZE. That finding was fixed with explicit positive-integer validation and focused tests.
- Second independent staged-diff review passed with no security concerns or logic errors. It offered only non-blocking suggestions for additional route-level malformed-capacity cases and rustfmt installation.

Limitations and blockers

- This slice does not qualify systemd installation/start/status, host `/dev` visibility, service-managed `/usr/bin/lsblk`, controller outage/restart recovery, upgrade, rollback, or release artifact checksums. The Docker sandbox lacks `systemctl`, `systemd-analyze`, `/run/systemd/private`, and the supported host/device boundary.
- `cargo fmt --check` is blocked because `cargo-fmt` is absent from the active Rust 1.98.1 toolchain. Strict Clippy is blocked because `cargo-clippy` is absent.
- `scripts/check-schema-migration-ownership.sh` exits 0 but emits `rg: command not found`; complete ownership verification is not established.
- `scripts/check-repository-hygiene.sh` fails on pre-existing tracked internal handoff/evidence and knowledge paths; no cleanup was performed.
- Runtime/release claims remain blocked; source and integration evidence must not be promoted to runtime or release-qualified.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No unrelated paths were reset, staged, committed, or pushed.

Retrospective

- Learned: CMDB normalization already treats `capacity_bytes` as the canonical positive integer field; producer-side validation is required so malformed lsblk SIZE values cannot leave the collector only to fail later at upload.
- Verified: canonical capacity mapping, producer malformed-input rejection, authenticated persistence, bounded route error/no-mutation behavior, focused and full backend tests, release-gate tests, source truth, diff checks, and independent review.
- End-user documentation changed: Linux collector and inventory-upload docs now specify `SIZE`→`capacity_bytes`, reject malformed SIZE values before upload, and distinguish the unsupported `size_bytes` alias.
- Reusable commands: the focused collector/router commands above; `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` from `backend`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:43-54` on a named supported Linux host or VM with real systemd and host `/dev`, capturing installation, enrollment, service start, bounded collection/upload, controller outage and process-restart recovery, upgrade, rollback, and checksum evidence. Do not promote runtime or release qualification from this sandbox.
