# 2026-09-18 — C3-03 supported-host runtime qualification session 18 (blocked)

Status: blocked at the supported-host runtime boundary; source and deterministic contract evidence rechecked
Tracked slice: C3-03 Linux agent supervision and service package qualification
Evidence base commit: 1b418a42f947fecb2bfceef0915b6e5f6e9e5bfa (`docs(agent): hand off capacity contract parity`)
Branch: dev

Implemented

- No product source or end-user documentation changed. This checkpoint records a fresh, reproducible C3-03 qualification blocker and appends continuity evidence to the living system map and documentation backlog.
- The required host boundary was probed directly: `systemctl` and `systemd-analyze` are absent; PID 1 is `/sbin/docker-init -- sleep infinity`; `/run/systemd/private` and `/dev/block` are absent; `/usr/bin/lsblk` exists; and the process runs as UID/GID 1000.
- The first full backend attempt exhausted the 512 MiB `/tmp` tmpfs and failed two tests with SQLite disk-I/O/full errors. Only generated `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures were removed; the rerun passed and no repository files or unrelated worktree paths were changed.

Verification evidence

- `cd backend && cargo test agent:: --all-features` — passed, 40 tests.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, 15 tests.
- `cd backend && cargo test cmdb::contracts::tests:: --all-features` — passed, 4 tests.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — passed, 19 tests.
- `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` — passed, 630 unit tests, 2 workflow-contract integration tests, and the example target.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only with `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — exit 0 but emitted `rg: command not found`; complete ownership verification is not established.
- `cd backend && cargo fmt --check` — blocked because `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — blocked because `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `bash -n scripts/install.sh && bash -n scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null && git diff --check` — passed.

Maturity and limitations

- Existing managed-node enrollment, transport, supervision, package, and authenticated CMDB router/database behavior remains at the previously recorded unit/integration-verified baseline. This checkpoint adds no integration coverage and makes no maturity promotion.
- `blocked`: `systemd-analyze verify`, service installation/start/status, protected host-state permissions, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox cannot provide the required named supported Linux host or supervisor/device boundary.
- `blocked`: strict repository Clippy/rustfmt and fully trustworthy schema-ownership verification due unavailable toolchain components and `rg`.
- No runtime or release claim is made from direct utility execution, source inventory, workflow-contract tests, mocked fixtures, or the Docker process environment.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No changes were staged, reset, pushed, or made to those paths.

Retrospective

- Learned: the runtime boundary remains the decisive C3-03 dependency; repeated source-only rechecks cannot substitute for a named systemd/device host. The full backend gate requires `GITHUB_WORKSPACE="$PWD/.."` from `backend/` so the workflow-contract fixture resolves correctly, and bounded cleanup of only `/tmp/vt-p1-*` and `/tmp/voidtower-*` restores the constrained 512 MiB test environment.
- Verified: direct systemd/device blocker probes, agent state/transport/supervision tests, CMDB inventory router tests, snapshot contract tests, release package/release-gate tests, full backend targets, repository truth, schema wrapper behavior, shell/JSON syntax, and diff hygiene.
- Remaining blocked: named supported Linux host/VM with real systemd and host `/dev` visibility; active-toolchain rustfmt/Clippy components; and `rg` for trustworthy schema ownership.
- Reusable commands and fixture cleanup guidance are recorded above and in `docs/internal/agent-knowledge/documentation-backlog.md`.
- End-user documentation changed: none. The supported-host runbook remains required and is already described in `docs/agent/linux-agent-service.md:43-54`.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:43-54` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Do not begin an unrelated source-only C3-03 change or promote runtime/release qualification from this Docker sandbox.
