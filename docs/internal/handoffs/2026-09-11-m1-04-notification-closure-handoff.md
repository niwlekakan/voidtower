# VoidTower Slice Handoff

- **Date:** 2026-09-11
- **Status:** integration-verified
- **Tracked plan slice:** M1-04 — Compatibility bypass closure.
- **Branch and code commit:** `dev`; `24a5e004c0ec3a646acac4fa1bf1eab014bfcb0b` (`[verified] close notification webhook compatibility mutations`).

## Outcome

The notification compatibility mutation paths now fail closed after administrator authentication. `POST /api/settings/notifications` returns a stable typed `503 feature_unavailable` response without changing stored settings or writing an audit row. `POST /api/settings/notifications/test` returns a stable typed `503 feature_unavailable` response without reading a webhook URL or making an outbound HTTP request. Unauthenticated requests remain `401 unauthorized`.

## Contract and invariants

- **Public seams changed:** `POST /api/settings/notifications`; `POST /api/settings/notifications/test`.
- **Canonical invariants preserved:**
  - Compatibility routes do not execute provider or destructive notification mutations outside a canonical operation adapter.
  - Existing administrator authentication is evaluated before the unavailable boundary for valid request bodies.
  - A configured legacy webhook setting remains unchanged when the closed settings mutation is called.
  - The test-delivery handler contains no direct database lookup, HTTP client, or outbound POST path.
  - No canonical resource identity, schema migration, durable job, approval, event, agent, CMDB, or provider adapter was changed.
  - Test fixtures and evidence use loopback placeholder URLs only; no credential values were read or persisted.
- **Explicit non-goals preserved:** canonical notification action/plan/policy/approval/job adapter; secret-manager migration; frontend notification UX; live ntfy/Discord/Slack delivery; runtime provider qualification; release qualification.

## Files and commit scope

- **Committed code file:** `backend/src/api/settings.rs`.
- **Local evidence artifacts:**
  - `docs/internal/evidence/2026-09-11-m1-04-notification-closure/slice-plan.md`
  - `docs/internal/evidence/2026-09-11-m1-04-notification-closure/batch.json`
  - `docs/internal/evidence/2026-09-11-m1-04-notification-closure/final-report/evidence.json`
  - `docs/internal/evidence/2026-09-11-m1-04-notification-closure/final-source-truth.json`
- **Preserved unrelated staged files:**
  - `backend/src/agent/mod.rs`
  - `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** none.

## Verification evidence

- **Delivery shape:** two ordered checkpoints, `notification-settings-boundary` and `notification-test-boundary`, share one settings compatibility trust boundary, one fail-closed contract, and one rollback commit.
- **TDD evidence:** the first focused test was run before implementation and failed for the intended reason: the current settings handler returned HTTP `200` instead of the required `503`. After implementation, the same public-seam tests passed.
- **Focused result:** Dockerized `cargo test api::settings::tests --all-features -- --nocapture` returned `7 passed; 0 failed; 501 filtered out`, including authenticated 503/no-persistence, outbound-test fail-closed, unauthenticated 401, and source-enforcement tests. `integration-verified`.
- **Full backend result:** Dockerized `cargo test --all-targets --all-features` returned `508 passed; 0 failed; 0 ignored`, the `golden_path` integration target returned `2 passed; 0 failed`, and the example target had `0 tests`. `integration-verified`.
- **Clippy result:** Dockerized `rustup component add clippy && cargo clippy --all-targets --all-features -- -D warnings` returned exit `0` with no warnings/errors.
- **Schema result:** `scripts/check-schema-migration-ownership.sh` returned exit `0` with `Schema migration ownership check passed.`
- **Source truth:** post-commit `python scripts/repo_truth.py --repo . --json --check` returned exit `0`; report is saved at `final-source-truth.json` and remains source-inventory-only.
- **Diff/security result:** `git diff --check` and the added-line static security scan returned clean. The scan found no hardcoded-secret assignment, shell-injection, eval/exec, unsafe-deserialization, or formatted-SQL finding.
- **Batch report:** the final manifest report records focused tests, full backend tests, schema ownership, and diff checks as passed. Its manifest-only Clippy and rustfmt steps are false because the disposable `rust:latest` toolchain lacks those components; the installed-component Clippy rerun passed. The installed-component scoped rustfmt check reached existing formatting drift in legacy portions of `backend/src/api/settings.rs`; no formatting changes were applied.
- **Independent review:** bounded reviewer attempts inspected the intended cached diff and source contract only; no high or medium security/logic finding was reported. The reviewer processes did not return a machine-readable JSON verdict before their bounded stop, so this is corroborating inspection rather than a released review artifact.

## Failures and limitations

- **Blocking failures:** none for the bounded compatibility-closure behavior.
- **Known limitations:**
  - Notification configuration and test delivery remain intentionally unavailable until a canonical typed notification operation adapter exists.
  - `GET /api/settings/notifications` still exposes the legacy stored notification values to an authenticated administrator; closing that plaintext settings read belongs to secret-manager convergence (S2-02) and was explicitly outside this slice.
  - The frontend still presents the legacy notification form and will receive the stable unavailable response on save/test; frontend UX was a non-goal.
  - No live provider, network delivery, deployment/runtime, upgrade/recovery, or release-artifact qualification was run.
  - Whole-workspace formatting is not qualified because pre-existing Rust formatting drift remains; the slice did not widen the diff to reformat it.
- **Recovery/rollback:** `git revert 24a5e004c0ec3a646acac4fa1bf1eab014bfcb0b` removes this code slice while preserving the unrelated staged agent files. Retaining the closure is the safe state until the canonical adapter is ready.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` / `24a5e004c0ec3a646acac4fa1bf1eab014bfcb0b` / ahead 94 / behind 0 before this handoff commit.
- **Staged paths:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`.
- **Modified paths:** none.
- **Remote publication:** not pushed.

## Next bounded slice

- **Next slice:** M1-04 remaining compatibility inventory — classify and close the next source-derived provider/destructive compatibility ingress.
- **Acceptance seam:** one public source-enforcement or representative real-router test for the selected remaining call site proving no provider call occurs outside an approved adapter or typed exception.
- **Non-goals:** no notification adapter implementation in the follow-up, no secret-manager migration in this M1-04 continuation, no broad inventory rewrite, and no unrelated agent/CMDB work.
- **Blockers:** none for the next source-classification step; canonical notification operation design and S2-02 plaintext read closure remain separate dependencies.

## Reproduction rule

From the repository root, read `docs/development-plan.md`, this handoff, and the slice plan; run `python scripts/repo_truth.py --repo . --json --check`; execute `docs/internal/evidence/2026-09-11-m1-04-notification-closure/batch.json` with the repository `slice_batch.py`; then run the installed-component Clippy command and the direct full backend test command listed above. Verify `git status --short --branch` retains only `M  backend/src/agent/mod.rs` and `M  backend/src/agent/state.rs` after the separate handoff commit.
