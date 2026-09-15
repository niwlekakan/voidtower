# 2026-09-15 — C3-03 runtime qualification recovery session 8 blocked handoff

Status: blocked at supported-host runtime qualification; C3-03 remains unit-verified
Tracked slice: C3-03 — Linux agent supervision and service package
Commit: 1a3e544136c6777c38e2513ebf9be68de3faf1fe
Base: 612c525f91466ad441eaa46a846b838fb0ee1205
Branch: dev (ahead 1, behind 0)

## Recovery scope

This bounded recovery attempt continued the newest blocked C3-03 handoff before considering unrelated work. The sandbox cannot provide the named supported Linux host runtime required by the plan. No product source, service unit, credentials, runtime configuration, or existing worktree path was changed by this session. Only this continuity handoff and the living agent-knowledge appendices are new.

## Blocker diagnosis

The required systemd boundary is not repairable inside this execution sandbox:

- `id` reports UID/GID 1000; package installation cannot be treated as host provisioning.
- `ps -p 1 -o pid,comm,args` reports PID 1 as Docker's `docker-init -- sleep infinity`, not systemd.
- `command -v systemctl` returns no result.
- `/run/systemd/private` is absent and `/proc/1/root/run/systemd/private` is not visible.
- The repository is mounted in a container without the host supervisor or Docker socket. Installing a userspace package would not supply a valid systemd-managed host, host `/dev`, or service lifecycle boundary.

A container-local process or package simulation would not satisfy C3-03's supported-host acceptance criteria and was not used as a workaround.

## Required acceptance boundary still blocked

C3-03 still requires a named supported Linux host or VM with real systemd evidence for service installation/startup/status, protected state permissions, service-managed `/usr/bin/lsblk` collection and authenticated upload to an adopted canonical host, outbound-only behavior, controller outage/restart recovery, binary upgrade, and rollback. No `runtime-verified` or `release-qualified` claim is made.

## Verification after final observed worktree state

Timestamp: `2026-09-15T18:47:41+00:00` UTC.

- `cd backend && cargo test agent::transport --all-features` — passed, 8 tests.
- `cd backend && cargo test agent::supervision --all-features` — passed, 4 tests.
- `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features` — passed, 3 tests.
- `cd backend && cargo test --all-targets --all-features` — passed, 604 unit tests, 2 integration tests, and examples; compiler emitted existing dead-code warnings.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only, `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — exit 0; emits the pre-existing `rg: command not found` diagnostic before reporting the ownership check passed.
- `git diff --check && git diff --cached --check` — passed.
- `cd backend && cargo fmt --check` — blocked by repository-wide formatting drift across existing agent, provider, operation, storage, terminal, and unrelated files; no formatting was applied.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — blocked by existing warnings/errors, including `api/ai_providers.rs`, unused deferred handlers, terminal items, and `api/studio.rs`; no lint fixes were applied.

## Evidence classification

- `unit-verified`: current focused agent transport/supervision and inventory router behavior, plus the full backend test target listed above.
- `implemented`: packaged foreground service boundary and source-level agent supervision paths remain present, but source presence is not runtime proof.
- `blocked`: systemd installation/startup/status, protected host state, service-managed collection/upload, outbound-only runtime observation, outage/restart recovery, upgrade, rollback, and enrollment-to-host-adoption.
- `blocked`: strict repository-wide rustfmt and clippy gates due existing drift/errors.
- No `integration-verified` claim is made for the supported-host runtime; no `runtime-verified` or `release-qualified` claim is made.

## Preserved unrelated worktree state

At session end, existing paths remain untouched:

- Modified: `backend/src/api/apps.rs`, `backend/src/api/cmdb/inventory.rs`, `backend/src/api/cmdb/tests.rs`, `backend/src/api/mod.rs`, `backend/src/api/node_enroll.rs`, `docs/agent/inventory-upload.md`, `docs/agent/linux-agent-service.md`, `scripts/release_gate.py`, `scripts/test_release_gate.py`.
- Untracked: `docs/agent/node-enrollment.md`, `scripts/__pycache__/`, and `testing/`.
- Before this checkpoint was staged, no staged paths were present at inspection time; the current index contains only these three new continuity files, and no unrelated product path was staged.
- No reset, checkout, merge, cleanup, commit, or push was performed; staging was limited to the three continuity files listed above.

## Retrospective

- Learned: this sandbox's PID 1 and missing systemd socket make the host-service gate structurally unavailable; adding a package would not produce valid supported-host evidence.
- Verified: the blocker reproduction, C3-03 focused suites, inventory router tests, full backend tests, release-gate tests, repository truth, schema ownership, and diff hygiene.
- Remaining blocked: all real systemd lifecycle and named-host inventory/upload/recovery scenarios, plus repository-wide rustfmt/clippy cleanliness.
- Reusable commands/fixtures: the focused commands above, `cd backend && cargo test --all-targets --all-features`, `python3 scripts/test_release_gate.py`, `python3 scripts/repo_truth.py --repo . --json --check`, `scripts/check-schema-migration-ownership.sh`, and `git diff --check`.
- End-user documentation: no end-user behavior changed in this blocked checkpoint; `docs/agent/linux-agent-service.md` remains the operator checklist and accurately states the qualification boundary. A supported-platform runbook with observed artifacts remains required after host access is available.

## Next dependency-ready slice

Run C3-03 on a named supported Linux host or VM with real systemd, host `/dev` visibility, and the required controller/test fixture. Capture redacted install/startup, permissions, enrollment → canonical host adoption → upload, outbound-only, outage/restart, upgrade, rollback, and duplicate-snapshot evidence before promoting C3-03 or starting V6-02.
