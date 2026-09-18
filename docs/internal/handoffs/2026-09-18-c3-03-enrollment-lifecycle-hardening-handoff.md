# 2026-09-18 — C3-03 enrollment lifecycle and error contract hardening

Status: implemented, unit-verified, and integration-verified at the enrollment/error seams; supported-host qualification remains blocked
Tracked slice: C3-03 managed-node contract hardening
Implementation commit: 682f111 (`[verified] harden enrollment lifecycle contracts`)
Base: e5ada1d273d21fff08ebf706da22a6cf9eed76ff
Branch: dev

Implemented

- `backend/src/api/node_enroll.rs` now records successful enrollment audit details as structured JSON containing the bounded display name and device type instead of delimiter-built metadata.
- Added a real-router concurrency test proving one pairing code can create exactly one node under two simultaneous enrollment requests: one `200 OK`, one `401 Unauthorized`, one claimed code.
- Added `backend/src/error.rs` response-contract coverage proving database/internal failures return bounded generic envelopes without representative SQL, provider, or credential text.
- Updated `docs/agent/inventory-upload.md` with enrollment single-use/concurrency, WireGuard non-consumption, structured audit, and generic error behavior.
- Updated the living system map and documentation backlog with verified facts, reusable commands, and the remaining qualification boundary.

Verification evidence

- `cd backend && cargo test api::node_enroll::tests --all-features` — passed, 11 tests.
- `cd backend && cargo test error::tests --all-features` — passed, 1 test.
- `cd backend && cargo test agent::state --all-features` — passed, 18 tests.
- `cd backend && cargo test agent::supervision --all-features` — passed, 4 tests.
- `cd backend && cargo test agent::transport --all-features` — passed, 14 tests.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, 13 tests.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — passed, 619 unit tests, 2 integration tests, and the example target.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `scripts/check-schema-migration-ownership.sh` — exited 0 and printed the known `rg: command not found` diagnostic; full ownership verification is not established.
- `git diff --check` — passed before commit.
- `cargo fmt --check` — blocked because `cargo-fmt` is unavailable in toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- Independent final review against the actual checkout diff — passed; no security or logic findings. One non-blocking suggestion was to assert exact error status/code per variant; current test verifies bounded generic redacted output.

Maturity and limitations

- `implemented`: structured enrollment audit details, public-seam concurrency evidence, and generic error response contract coverage.
- `unit-verified`: focused enrollment/error/state/supervision/transport tests.
- `integration-verified`: real Axum-router/database enrollment concurrency and existing authenticated CMDB boundary in the full backend target.
- `blocked`: systemd installation/start/status, protected host-state observation, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox lacks `systemctl`, `systemd-analyze`, `/run/systemd/private`, and the supported host/device boundary.
- Strict Clippy was not run successfully in this environment; repository-wide formatting/tooling prerequisites remain incomplete. Schema ownership is not fully verified because `rg` is unavailable despite the wrapper exit code 0.
- This slice does not redesign caller-selected `agent_capable` semantics, add pairing-code rate limiting, create canonical CMDB host resources during enrollment, or claim runtime recovery.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`.
- Untracked and not included: `scripts/__pycache__/`, `testing/`.

Retrospective

- Learned: the atomic pairing-code claim was already present in production code but needed a concurrent public-router test; structured JSON is safer than delimiter-built audit details for user-provided fields; generic error rendering must be tested at the response seam, not inferred from enum annotations.
- Verified: concurrent single-use enrollment, structured audit persistence, redacted/bounded internal error responses, the full backend target, source truth, release-gate tests, and diff hygiene.
- Remaining blocked: named supported Linux systemd/device qualification and unavailable rustfmt/Clippy/`rg` prerequisites.
- Reusable commands: the focused commands above; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/test_release_gate.py`; `git diff --check`; and the supported-host checklist at `docs/agent/linux-agent-service.md:43-54`.
- End-user documentation changed: `docs/agent/inventory-upload.md`. The supported-host runbook remains required after real host evidence exists.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not promote runtime or release qualification from this Docker sandbox.
