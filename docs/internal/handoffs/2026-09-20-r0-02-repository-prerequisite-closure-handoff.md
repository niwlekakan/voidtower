# R0-02 repository prerequisite closure handoff — 2026-09-20

Status: implemented, unit-verified, and integration-verified at the repository/Git boundary; release-qualified: blocked.

Implementation commit: b6033eb ([verified] harden repository prerequisite gates)
Branch: dev

Implemented

- Replaced the optional-`rg` schema ownership shell scan with `scripts/check_schema_migration_ownership.py`, retained behind `scripts/check-schema-migration-ownership.sh`.
- Enforced fail-closed production SQLite schema ownership: SQLx DDL call and macro variants are scanned outside `backend/src/db/legacy.rs`; comment-separated paths, typed/turbofish arguments including lifetimes, and the complete digit-bearing `query_file_*` family are covered; source/migration symlinks and canonical paths escaping the repository are rejected; numbered migrations must be contiguous from `0001` and Git-tracked.
- Hardened `scripts/check-repository-hygiene.sh`: NUL-safe `git ls-files -s -z` parsing, explicit Git-enumeration failure, repository-root anchoring, tracked-symlink rejection, case-insensitive basename credential/key/token/password/auth/netrc policy, and an explicit fixture-proven `app-vault/apps/authentik.yml` non-secret acceptance.
- Added `scripts/test_repository_prerequisites.py` with nine adversarial contract tests covering continuity allowlists, unusual and mixed-case sensitive paths, symlinks, SQLx DDL/query-file forms, missing optional tools, migration gaps, and canonical containment.
- Added the prerequisite contract suite to `.github/workflows/ci.yml`; updated `scripts/test_repo_truth.py` to assert the CI ordering.
- Updated `backend/Cargo.lock` (`rustls` 0.23.40 → 0.23.45), CI cargo-deny installation (`cargo-deny@0.20.2`), and `backend/deny.toml` compatibility configuration.
- Updated `CONTRIBUTING.md`, `docs/release-gates.md`, `docs/internal/agent-knowledge/system-map.md`, and `documentation-backlog.md` with the executable prerequisite contract, evidence boundary, and future operator documentation gaps.

Independent review

- Final independent staged-diff review passed: `passed=true`, `security_concerns=[]`, `logic_errors=[]`, `suggestions=[]`.
- Review exercised typed lifetime SQLx macros, comment-separated SQLx boundaries, `query_file_v2!`, mixed-case and bare sensitive basenames, the Authentik fixture, symlink/canonical containment, Git failure, migration tracking/numbering, CI ordering, documentation consistency, and added-line security scans.

Verification evidence

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_repository_prerequisites scripts.test_release_gate scripts.test_repo_truth -v` — passed, 48 tests.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_repository_prerequisites -v` — passed, 9 tests.
- `bash scripts/check-repository-hygiene.sh` — passed; `Repository hygiene check passed (650 tracked files checked).` The same absolute script invocation from `/tmp` also passed.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `bash -n scripts/check-repository-hygiene.sh scripts/check-schema-migration-ownership.sh` — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile scripts/check_schema_migration_ownership.py scripts/test_repository_prerequisites.py` — passed.
- `git diff --cached --check` — passed before commit.
- `cargo test --all-targets --all-features` — passed: 671 tests, 2 integration tests, and 0 example tests failed.
- `cargo deny check` — passed: advisories, bans, licenses, and sources all OK. Existing yanked/duplicate dependency warnings remain in tool output.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/release_gate.py --repo . --scope changed --json --output dev-data/release-gate-r0-02-final-staged.json` — exit 1 fail-closed; all selected gates passed except `backend-clippy` (exit 101). The report is machine-readable evidence, not release qualification.

Limitations and blockers

- `cargo fmt --all -- --check` remains exit 1 on pre-existing untouched backend formatting findings, including `backend/src/agent/mod.rs` and `backend/src/agent/state.rs`.
- `cargo clippy --all-targets --all-features -- -D warnings` remains exit 101 on pre-existing untouched backend findings (including `backend/src/api/ai_providers.rs`, dead-code items, and other existing lint diagnostics). This is the sole failing selected gate in the final changed-scope report.
- No runtime, browser, Docker, clean-install, upgrade/recovery, or named-platform qualification was claimed; the hardened sandbox has no host Docker/browser qualification path.
- Unrelated pre-existing untracked `scripts/__pycache__/` and `testing/` paths were preserved and not staged or modified.
- The implementation commit is local only; it was not pushed.

Retrospective

- Learned that repository governance scanners must treat path names and source syntax as hostile input: Git output needs NUL framing, tracked symlinks need explicit rejection, canonical containment must be checked, SQLx macro syntax needs comment/turbofish/typed-argument coverage, and sensitive basename matching must be normalized without blocking legitimate `authentik.yml` application configuration.
- Verified that the governance boundary is reproducible without `rg`, works when invoked outside the repository root, and remains useful offline; the backend test and cargo-deny evidence are independent of AI or external cloud services.
- Reusable commands/fixtures are the 48-test focused unittest command, the nine-test prerequisite suite, the two shell gates, `cargo test --all-targets --all-features`, `cargo deny check`, and `dev-data/release-gate-r0-02-final-staged.json`.
- End-user/developer documentation changed in `CONTRIBUTING.md` and `docs/release-gates.md`; the release-operator artifact/install/runtime/upgrade/recovery runbook remains required and is recorded in `documentation-backlog.md`.

Next dependency-ready slice

- Resolve the pre-existing backend `rustfmt` and strict Clippy baseline findings as one bounded release-gate prerequisite slice, then rerun the complete release-gate manifest before attempting runtime or packaged release qualification.
