# 2026-09-18 — C3-03 supported-host runtime qualification session 15 (blocked)

Status: blocked at the supported-host runtime boundary; available source and router/database evidence rechecked
Tracked slice: C3-03 Linux agent supervision and service package qualification
Evidence base commit: 238bd406615bf30fca70e39dd274f771c9c60678 (`docs: hand off agent recovery hardening`)
Branch: dev

Implemented

- No product source or end-user documentation changed. This checkpoint records a fresh, reproducible C3-03 qualification blocker and appends evidence to the living system map and documentation backlog.
- The supported-host prerequisite was probed directly: `systemctl` and `systemd-analyze` are unavailable; PID 1 is `/sbin/docker-init -- sleep infinity`; `/run/systemd/private` and `/dev/block` are absent; and the process runs as UID/GID 1000.
- The failed test attempt caused only known disposable test-fixture storage pressure. After removing 1,078 generated `/tmp` entries matching `vt-p1-*` and `voidtower-*` (351,062,140 bytes), the affected checks were rerun successfully. No repository files or unrelated worktree paths were removed.

Verification evidence

- `cd backend && cargo test agent:: --all-features` — passed, 40 tests.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, 14 tests.
- `cd backend && cargo test cmdb::contracts::tests:: --all-features` — passed, 4 tests.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` — passed, 627 unit tests, 2 workflow-contract integration tests, and the example target.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only with `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — exit 0 but emitted `rg: command not found`; complete ownership verification is not established.
- `cd backend && cargo fmt --check` — blocked because `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — blocked because `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `git diff --check` — passed after the continuity documentation changes.
- The explicit three-file whitespace check (`python3` over the two knowledge files and this handoff, rejecting any line where `line.rstrip() != line`) — passed, `checked_files=3 trailing_whitespace=0`.

Maturity and limitations

- Existing managed-node enrollment, transport, supervision, and authenticated CMDB router/database behavior remains at the previously recorded unit/integration-verified baseline. This checkpoint adds no integration coverage and makes no maturity promotion.
- `blocked`: `systemd-analyze verify`, service installation/start/status, protected host-state permissions, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox cannot provide the required named supported Linux host or supervisor/device boundary.
- `blocked`: strict repository Clippy/rustfmt and fully trustworthy schema-ownership verification due unavailable toolchain components and `rg`.
- No runtime or release claim is made from direct utility execution, source inventory, workflow-contract tests, mocked fixtures, or the Docker process environment.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `scripts/__pycache__/` and `testing/` (including the existing frontend concept files).
- No changes were staged, reset, pushed, or made to those paths.

Retrospective

- Learned: the runtime boundary remains the decisive C3-03 dependency; repeated source-only rechecks cannot substitute for a named systemd/device host. Full-test evidence is reproducible after cleaning only the repository's disposable `vt-p1-*`/`voidtower-*` SQLite and state fixtures from the 512 MiB `/tmp` tmpfs.
- Verified: direct systemd/device blocker probes, agent state/transport/supervision tests, CMDB inventory router tests, snapshot contract tests, full backend targets, release-gate tests, repository truth, schema wrapper behavior, and diff hygiene.
- Remaining blocked: named supported Linux host/VM with real systemd and host `/dev` visibility; active-toolchain rustfmt/Clippy components; and `rg` for trustworthy schema ownership.
- Reusable commands and fixture cleanup guidance are recorded above and in `docs/internal/agent-knowledge/documentation-backlog.md`.
- End-user documentation changed: none. The supported-host runbook remains required and is already described in `docs/agent/linux-agent-service.md:43-54`.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:43-54` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Do not begin an unrelated source-only C3-03 change or promote runtime/release qualification from this Docker sandbox.