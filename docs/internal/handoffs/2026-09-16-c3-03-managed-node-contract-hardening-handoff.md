# 2026-09-16 — C3-03 managed-node contract hardening handoff

Status: unit-verified and integration-verified for the staged managed-node request boundaries; host runtime qualification remains blocked
Tracked slice: C3-03 continuation — enrollment, heartbeat, and authenticated inventory contract hardening
Commit: d73bb7516e6834fb2c1b3c40fcaf19f9a247ac2b
Base: 5c1fea0739fe6b9a0936bac3bc8469fdf526b505
Branch: dev

Implemented

- Enrollment validation is bounded and aligned between controller and agent transport: pairing code is capped at 512 bytes, display names at 128 bytes and reject control characters, and device types remain an explicit enum.
- Node bearer verification is path-bound, approved/agent-capable bound, and rejects empty or over-512-byte token values before hashing/database matching.
- Heartbeat and inventory routes authenticate in route middleware before their bounded body extractors. Heartbeat is capped at 64 KiB; inventory retains its 4 MiB application bound.
- Authenticated body-limit failures return the stable `payload_too_large` envelope; other body-buffer failures return bounded `bad_request` without raw extractor diagnostics.
- Inventory remains bound to the single canonical `resources.id` CMDB host projection and keeps replay/conflict/revocation behavior intact.
- `docs/agent/node-enrollment.md` documents the request/response, validation, authentication ordering, limits, and qualification boundary.
- Living system map and documentation backlog were appended with verified facts and remaining runtime documentation needs.

Verification after final source state

- `cd backend && cargo test agent::transport --all-features` — passed, 13 tests.
- `cd backend && cargo test api::node_enroll::tests --all-features` — passed, 8 tests.
- `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features` — passed, 3 tests.
- `cd backend && cargo test --all-targets --all-features` — passed, 613 unit tests, 2 integration tests, and the example target.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — wrapper exited 0 but emitted `rg: command not found`; ownership is not fully verified in this sandbox.
- `git diff --check` — passed.
- Independent final review — passed; no security or logic findings. Non-blocking suggestions were duplicate middleware/handler verification and exact-limit tests.

Maturity and limitations

- `integration-verified`: real Axum-router/database tests cover authentication ordering, malformed/oversized requests, token/path binding, validation, inventory canonical-host binding, replay, conflict, and revocation behavior.
- `unit-verified`: agent transport validation and typed response behavior.
- `blocked`: supported-host systemd installation/start/status, protected state on host, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, artifact checksum, and release qualification. The Docker sandbox has no `systemctl`, no `/run/systemd/private`, and PID 1 is `docker-init`.
- `blocked`: strict repository Clippy remains blocked by existing repository-wide diagnostics. Schema ownership is only partially evidenced because `rg` is unavailable.

Preserved unrelated worktree

Unstaged and not included in this commit: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, untracked `scripts/__pycache__/`, and untracked `testing/`. These were deliberately left untouched by the commit.

Retrospective

Learned: handler-level authentication cannot establish auth-before-body semantics when Axum body extraction or its default limit runs first; route middleware must own authentication, and handler-level rejection mapping must preserve the public error envelope.

Verified: controller/agent enrollment parity, node-token bounds, middleware ordering, stable body-limit errors, canonical inventory upload integration, full backend tests, release-gate tests, source truth, schema wrapper behavior, and diff hygiene.

Reusable commands/fixtures: the three focused Cargo commands above; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`.

End-user documentation changed: `docs/agent/node-enrollment.md` now documents the 512-byte token bound and middleware-first body handling. Future documentation still required: named supported-host service/runtime qualification runbook with systemd lifecycle, real collection/upload, outage/restart, upgrade, rollback, enrollment-to-host adoption, redacted artifacts, and checksum.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not promote C3-03 to runtime/release qualification from this sandbox.
