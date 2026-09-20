# M1-03 encrypted inbound webhook secret-manager handoff — 2026-09-20

## Implemented

- Commit `c738fe9` (`[verified] feat: converge Odysseus webhook secrets`) moves the Odysseus inbound webhook credential to the encrypted `secrets` store, referenced by `settings.key = 'odysseus.webhook_secret_id'`.
- Startup migration transactionally encrypts and removes legacy `odysseus.webhook_secret` only after the canonical reference and encrypted row are written; invalid or oversized legacy values remain recoverable.
- Odysseus configuration supports explicit regenerate/revoke lifecycle flags, one-time regeneration response, GET redaction, plaintext credential rejection, and rejection of simultaneous regenerate/revoke requests. Revoke lookup failures now propagate instead of reporting false success.
- Signed webhook verification resolves the canonical secret reference and fails closed for missing, disabled, corrupt, oversized, or unavailable credentials.
- Frontend and user/API documentation describe one-time reveal, encrypted-at-rest storage, GET redaction, and revoke/regenerate operation.
- Living project knowledge was updated in `docs/internal/agent-knowledge/system-map.md` and `documentation-backlog.md`.

## Verification

- `cd backend && cargo test api::secrets::webhook_migration_tests --all-features -- --nocapture` — passed.
- `cd backend && cargo test api::integrations::tests --all-features -- --nocapture` — passed.
- `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture` — passed.
- `cd backend && cargo test api::authz_matrix_tests --all-features -- --nocapture` — 34 passed.
- `cd backend && cargo test --all-targets --all-features` — 688 unit tests and 2 golden-path tests passed on the final serial run. A concurrent earlier run had one transient SQLite lock; the isolated rerun passed.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cd backend && cargo fmt --all -- --check` — passed.
- `cd frontend && npm test -- --passWithNoTests` — 59 tests passed.
- `cd frontend && npm run type-check` — passed.
- `cd frontend && npm run lint` — passed.
- `cd frontend && npm run build` — passed; Vite emitted only existing chunk-size/dynamic-import warnings.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only.
- `git diff --check` — passed.
- Independent review round 1 found and blocked a revoke database-error swallowing path, ambiguous simultaneous flags, and an inaccurate unknown-field knowledge claim. Those were fixed.
- Independent review round 2 passed with no security or logic blockers. Remaining suggestions were non-blocking regression/concurrency coverage.

## Evidence boundary and limitations

- Evidence is `integration-verified` for the backend Axum/SQLite and frontend build/test boundaries; source inventory is not runtime evidence.
- No host runtime, Docker Compose runtime, browser qualification, provider execution, installation, upgrade/recovery, or release qualification was performed because the sandbox has no Docker socket or host runtime/browser supervisor.
- Existing unrelated untracked paths `scripts/__pycache__/` and `testing/` were preserved and were not included in the commit.
- No credential values are recorded in this handoff.

## Reusable fixtures and commands

- `cargo test api::secrets::webhook_migration_tests --all-features -- --nocapture`
- `cargo test api::integrations::tests::odysseus_config_creates_metadata_only_secret_and_revoke_disables_it --all-features -- --nocapture`
- `cargo test api::operation_workflows_tests --all-features -- --nocapture`
- `cargo test --all-targets --all-features` should be run serially when reproducing the full suite to avoid the known concurrent SQLite-lock flake in an unrelated CMDB concurrency test.

## Next dependency-ready slice

Qualify outbound webhook URL/egress hardening for Odysseus as one bounded slice: validate the configured destination at save/runtime, preserve private-address/SSRF protections, add real-router tests and user documentation, and stop before provider/runtime or browser qualification unless the supervisor supplies the required host runtime.
