# VoidTower Slice Handoff

- **Date:** 2026-09-10
- **Status:** integration-verified
- **Tracked plan slice:** M1-04 Compatibility bypass closure
- **Branch and commit:** `dev`, `82fd88009ea36d082578096887bf3775eed7b10d`, `[verified] close WireGuard compatibility mutations`

## Outcome

WireGuard compatibility mutation ingress now fails closed until a canonical operation adapter exists. Authenticated peer create/delete requests return the stable `feature_unavailable`/503 contract, unauthenticated requests are rejected first, node enrollment defaults to no WireGuard provisioning, explicit provisioning fails before pairing-code claim, and node deletion refuses to remove a node with an existing WireGuard peer. Read-only WireGuard inspection remains available.

## Contract and invariants

- **Public seams changed:** `POST /api/wireguard/peers`; `DELETE /api/wireguard/peers/:peer_id`; `POST /api/nodes/enroll`; `DELETE /api/nodes/:id`.
- **Canonical invariants preserved:**
  - Provider/destructive mutation does not execute from the compatibility module: mutation key generation, config writes, and `wg set`/`wg remove` paths were removed; a source-enforcement test asserts the production section contains none of those write/mutation markers.
  - Auth runs before the feature boundary: authenticated peer routes return 503/`feature_unavailable`; unauthenticated POST and DELETE return 401/`unauthorized`.
  - Pairing-code state is not consumed when explicit WireGuard provisioning is unavailable; the enrollment test verifies `used_at IS NULL` and zero node rows.
  - Node deletion with a recorded WireGuard peer returns the same unavailable contract and preserves the node row.
  - No schema or migration changed; canonical resource/CMDB, audit/event, managed-node outbound, and secret invariants are unaffected.
- **Explicit non-goals preserved:** no canonical WireGuard action/plan/policy/approval/job adapter; no live WireGuard mutation; no frontend/MCP expansion; no runtime or release-support claim; no changes to unrelated agent work.

## Files and commit scope

- **Committed files:**
  - `backend/src/api/node_enroll.rs`
  - `backend/src/api/wireguard.rs`
  - `docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/batch.json`
  - `docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/final-report/evidence.json`
- **Preserved unrelated staged files:**
  - `backend/src/agent/mod.rs`
  - `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** none

## Verification evidence

- **Delivery shape:** one M1-04 compatibility closure with ordered checkpoints: (1) public mutation routes fail closed, (2) enrollment defaults/offers safe unavailable behavior, (3) node revocation preserves state, (4) source and full gates. The checkpoints share the WireGuard provider trust boundary and one rollback commit.
- **Checkpoint results:**
  - `wireguard-public-routes`: 5 tests passed (authenticated POST/DELETE unavailable, unauthenticated POST/DELETE unauthorized, no mutation write path); `unit-verified`.
  - `node-enrollment-and-revocation`: 4 tests passed (default no provisioning, explicit provisioning pre-claim failure, no-wireguard enrollment, peer-preserving deletion); `integration-verified`.
  - `full-backend-regression`: 500 unit tests and 2 integration tests passed; `integration-verified`.
- **Automation manifest/report:** `docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/batch.json` and `docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/final-report/evidence.json`. The final report records all six manifest steps with exit code 0 and unchanged staged paths.

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `python scripts/repo_truth.py --repo . --json --check` | 0 | Source report passed; branch `dev`, HEAD before commit `81c8ea8dfa7364fe3fcbd5af1c0de97a381c0dc8`; changed source paths were the two batch files; no migration changes. | implemented |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test api::wireguard::tests --all-features -- --nocapture` | 0 | 5 tests passed, 0 failed. | unit-verified |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test api::node_enroll::tests --all-features -- --nocapture` | 0 | 4 tests passed, 0 failed. | integration-verified |
| `docker run --rm -v .:/workspace -w /workspace/backend rust:latest cargo test --all-targets --all-features` | 0 | 500 unit tests passed; 2 `golden_path` integration tests passed; 0 failed. | integration-verified |
| `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add rustfmt clippy >/dev/null && rustfmt --edition 2021 src/api/node_enroll.rs src/api/wireguard.rs && cargo clippy --all-targets --all-features -- -D warnings'` | 0 | Scoped rustfmt and strict Clippy passed. | unit-verified |
| `scripts/check-schema-migration-ownership.sh` | 0 | `Schema migration ownership check passed.` | unit-verified |
| `git diff --check` | 0 | No whitespace errors. | implemented |
| `python .../slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/batch.json --output docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/final-report` | 0 | All six manifest steps passed and report was written. | integration-verified |

- **Independent review:** reviewer inspected only the two source-file diffs; verdict `passed: true`, `security_concerns: []`, `logic_errors: []`. Non-blocking suggestions for unauthenticated DELETE coverage and exact error assertions were incorporated. The broader transaction suggestion concerned pre-existing post-claim enrollment failures and was outside this slice; this slice proves the unavailable provisioning path fails before claim.
- **Security/redaction review:** added-line static scan found no hardcoded secret assignments, shell-injection patterns, eval/exec, unsafe deserialization, or interpolated SQL. No credentials were read or persisted.
- **Not run:** whole-workspace `cargo fmt -- --check` is not promoted: it exits 1 on extensive pre-existing formatting drift in unrelated files (`backend/src/ai`, operations, terminal, storage, and tests). The changed files themselves passed scoped rustfmt. No live provider/runtime or release gate was run.

## Failures and limitations

- **Blocking failures:** none for this bounded slice. The first batch attempt was rejected by an incorrect Docker cwd/mount combination (exit 101 before tests); the manifest was corrected and the final batch passed all six steps.
- **Known limitations:** WireGuard mutation is intentionally unavailable until a typed canonical adapter is implemented. No live `wg` command or external runtime was exercised. Whole-workspace formatting remains pre-existing drift and is not release-qualified.
- **Recovery/rollback:** `git revert 82fd88009ea36d082578096887bf3775eed7b10d` removes this slice without touching the preserved staged agent files. With this slice retained, failed/unavailable WireGuard requests make no provider or node state mutation; explicit enrollment leaves the pairing code unclaimed, and peer-backed node deletion leaves the node intact.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` / `82fd88009ea36d082578096887bf3775eed7b10d` / ahead 88 / behind 0.
- **Staged paths:** `backend/src/agent/mod.rs`, `backend/src/agent/state.rs`.
- **Modified paths:** none.
- **Remote publication:** not pushed.

## Next bounded slice

- **Next slice:** M1-04 remaining compatibility inventory — classify and close the `backend/src/api/settings.rs` provider/destructive mutation paths.
- **Acceptance seam:** add a real-router test for the first selected settings mutation proving authentication, typed canonical boundary or stable unavailable response, and no direct provider/file mutation from the compatibility handler.
- **Non-goals:** no WireGuard adapter work in the next slice, no broad settings refactor, no AI secret-manager migration, and no frontend/runtime qualification.
- **Blockers:** none identified; select the exact settings call site from a fresh source-truth inventory before editing.

## Reproduction rule

A fresh agent can read `docs/development-plan.md`, run `python scripts/repo_truth.py --repo . --json --check`, execute `docs/internal/evidence/2026-09-10-m1-04-wireguard-closure/batch.json` through `scripts/slice_batch.py`, and reach the same evidence classification without relying on this conversation.
