# 2026-09-19 — C3-03 supported-host runtime qualification session 21 (blocked)

Status: blocked at the supported-host runtime boundary; deterministic source and contract evidence rechecked
Tracked slice: C3-03 Linux agent supervision and service package qualification
Evidence base commit: cb7660fbcaa15897ea81809683e4caa430d44194 (`docs(agent): record blocked c3-03 qualification session 20`)
Implementation commit: cafedb82dc7712ec3ef3968e65d6dca2d0331689 (`[verified] docs(agent): record c3-03 qualification evidence`)
Branch: dev

Implemented

- Added this dated evidence-backed handoff for the C3-03 qualification attempt.
- Appended session-21 evidence to `docs/internal/agent-knowledge/system-map.md` and `docs/internal/agent-knowledge/documentation-backlog.md` without rewriting prior history.
- No product source, service package, or end-user documentation changed in this checkpoint because the required supported-host runtime is unavailable in this sandbox.

Active slice and public seams

- The tracked outcome is `docs/development-plan.md:420`: independent heartbeat/inventory loops, bounded backoff, service installation, protected state, upgrade, and rollback, with runtime and artifact evidence.
- The public qualification seam is `docs/agent/linux-agent-service.md:56-64`; the implementation seams are the systemd unit, agent state/transport/supervision, collector, and authenticated CMDB inventory route.
- Non-goals remain inbound listeners, generic remote command execution, Windows support, frontend work, provider mutation, and unrelated worktree changes.

Verification evidence

- Runtime prerequisite probe at `2026-09-19T12:19:08+00:00`: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` and `/dev/block` are absent; `/usr/bin/lsblk` is available; the process runs as UID/GID 1000. Service installation/status, protected host-state, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact qualification cannot run here.
- `cd backend && cargo test agent:: --all-features` — exit 0; 40 passed, 0 failed.
- `cd backend && cargo test api::cmdb::tests --all-features` — exit 0; 15 passed, 0 failed.
- `cd backend && cargo test cmdb::contracts::tests:: --all-features` — exit 0; 4 passed, 0 failed.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — exit 0; 19 passed, 0 failed.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` from the repository root — exit 0; 630 unit tests, 2 workflow-contract integration tests, and the example target passed.
- `python3 scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed and reported `runtime_support_claimed: false`.
- Schema ownership check is blocked: `scripts/check-schema-migration-ownership.sh` exited 0 but emitted `rg: command not found` before reporting success; that exit code is not treated as verification.
- `bash -n scripts/install.sh scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null && git diff --check` — exit 0.
- `git diff --cached --check` for the final staged handoff — exit 0.
- `cd backend && cargo fmt --check` — exit 1 because `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — exit 1 because `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.

Maturity and limitations

- Existing managed-node enrollment, transport, supervision, package, collector, and authenticated CMDB router/database behavior remains at its previously recorded unit/integration-verified baseline. This checkpoint adds no product behavior or maturity promotion.
- `blocked`: `systemd-analyze verify`, service installation/start/status, protected host-state permissions, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification.
- No runtime or release claim is made from direct utility availability, source inventory, workflow-contract tests, controlled fixtures, or the Docker process environment.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No changes were staged, reset, pushed, or made to those paths.

Retrospective

- Learned: the repository-root invocation `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` is the reproducible full-backend command in this sandbox; running the same form from `backend` can lose the manifest context.
- Verified: the supported-host blocker probe, agent/CMDB/contract focused tests, package and release-gate tests, full backend targets, repository truth, shell/JSON syntax, schema-wrapper behavior, and diff hygiene.
- Remaining blocked: a named supported Linux host or VM with real systemd and host `/dev` visibility; active-toolchain rustfmt and Clippy components; and `rg` for trustworthy schema ownership.
- Reusable commands and future end-user documentation requirements remain recorded in `docs/internal/agent-knowledge/documentation-backlog.md`.
- End-user documentation changed: none; `docs/agent/linux-agent-service.md` remains accurate and explicitly marks runtime/release qualification as blocked.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:56-64` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Do not begin an unrelated source-only C3-03 change or promote runtime/release qualification from this Docker sandbox.
