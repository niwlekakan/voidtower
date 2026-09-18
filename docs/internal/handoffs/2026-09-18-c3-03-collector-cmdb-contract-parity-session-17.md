# 2026-09-18 — C3-03 collector/CMDB contract parity session 17

Status: integration-verified for the collector-to-router/database boundary; supported-host runtime and release qualification remain blocked
Tracked slice: C3 Linux collector output contract and CMDB physical-disk classification parity
Milestone commit: 9d2bc0957191adca506a11a0a237bfe4a41c6994 (`feat(agent): align linux collector with cmdb contract`)
Branch: dev
Session timestamp: 2026-09-18T15:08:44Z

Implemented

- `backend/src/collector.rs::collect_linux_snapshot` now validates the complete `InventorySnapshotV1` before returning. Invalid UUID snapshot IDs and non-positive collection times fail closed as `CollectorError::InvalidSnapshot`.
- `collect_linux_fixture` now emits a fixed valid UUID and positive timestamp so sanitized fixtures are directly serializable to the controller contract.
- Physical-disk observations map lsblk `TRAN` to `attributes.protocol`, `ROTA` to `attributes.rotation`, and retain bounded serial/WWN values in attributes as well as identity evidence. Runtime data is no longer used for physical-type classification.
- `backend/src/api/cmdb/tests.rs` adds a real Axum-router/database integration test proving a sanitized collector snapshot authenticates through the enrolled node path, registers one trusted physical disk without review, and persists the classification and WWN identity fields.
- `docs/agent/linux-inventory-collector.md` and `docs/agent/inventory-upload.md` document the field mapping and collector-side shared validation.
- `docs/internal/agent-knowledge/system-map.md` and `docs/internal/agent-knowledge/documentation-backlog.md` append the verified architecture/evidence/retrospective record without deleting prior history.

Verification evidence

- RED: `cargo test collector::tests --all-features` initially failed to compile because the new `InvalidSnapshot` test variant did not yet exist.
- `cd backend && cargo test collector::tests --all-features` — passed, 6 tests.
- `cd backend && cargo test api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features` — passed; real router/database boundary registered one physical disk and persisted attributes plus identity evidence.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, 15 tests.
- `cd backend && cargo test cmdb::contracts::tests:: --all-features` — passed, 4 tests.
- `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` — passed, 629 unit tests, 2 workflow-contract integration tests, and the example target.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_release_gate -v` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`. The report's deterministic dated-handoff selector still names session 15 rather than this newer session 17 file; this is a tooling recency limitation, not product evidence.
- `scripts/check-schema-migration-ownership.sh` — exited 0 but emitted the known `rg: command not found` diagnostic; complete ownership verification is not established.
- `git diff --check` and final staged diff check — passed.
- Independent final staged-diff review — passed with no security concerns or logic errors; the reviewer also verified staged scope, documentation accuracy, and the real-router assertions.

Limitations and blockers

- This slice does not qualify systemd service installation/start/status, host `/dev` visibility, service-managed `/usr/bin/lsblk`, controller outage/restart recovery, upgrade, rollback, or release artifact checksums. The Docker sandbox lacks `systemctl`, `systemd-analyze`, `/run/systemd/private`, and the supported host/device boundary. C3-03 remains blocked at the named supported-host gate.
- `cargo fmt --check` and strict Clippy were not promoted here because the active toolchain/environment has the repository/toolchain limitations recorded in the preceding handoff. No formatting or lint churn was introduced.
- The schema ownership wrapper cannot be treated as complete while its required `rg` executable is absent.
- No migration, provider mutation, Windows, frontend, or node-lifecycle change was included.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No unrelated paths were reset, staged, committed, or pushed.

Retrospective

- Learned: parser-level fixtures must also satisfy downstream identity normalization; a synthetic WWN that is not valid non-zero hex can silently turn a trusted strong snapshot into review. The end-to-end fixture now uses a valid non-zero 16-hex WWN.
- Verified: shared snapshot validation, collector field mapping, UUID/timestamp failure behavior, trusted physical-disk registration, persisted attribute and identity evidence, focused CMDB/contract tests, full backend tests, release-gate tests, repository truth, and independent review.
- Remaining blocked: named supported Linux host/VM with real systemd and host `/dev`; runtime outage/restart, upgrade/rollback, release checksum/artifact evidence; strict Clippy/rustfmt and complete schema ownership in this environment.
- Reusable commands: the focused collector/router commands above; `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` from `backend`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `git diff --check`. Clean only disposable generated `/tmp/vt-p1-*` and `/tmp/voidtower-*` entries if the constrained tmpfs fills.
- End-user documentation changed: collector and upload docs now describe reconciliation field mapping and collector-side contract validation. The supported-host runtime runbook still requires observed systemd, service-managed upload, outage/restart, upgrade/rollback, redacted evidence, and checksum results.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:43-54` on a named supported Linux host or VM with real systemd and host `/dev`, capturing installation, enrollment, service start, bounded collection/upload, controller outage and process-restart recovery, upgrade, rollback, and checksum evidence. Do not promote runtime or release qualification from this sandbox.
