# 2026-09-19 — C3-02 inventory convergence safety

Status: implemented and integration-verified at the router/database boundary; supported-host runtime and release qualification remain blocked
Tracked slice: C3-02 bounded Linux inventory evidence and reconciliation safety
Base commit: 906020c518551412b735d4db5e78cd612f69552d
Implementation commit: 3ce428d67177bb7202992e8fa1d610d87db5b052
Branch: dev

Active slice and boundary

- Hardened the coupled inventory-ingestion and reconciliation seams in `backend/src/api/cmdb/tests.rs` and `backend/src/cmdb/observations.rs`.
- A valid host-only/empty-entity snapshot is recorded successfully but is non-converging: it reports `missing: 0` and cannot mark existing observations or assets missing.
- A non-empty successful snapshot retains omission convergence, marking omitted observations missing without deleting their assets.
- Discovery refresh preserves administrator-owned resource and CMDB fields, including display/friendly names, description, manufacturer/model, serial/part identifiers, lifecycle, condition, location, metadata, and notes.
- Router coverage verifies node attribution, request/event correlation, and that the node token is absent from event payloads.
- Updated `docs/agent/inventory-upload.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md` with the verified contract, evidence, limitations, and follow-up documentation.
- Non-goals: service/systemd qualification, Windows collection, CMDB frontend, arbitrary resource adoption, provider mutation, and unrelated modified or untracked paths.

Acceptance evidence

- Focused API integration checks, each run independently and passing:
  - `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features` — 1 passed.
  - `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml api::cmdb::tests::empty_inventory_snapshot_does_not_mark_existing_inventory_missing --all-features` — 1 passed.
  - `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml api::cmdb::tests::inventory_reconciliation_preserves_administrator_owned_asset_fields --all-features` — 1 passed.
- Focused observation check: `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml cmdb::observations::tests::completed_snapshot_marks_omitted_disk_missing_without_deleting_it --all-features` — 1 passed.
- Full backend gate: `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — exit 0; 651 unit tests, 2 integration tests, and the example target passed.
- Package/release checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — exit 0; 19 passed.
- Source/reproducibility: `python3 scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed and `runtime_support_claimed` remained false.
- Diff hygiene: `git diff --check` and `git diff --cached --check` — exit 0 before commit; staged static scans found no credential, shell-injection, eval/exec, unsafe-deserialization, or interpolated-SQL matches.
- Schema wrapper: `bash scripts/check-schema-migration-ownership.sh` — exit 0 and printed `Schema migration ownership check passed`, but emitted `rg: command not found`; schema ownership is not promoted as fully trustworthy in this environment.
- Independent review of the exact staged slice: passed with `security_concerns: []` and `logic_errors: []`. Non-blocking suggestions were not required for this bounded acceptance; empty-snapshot event absence is covered by the response/state assertions and existing transactional event behavior.
- Commit scope: `3ce428d67177bb7202992e8fa1d610d87db5b052` contains only `backend/src/api/cmdb/tests.rs`, `backend/src/cmdb/observations.rs`, `docs/agent/inventory-upload.md`, `docs/internal/agent-knowledge/documentation-backlog.md`, and `docs/internal/agent-knowledge/system-map.md`.

Limitations and blockers

- A first post-change full backend attempt reached 650 passing tests but failed because the 512 MiB `/tmp` tmpfs was full during an unrelated SQLite migration fixture. Only disposable `/tmp/vt-p0-*`, `/tmp/vt-p1-*`, and `/tmp/voidtower-*` test artifacts were removed; the exact full command then passed with 651 tests.
- `cargo fmt --manifest-path backend/Cargo.toml --check` is blocked because `cargo-fmt` is unavailable for Rust 1.98.1.
- `cargo clippy --manifest-path backend/Cargo.toml --all-targets --all-features -- -D warnings` is blocked because `cargo-clippy` is unavailable.
- Runtime qualification remains blocked by the Docker sandbox: no `systemctl`, `/run/systemd/private`, supported host `/dev`, installation privileges, or packaged-host procedure. No service-managed collection/upload, outage/restart, upgrade, rollback, or release-qualified artifact claim is made.
- Existing unrelated worktree state is preserved and not part of this commit: modified `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, unstaged portions of the two agent-knowledge files, and untracked `scripts/__pycache__/` and `testing/` paths.

Retrospective

- Learned: an empty or host-only inventory payload is valid evidence of a host snapshot but insufficient evidence of absence; missing convergence must require a non-empty authoritative entity set. Discovery correlation and reconciliation must continue to treat `resources.id` as canonical and leave administrator-owned fields untouched.
- Verified: real Axum-router/database ingestion, empty-snapshot safety, non-empty omission-to-missing retention, admin-field preservation, audit/event correlation and token non-disclosure, observation-level reconciliation, full backend, package/release, repository-truth, schema-wrapper, static scan, and diff checks. Evidence is unit/integration-verified, not runtime-qualified.
- Reusable commands: the three focused API commands above; the observation focused command; serial full backend with `GITHUB_WORKSPACE="$PWD"`; package/release unittest command; repository truth; schema wrapper; and `git diff --check`. Clean only disposable test fixture patterns under `/tmp` if the tmpfs fills before a rerun.
- End-user documentation changed: `docs/agent/inventory-upload.md` now defines host-only non-converging uploads, `missing: 0`, non-empty omission convergence, administrator-field preservation, and correlation/actor attribution. The supported-host C3-03 service/recovery runbook remains required.

Next dependency-ready slice

Run the supported-host C3-03 qualification procedure in `docs/agent/linux-agent-service.md` on a named Linux host or VM with real systemd, `/run/systemd/private`, host `/dev`, a packaged candidate artifact, an enrolled node, and a pre-existing canonical host resource bound to that node. Capture install/status, protected state, service-managed collection/upload, outbound-only behavior, controller outage and process-restart recovery, upgrade, rollback, redacted evidence, and SHA256SUMS before promoting runtime or release maturity.
