# 2026-09-16 — C3-03 runtime qualification recovery session 10

Status: blocked; C3-03 remains unit-verified
Tracked slice: C3-03 — Linux agent supervision and service package
Base: a2415e35771111e79c03c3efdf67d4cc5488a9e0
Commit: pending local continuity commit
Branch: dev

## Boundary and acceptance

The active boundary is supported-host Linux runtime qualification: service installation/start/status, protected state permissions, canonical host adoption, real `/usr/bin/lsblk` collection/upload, outbound-only behavior, controller outage and process-restart recovery with pending snapshot reuse, duplicate-safe replay, upgrade, rollback, and artifact checksum evidence.

No product source was changed in this checkpoint. Existing uncommitted source and test work was treated as pre-existing and preserved. Non-goals remain inbound listeners, generic remote commands, Windows support, frontend work, arbitrary resource adoption, provider mutation, and V6-02 promotion.

## Evidence after the current worktree state

Focused checks passed:

- `cd backend && cargo test api::cmdb::tests --all-features` — 13 passed.
- `cd backend && cargo test api::node_enroll::tests --all-features` — 7 passed.
- `cd backend && cargo test api::apps::tests --all-features` — 43 passed.
- `cd backend && cargo test agent::collector --all-features` — 4 passed.
- `cd backend && cargo test agent::transport --all-features` — 12 passed.
- `cd backend && cargo test agent::supervision --all-features` — 4 passed.
- `python3 scripts/test_release_gate.py` — 11 passed.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only, `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — wrapper exited 0 but emitted `rg: command not found`; ownership result is therefore not fully verified.
- `git diff --check` — passed.

Full gate:

- `cd backend && cargo test --all-targets --all-features` — passed after removing only disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` test artifacts; 611 unit tests, 2 integration tests, and the example target passed.
- The earlier same-session attempt (separate from the historical SQLite-lock attempt recorded in prior handoffs) reached 513 passed and 98 failed because `/tmp` exhausted during concurrent fixture creation. It is retained as transient environment evidence, not a current code failure. The cleaned rerun is the authoritative final result for this checkpoint.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — blocked by existing repository-wide warnings/errors, including dead-code diagnostics, possible-missing-else diagnostics in the existing AI-provider code, and existing Studio/style diagnostics. No Clippy fixes were made.

Runtime boundary probe:

- `systemctl` is unavailable.
- `/run/systemd/private` is absent.
- PID 1 is Docker `docker-init`, not systemd.
- `/usr/bin/lsblk` exists and direct execution produced a 4258-byte JSON fixture, but direct execution is not service-managed runtime evidence.

## Maturity and blocker

Current C3-03 source contracts and focused/full deterministic tests are `unit-verified`. Service lifecycle, protected host state, service-managed collection/upload, outbound-only runtime observation, controller outage/restart recovery, process-restart durability qualification, upgrade, rollback, artifact checksum, and release support remain `blocked` because this Docker sandbox has no supported systemd host boundary or host `/dev` qualification environment.

## Preserved worktree

Pre-existing modified paths were not edited, staged, reset, cleaned, or committed by this checkpoint: `backend/src/api/apps.rs`, `backend/src/api/cmdb/inventory.rs`, `backend/src/api/cmdb/tests.rs`, `backend/src/api/mod.rs`, `backend/src/api/node_enroll.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`. Pre-existing untracked paths remain `docs/agent/node-enrollment.md`, `scripts/__pycache__/`, and `testing/`. Disposable `/tmp` test artifacts were removed only to repeat the full gate.

## Retrospective

Learned: the current uncommitted API/release hardening changes compile and pass their focused tests and the cleaned full backend suite, but the first full attempt can exhaust the 512 MiB `/tmp` tmpfs through concurrent SQLite and artifact fixtures. Cleanup of only the documented disposable patterns restores reproducible full-test execution.

Verified: current CMDB upload, enrollment/heartbeat validation, App Vault security, collector, transport, supervision, repository-truth, release-gate, full backend, diff, and systemd-boundary probes. The direct `lsblk` probe remains useful as a utility smoke check only.

Blocked: named supported-host systemd install/start/status, protected state, real service-managed collection/upload, outbound-only socket observation, controller outage and process restart recovery, upgrade, rollback, and release qualification. Strict Clippy is also blocked by existing repository-wide diagnostics; schema ownership is partially blocked by missing `rg`.

Reusable commands/fixtures: `rm -rf /tmp/vt-p1-* /tmp/voidtower-* /tmp/vt-lsblk.json`; the focused Cargo commands above; `cd backend && cargo test --all-targets --all-features` after that disposable-fixture cleanup; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`.

End-user documentation: no end-user documentation changed because the required host runtime is unavailable. `docs/agent/linux-agent-service.md` remains the qualification checklist and accurately labels C3-03 service/release support as blocked.

## Next bounded slice

On a named supported Linux host or VM with real systemd and host `/dev` visibility, execute the checklist in `docs/agent/linux-agent-service.md:43-54`, capture redacted artifacts and a checksum, then promote only the observed scenarios. Do not claim runtime or release qualification from this sandbox.
