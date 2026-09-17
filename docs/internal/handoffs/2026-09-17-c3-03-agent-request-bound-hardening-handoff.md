# 2026-09-17 — C3-03 agent request-bound hardening

Status: implemented and unit-verified; supported-host qualification remains blocked
Tracked slice: C3-03 Linux agent supervision/service qualification prequalification hardening
Base: 26e045179adaeee66c275bf1ae8eb9e9bde488f7
Milestone commit: 33d2758 ([verified] harden agent inventory request bounds)
Branch: dev

Implemented

- `backend/src/agent/transport.rs` now serializes inventory snapshots through a bounded 256 KiB writer. Oversized snapshots fail before endpoint construction or network send with a bounded error; successful uploads retain explicit JSON content type and exact typed `snapshot_id` acknowledgement validation.
- Added `agent::transport::tests::inventory_upload_rejects_oversized_snapshot_before_network_request`.
- Corrected `docs/agent/linux-inventory-collector.md` to describe the shipped fixed `lsblk` supervision, pending snapshot persistence, enrolled-node upload, 256 KiB client bound, and the independent 4 MiB authenticated controller body limit.
- Appended verified facts, commands, documentation backlog, and blocker evidence to `docs/internal/agent-knowledge/system-map.md` and `documentation-backlog.md`.

Verification evidence

- `cd backend && cargo test agent::transport --all-features` — passed, 14 tests.
- `cd backend && cargo test --all-targets --all-features` — passed, 617 unit tests, 2 integration tests, and the example target, after cleaning only disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` test artifacts.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — passed, same 617 unit + 2 integration + example result; the environment variable is required for the tracked golden-path workflow fixture in this sandbox.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only, `runtime_support_claimed: false`.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `git diff --check` — passed before commit.
- `cargo fmt --check` — blocked: `cargo-fmt` is unavailable for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- Strict Clippy — not established; `cargo-clippy` is unavailable in the active toolchain.
- Schema ownership — wrapper exits 0 but emits the known `rg: command not found` diagnostic; full ownership verification is not established.

Independent review

- First independent review identified unbounded pre-check JSON materialization; replaced with `BoundedJsonWriter` and re-ran the focused suite successfully.
- Final independent content review passed with no logic errors or security blockers. Non-blocking suggestions were boundary tests and documentation of the two limits; documentation now records both limits.

Maturity and limitations

- `implemented`: client-side bounded inventory serialization, explicit request content type, documentation/continuity updates.
- `unit-verified`: focused agent transport suite and full backend targets.
- `integration-verified`: existing authenticated inventory route/database behavior remains covered by the full backend suite; this change does not add a new controller integration route.
- `blocked`: systemd installation/start/status, protected host-state observation, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox lacks `systemctl`, `systemd-analyze`, `/run/systemd/private`, and the supported host/device boundary.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`.
- Untracked and not included: `scripts/__pycache__/`, `testing/`.

Retrospective

- Learned: client-side request bounds must stop serialization itself, not only inspect a fully materialized JSON buffer; the bounded writer preserves the existing typed acknowledgement contract. Full tests can exhaust the 512 MiB `/tmp` tmpfs with disposable SQLite/WAL fixtures; remove only the established fixture patterns before rerunning.
- Verified: 256 KiB pre-network rejection, successful transport regression coverage, full backend/unit/integration/example suite, source truth, release-gate tests, and documentation parity.
- Remaining blocked: named supported Linux systemd/device qualification and unavailable rustfmt/Clippy/`rg` prerequisites.
- Reusable commands: `cd backend && cargo test agent::transport --all-features`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/test_release_gate.py`; `git diff --check`; and on a supported host, the checklist at `docs/agent/linux-agent-service.md:43-54`.
- End-user documentation changed: `docs/agent/linux-inventory-collector.md`. The supported-host runbook remains required after real host evidence exists.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not promote runtime or release qualification from this Docker sandbox.
