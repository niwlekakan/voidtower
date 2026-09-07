# Handoff — S2-02 terminal SSH credential closure

## Slice identity

- **Repository:** `/home/elwla/Documents/voidtower_project_files_full/hive/voidtower`
- **Branch:** `dev`
- **Commit:** `2f25138` (`[verified] migrate terminal SSH credentials to secret manager`)
- **Tracked outcome:** S2-02 Legacy secret closure and rotation
- **Bounded checkpoint:** terminal SSH password consumer migration and resolver closure
- **Authority:** `docs/development-plan.md`; this checkpoint does not promote S2-02 as a whole until remaining supported consumers are closed.

## Implemented

- Added migration `0005_terminal_ssh_secret_reference.sql` with nullable `ssh_sessions.password_secret_id`.
- Added transactional, restart-safe migration of legacy `ssh_sessions.password_enc` values into canonical encrypted `secrets` rows.
- Updated terminal SSH create/update handlers to write and rotate encrypted secret records instead of session ciphertext; the session stores only the secret reference.
- Added the `terminal_ssh` resolver purpose and routed SSH connection credential loading through the shared secret resolver.
- SSH connections now fail closed for missing, disabled, corrupt, oversized, or unavailable credentials without exposing secret material or resolver details.
- Updated schema canonicalization, golden schema, migration counts, and startup migration ordering.
- Existing staged `backend/src/agent/mod.rs` and `backend/src/agent/state.rs` were preserved and were not included in commit `2f25138`.

## Verification evidence

Machine-readable report:

`docs/internal/evidence/2026-09-07-s2-02-terminal-ssh-final-batch-report.json/evidence.json`

Manifest:

`docs/internal/evidence/2026-09-07-s2-02-terminal-ssh-final-batch.json`

Post-commit batch result: every declared step passed.

- `python scripts/repo_truth.py --repo . --json --check` — exit 0; source inventory passed.
- `cargo test api::terminal::tests --all-features -- --nocapture` — exit 0; terminal resolver and handler tests passed.
- `cargo test api::secrets::tests::legacy_ssh_password --all-features -- --nocapture` — exit 0; migration success/idempotence and failure-preservation tests passed.
- `cargo test --all-targets --all-features` — exit 0; 463 unit tests and 2 integration tests passed.
- `cargo clippy --all-targets --all-features -- -D warnings` — exit 0.
- `scripts/check-schema-migration-ownership.sh` — exit 0.
- `git diff --check` — exit 0.
- `hermes verify --json --port 80` — exit 0; detected Docker Compose build completed, the HTTP readiness probe returned status 200, and the stack was torn down cleanly.

## Evidence classification

- **implemented:** commit `2f25138`, migration/schema/startup and terminal consumer changes.
- **unit-verified:** terminal resolver state matrix, handler create/rotate storage, legacy SSH migration idempotence and failure preservation.
- **integration-verified:** all-targets backend tests, schema ownership, canonical schema golden test, clippy.
- **runtime-verified:** `hermes verify --json --port 80` passed the Docker Compose startup and HTTP readiness probe (200); no live SSH target was available or contacted.
- **release-qualified:** blocked; no packaging, deployment, or release-environment validation was performed.

## Limitations and recovery

- `password_enc` remains in the schema as a one-time legacy migration source; migrated rows have it cleared. Removing that column is a later migration and is not part of this checkpoint.
- A corrupt legacy ciphertext causes the startup migration to fail closed while preserving the old ciphertext and reference state for recovery; it is not silently discarded.
- SSH private-key paths remain path references consumed by the existing terminal implementation; this checkpoint closes password credentials only.
- No live provider/SSH session was executed.
- The pre-existing staged agent changes remain in the worktree and must not be reset, cleaned, unstaged, or included in future commits without their owner’s direction.

## Next dependency-ready slice

Finish the remaining S2-02 supported secret consumers and run the same DB/log/export scan. Only after the complete S2-02 acceptance is green should H5-02 machine-secret grants or V6-02 release qualification proceed.
