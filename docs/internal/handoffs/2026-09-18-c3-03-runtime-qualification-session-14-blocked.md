# 2026-09-18 — C3-03 supported-host runtime qualification session 14 (blocked)

Status: blocked at the supported-host runtime boundary; available source and router/database evidence rechecked
Tracked slice: C3-03 Linux agent supervision and service package qualification
Current commit: c8ee67eaca172259488e96fb7cc4c2691435a999 (`docs: hand off enrollment persistence hardening`)
Branch: dev

Implemented in this checkpoint

- No product source or end-user documentation was changed. This checkpoint records the dependency blocker and preserves unrelated worktree state.
- Added this dated handoff plus durable system-map and documentation-backlog evidence for the reproduced boundary and exact checks.

Runtime blocker evidence

- `command -v systemctl` and `systemctl --version` — `systemctl: command not found`.
- PID 1 — `docker-init /sbin/docker-init -- sleep infinity`.
- `/run/systemd/private` — absent.
- `/dev` is visible, but `/dev/block` is absent; the sandbox runs as `uid=1000 gid=1000`.
- Direct `/usr/bin/lsblk --json --bytes --output NAME,KNAME,TYPE,SIZE,MODEL,SERIAL,WWN,ROTA,TRAN,RM,RO,PATH,MOUNTPOINTS` parsed a JSON response with 4 top-level block devices. This is a utility smoke check only, not service-managed collection evidence.

Verification evidence from the current checkout

- `cd backend && cargo test agent::transport --all-features` — passed, 14 tests.
- `cd backend && cargo test agent::supervision --all-features` — passed, 4 tests.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, 13 tests.
- `cd backend && cargo test api::node_enroll::tests --all-features` — passed, 14 tests.
- `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` — passed, 622 unit tests, 2 integration tests, and the example target. The explicit workspace variable is required by the golden-path workflow fixture in this sandbox.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `git diff --cached --check` — passed after staging the continuity-only Markdown updates.
- `scripts/check-schema-migration-ownership.sh` — exit 0, but emitted `rg: command not found`; complete ownership verification is not established.
- `cd backend && cargo fmt --check` — blocked because `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — blocked because `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `scripts/check-repository-hygiene.sh` — failed on the repository's pre-existing tracked `docs/internal/` history/evidence paths and the intentionally staged session-14 handoff; no cleanup was attempted.
- The source/test commands above ran before staging these continuity-only Markdown updates; the staged diff check and the post-staging hygiene check were run after the updates were staged.

Maturity and limitations

- Existing managed-node enrollment, transport, supervision, and authenticated CMDB router/database behavior remains at the previously recorded `integration-verified` baseline from the real-router/database tests in the preceding handoffs; this blocked checkpoint adds no new integration coverage or maturity promotion.
- `blocked`: systemd installation/start/status, protected host-state permissions, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox cannot provide the required supported Linux host or supervisor boundary.
- `blocked`: strict repository Clippy/rustfmt and fully trustworthy schema-ownership verification due unavailable toolchain components and `rg`.
- No runtime or release claim is made from direct `lsblk`, source inventory, workflow-contract tests, or the Docker process environment.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`.
- Untracked and not included: `scripts/__pycache__/`, `testing/`.

Retrospective

- Learned: direct host-device access does not substitute for systemd-managed service evidence; the supervisor, state-permission, outage/restart, upgrade, rollback, and checksum claims require a named supported host or VM.
- Verified: the runtime prerequisite failure directly, all available C3-03 focused suites, the full backend target, release-gate tests, repository truth, schema wrapper exit, hygiene status, and diff hygiene.
- Remaining blocked: named supported Linux host/VM with real systemd and host `/dev` visibility; rustfmt/clippy components; `rg` for trustworthy schema ownership.
- Reusable commands: the exact commands in this handoff and the qualification checklist at `docs/agent/linux-agent-service.md:43-54`.
- End-user documentation changed: none; existing `docs/agent/linux-agent-service.md`, `docs/agent/node-enrollment.md`, and `docs/agent/inventory-upload.md` remain accurate for the current unit/integration boundary. The supported-host runbook still requires observed evidence after the blocker is removed.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:43-54` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Do not begin an unrelated source-only C3-03 milestone or promote runtime/release qualification from this Docker sandbox.
