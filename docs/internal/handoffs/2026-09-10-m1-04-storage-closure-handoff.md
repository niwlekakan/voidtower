# VoidTower Slice Handoff

- **Date:** 2026-09-10
- **Status:** integration-verified
- **Tracked plan slice:** `M1-04 Compatibility bypass closure` — `docs/development-plan.md` line 418. This bounded checkpoint closes the local storage compatibility mutation paths until canonical operation adapters exist.
- **Branch and commit:** `dev`, `a874de010ce8d3a1d557f69aec53983187b82255`, `[verified] Close storage compatibility mutation bypasses` (handoff documentation commit follows)

## Outcome

The seven legacy local/destructive storage mutation routes now fail closed after admin authentication instead of invoking host commands or writing `/etc/fstab`. Authenticated callers receive the stable `FeatureUnavailable` contract; unauthenticated callers remain unauthorized. Read-only storage inspection and configurable storage-path persistence are unchanged.

## Contract and invariants

- **Public seams changed:** `POST /api/storage/mount`; `POST /api/storage/umount`; `POST /api/storage/fstab`; `DELETE /api/storage/fstab/:idx`; `POST /api/storage/raid/create`; `POST /api/storage/raid/stop`; `POST /api/storage/format`.
- **Canonical invariants preserved:**
  - No changed storage mutation handler constructs `Command`, calls `run_privileged`, writes `/etc/fstab`, or invokes a provider/destructive host command.
  - Every closed mutation handler authenticates with the existing owner/admin session boundary before returning `FeatureUnavailable`.
  - The real Axum router serializes the authenticated unavailable result as HTTP `503`, code `feature_unavailable`, with the stable message `local storage mutations require a canonical operation adapter`.
  - The real Axum router rejects an unauthenticated storage mutation with HTTP `401`, code `unauthorized`.
  - No canonical resource identity, CMDB projection, plan, approval, durable job, event, schema, node-auth, or secret contract was changed; no provider mutation was attempted.
  - Read-only storage handlers and storage-path settings persistence remain present and compiled; VoidTower remains useful without storage providers.
- **Explicit non-goals preserved:** no canonical storage operation adapter, immutable plan/job/approval workflow, provider execution, live disk/mount/RAID/format operation, frontend/MCP expansion, broad mutation inventory rewrite, runtime provider qualification, or release qualification.

## Files and commit scope

- **Committed files:** `backend/src/api/storage.rs`; `docs/internal/evidence/2026-09-10-m1-04-storage-closure/batch.json`; `docs/internal/evidence/2026-09-10-m1-04-storage-closure/final-report/evidence.json`; this handoff.
- **Preserved unrelated staged files:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`.
- **Preserved unrelated modified files:** none.

## Verification evidence

- **Delivery shape:** one M1-04 compatibility-bypass batch with checkpoints `contract-red`, `fail-closed-implementation`, `focused-regression`, and `final-gates`. They share the storage mutation trust boundary, one rollback boundary, and one contract result: no direct provider execution from these compatibility routes.
- **Checkpoint results:**
  - `contract-red`: **blocked as reproducible RED evidence** — the focused test was added during the implementation sequence, but a disposable pre-slice RED run was not preserved as a valid result. The final public seam is green below.
  - `fail-closed-implementation`: **implemented** — `backend/src/api/storage.rs` removes the local `Command`/privileged helper and closes all seven mutation handlers after auth.
  - `focused-regression`: **integration-verified** — real-router authenticated/unauthenticated tests, direct handler fail-closed test, and source execution-path guard pass.
  - `final-gates`: **integration-verified** — source truth, full backend tests, schema ownership, diff checks, and independently installed Clippy pass; direct batch Clippy/Rustfmt probes remain environment/formatting-blocked as recorded below.
- **Automation manifest/report:** `docs/internal/evidence/2026-09-10-m1-04-storage-closure/batch.json`; `docs/internal/evidence/2026-09-10-m1-04-storage-closure/final-report/evidence.json`.

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `python scripts/repo_truth.py --repo . --json --check` | 0 | Check passed; post-commit report records branch `dev`, HEAD `a874de010ce8d3a1d557f69aec53983187b82255`, source inventory only | implemented |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test api::storage::tests --all-features -- --nocapture` | 0 | 4 passed, 0 failed, 0 ignored; 490 filtered; secondary target 0 passed | integration-verified |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test --all-targets --all-features` | 0 | Exit 0; stdout began `running 494 tests`; all backend test targets completed successfully, including the golden-path target | integration-verified |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo clippy --all-targets --all-features -- -D warnings` | 1 | Base `rust:latest` toolchain `1.98.0` lacks `cargo-clippy`; exact stderr says `rustup component add clippy` | blocked (batch environment) |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add clippy >/dev/null 2>&1 && cargo clippy --all-targets --all-features -- -D warnings'` | 0 | Clippy finished with no warnings/errors after installing the missing component in the disposable container | integration-verified |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest rustfmt --edition 2021 --config skip_children=true --check src/api/storage.rs` | 1 | Base toolchain lacks `rustfmt`; exact stderr says `rustup component add rustfmt` | blocked (batch environment) |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add rustfmt >/dev/null 2>&1 && rustfmt --edition 2021 --config skip_children=true --check src/api/storage.rs'` | 1 | Rustfmt installed, then reported pre-existing formatting drift in untouched/read-only and surrounding code plus test assertion formatting; no formatter was applied to avoid unrelated churn | blocked (pre-existing formatting drift) |
| `scripts/check-schema-migration-ownership.sh` | 0 | `Schema migration ownership check passed.` | unit-verified |
| `git diff --check` | 0 | No whitespace errors | implemented |
| `python /tmp/voidtower_added_scan.py` | 0 | Six added-line scan classes all reported `0`: sensitive assignment, shell injection, eval, unsafe deserialization, formatted SQL, provider execution | implemented |
| `python /home/elwla/.hermes/profiles/voidtower-dev/skills/software-development/voidtower-dev/scripts/slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-10-m1-04-storage-closure/batch.json --output docs/internal/evidence/2026-09-10-m1-04-storage-closure/final-report` | 1 | Source truth, focused tests, full backend tests, schema ownership, and diff check passed; batch Clippy and Rustfmt probes exited 1 for the exact component/formatting limitations above | integration-verified with blocked sub-gates |

- **Independent review:** completed reviewer transcript `deleg_3d58f598/task-0.log`; verdict `passed=true`, security concerns `[]`, logic errors `[]`. Non-blocking suggestions: add route-level tests for the other six mutation endpoints and read-only regressions; deferred to keep this closure bounded. Reviewer independently confirmed no `pct`/provider-style execution in the changed handlers and the stable auth/error boundary.
- **Security/redaction review:** `/tmp/voidtower_added_scan.py` over added lines exited 0 with all six finding classes empty. No credentials were read or persisted. The batch runner filters sensitive environment names and redacts captured output; evidence contains no credential values.
- **Not run:** live storage provider mutation, canonical storage adapter workflow, durable storage job/plan/approval path, runtime disk/mount/RAID/format qualification, provider uncertainty recovery, frontend/MCP qualification, and release qualification.

## Failures and limitations

- **Blocking failures:** the tracked batch’s direct `cargo-clippy` and `rustfmt` argv steps fail because `rust:latest` omits those components; one-shot Clippy installation passes. Rustfmt with installation exits 1 on existing file-wide formatting drift; this slice did not apply broad formatter churn. No source/test failure remains.
- **Known limitations:** storage mutations remain intentionally unavailable until a canonical typed operation adapter and durable plan/job path exist. The source guard is supplemental and brittle; the real-router tests cover representative mount behavior, while other closed endpoints are covered by the shared handler implementation and source guard. This checkpoint does not complete the entire M1-04 mutation inventory or establish runtime/release support. Evidence is source/integration proof, not live provider proof.
- **Recovery/rollback:** `git revert a874de010ce8d3a1d557f69aec53983187b82255` removes the storage closure and its evidence artifacts while leaving the unrelated staged agent files untouched. Re-enabling direct host mutation is not release-safe without an approved canonical adapter.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` / `a874de010ce8d3a1d557f69aec53983187b82255` for the product commit, ahead 86 / behind 0 before this handoff documentation commit.
- **Staged paths:** `backend/src/agent/mod.rs`; `backend/src/agent/state.rs`.
- **Modified paths:** none after staging the final evidence update and handoff for the documentation commit.
- **Remote publication:** not pushed.

## Next bounded slice

- **Next slice:** `M1-04 Compatibility bypass closure` — classify and close the next unclassified provider/destructive compatibility ingress identified by the current source-derived inventory.
- **Acceptance seam:** one public source-enforcement or representative real-router test for that exact call site proving no provider call occurs outside an approved adapter or typed exception.
- **Non-goals:** no broad refactor, no hand-maintained totals, no storage adapter implementation in this next closure follow-up, and no unrelated agent/CMDB/secret work.
- **Blockers:** none for the next source-classification step; canonical storage adapter and formatter/toolchain cleanup remain outside this bounded follow-up.

## Reproduction rule

From the repository root, read `docs/development-plan.md`, `ROADMAP.md`, and this handoff; run `python scripts/repo_truth.py --repo . --json --check`; execute the tracked manifest with `python /home/elwla/.hermes/profiles/voidtower-dev/skills/software-development/voidtower-dev/scripts/slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-10-m1-04-storage-closure/batch.json --output docs/internal/evidence/2026-09-10-m1-04-storage-closure/final-report`; inspect `evidence.json`; then verify `git status --short --branch` retains only the two unrelated staged agent paths after the handoff commit. Expected contract evidence is the stable 503/401 storage boundary, four focused tests, full backend test exit 0 with 494 tests, schema ownership pass, the independent review pass, and the explicitly recorded Clippy/Rustfmt limitations.
