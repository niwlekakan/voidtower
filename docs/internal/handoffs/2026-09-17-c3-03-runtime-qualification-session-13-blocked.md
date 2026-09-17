# 2026-09-17 — C3-03 runtime qualification session 13

Status: blocked; source and managed-node/CMDB router contracts remain unit-verified and integration-verified
Tracked slice: C3-03 supported-host Linux agent supervision and service-package qualification
Base: fd3c60cb264a19568cc9115c298fa063a21bad50
Commit: 4033069 (docs: record blocked C3-03 qualification session 13)
Branch: dev (ahead 2, behind 0 before this handoff)

Implemented

- No product source or end-user documentation changes were made because the required supported-host runtime boundary is unavailable.
- Appended this checkpoint to `docs/internal/agent-knowledge/system-map.md` and `docs/internal/agent-knowledge/documentation-backlog.md`.
- Preserved unrelated worktree paths: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, untracked `scripts/__pycache__/`, and untracked `testing/`.

Verification evidence

- Runtime boundary probe at `2026-09-17T15:15:03+00:00`: `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` is absent; PID 1 is `/sbin/docker-init -- sleep infinity`; `/usr/bin/lsblk` is present but direct execution is not service-managed evidence.
- `cd backend && cargo test agent::state --all-features` — passed, 18 tests.
- `cd backend && cargo test agent::transport --all-features` — passed, 13 tests.
- `cd backend && cargo test agent::supervision --all-features` — passed, 4 tests.
- `cd backend && cargo test api::node_enroll::tests::lowercase_bearer_scheme_authenticates_approved_agent --all-features` — passed, 1 test.
- `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features` — passed, 3 tests.
- `GITHUB_WORKSPACE=/workspace/Documents/voidtower_project_files_full/hive/voidtower cargo test --test golden_path --all-features` — passed, 2 workflow-contract test cases. The prior unqualified run failed only because the sandbox did not provide `GITHUB_WORKSPACE`; the tracked `.github/workflows/ci.yml` exists. This is workflow-contract evidence, not assembled runtime integration evidence.
- `GITHUB_WORKSPACE=/workspace/Documents/voidtower_project_files_full/hive/voidtower cargo test --all-targets --all-features` — passed, 616 unit tests, 2 workflow-contract test cases, and the example target.
- `cargo fmt --check` — blocked: `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cargo clippy --all-targets --all-features -- -D warnings` — blocked: `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `scripts/check-schema-migration-ownership.sh` — exited 0 but printed `rg: command not found`; full ownership verification is not established in this environment.
- `git diff --check` — passed.

Maturity and limitations

- `unit-verified`: managed-node state, enrollment/authentication, typed transport, supervision, and inventory upload focused suites.
- `integration-verified`: existing real-router/database enrollment and authenticated inventory upload behavior, including token/path binding, replay/conflict, revocation, bounded bodies, and protected CMDB behavior; golden-path workflow contract test also passes with the required workspace environment.
- `blocked`: systemd install/start/status, owner-only host-state observation, service-managed real `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage and process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox cannot provide the required host supervisor or host-device boundary.
- `blocked`: strict formatting and Clippy are unavailable; schema ownership has an unavailable `rg` prerequisite. These are recorded as environment prerequisites, not silently waived.

Retrospective

Learned: `backend/tests/golden_path.rs` resolves the workflow from `GITHUB_WORKSPACE` when present, so a hermetic shell invocation must set that variable to the repository root. The earlier missing-fixture failure was an invocation-environment issue; `.github/workflows/ci.yml` is tracked and valid. This does not alter the C3-03 runtime blocker: direct `/usr/bin/lsblk` and source tests cannot prove packaged systemd behavior.

Verified: focused managed-node and CMDB tests, full backend targets with the correct workspace variable, workflow contract, release-gate tests, repository truth, runtime boundary absence, and diff hygiene.

- Reusable commands/fixtures: the focused commands above; from repository root, `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/test_release_gate.py`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`; and, on a named supported host, `systemd-analyze verify packaging/systemd/voidtower-agent.service` followed by `docs/agent/linux-agent-service.md:43-54`.

End-user documentation changed: none. Existing `docs/agent/linux-agent-service.md`, `docs/agent/node-enrollment.md`, and `docs/agent/inventory-upload.md` remain accurate and do not claim supported-host qualification. The backlog retains the required named-host evidence list.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not start another source-only C3-03 implementation slice or promote C3-03 beyond integration-verified from this sandbox.
