# 2026-09-17 — C3-03 runtime qualification session 12

Status: blocked; managed-node and inventory contracts remain unit-verified and integration-verified
Tracked slice: C3-03 supported-host Linux agent supervision and service-package qualification
Base: 76d1dd41793f6f5190e0be2cc7bd164f4026ff94
Branch: dev (ahead 1, behind 0 before this handoff)

Implemented

- No product source or end-user documentation changes were made because the required supported-host runtime boundary is unavailable.
- Appended this checkpoint to `docs/internal/agent-knowledge/system-map.md` and `docs/internal/agent-knowledge/documentation-backlog.md`.
- Preserved unrelated worktree paths: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, untracked `scripts/__pycache__/`, and untracked `testing/`.

Verification evidence

- Runtime boundary probe at `2026-09-17T14:12:37+00:00`: `systemctl` absent; `/run/systemd/private` absent; PID 1 is `/sbin/docker-init -- sleep infinity`; `/usr/bin/lsblk` exists. Direct `lsblk` availability is not service-managed evidence.
- `cd backend && cargo test agent::state --all-features` — passed, 18 tests.
- `cd backend && cargo test api::node_enroll::tests --all-features` — passed, 10 tests.
- `cd backend && cargo test agent::supervision --all-features` — passed, 4 tests.
- `cd backend && cargo test agent::transport --all-features` — passed, 13 tests.
- `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features` — passed, 3 tests.
- `cd backend && cargo test --all-targets --all-features` — 616 unit tests passed; 1 of 2 integration tests passed; `golden_path_job_runs_on_pull_request_and_push_to_main` failed because `backend/tests/golden_path.rs:26` expects a missing `ci.yml` fixture.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — blocked: `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — exited 0 and printed the known `rg: command not found` diagnostic before reporting passed; full ownership verification is not established here.
- `git diff --check` — passed.

Maturity and limitations

- `unit-verified`: managed-node state, enrollment/authentication, typed transport, supervision, and inventory upload focused suites above.
- `integration-verified`: existing real-router/database enrollment and authenticated inventory upload behavior, including token/path binding, replay/conflict, revocation, bounded bodies, and protected CMDB behavior.
- `blocked`: systemd install/start/status, owner-only host-state observation, service-managed real `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage and process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox cannot provide the required host supervisor or host-device boundary.
- `blocked`: one repository integration gate is red because the expected `ci.yml` fixture is absent; strict Clippy is unavailable; schema ownership has an unavailable `rg` prerequisite. These are recorded as environment/repository prerequisites, not silently waived.

Retrospective

Learned: the current sandbox can repeatedly prove the managed-node and CMDB router/database contracts, but cannot prove the service lifecycle that C3-03 explicitly requires. Direct `/usr/bin/lsblk` execution must not be promoted to service evidence. The full test gate reached 616 passing unit tests, with the missing-fixture failure isolated to the golden-path integration test.

Verified: exact token/state bounds, case-insensitive Bearer authentication, typed heartbeat and upload responses, cancellation/backoff supervision, authenticated inventory upload/replay/conflict/revocation, repository truth, release-gate tests, full unit target, the sandbox runtime-boundary probe, and diff hygiene.

Reusable commands/fixtures: the five focused Cargo commands above; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/test_release_gate.py`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`; and, on a named supported host, `systemd-analyze verify packaging/systemd/voidtower-agent.service` followed by `docs/agent/linux-agent-service.md:43-54`.

End-user documentation changed: none. Existing `docs/agent/linux-agent-service.md`, `docs/agent/node-enrollment.md`, and `docs/agent/inventory-upload.md` remain accurate for the current boundary and explicitly do not claim supported-host qualification. The backlog retains the required named-host evidence list.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not start another source-only C3-03 implementation slice or promote C3-03 beyond integration-verified from this sandbox.
