# 2026-09-19 — C3-03 supported-host runtime qualification session 23

Status: blocked; no runtime or release maturity promotion
Tracked slice: C3-03 Linux agent supervision and service package qualification
Base commit before this continuity record: 0422bf34f60e3abfc4b1c154f9c5cd07bbad4dc2
Continuity record commit: 1c73097c151cf89f984ae2a8f7b1b8104ebc564f
Branch: dev

Active slice and boundary

- The tracked plan and newest handoff select C3-03 supported-host qualification as the next dependency-ready outcome.
- The public qualification seams are the packaged Linux agent/systemd unit, protected state directory and file, fixed `/usr/bin/lsblk` collection, enrolled-node upload, controller outage/restart recovery, upgrade, rollback, and artifact checksum evidence.
- This session did not alter product source or claim a substitute container runtime. It stopped at the real host-runtime blocker and recorded reproducible evidence.
- Non-goals preserved: no inbound listener, remote command channel, Windows support, frontend/CMDB UI, provider mutation, deployment initiative, or unrelated worktree cleanup.

Runtime blocker evidence

Probe command, run from the repository root at `2026-09-19T13:50:55+00:00`, was:

`date -Iseconds && printf 'pid1=' && tr '\0' ' ' </proc/1/cmdline && printf '\nsystemctl=' && command -v systemctl || true && printf 'systemd-analyze=' && command -v systemd-analyze || true && printf 'private_socket=' && if [ -S /run/systemd/private ]; then printf yes; else printf no; fi && printf '\ndev_block=' && if [ -d /dev/block ]; then printf yes; else printf no; fi && printf '\nuid=' && id -u && printf ' gid=' && id -g && printf '\nlsblk=' && command -v lsblk || true`

The exact probe returned exit 0 with:

- `pid1=/sbin/docker-init -- sleep infinity`
- `systemctl` unavailable
- `systemd-analyze` unavailable
- `/run/systemd/private` absent
- `/dev/block` absent
- process UID/GID `1000/1000`
- `/usr/bin/lsblk` present

This environment cannot provide valid evidence for systemd installation/start/status, service-managed host-device collection, protected service state, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, or packaged artifact checksum. Direct `/usr/bin/lsblk` availability is not service qualification.

Verification evidence

All commands below ran against the final pre-commit worktree, with existing unrelated changes preserved:

- `cargo test --manifest-path backend/Cargo.toml agent:: --all-features` — exit 0; 41 passed.
- `cargo test --manifest-path backend/Cargo.toml api::cmdb::tests --all-features` — exit 0; 15 passed.
- `cargo test --manifest-path backend/Cargo.toml cmdb::contracts::tests:: --all-features` — exit 0; 4 passed.
- `python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — exit 0; 19 passed.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — final serial rerun exit 0; 635 unit tests, 2 workflow-contract integration tests, and the example target passed.
- `python3 scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed and reported `runtime_support_claimed: false`.
- `bash -n scripts/install.sh scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null` — exit 0.
- `git diff --check && git diff --cached --check` — exit 0 before this handoff was added; after the three documentation files were staged, `git diff --cached --check` was rerun and also passed.
- `cargo fmt --check` — blocked because `cargo-fmt` is not installed for Rust 1.98.1.
- `cargo clippy --all-targets --all-features -- -D warnings` — blocked because `cargo-clippy` is not installed for Rust 1.98.1.
- `scripts/check-schema-migration-ownership.sh` — exits 0 but emits `rg: command not found`; its success is not treated as verified schema ownership.
- `scripts/check-repository-hygiene.sh` — exit 1 on the pre-existing tracked internal-history policy list; no hygiene claim is made.

The first full test invocation was run concurrently with other checks and failed after the 512 MiB `/tmp` tmpfs filled, producing database/disk-full failures. Only disposable generated `/tmp/vt-p1-*` and `/tmp/voidtower-*` artifacts were removed, then the exact full command was rerun serially and passed. No repository source or unrelated worktree path was removed.

Maturity and limitations

- Existing C3-03 implementation remains `unit-verified` for bounded collector failure behavior and `integration-verified` for the sanitized collector → supervision → loopback transport recovery seam, as established by the prior handoff.
- This session adds no runtime or release qualification. The C3-03 supported-host gate remains `blocked` on the named systemd, host `/dev`, service-account, and packaged-artifact prerequisites.
- No end-user documentation behavior changed in this blocked checkpoint. The supported-host operator evidence still must be added to `docs/agent/linux-agent-service.md` or a linked dated evidence record after a real host run.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, and `scripts/test_release_gate.py`.
- Untracked and not included: `testing/`.
- No unrelated path was staged, reset, cleaned, overwritten, or committed.

Retrospective

- Learned: the sandbox can run deterministic agent/CMDB/package/full backend checks but cannot emulate the systemd and host-device boundary without producing invalid qualification evidence; concurrent SQLite-heavy tests can exhaust the 512 MiB `/tmp` fixture space.
- Verified: blocker probes, focused agent/CMDB/contracts/package tests, serial full backend targets, repository truth, shell/JSON syntax, and diff hygiene. Runtime/service behavior remains unverified.
- Reusable commands: the focused commands above, the serial full command with `GITHUB_WORKSPACE="$PWD"`, and cleanup limited to `/tmp/vt-p1-*` and `/tmp/voidtower-*` when the documented disposable tmpfs fills.
- End-user documentation changed: none. Required next documentation is redacted named-host evidence for systemd status, protected state, real collection/upload, outage/restart recovery, upgrade, rollback, and checksum.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:56-64` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Capture installation/status, protected state, service-managed collection/upload, controller outage and process-restart recovery, upgrade, rollback, and artifact checksum before promoting C3-03 beyond the current unit/integration evidence.
