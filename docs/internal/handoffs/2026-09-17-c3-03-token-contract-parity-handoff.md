# 2026-09-17 — C3-03 token contract parity handoff

Status: unit-verified and integration-verified; supported-host runtime qualification remains blocked
Tracked slice: C3-03 continuation — managed-node token contract parity and bounded auth evidence
Implementation commit: 8b3ddff [verified] align managed-node token bounds
Base: 7a11941
Branch: dev (ahead 1, behind 0 before this handoff commit)

Implemented

- Added `agent::state::MAX_NODE_TOKEN_BYTES` as the shared 512-byte node-token bound.
- Persisted `HeartbeatToken` validation now accepts exactly 512 bytes and rejects 513 bytes, matching controller authentication.
- Controller node authentication imports the shared bound; the existing case-insensitive Bearer route test now proves an approved node accepts an exactly 512-byte credential and rejects a 513-byte credential against the same node.
- Updated Linux agent operator documentation and living system map/documentation backlog with the parity contract and evidence limits.

Verification evidence

- `cd backend && cargo test agent::state --all-features` — passed, 18 tests.
- `cd backend && cargo test api::node_enroll::tests::lowercase_bearer_scheme_authenticates_approved_agent --all-features` — passed, 1 test; covers lowercase scheme, exact 512-byte acceptance, and same-node 513-byte rejection.
- `cd backend && cargo test agent::supervision --all-features` — passed, 4 tests; backoff bounds, cancellation, and invalid-state fail-closed behavior.
- `cd backend && cargo test --all-targets --all-features` — 616 unit tests passed and 1 golden-path integration test passed; 1 existing golden-path test failed because `ci.yml` is absent at the path expected by `backend/tests/golden_path.rs:26`. This is a repository prerequisite failure, not introduced by this slice.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `git diff --check` — passed.
- `systemd-analyze --version` — blocked: command not found in the Docker sandbox.
- Independent final review — passed; no security or logic findings. Reviewer noted only unrelated pre-existing worktree paths were preserved.

Maturity and limitations

- `unit-verified`: agent state exact-boundary validation, existing pending-snapshot persistence/restart-resumability tests, and supervision cancellation/backoff tests.
- `integration-verified`: real Axum router/database authentication path with exact-limit and over-limit credentials.
- `blocked`: supported-host systemd install/start/status, real `/usr/bin/lsblk` collection, host `/dev` visibility, enrollment-to-CMDB operational run, controller outage and process-restart runtime recovery, upgrade, rollback, artifact checksum, and release qualification. The sandbox has no systemd or host runtime boundary.
- `blocked`: one full backend integration gate remains red because the repository's expected `ci.yml` fixture/file is absent; strict repository Clippy remains blocked by existing repository-wide diagnostics as documented by the prior handoff.
- Documentation still required before C3-03 runtime/release promotion: named host/platform matrix, packaged artifact checksum, redacted install/status/journal captures, outage/restart/upgrade/rollback results.

Preserved unrelated worktree

- Left unstaged and uncommitted: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, untracked `scripts/__pycache__/`, and untracked `testing/`.

Retrospective

Learned: keeping the node-token limit in two modules allowed the agent to accept credentials the controller could never use. A shared byte-count constant closes that compatibility gap without widening the trust boundary. The over-limit router test must target a valid approved node so a 401 is evidence of the size guard rather than a missing-record lookup.

Verified: exact 512-byte agent acceptance, 513-byte agent rejection, exact 512-byte real-router authentication, 513-byte same-node rejection, case-insensitive Bearer behavior, existing supervision/pending-state behavior, release-gate tests, and diff hygiene.

Reusable commands/fixtures: the three focused Cargo commands above; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/test_release_gate.py`; `git diff --check`; and `systemd-analyze verify packaging/systemd/voidtower-agent.service` on a real supported host.

End-user documentation changed: `docs/agent/linux-agent-service.md` records the shared token bound and exact-limit semantics. Future docs remain required for supported-host runtime and release evidence.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host/VM with real systemd and host `/dev` visibility. Do not promote C3-03 beyond integration-verified from this sandbox.
