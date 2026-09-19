# 2026-09-19 — C3-03 state recovery integrity hardening

Status: implemented and unit-verified; supported-host runtime/release qualification remains blocked
Tracked slice: C3-03 Linux agent supervision and service package
Base commit: 8223c77db6b57c4045b8bea61a23a310bb5263b8
Implementation commit: 1590f9e44db3a360f330f1d80a522fae13871d55
Branch: dev

Active slice and boundary

- Hardened the two protected recovery seams: `AgentState::load` and `PendingSnapshotStore::load`.
- On Unix, parent components are walked through stable directory descriptors with `openat` and `O_NOFOLLOW`; the protected file is opened relative to the stable parent descriptor with `O_NOFOLLOW` and `O_NONBLOCK`.
- Recovery fails closed for symlink/traversal parents, non-directory components, non-sticky group/other writable directories, immediate parent ownership mismatch, non-regular targets, non-0600 targets, and bounded parse/validation errors. No protected state or diagnostic contents are returned in the errors.
- The operator contract in `docs/agent/linux-agent-service.md` and the living knowledge files records the parent-chain and owner-match invariant. Unrelated worktree paths were preserved.
- Non-goals: systemd qualification, host `/dev` qualification, transport retry-classification redesign, enrollment admission policy, Windows support, frontend, provider mutations, and unrelated modified/untracked paths.

Acceptance evidence

- RED: before the helper existed, both new writable-parent tests failed because `AgentState::load` and `PendingSnapshotStore::load` returned `Ok` for a 0770 parent.
- GREEN: `cargo test --manifest-path backend/Cargo.toml agent::state::tests:: --all-features` — exit 0; 23 passed.
- Focused agent suite: `cargo test --manifest-path backend/Cargo.toml agent:: --all-features` — exit 0; 45 passed.
- Full backend gate: `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — exit 0; 639 unit tests, 2 workflow-contract integration tests, and the example passed.
- Package/release checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` — exit 0; 19 passed.
- Source/reproducibility: `python3 scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed and `runtime_support_claimed` remained false.
- Syntax and diff hygiene: `bash -n scripts/install.sh scripts/build-release.sh && python3 -m json.tool scripts/release-gates.json >/dev/null` — exit 0; `git diff --check` and `git diff --cached --check` — exit 0.
- Independent review: first review correctly found a path-based TOCTOU and the implementation was revised to descriptor-based traversal; second review correctly found blocking special-file opens and the implementation added `O_NONBLOCK` plus FIFO tests; final independent review passed with `passed: true`, empty `security_concerns`, and empty `logic_errors`.
- Commit scope: `1590f9e44db3a360f330f1d80a522fae13871d55` contains only `backend/src/agent/state.rs`, `docs/agent/linux-agent-service.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md`.

Limitations and blockers

- Runtime probe at `2026-09-19T14:51:42+00:00`: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are unavailable; `/run/systemd/private` and `/dev/block` are absent; UID/GID is 1000/1000; `/usr/bin/lsblk` exists. This cannot qualify systemd installation/start/status, service-managed host-device collection, outage/process-restart recovery, upgrade, rollback, or artifact checksums.
- `cargo fmt --check` is blocked because `cargo-fmt` is unavailable for Rust 1.98.1; strict Clippy is blocked because `cargo-clippy` is unavailable.
- `scripts/check-schema-migration-ownership.sh` exits 0 but emits `rg: command not found`; its output is not promoted as trustworthy schema-ownership verification.
- `scripts/check-repository-hygiene.sh` remains blocked by the pre-existing tracked internal-history policy list. No hygiene claim is made.
- Existing unrelated worktree state after the commit: modified `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, and untracked `testing/`; none was staged or committed.

Retrospective

- Learned: secure load validation must bind all parent checks to stable directory descriptors; path-only checks leave a rename/replace window. Protected state paths must use nonblocking opens so FIFOs cannot hang agent startup or recovery.
- Verified: state and pending-sidecar permission, ownership, symlink, traversal, special-file, size, parse, node-binding, atomic persistence, supervision, package-contract, full backend, source-truth, syntax, and diff checks. Evidence is unit/integration-verified, not runtime-qualified.
- Reusable commands: `cargo test --manifest-path backend/Cargo.toml agent::state::tests:: --all-features`; `cargo test --manifest-path backend/Cargo.toml agent:: --all-features`; serial full backend with `GITHUB_WORKSPACE="$PWD"`; package/release unittest command; repository truth; shell/JSON syntax; and `git diff --check`.
- End-user documentation changed: `docs/agent/linux-agent-service.md` now states the Unix parent-chain, writable-directory, immediate-parent-owner, and protected-target recovery requirements. The supported-host evidence runbook is still required.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:56-64` on a named supported Linux host or VM with real systemd, `/run/systemd/private`, host `/dev`, a packaged candidate artifact, an enrolled node, and a pre-existing canonical host resource bound to that node. Capture install/status, protected state, service-managed collection/upload, outage and process-restart recovery, upgrade, rollback, redacted evidence, and SHA256SUMS before promoting C3-03 beyond unit/integration evidence.
