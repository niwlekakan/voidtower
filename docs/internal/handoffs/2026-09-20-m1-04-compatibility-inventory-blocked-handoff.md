# M1-04 compatibility mutation inventory — blocked handoff (2026-09-20)

Status: blocked; no commit created.

Active slice

The session implemented a source-boundary inventory for provider/destructive compatibility callsites under `backend/src/api`, including request/provider/filesystem/process markers, exact module-qualified classifications, deferred exception checks, CI enforcement, API documentation, and agent-knowledge updates. The intended outcome was continuous detection of new compatibility bypasses without weakening the canonical operation-adoption boundary.

Implemented in the worktree

- `scripts/compatibility_mutation_inventory.py` scans production API Rust functions, masks comments/strings/raw strings and test modules, rejects source symlinks, emits credential-safe metadata, and fails the real checkout check with zero unknown findings.
- `scripts/test_compatibility_mutation_inventory.py` contains 13 focused fixtures covering unknown/private/comment-spoofed calls, canonical/deferred ordering, conditional deferred errors, exact nested module identity, raw strings, cfg(test) handling, marker variants, source/API-root symlinks, deterministic output, and redaction.
- `.github/workflows/ci.yml` invokes repository truth, compatibility fixture tests, and the compatibility check with `PYTHONDONTWRITEBYTECODE=1`.
- `scripts/test_repo_truth.py` matches the CI command contract.
- `docs/api.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md` record the source-only evidence boundary and future adapter/documentation gaps.

Verification evidence

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`: passed, 31 tests.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`: passed; classified findings are source-derived and unknown findings are zero.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`: passed.
- `bash scripts/check-schema-migration-ownership.sh`: passed.
- `bash scripts/check-repository-hygiene.sh`: passed.
- `git diff --cached --check`: passed.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`: passed in the earlier final-state run.
- Frontend `npm test -- --passWithNoTests`, `npm run type-check`, `npm run lint`, and `npm run build`: passed.
- Backend full suite: one earlier final-state run passed 693/693 plus two golden-path tests; later reruns intermittently failed only `cmdb::assets::tests::concurrent_manual_creates_receive_distinct_identifiers` with SQLite `database is locked`. This is unrelated backend code and remains unqualified as a deterministic full gate.

Review result and blocker

The final independent review failed closed, so this slice must not be committed or described as complete. The reviewer reproduced these unresolved security/logic limitations:

1. `#[cfg(test)]` masking associates an attribute with a later module rather than validating the immediately following item, so malformed/atypical source can hide production code.
2. The lexical scanner does not establish complete Rust syntax or semantic coverage for whitespace-separated paths, imports/aliases, `OpenOptions`, directory mutation APIs, and related valid forms.
3. Canonical delegation is detected by textual call-marker presence rather than a validated call shape; a spoofed `operation_adoption::...` token can allow a direct mutation.
4. Broad module/function exception entries are not tied to an immutable body digest, exact call shape, verified caller, or semantic operation contract; later mutation changes inside an allowlisted function could pass.
5. File-derived module/function identity does not distinguish all inline modules and impl-method contexts.

These are real trust-boundary gaps. The focused fixtures prove the currently covered cases only; they do not establish fail-closed coverage of all valid Rust syntax. No runtime, browser, Docker, external-provider, installation, upgrade, or recovery qualification was performed.

Preserved unrelated work

The pre-existing untracked `testing/` tree was not modified. No credentials, provider tokens, supervisor configuration, skills, cron, system services, main branch, remote, or unrelated source paths were changed.

Next bounded slice

Replace the lexical production inventory with a Rust-parser/AST-backed checker (or an explicitly fail-closed parser wrapper) that validates exact call shapes, imports/aliases, inline-module identity, cfg attributes, and immutable exception evidence before re-enabling commit/review. Add reviewer-reproduced fixtures first, then rerun the same source, CI, full-suite, and independent-review gates. Do not widen to provider runtime or unrelated adapter migrations until this enforcement boundary passes review.
