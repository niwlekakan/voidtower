# M1-04 parser-resolution final blocked handoff

Date: 2026-09-21T02:19:09Z
Base: branch `dev`, HEAD `b9a24729c2a7750900f285d61daa4439e0cd95f9`; no commit created because independent review remains blocking. Candidate paths are staged; unrelated staged/unstaged and untracked paths were preserved.

Active slice

Parser-backed source inventory for provider/destructive compatibility callsites under `backend/src/api`. The candidate hardens bounded Rust parsing and inventory enforcement against unsafe imports, call shapes, helper provenance, deferred-error scope, and canonical delegation spoofing.

Implemented in the current worktree

- `scripts/rust_source_parser.py`: rustfmt-backed bounded lexer/parser; cfg(test) exclusion; import/module/impl extraction; explicit rejection of unsafe re-exports and wildcard imports; fail-closed UFCS/qualified filesystem and IO mutators; allowlisted resolved provenance for bare/module aliases; same-module canonical helper propagation; parameter/local shadow tracking; local canonical-module rejection; nested-function deferred-error rejection.
- `scripts/compatibility_mutation_inventory.py`: production inventory and credential-safe classification/check output.
- `scripts/test_compatibility_mutation_inventory.py`: 52 adversarial parser/inventory tests covering aliases, wildcard imports, raw/Unicode identifiers, qualified calls, helper shadowing, local-module spoofing, nested exceptions, cfg scopes, macros, evidence, symlinks, and deterministic redacted output.
- `.github/workflows/ci.yml`, `docs/api.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md`: CI enforcement, public developer contract, knowledge, and retrospective/backlog updates.

Verification evidence after the latest source change

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v` — passed 70 tests (52 compatibility inventory, 18 repository truth).
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check` — passed with `status=passed classified=146 unknown=0`.
- `python3 -m py_compile scripts/rust_source_parser.py scripts/compatibility_mutation_inventory.py scripts/test_compatibility_mutation_inventory.py` — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed on branch `dev`, HEAD `b9a24729c2a7750900f285d61daa4439e0cd95f9`.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `bash scripts/check-repository-hygiene.sh` — passed (662 tracked files checked).
- `cd backend && cargo fmt --all -- --check` — passed.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` — passed 693 unit tests, 2 integration tests, and examples (final run after the candidate's final source changes; backend source was unchanged by this slice).
- `git diff --check && git diff --cached --check` — passed.

Independent review result

Blocked. Three final adversarial gaps remain in the bounded token resolver:

1. Prefix-based provenance allowlists are too broad. Aliases resolving under `crate::operations::` or `crate::networking::` can still bypass mutation detection when they target arbitrary names; provenance must use exact canonical targets or an explicit, reviewed non-mutating allowlist rather than module prefixes.
2. Canonical imported helper names remain vulnerable to lexical shadowing. A canonical import shadowed by a parameter or local binding can still synthesize canonical proof.
3. Canonical calls inside nested functions/scopes can contaminate the containing function's call set and authorize an enclosing direct mutation.

These are trust-boundary findings, not missing test-count issues. The independent reviewer confirmed staged/worktree convergence, but rejected commit readiness. The checkout-local SHA-256 exception manifest remains edit detection only, not an independently trusted approval ledger.

Current maturity and limitations

- `unit-verified` for the covered parser/inventory fixtures and source check; `implemented` for CI wiring.
- `blocked` for the M1-04 acceptance pending exact canonical target provenance and lexical/item-scope resolution or explicit fail-closed rejection of all unresolved canonical forms.
- No provider, host, browser, Docker, runtime, installation, upgrade/recovery, or release qualification was performed. Do not promote the source inventory or classified total to runtime/release evidence.

Preserved unrelated work

- Existing unrelated staged paths, including pre-existing staged backend work, were not reset, unstaged, cleaned, or committed.
- Untracked `testing/`, `scripts/__pycache__/`, and `odysseus-mcp-servers/tests/__pycache__/` were preserved.
- No credentials, provider tokens, supervisor configuration, installed skills, cron, services, main branch, remote, or unrelated backend product paths were changed.

Retrospective

- Learned: a bounded parser must treat canonical delegation as a scoped provenance proof, not a textual function-name or broad module-prefix match. Nested Rust items and imported bindings require compiler-grade scope handling or explicit rejection.
- Verified: 70 focused Python tests, inventory 146/0, repository truth, schema ownership, hygiene, Python compilation, rustfmt, strict Clippy, and final full backend tests passed.
- Remaining blocker: remove broad provenance prefixes and reject or structurally resolve shadowed/nested canonical calls; obtain a fresh independent review on the exact staged candidate before any commit.
- Reusable commands: the focused 70-test command, inventory `--check`, repository truth `--json --check`, schema/hygiene checks, `cargo fmt -- --check`, strict Clippy, and `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features`.
- Documentation changed: `docs/api.md` documents the bounded fail-closed contract and explicit limitations; `docs/internal/agent-knowledge/` records the system map and future documentation requirements.

Next dependency-ready slice

Converge canonical resolution only: replace broad module-prefix trust with exact resolved adapter identities, make canonical proof lexical-scope/item aware, add reviewer-reproduced fixtures for trusted-prefix aliases, imported-helper shadowing, nested canonical calls, and direct-mutation-before-nested-delegation, then rerun all final gates and independent review. Do not advance to runtime qualification, V6-01, or unrelated adapter migration.
