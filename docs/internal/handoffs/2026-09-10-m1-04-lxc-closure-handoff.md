# VoidTower Slice Handoff

- **Date:** 2026-09-10
- **Status:** integration-verified
- **Tracked plan slice:** `M1-04 Compatibility bypass closure` — `docs/development-plan.md` line 418. This bounded follow-up closes the local LXC compatibility mutation path until a canonical operation adapter exists.
- **Branch and commit:** `dev`, `28a71b8bf5928e118c90a6bfd6cb88e381b0bcf4`, `[verified] Close LXC compatibility mutation bypass`

## Outcome

`POST /api/lxc/:vmid/action` no longer invokes `pct` or another provider directly. After admin authentication, the route returns the stable typed `FeatureUnavailable` boundary with the message `local LXC mutations require a canonical operation adapter` until the canonical operation adapter is implemented.

## Contract and invariants

- **Public seams changed:** `POST /api/lxc/:vmid/action`.
- **Canonical invariants preserved:**
  - The compatibility mutation route does not execute a provider or destructive host command; the handler source contains no `pct` command construction or `process::Command` execution path.
  - Session authentication remains first; an unauthenticated request receives `401 unauthorized` rather than the feature-unavailable result.
  - The authenticated public router serializes the unavailable result as HTTP `503` with error code `feature_unavailable` and the stable message above.
  - No canonical resource identity, CMDB projection, durable job, plan, approval, event, schema, node-auth, or secret contract was changed.
  - The route fails closed while VoidTower remains usable without a local LXC provider.
- **Explicit non-goals preserved:** no LXC operation adapter, typed plan/job/approval workflow, provider execution, host `pct` invocation, VM/LXC UI or MCP expansion, runtime LXC qualification, or broad mutation-inventory rewrite.

## Files and commit scope

- **Committed files:** `backend/src/api/lxc.rs`; `docs/internal/evidence/2026-09-10-m1-04-lxc-closure/batch.json`; `docs/internal/evidence/2026-09-10-m1-04-lxc-closure/final-report/evidence.json`; `docs/internal/handoffs/2026-09-10-m1-04-lxc-closure-handoff.md`
- **Preserved unrelated staged files:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** none

## Verification evidence

- **Delivery shape:** one M1-04 compatibility-bypass batch with checkpoints `contract-red`, `fail-closed-implementation`, `focused-regression`, and `final-gates`. They share the LXC mutation trust boundary, one rollback boundary, and one contract result: no direct provider execution from this compatibility ingress.
- **Checkpoint results:**
  - `contract-red`: **blocked as reproducible TDD evidence** — the finalized public tests were prepared, but the disposable pre-slice worktree command was stopped by the environment’s destructive-action approval gate while creating/removing `/tmp/voidtower-lxc-red`. An earlier focused run exited `101` during a malformed intermediate edit and is not promoted as intended RED evidence.
  - `fail-closed-implementation`: **implemented** — `backend/src/api/lxc.rs` removes the direct `pct` execution path and returns `AppError::FeatureUnavailable` after admin authentication.
  - `focused-regression`: **integration-verified** — the real Axum router test, authentication-negative test, direct handler test, and source execution-path guard all pass.
  - `final-gates`: **integration-verified** — the batch report passes all five recorded steps; full backend tests and Clippy also pass. Stack readiness is runtime-verified with the repository’s host-facing port override; authenticated LXC runtime mutation remains intentionally unexercised.
- **Automation manifest/report:** `docs/internal/evidence/2026-09-10-m1-04-lxc-closure/batch.json`; `docs/internal/evidence/2026-09-10-m1-04-lxc-closure/final-report/evidence.json`. The report was generated before the source commit and records the exact pre-commit staged state; the committed source is the same tested content.

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `docker run --rm -v /home/elwla/Documents/voidtower_project_files_full/hive/voidtower:/workspace -w /workspace/backend rust:latest cargo test api::lxc::tests --all-features -- --nocapture` (final) | 0 | 4 passed, 0 failed; 486 filtered out; secondary target 0 passed | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test --all-targets --all-features` | 0 | 490 backend tests passed; 2 golden-path tests passed; example target had 0 tests | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add clippy >/dev/null 2>&1 && cargo clippy --all-targets --all-features -- -D warnings'` | 0 | Clippy finished with no warnings/errors | integration-verified |
| `python scripts/repo_truth.py --repo . --json --check` | 0 | Source-truth check passed; generated report is recorded in the batch evidence | implemented |
| `scripts/check-schema-migration-ownership.sh` | 0 | `Schema migration ownership check passed.` | implemented |
| `git diff --check` | 0 | No whitespace errors in the batch run | implemented |
| `python /tmp/voidtower_added_scan.py` over the staged LXC diff | 0 | Empty findings for hardcoded-secret assignments, shell injection, eval/exec, unsafe pickle, and formatted SQL | implemented |
| `python /home/elwla/.hermes/profiles/voidtower-dev/skills/software-development/voidtower-dev/scripts/slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-10-m1-04-lxc-closure/batch.json --output docs/internal/evidence/2026-09-10-m1-04-lxc-closure/final-report` | 0 | Source truth, focused LXC tests, schema ownership, full backend tests, and diff check all passed | integration-verified |
| `hermes verify --json` | 1 | Compose build exited 0 with `No services to build`; readiness was false after 60.882 seconds because the probe received connection refused from `127.0.0.1:8000`; the detected service entered setup-incomplete mode and the stack stopped gracefully | blocked |
| `hermes verify --json --port 80` | 0 | Compose build exited 0; readiness returned HTTP `200` in 2.616 seconds; the stack then tore down cleanly | runtime-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add rustfmt >/dev/null 2>&1 && cargo fmt --all -- --check'` | 1 | Rustfmt was unavailable/installation failed in the container; repository-wide formatting remains unestablished and is known to contain pre-existing drift outside this slice | blocked |

- **Independent review:** reviewer `sa-0-a56531ed` returned `passed=true`; security concerns `[]`; logic errors `[]`. The reviewer confirmed the authenticated handler has no `pct`/provider execution, the stable `503` public error boundary, and unauthenticated rejection. Non-blocking suggestion: prefer behavior-focused regression coverage over the source-text guard when a canonical adapter test seam exists; no finding required remediation for this bounded fail-closed slice.
- **Security/redaction review:** `/tmp/voidtower_added_scan.py` over the staged slice diff exited 0 with all five finding classes empty. No credentials were read or persisted. Runtime output was not copied into this handoff when it contained setup credentials.
- **Not run:** live LXC/provider mutation, canonical LXC adapter workflow, provider runtime qualification, release qualification, and a successful `hermes verify` readiness path. Repository-wide formatting remains blocked by the pre-existing toolchain/drift issue.

## Failures and limitations

- **Blocking failures:** The default `hermes verify --json` invocation still probes the wrong `127.0.0.1:8000` port, but the explicit repository host-port invocation `hermes verify --json --port 80` passed. The disposable pre-slice RED reproduction was blocked by the environment approval gate; the malformed intermediate run is not treated as RED evidence. Rustfmt remains unavailable/failed independently of this slice.
- **Known limitations:** LXC mutations remain unavailable by design until a canonical typed operation adapter and durable plan/job path are implemented. The runtime stack was setup-incomplete, so no authenticated LXC route or live `pct` operation was exercised. This slice does not complete the entire M1-04 mutation inventory or establish release qualification. The source-truth and batch reports are source/check evidence, not provider runtime proof.
- **Recovery/rollback:** `git revert 28a71b8bf5928e118c90a6bfd6cb88e381b0bcf4` removes the LXC source closure and its committed evidence while leaving the unrelated staged agent files untouched. Re-enabling direct provider execution is not release-safe without an approved canonical adapter.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` / `28a71b8bf5928e118c90a6bfd6cb88e381b0bcf4` before this handoff commit / ahead 83 / behind 0
- **Staged paths:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`
- **Modified paths:** none before this handoff commit
- **Remote publication:** not pushed

## Next bounded slice

- **Next slice:** `M1-04 Compatibility bypass closure` — classify and close the next unclassified provider/destructive compatibility ingress identified by the current source-derived exception inventory.
- **Acceptance seam:** one public source-enforcement or representative real-router test for that exact call site proving no provider call occurs outside an approved adapter or typed exception.
- **Non-goals:** no broad refactor, no hand-maintained totals, no LXC/service adapter implementation in the next closure follow-up, and no unrelated agent/CMDB/secret work.
- **Blockers:** none for the next source-classification step; the canonical LXC adapter and runtime setup/readiness are outside this bounded follow-up.

## Reproduction rule

From the repository root, read `docs/development-plan.md` and this handoff, run `python scripts/repo_truth.py --repo . --json --check`, execute the commands in the verification table that do not require unavailable provider/runtime setup, inspect `docs/internal/evidence/2026-09-10-m1-04-lxc-closure/final-report/evidence.json`, and verify `git status --short --branch` shows only the two preserved staged agent paths after the handoff commit. The expected contract evidence is the stable `FeatureUnavailable` LXC boundary, four passing focused tests, 490 full backend tests, two golden-path tests, Clippy success, schema ownership success, the independent review pass, and the explicitly recorded runtime/readiness and formatting blockers.
