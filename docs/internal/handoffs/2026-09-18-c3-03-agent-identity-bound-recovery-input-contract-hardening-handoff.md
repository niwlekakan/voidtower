# 2026-09-18 — C3-03 agent identity-bound recovery and input-contract hardening

Status: implemented and unit/integration verified; runtime and release qualification remain blocked
Tracked slice: C3-03 Linux agent supervision, durable pending inventory, and authenticated CMDB upload contracts
Commit: ffe31b9ddb26d57f5a03fd0eb79f5d2f3595fbda (`[verified] harden agent inventory recovery contracts`)
Branch: dev

Implemented

- `PendingSnapshotStore` now persists a schema-versioned envelope containing the enrolled node UUID and validates the embedded `InventorySnapshotV1` before save/load. An absent sidecar remains the normal first-run condition; legacy, malformed, semantically invalid, oversized, or node-mismatched present sidecars fail closed. Writes remain atomic and owner-only.
- `InventorySnapshotV1::validate` is the shared bounded contract for schema version, UUID snapshot identity, positive collection time, bounded text and identity evidence, host/entity namespace uniqueness, entity count, control characters, JSON object shape, and bounded nested JSON.
- The authenticated inventory route validates the snapshot after node authentication and before canonical host lookup or CMDB mutation. Existing node binding, replay/idempotency, and stable error behavior remain covered.
- Agent transport accepts an acknowledgement snapshot ID only when it matches the uploaded contract identity after the allowed surrounding whitespace normalization; mismatches remain retryable.
- Custom CA loading rejects group/world-readable Unix files and retains regular-file, symlink-chain, size, and PEM checks.
- Updated `docs/agent/inventory-upload.md`, `docs/agent/linux-agent-service.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md` with verified behavior, evidence, and limitations.

Files in the focused commit

- `backend/src/agent/mod.rs`
- `backend/src/agent/state.rs`
- `backend/src/agent/supervision.rs`
- `backend/src/agent/transport.rs`
- `backend/src/api/cmdb/inventory.rs`
- `backend/src/api/cmdb/tests.rs`
- `backend/src/cmdb/contracts.rs`
- `docs/agent/inventory-upload.md`
- `docs/agent/linux-agent-service.md`
- `docs/internal/agent-knowledge/system-map.md`
- `docs/internal/agent-knowledge/documentation-backlog.md`

Verification evidence

- `cd backend && cargo test agent:: --all-features` — passed.
- `cd backend && cargo test cmdb::contracts::tests:: --all-features` — passed, including the host/entity collision and non-object payload regression.
- `cd backend && cargo test api::cmdb::tests --all-features` — passed, including semantic rejection and authentication-before-oversize behavior.
- `cd backend && cargo test agent::transport::tests::inventory_upload_uses_node_path_and_scoped_token --all-features` — passed, including whitespace-normalized acknowledgement identity.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — passed; full backend targets, workflow-contract integration tests, and example target completed successfully.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed with `runtime_support_claimed: false`; source inventory only.
- `git diff --check` and `git diff --cached --check` — passed.
- Independent final review — passed with no security concerns or logic errors. Non-blocking suggestions were additional boundary tests only; no changes were required for this slice.
- Added-line security scan — passed: no hardcoded credentials, shell injection, eval/exec, unsafe pickle loading, or formatted SQL query findings.

Blocked or not qualified

- `cargo fmt --check` — blocked because `cargo-fmt` is not installed for toolchain `1.98.1-x86_64-unknown-linux-gnu`.
- `cargo clippy --all-targets --all-features -- -D warnings` — blocked because `cargo-clippy` is not installed for the active toolchain.
- `scripts/check-schema-migration-ownership.sh` exited 0 but emitted `rg: command not found`; complete ownership verification is not established.
- Runtime qualification is blocked: the sandbox has no `systemctl` or `/run/systemd/private`, PID 1 is `docker-init`, and the required named supported Linux host/device boundary is unavailable. Direct `lsblk`, source inventory, and workflow tests do not establish service-managed runtime or release support.
- Existing unrelated worktree changes were preserved and excluded from the commit: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, `scripts/__pycache__/`, and `testing/`.

Retrospective

- Learned: a durable inventory sidecar must carry the enrolled node identity because a replaced state file can otherwise make valid old evidence appear valid under a different node; shared validation must reject host/entity namespace collisions before CMDB lookup; acknowledgement identity must use the same normalization as the contract.
- Verified: node-bound atomic persistence, fail-closed present-sidecar handling, bounded semantic snapshot validation, authenticated route ordering, replay-safe upload behavior, CA file permissions, focused Rust tests, full backend targets, release-gate tests, repository truth, and diff hygiene.
- Remaining blocked: named supported Linux systemd/device runtime, rustfmt/clippy components, and trustworthy schema ownership verification without `rg`.
- Reusable commands and fixtures are listed above and in `docs/internal/agent-knowledge/documentation-backlog.md`; the UUID fixture used by persistence/contract tests is `58b99686-7b8e-4d7f-a169-89cc56a6052c`.
- End-user documentation changed: inventory upload validation/normalization and Linux sidecar/CA permission behavior are now documented. A supported-host runbook still requires observed systemd installation/status, real service-managed collection/upload, outage/restart recovery, upgrade/rollback, redacted artifacts, and checksum evidence.

Next dependency-ready slice

Run `docs/agent/linux-agent-service.md:43-54` on a named supported Linux host or VM with real systemd and host `/dev` visibility. Do not promote runtime or release qualification from this Docker sandbox and do not begin an unrelated backend, frontend, collector, adoption, deployment, or release initiative before that boundary is resolved.
