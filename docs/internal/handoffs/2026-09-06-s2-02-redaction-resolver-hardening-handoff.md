# VoidTower Slice Handoff

- **Date:** 2026-09-06
- **Status:** unit-verified
- **Tracked plan slice:** `S2-02 — Legacy secret closure and rotation`
- **Branch and commit:** `dev`, `804fcf45090c0c5ad5921696e3f1702525351a10`, `[verified] harden AI redaction resolver failures`

## Outcome

The shared AI redaction boundary now remains protective when secret usage bookkeeping fails: a successfully decrypted, size-valid secret is retained for redaction even if its `last_used_at` update cannot be persisted. Provider and other non-redaction resolver purposes retain fail-closed last-use behavior. Oversized secret records remain outside the bounded resolver contract and are skipped by redaction; supported create, update, and migration paths reject oversized values before storage.

## Contract and invariants

- **Public seams changed:** `backend/src/api/redact.rs::known_secret_values` and `redact_for_ai`; internal `backend/src/api/secrets.rs::resolve` purpose behavior.
- **Canonical invariants preserved:**
  - Secret plaintext remains encrypted at rest and is decrypted only inside the shared resolver.
  - The `redaction` purpose is explicit and remains purpose-validated before lookup and decryption.
  - Disabled, corrupt, missing, and oversized values are not admitted to the known-value set.
  - A valid redaction value is retained when only last-use metadata persistence fails; non-redaction consumers still fail closed on that persistence failure.
  - Resolver errors remain generic and do not expose database diagnostics or secret material.
  - Secret IDs are selected deterministically with `ORDER BY id`; redaction does not select a server/resource identity.
  - No provider mutation, CMDB identity, agent transport, schema migration, or external service behavior changed.
- **Explicit non-goals preserved:** container/proxy direct-consumer closure, new providers, schema migration, household/resource-grant authorization, secret reveal, frontend work, live provider qualification, and release qualification.

## Files and commit scope

- **Committed files:**
  - `backend/src/api/redact.rs`
  - `backend/src/api/secrets.rs`
  - `docs/internal/evidence/2026-09-06-s2-02-redaction-resolver-hardening-batch.json`
- **Preserved unrelated staged files:**
  - `backend/src/agent/mod.rs`
  - `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** `none`

## Verification evidence

- **Delivery shape:** one review-fix checkpoint under S2-02: (1) preserve valid redaction values across last-use write failure, and (2) make oversized-record handling explicit at the resolver and redaction seams. Both remain within the same purpose-scoped secret-resolution boundary and rollback story; no adjacent consumer or schema work was bundled.
- **Checkpoint results:**
  - Redaction with failed last-use recording: `unit-verified`; 1 new regression plus the existing redaction cases passed.
  - Oversized resolver/redaction boundary: `unit-verified`; resolver rejection and redaction omission tests passed.
- **Automation manifest/report:**
  - Manifest: `docs/internal/evidence/2026-09-06-s2-02-redaction-resolver-hardening-batch.json`
  - Pre-commit report: `docs/internal/evidence/2026-09-06-s2-02-redaction-resolver-hardening-pre/evidence.json/evidence.json`
  - Post-commit report: `docs/internal/evidence/2026-09-06-s2-02-redaction-resolver-hardening-final/evidence.json/evidence.json`

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `python scripts/repo_truth.py --repo . --json --check` | 0 | source report passed; two runs were byte-identical; SHA-256 `85a98e1df3594fa1adb37b4d5e8f1fea4ab848f85fdef918acc24e48f35aac2f` | implemented |
| `cargo test --all-targets --all-features known_secret_values -- --nocapture` | 0 | 3 redaction tests passed, 0 failed; other targets had 0 selected tests | unit-verified |
| `cargo test --all-targets --all-features api::secrets::tests::resolver_ -- --nocapture` | 0 | 3 resolver tests passed, 0 failed; unsupported purpose, last-use failure, and oversized rejection covered | unit-verified |
| `cargo test --all-targets --all-features --quiet` | 0 | 458 backend tests passed, 0 failed; golden-path target 2 passed, 0 failed; examples target 0 tests | unit-verified |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | completed with no warnings or errors | unit-verified |
| `rustfmt --edition 2021 --check src/api/redact.rs src/api/secrets.rs` | 0 | both changed Rust files formatted | unit-verified |
| `git diff --check` and `git diff --cached --check` | 0 | no whitespace errors | implemented |
| added-line security scan | 0 | credential assignments 0; shell injection 0; eval/exec 0; unsafe pickle 0; SQL interpolation 0 | unit-verified |
| `python .../slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-06-s2-02-redaction-resolver-hardening-batch.json --output .../pre/evidence.json` | 0 | all 6 manifest steps passed | unit-verified |
| same batch command with `.../final/evidence.json` output | 0 | all 6 post-commit manifest steps passed; report records HEAD `804fcf4...` and preserved staged paths | unit-verified |
| `hermes verify --json` | 1 | build exit 0; default readiness probe refused at `127.0.0.1:8000` while container logged the service on exposed port 80/internal 8743 | blocked for detector default only |
| `hermes verify --json --port 80` | 0 | packaged service readiness returned HTTP 200; startup and teardown completed | runtime-verified |

- **Independent review:** delegated reviewer `deleg_9bad63ca` passed with `passed: true`, empty `security_concerns` and `logic_errors`. Suggestions were non-blocking: add exact-boundary UTF-8 coverage, expand producer write-path coverage, and consider pre-decryption record-size bounds if hostile database contents are in scope.
- **Security/redaction review:** valid file-based added-line scan returned zero findings in all five categories; focused tests proved redaction survives last-use write failure and excludes oversized records.
- **Not run:** repository-wide `cargo fmt --all -- --check` is not a slice gate because it reports pre-existing formatting drift across unrelated files; no live MCP/Studio/provider redaction path, external AI provider, production-data corpus scan, or release qualification was exercised.

## Failures and limitations

- **Blocking failures:** none for the bounded source/test slice. The default Hermes detector remains blocked on its hard-coded port-8000 readiness probe; the supported exposed-port run passed.
- **Known limitations:** the redaction behavior is unit-verified; the successful Hermes run establishes packaged startup/readiness only, not live AI/MCP redaction. Oversized values are intentionally invalid at supported storage/migration boundaries and are rejected before redaction inclusion; hostile or manually corrupted database contents beyond this bound are not a release-qualified scenario. `operations/adapters/containers.rs` and `operations/adapters/proxy.rs` still contain separate direct encrypted-secret consumers.
- **Recovery/rollback:** run `git revert 804fcf45090c0c5ad5921696e3f1702525351a10` to restore the pre-hardening redaction behavior, then rerun the focused tests and batch manifest. No schema or persisted-data rollback is required. Do not reset or unstage the preserved agent files.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev`, `804fcf45090c0c5ad5921696e3f1702525351a10`, ahead 50 / behind 0 relative to `origin/dev` before this handoff-only documentation commit.
- **Staged paths:** `backend/src/agent/mod.rs`, `backend/src/agent/state.rs`
- **Modified paths:** `none`
- **Remote publication:** `not pushed`

## Next bounded slice

- **Next slice:** continue `S2-02 — Legacy secret closure and rotation` with `ContainerAdapter::redact_provider_output` direct encrypted-value closure.
- **Acceptance seam:** add a focused container-output test proving canonical secret IDs are resolved through the purpose-scoped resolver and covering usable, disabled, corrupt, oversized, and unavailable handling without exposing provider diagnostics.
- **Non-goals:** no proxy-adapter closure in the same slice, no new provider, no broad migration rewrite, no frontend, no household grants, no secret reveal, and no runtime/release claim without a named live dependency.
- **Blockers:** none for the next source/test checkpoint; live provider qualification remains outside this dependency.

## Reproduction rule

From the repository root, inspect commit `804fcf45090c0c5ad5921696e3f1702525351a10`, confirm the two preserved staged agent paths, run the manifest with `scripts/slice_batch.py`, run `hermes verify --json --port 80`, and compare the two source-truth outputs. The exact commands and report paths above reproduce this handoff without relying on this conversation.
