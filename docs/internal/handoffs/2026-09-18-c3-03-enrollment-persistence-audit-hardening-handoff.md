# 2026-09-18 — C3-03 enrollment persistence and audit-boundary hardening

Status: implemented, unit-verified, and integration-verified at the managed-node enrollment and deletion-audit seams; supported-host qualification remains blocked.

Tracked slice: C3-03 managed-node enrollment lifecycle hardening
Implementation commit: 833bf6d33ea4d41714c83a01611851888ccee6ee (`[verified] harden enrollment persistence and audit boundaries`)
Base: 4c2f9f02c9f7a294bc41b316a3823cb9b527bf13
Branch: dev

Implemented

- `backend/src/api/node_enroll.rs` now performs pairing-code claim, owner existence validation, and node insertion in one SQLite transaction. Node persistence failure rolls back the claim; successful commit precedes best-effort audit logging.
- The atomic claim uses SQLite write-time epoch evaluation for `used_at` and `expires_at`, preventing a stale request timestamp from enrolling a code that expires while waiting for a database lock.
- Node-deletion audit details now serialize the display name as structured JSON rather than delimiter-built text.
- Added public real-router tests for persistence rollback, exact expiry-boundary rejection, structured deletion audit values, and retained existing concurrency/success coverage.
- Updated `docs/agent/node-enrollment.md` with transaction rollback/retry and structured audit contracts.
- Updated living evidence files under `docs/internal/agent-knowledge/` with the verified system-map and documentation-backlog entries.

Verification evidence

- `cd backend && cargo test api::node_enroll::tests --all-features` — passed, 14 tests.
- `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` — passed, 622 unit tests, 2 integration tests, and the example target.
- `python3 scripts/test_release_gate.py` — passed, 11 tests.
- `python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only with `runtime_support_claimed: false`.
- `git diff --cached --check` before commit — passed.
- Independent final review against the actual staged diff — passed with no security or logic findings. Non-blocking suggestions were fixture naming and a separate owner-resolution rollback test.

Maturity and limitations

- `implemented`: transactional enrollment persistence, write-time expiry enforcement, structured deletion audit, and reachable public test seams.
- `unit-verified`: focused enrollment suite and full backend unit target.
- `integration-verified`: real Axum-router/database enrollment and deletion-audit boundaries in the full backend target.
- `blocked`: systemd installation/start/status, protected host-state observation, service-managed `/usr/bin/lsblk` collection/upload, outbound-only runtime observation, controller outage/process-restart recovery, upgrade, rollback, packaged artifact checksum, and release qualification. The Docker sandbox lacks `systemctl`, `systemd-analyze`, `/run/systemd/private`, and the supported host/device boundary.
- `cargo fmt --check` and strict Clippy were not run successfully because the active environment lacks the required toolchain components. Schema ownership is not fully verified because the wrapper emits the known `rg: command not found` diagnostic despite exit code 0.

Preserved unrelated worktree state

- Modified but not included: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`.
- Untracked and not included: `scripts/__pycache__/`, `testing/`.
- Branch is one commit ahead of `origin/dev`; nothing was pushed.

Retrospective

- Learned: expiry must be enforced in the database write predicate, not only in an earlier application read; SQLite write-time epoch evaluation closes the lock-wait race. Transactional claim-plus-node persistence makes failed node creation retryable without consuming a pairing code. Structured JSON prevents audit ambiguity for operator-controlled display names.
- Verified: 14 enrollment public-seam tests, full backend target, release-gate tests, repository truth, diff hygiene, and independent review.
- Remaining blocked: named supported Linux systemd/device qualification and unavailable rustfmt/strict Clippy/fully trustworthy schema ownership prerequisites.
- Reusable commands: the focused and full commands above; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.
- End-user documentation changed: `docs/agent/node-enrollment.md`. Future documentation still required after host evidence: supported-host runtime/recovery runbook with observed systemd status, protected state, collection/upload, outage/restart, upgrade, rollback, and artifact evidence.

Next bounded slice

Run the C3-03 supported-host qualification checklist on a named Linux host or VM with real systemd and host `/dev` visibility. Do not promote runtime or release qualification from this Docker sandbox.
