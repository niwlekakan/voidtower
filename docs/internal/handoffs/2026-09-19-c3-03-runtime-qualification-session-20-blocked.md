# 2026-09-19 — C3-03 supported-host runtime qualification session 20 (blocked)

Status: blocked at the supported-host runtime boundary; deterministic source and contract evidence rechecked
Tracked slice: C3-03 Linux agent supervision and service package qualification
Evidence base commit: 4152597d31400138350bfd661fa11707d100c90b (`docs(agent): record blocked c3-03 qualification session 19`)
Branch: dev

Implemented

- No product source or end-user documentation changed. This checkpoint appends only evidence-backed continuity records to the living system map, documentation backlog, and dated handoff.
- The runtime boundary was directly probed with `command -v systemctl || true; command -v systemd-analyze || true; ps -p 1 -o pid=,args=; test -S /run/systemd/private; printf 'systemd_socket_exit=%s\n' "$?"; test -e /dev/block; printf 'dev_block_exit=%s\n' "$?"; test -x /usr/bin/lsblk; printf 'lsblk_exit=%s\n' "$?"; id`: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` and `/dev/block` are absent (exit 1); `/usr/bin/lsblk` exists (exit 0); and the process runs as UID/GID 1000.
- The exact supported-host procedure remains the public seam at `docs/agent/linux-agent-service.md:56-64`; it cannot run in this sandbox.

Verification evidence

- `cargo test agent:: --all-features` from `backend` — exit 0; 40 passed, 0 failed.
- `cargo test api::cmdb::tests --all-features` from `backend` — exit 0; 15 passed, 0 failed.
- `cargo test cmdb::contracts::tests:: --all-features` from `backend` — exit 0; 4 passed, 0 failed.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — exit 0; 19 tests passed.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` from the repository root — exit 0; 630 unit tests, 2 workflow-contract integration tests, and the example target passed.
- `python3 scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed and reported `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — exit 0 but emitted `rg: command not found`; complete ownership verification is not established.
- `cd backend && cargo fmt --check` — exit 1 because `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — exit 1 because `cargo-clippy` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `bash -n scripts/install.sh scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null && git diff --check` — exit 0.

Independent review

- Independent reviewer verdict: passed. The reviewer found no security concerns or logic errors; the staged diff contains only the two intended agent-knowledge updates and this dated handoff, uses the corrected `docs/agent/linux-agent-service.md:56-64` reference, preserves unrelated worktree paths, and makes no runtime or release overclaim.

Maturity and limitations

- Existing managed-node enrollment, transport, supervision, package, and authenticated CMDB router/database behavior remains at the previously recorded unit/integration-verified baseline. This checkpoint adds no integration coverage and makes no maturity promotion.
- `blocked`: `systemd-analyze verify`, service installation/start/status, protected host-state permissions, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification.
- No runtime or release claim is made from direct utility availability, source inventory, workflow-contract tests, controlled fixtures, or the Docker process environment.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No changes were staged, reset, pushed, or made to those paths.

Retrospective

- Learned: the corrected full-backend invocation must run from the repository root with `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; invoking the same command from `backend` with a parent workspace value can lose the manifest context in this sandbox. This is an invocation detail, not a product failure.
- Verified: supported-host blocker probes, agent state/transport/supervision tests, CMDB inventory router and contract tests, package/release-gate tests, full backend targets, repository truth, schema wrapper behavior, shell/JSON syntax, and diff hygiene.
- Remaining blocked: a named supported Linux host/VM with real systemd and host `/dev` visibility; active-toolchain rustfmt and Clippy components; and `rg` for trustworthy schema ownership.
- Reusable commands and future end-user documentation requirements are recorded in `docs/internal/agent-knowledge/documentation-backlog.md`.
- End-user documentation changed: none. The supported-host runbook remains required and is already described in `docs/agent/linux-agent-service.md:56-64`.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:56-64` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Do not begin an unrelated source-only C3-03 change or promote runtime/release qualification from this Docker sandbox.
