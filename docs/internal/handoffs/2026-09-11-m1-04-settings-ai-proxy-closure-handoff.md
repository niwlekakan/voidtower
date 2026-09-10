# VoidTower Slice Handoff

- **Date:** 2026-09-11
- **Status:** integration-verified
- **Tracked plan slice:** M1-04 — Compatibility bypass closure.
- **Branch and commit:** `dev`, `7b4859b8c6c7d43e132e9a68d79eac1af54aa981`, `[verified] close AI proxy settings compatibility mutation`.

## Outcome

The selected `POST /api/settings/ai-url` compatibility mutation now authenticates administrators and fails closed with a stable `503 feature_unavailable` response until a canonical operation adapter exists. The handler performs no settings, filesystem, Docker/compose, nginx, firewall, or provider mutation.

## Contract and invariants

- **Public seams changed:** `POST /api/settings/ai-url`; production-source compatibility-bypass inventory in `backend/src/operations/registry.rs`.
- **Canonical invariants preserved:**
  - Compatibility routes remain ingress adapters and do not execute provider or destructive mutations; real-router tests and the registry inventory test pass.
  - Authentication is evaluated before the unavailable boundary; unauthenticated requests return `401 unauthorized`, while authenticated requests return the exact `feature_unavailable` envelope.
  - No canonical resource identity, schema migration, durable operation, approval, job, event, or inventory ownership was changed.
  - AI remains optional; the closed path does not require an AI provider or external cloud service.
  - Test payloads and evidence contain no credentials or secret values; the added-line static security scan was clean.
- **Explicit non-goals preserved:** canonical AI-proxy action/plan/policy/approval/job adapter; notification webhook convergence; AI secret-manager migration; frontend changes; provider runtime qualification; release qualification.

## Files and commit scope

- **Committed files:**
  - `backend/src/api/proxy.rs`
  - `backend/src/api/settings.rs`
  - `backend/src/operations/registry.rs`
- **Preserved unrelated staged files:**
  - `backend/src/agent/mod.rs`
  - `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** none.

## Verification evidence

- **Delivery shape:** two ordered checkpoints, `ai-url-public-boundary` and `ai-url-bypass-inventory`, share one route, one compatibility trust boundary, one stable fail-closed contract, and one rollback commit; notification and secret-manager paths remain separate slices.
- **Checkpoint results:**
  - `ai-url-public-boundary`: 3 focused settings tests passed, including authenticated no-persistence, unauthenticated auth ordering, and source-level no-direct-mutation checks — `integration-verified`.
  - `ai-url-bypass-inventory`: `deferred_direct_execution_inventory_is_exact` passed with 1 test — `unit-verified`.
- **Automation manifest/report:**
  - Manifest: `docs/internal/evidence/2026-09-11-m1-04-settings-ai-proxy-closure/batch.json`
  - Post-commit report: `docs/internal/evidence/2026-09-11-m1-04-settings-ai-proxy-closure/post-commit-report/evidence.json`
  - Source-truth output: `docs/internal/evidence/2026-09-11-m1-04-settings-ai-proxy-closure/source-truth.json`
  - Independent reviewer transcript: `/home/elwla/.hermes/profiles/voidtower-dev/cache/delegation/live/deleg_49122e9a/task-0.log`

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `python scripts/repo_truth.py --repo . --json --check` | 0 | `check.status=passed`; branch `dev`; ahead 89/behind 0 at pre-commit capture; 371 route metadata, 67 structured actions, 41 standalone MCP servers, 55 App Vault YAML entries | `unit-verified` |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test api::settings::tests --all-features -- --nocapture` | 0 | 3 passed, 0 failed | `integration-verified` |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test deferred_direct_execution_inventory_is_exact --all-features -- --nocapture` | 0 | 1 passed, 0 failed | `unit-verified` |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test --all-targets --all-features` | 0 | 504 passed, 0 failed | `integration-verified` |
| `docker run --rm -v "$PWD:/workspace" -w /workspace rust:latest sh -c 'rustup component add clippy >/dev/null && cargo clippy --manifest-path backend/Cargo.toml --all-targets --all-features -- -D warnings'` | 0 | `Finished dev profile`; Clippy warnings denied and none emitted | `integration-verified` |
| `scripts/check-schema-migration-ownership.sh` | 0 | `Schema migration ownership check passed.` | `unit-verified` |
| `git diff --cached --check` | 0 | no whitespace errors before code commit | `implemented` |
| `git diff --check` | 0 | no whitespace errors in post-commit worktree | `implemented` |
| Added-line static security scan over the three slice diffs | 0 | `STATIC_SCAN=clean` | `unit-verified` |
| `rustfmt --edition 2021 --check backend/src/api/proxy.rs backend/src/api/settings.rs backend/src/operations/registry.rs` in `rust:latest` | 1 | formatter reports pre-existing formatting drift in touched legacy files; no formatting changes applied | `blocked` |

- **Independent review:** fresh reviewer inspected only the three intended unstaged diffs. Verdict `passed=true`, with empty `security_concerns` and `logic_errors`. Non-blocking suggestions: add stronger external side-effect assertions in a future runtime-capable slice and prefer AST-aware inventory checks over substring checks in future hardening.
- **Security/redaction review:** added-line scan found no hardcoded secret, shell-injection, eval/exec, unsafe-deserialization, or obvious SQL-formatting matches; no credential values were persisted in source or evidence.
- **Not run:** native host Cargo was unavailable (`cargo` not found); Dockerized Rust commands were used. Whole-workspace formatting is not qualified because the repository has pre-existing rustfmt drift; the scoped formatter check also reports legacy drift. No live provider, Docker/App Vault, browser, or release-artifact runtime was claimed for this slice.

## Failures and limitations

- **Blocking failures:** none for the bounded slice.
- **Known limitations:** AI proxy settings remain intentionally unavailable through this compatibility endpoint until the canonical adapter is delivered. Formatter cleanliness is not established because existing formatting drift spans touched legacy files. External host side effects are proven absent by handler behavior and database assertions, not by a live firewall/nginx filesystem runtime harness.
- **Recovery/rollback:** retain the closure until the canonical adapter is ready. To revert only this code slice, run `git revert 7b4859b8c6c7d43e132e9a68d79eac1af54aa981`; this preserves the unrelated staged agent changes. Do not reset or clean the worktree.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` / `7b4859b8c6c7d43e132e9a68d79eac1af54aa981` / ahead 90 / behind 0.
- **Staged paths:** `backend/src/agent/mod.rs`, `backend/src/agent/state.rs`.
- **Modified paths:** none at the code-commit boundary.
- **Remote publication:** not pushed.

## Next bounded slice

- **Next slice:** M1-04 remaining compatibility inventory — classify and close the `backend/src/api/settings.rs` notification webhook test/mutation path.
- **Acceptance seam:** real Axum-router tests for the notification test endpoint and settings mutation must prove the intended synchronous exception is explicitly typed/read-only or is routed through canonical operation planning, with authentication and no direct provider mutation.
- **Non-goals:** AI secret-manager migration (S2-01/S2-02), canonical AI proxy adapter design, frontend notification UX, and live webhook delivery qualification.
- **Blockers:** none for inventory/design; canonical operation semantics must be selected before implementing any notification mutation.

## Reproduction rule

A fresh agent can read `docs/development-plan.md`, this handoff, and the tracked code commit; run `python scripts/repo_truth.py --repo . --json --check`; execute the commands in `batch.json`; and reach the same evidence classification without relying on this conversation.
