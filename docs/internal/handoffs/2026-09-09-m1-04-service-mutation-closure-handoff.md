# VoidTower Slice Handoff

- **Date:** 2026-09-09
- **Status:** integration-verified
- **Tracked plan slice:** `M1-04 Compatibility bypass closure` — `docs/development-plan.md` line 418. This bounded follow-up closes the remaining service compatibility mutation paths after the prior Libvirt closure.
- **Branch and commit:** `dev`, `4f17a4c302fff16112bd6dda00969f1d856f3cc9`, `[verified] Close service mutation compatibility bypasses`

## Outcome

`POST /api/services/:name/action` and the structured service-action branch of `POST /api/integrations/webhooks` no longer execute the legacy systemd provider. Authenticated requests fail closed with the stable typed `AppError::FeatureUnavailable` response until a canonical service operation adapter exists. The obsolete `MaybeTokenActor` extractor and unreachable `run_service_action` provider helper were removed.

## Contract and invariants

- **Public seams changed:** `POST /api/services/:name/action`; `POST /api/integrations/webhooks` for `service.start`, `service.stop`, and `service.restart`.
- **Canonical invariants preserved:**
  - Provider/destructive service mutations do not execute inside compatibility ingress handlers; both paths fail closed before provider execution.
  - The service API authenticates the session and enforces the operator role before returning the unavailable result.
  - Webhook authentication and policy evaluation remain before the deferred service boundary; an explicit allow does not bypass the missing canonical adapter.
  - No resource identity, CMDB identity, durable event, schema, or secret contract changed.
  - The webhook regression fixture verifies the allowed-policy case reaches the intended unavailable boundary rather than an environment-dependent systemd error.
- **Explicit non-goals preserved:** no service operation adapter, durable service plan/job implementation, systemd runtime qualification, UI/MCP/scheduler expansion, or broad mutation inventory rewrite.

## Files and commit scope

- **Committed files:** `backend/src/api/integrations.rs`; `backend/src/api/services.rs`; `backend/src/policy.rs`; `backend/src/services/mod.rs`; `backend/src/voidwatch/allowlist_seed.rs`; `docs/internal/evidence/2026-09-09-m1-04-service-mutation-closure/batch.json`; `docs/internal/evidence/2026-09-09-m1-04-service-mutation-closure/final-report/evidence.json`
- **Preserved unrelated staged files:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** none

## Verification evidence

- **Delivery shape:** one M1-04 compatibility-bypass batch with checkpoints `contract-red`, `fail-closed-implementation`, `focused-regression`, and `final-gates`. They share the service mutation trust boundary, one rollback boundary, and one acceptance result: no legacy service provider execution from either compatibility ingress.
- **Checkpoint results:**
  - `contract-red`: **unit-verified** — with an explicit allow rule, the pre-change webhook test reached the legacy helper and returned the environment-dependent `systemd is not available on this system` error, proving the test exercised the old provider path.
  - `fail-closed-implementation`: **implemented** — service API and webhook now return `FeatureUnavailable`; `run_service_action` was removed; source search found no `run_service_action` reference or service mutation `systemctl` call in the affected path.
  - `focused-regression`: **integration-verified** — service module: 1 passed; integration module: 6 passed; the new service webhook and service API tests both passed.
  - `final-gates`: **integration-verified** — batch runner report passed all six recorded steps; full backend targets passed 486 tests plus 2 golden-path tests; clippy passed with `-D warnings`; schema ownership passed.
- **Automation manifest/report:** `docs/internal/evidence/2026-09-09-m1-04-service-mutation-closure/batch.json`; `docs/internal/evidence/2026-09-09-m1-04-service-mutation-closure/final-report/evidence.json`.

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test api::integrations::tests::service_webhook_mutation_fails_closed_until_canonical_adapter_exists --all-features -- --nocapture` (pre-change RED) | 101 | Test reached legacy service helper and failed on `systemd is not available on this system` | unit-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test api::services::tests --all-features -- --nocapture` | 0 | 1 passed, 0 failed | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test api::integrations::tests --all-features -- --nocapture` | 0 | 6 passed, 0 failed | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test --all-targets --all-features` | 0 | 486 unit tests passed; 2 golden-path tests passed; example target had 0 tests | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add clippy >/dev/null 2>&1 && cargo clippy --all-targets --all-features -- -D warnings'` | 0 | Clippy finished with no warnings/errors | integration-verified |
| `scripts/check-schema-migration-ownership.sh` | 0 | Schema migration ownership check passed | implemented |
| `git diff HEAD^ HEAD --check` | 0 | No whitespace errors | implemented |
| `python scripts/repo_truth.py --repo . --json --check` | 0 | Passed; two post-commit runs had identical output | implemented |
| `python /tmp/voidtower_added_scan.py` over the staged slice diff | 0 | Empty findings for hardcoded-secret assignments, shell injection, eval/exec, unsafe pickle, and formatted SQL | implemented |
| `python /home/elwla/.hermes/profiles/voidtower-dev/skills/software-development/voidtower-dev/scripts/slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-09-m1-04-service-mutation-closure/batch.json --output docs/internal/evidence/2026-09-09-m1-04-service-mutation-closure/final-report` | 0 | Source truth, focused services, focused integrations, full backend tests, schema ownership, and diff check all passed | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add rustfmt >/dev/null 2>&1 && cargo fmt --all -- --check'` | 1 | Repository-wide rustfmt reported pre-existing drift in unrelated `backend/src/ai/*` files; no slice source was modified by the check | blocked |

- **Independent review:** reviewer `sa-0-43c03ead` returned `passed=true`; security concerns `[]`; logic errors `[]`. The reviewer confirmed both legacy service mutation paths fail closed before systemd/provider execution and that unrelated staged paths were preserved. Non-blocking suggestions: add router-level 503 assertions, annotate that the evidence report captures the pre-commit run at `0dc2be21`, and reconsider advertising `services:restart` while the adapter is unavailable. These do not change the current fail-closed acceptance result.
- **Security/redaction review:** `/tmp/voidtower_added_scan.py` over the staged slice diff exited 0 with all five finding classes empty. No credentials were read or persisted.
- **Not run:** live systemd service mutation, canonical service adapter workflow, external-provider runtime qualification, and release qualification. Repository-wide formatting remains blocked by unrelated pre-existing drift.

## Failures and limitations

- **Blocking failures:** `cargo fmt --all -- --check` exits 1 because the repository already contains rustfmt drift in unrelated AI files; this slice does not claim formatting or release qualification.
- **Known limitations:** service mutations remain unavailable by design until a canonical typed service adapter and durable operation path are implemented. No live systemd execution was exercised. The source-truth report is inventory evidence, not runtime proof.
- **Recovery/rollback:** `git revert 4f17a4c302fff16112bd6dda00969f1d856f3cc9` removes this service-closure batch and evidence while leaving the unrelated staged agent files untouched. Re-enabling the removed direct provider path is not release-safe without an approved canonical adapter.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` / `4f17a4c302fff16112bd6dda00969f1d856f3cc9` / ahead 80 / behind 0
- **Staged paths:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`
- **Modified paths:** none
- **Remote publication:** not pushed

## Next bounded slice

- **Next slice:** `M1-04 Compatibility bypass closure` — classify and close the next unclassified provider/destructive compatibility ingress identified by the source-derived exception ledger.
- **Acceptance seam:** one public source-enforcement or representative real-router test for that exact remaining mutation call site, proving no provider call occurs outside an approved adapter or typed exception.
- **Non-goals:** no broad refactor, no hand-maintained totals, no service adapter in this follow-up, and no unrelated agent/CMDB/secret work.
- **Blockers:** none for this next source-classification step; canonical service adapter design remains outside this batch.

## Reproduction rule

From the repository root, read `docs/development-plan.md` and this handoff, run `python scripts/repo_truth.py --repo . --json --check`, execute the commands in the verification table, inspect the committed batch report, and verify `git status --short --branch` shows only `M  backend/src/agent/mod.rs` and `M  backend/src/agent/state.rs`. The expected evidence is the stable `FeatureUnavailable` service boundary, 1 focused service test, 6 focused integration tests, 486 full backend unit tests, 2 golden-path tests, clippy success, schema ownership success, and the explicitly recorded unrelated formatting blocker.
