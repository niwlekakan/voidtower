# 2026-09-19 — C3-03 bounded inventory recovery and collector failure contracts

Status: implemented and unit/integration-verified; supported-host runtime and release qualification remain blocked
Tracked slice: C3-03 Linux agent supervision and service package qualification
Implementation commit: 3a8708efe8ed14899aa28996712d45ca9770b7fb (`[verified] feat(agent): harden bounded inventory recovery`)
Branch: dev

Implemented

- `backend/src/collector.rs` now has an explicit bounded Linux program seam while production collection remains fixed to `/usr/bin/lsblk` with the 10-second timeout and the existing argument contract.
- Collector stdout and stderr are drained concurrently with bounded retention. Missing/empty programs, non-zero exit, non-UTF-8 output, stderr overflow, empty/malformed JSON, and timeout fail closed without returning a partial snapshot or diagnostic text.
- Timeout cleanup explicitly kills and awaits the child process. The Linux regression fixture records the child PID and verifies it has been reaped before the collector returns.
- `backend/src/agent/supervision.rs` runs the bounded collector in the inventory loop, preserves cancellation checks at loop and retry-wait boundaries, persists the node-bound pending snapshot before upload, retries the same snapshot after an ambiguous upload/process restart, and clears the sidecar only after a typed successful response.
- The loopback integration test exercises production supervision, transport, pending-sidecar persistence, and typed inventory upload through an injected executable fixture. It intentionally does not claim real systemd, controller/database, or HTTPS runtime qualification.
- Updated `docs/agent/linux-agent-service.md` with the bounded command failure and timeout behavior.
- Appended the verified system-map and documentation-backlog retrospective under `docs/internal/agent-knowledge/`, preserving prior entries.

Active seams and acceptance evidence

- Collector command seam: `cargo test collector::tests --all-features` — exit 0; 11 passed, 0 failed.
- Agent supervision/state/transport seam: `cargo test agent:: --all-features` — exit 0; 41 passed, 0 failed.
- Authenticated CMDB assembled seam: `cargo test api::cmdb::tests --all-features` — exit 0; 15 passed, 0 failed (also included in the final full target run).
- Full backend targets from repository root: `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — exit 0; 635 unit tests, 2 workflow-contract integration tests, and the example target passed.
- Package/release contract checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — exit 0; 19 passed, 0 failed.
- Repository truth: `python3 scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed and reported `runtime_support_claimed: false`.
- Syntax and hygiene: `bash -n scripts/install.sh scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null && git diff --check && git diff --cached --check` — exit 0.
- Added-line static security scan for hardcoded credentials, shell/eval/exec/pickle, and interpolated SQL patterns — passed.
- Independent staged-diff review — passed with no blocking security or logic findings. Non-blocking suggestions were to add cancellation-during-collection coverage and assert request path/auth headers in the integration fixture; those are follow-up improvements, not unresolved defects for this slice.

Maturity and limitations

- `integration-verified` for the bounded collector-to-supervision recovery seam using sanitized executable and loopback HTTP fixtures.
- `unit-verified` for bounded collector failures and child cleanup.
- `blocked`: supported-host `systemd-analyze verify`, service installation/start/status, owner-only host-state permissions under service management, real `/usr/bin/lsblk` collection/upload, outbound-only host runtime observation, controller outage and process-restart qualification on a real service, upgrade, rollback, packaged artifact checksum, and release qualification.
- Runtime blocker probe remains the current Docker sandbox: PID 1 is `docker-init`; `systemctl` and `systemd-analyze` are unavailable; `/run/systemd/private` and `/dev/block` are absent; the process runs as UID/GID 1000. No runtime or release claim is made from this environment.
- `cargo fmt --check` and strict Clippy could not run because the active Rust 1.98.1 toolchain lacks the `rustfmt` and `clippy` components. The schema ownership wrapper cannot be treated as verified because it emits `rg: command not found`.
- A first concurrent final-gate attempt exhausted the documented 512 MiB `/tmp` disposable-fixture space. Only `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures were removed; serial reruns passed. No source or unrelated worktree paths were removed.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No changes were staged, reset, pushed, or made to those paths. Branch `dev` is six commits ahead and zero behind `origin/dev` after the implementation commit.

Retrospective

- Learned: timed-out Tokio child processes need explicit kill-and-wait cleanup; `kill_on_drop` alone does not provide a reap guarantee. A supervision cancellation branch must not drop the collector future while it owns the child, so cancellation is checked before the next iteration and at bounded retry waits instead.
- Verified: collector failure bounds, child reaping, pending snapshot reuse and clearing, agent cancellation/backoff, authenticated CMDB behavior, package contracts, full backend targets, repository truth, syntax, static scan, diff hygiene, and independent review.
- Reusable commands and fixture patterns are recorded in `docs/internal/agent-knowledge/documentation-backlog.md`; the system map records the dependency and evidence boundary.
- End-user documentation changed: `docs/agent/linux-agent-service.md` now documents collector command failure and timeout behavior. A supported-host runtime runbook remains required for observed service and release evidence.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:56-64` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Capture installation/status, protected state, service-managed collection/upload, controller outage and process-restart recovery, upgrade, rollback, and artifact checksum. Do not claim runtime or release qualification from this Docker sandbox and do not bundle unrelated frontend, deployment, Windows, or provider work.
