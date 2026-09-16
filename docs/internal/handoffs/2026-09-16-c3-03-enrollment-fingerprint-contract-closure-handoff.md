# 2026-09-16 — C3-03 enrollment/fingerprint contract closure handoff

Status: unit-verified and integration-verified; supported-host runtime qualification remains blocked
Tracked slice: C3-03 continuation — bounded enrollment and canonical inventory replay identity
Commit: b8312d418bc9ac671e3cc39db46bc296b95903c9
Base: e808c49b088362160538456164ecc20eaad43873
Branch: dev (ahead 1, behind 0)

Implemented

- `/api/nodes/enroll` now has a 64 KiB route body limit.
- Axum JSON length-limit rejection maps to the stable `413 payload_too_large` response; other JSON extraction failures map to bounded `400 bad_request` without raw extractor diagnostics.
- Inventory snapshot fingerprinting trims `snapshot_id` on a cloned snapshot before hashing, matching the existing trimmed persistence/replay identity. Equivalent surrounding-whitespace representations replay safely.
- Real-router tests cover the oversized enrollment response envelope and whitespace-normalized inventory replay.
- `docs/agent/node-enrollment.md` documents the new limits and replay contract.
- Living `system-map.md` and `documentation-backlog.md` record the verified behavior, evidence, reusable commands, and remaining operator documentation.

Verification after final source state

- `cd backend && cargo test agent::transport --all-features` — passed, 13 tests.
- `cd backend && cargo test api::node_enroll::tests --all-features` — passed, 9 tests.
- `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features` — passed, 3 tests.
- `cd backend && cargo test --all-targets --all-features` — passed, 614 unit tests, 2 integration tests, and the example target.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed: false`.
- `scripts/check-schema-migration-ownership.sh` — exited 0 but emitted `rg: command not found`; schema ownership is not fully verified in this sandbox.
- `git diff --check` — passed.
- Independent final review — passed; no security or logic findings. Non-blocking suggestions were malformed/exact-boundary enrollment tests and running rustfmt; existing extraction mapping and over-limit coverage are present, and rustfmt is unavailable in this environment.

Maturity and limitations

- `integration-verified`: real Axum-router/database paths prove the enrollment body-limit response and authenticated inventory replay behavior.
- `unit-verified`: typed enrollment extraction mapping and existing agent/CMDB validation suites pass.
- `blocked`: supported-host systemd installation/start/status, protected host state, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, artifact checksum, and release qualification. This Docker sandbox has no `systemctl`, no `/run/systemd/private`, and PID 1 is Docker `docker-init`.
- `blocked`: strict repository Clippy remains blocked by existing repository-wide diagnostics; schema ownership is only partially evidenced because `rg` is unavailable.

Preserved unrelated worktree

Unstaged and not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, untracked `scripts/__pycache__/`, and untracked `testing/`.

Retrospective

Learned: a route-level Axum body limit alone does not produce VoidTower's stable error envelope; the handler must accept `JsonRejection` and translate only the length-limit variant. Snapshot identity must normalize before fingerprinting as well as before persistence/replay lookup.

Verified: bounded enrollment rejection, bounded generic JSON errors, whitespace-normalized snapshot replay, full backend tests, release-gate tests, source truth, schema-wrapper behavior, independent review, and diff hygiene.

Reusable commands/fixtures: the three focused Cargo commands above; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`.

End-user documentation changed: `docs/agent/node-enrollment.md` now documents the 64 KiB enrollment cap, stable oversized-body error, and canonical snapshot replay identity. Future documentation still required: named supported-host service/runtime qualification runbook with systemd lifecycle, real collection/upload, outage/restart, upgrade, rollback, redacted artifacts, and checksum.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not promote C3-03 to runtime/release qualification from this sandbox.
