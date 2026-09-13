# M1-03 Automation Convergence Hardening Handoff

Date: 2026-09-09
Branch: `dev`
Parent slice: M1-03 automation convergence

## Scope

Corrective hardening identified by an independent review of the canonical automation operation path. The changes remain within the automation trust boundary and preserve the unrelated staged agent changes in:

- `backend/src/agent/mod.rs`
- `backend/src/agent/state.rs`

## Implemented

- HTTP automation execution now derives `CredentialContext` from either session or bearer authentication instead of forcing bearer requests into a session identity.
- Existing automation resources are resolved through the active-resource path; retired resources are not silently reactivated and administrator-owned display names are not re-observed on every run.
- Scheduled automation uses the same active-resource resolution path as HTTP and webhook ingress.
- Odysseus automation webhooks use an explicit `Idempotency-Key` when supplied, otherwise a deterministic bounded key derived from the webhook payload.
- The automation adapter receives the application secret key and redacts exact stored secret values as well as heuristic secret-shaped output before persistence or response projection.
- Automation execution revalidates the durable job fingerprint immediately before creating its execution record, refusing changed commands without leaving a new running row.
- Local command execution runs in a dedicated process group with piped, bounded output; timeout handling sends SIGKILL to the process group.
- Added a regression test proving exact stored secret values are absent from automation output.

## Verification

All commands were run from `/home/elwla/Documents/voidtower_project_files_full/hive/voidtower`.

- `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test operations::adapters::automation::tests --all-features -- --nocapture`
  - **unit-verified**: 3 passed, 0 failed.
- `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test api::operation_workflows_tests::automation_run_uses_canonical_job_and_replays_by_idempotency_key --all-features -- --nocapture`
  - **unit-verified**: 1 passed, 0 failed.
- `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest cargo test --all-features`
  - **unit-verified**: 480 passed, 0 failed; 2 golden-path tests passed.
- `docker run --rm -v "$PWD:/workspace" -w /workspace/backend rust:latest sh -c 'rustup component add clippy >/dev/null && cargo clippy --all-targets --all-features -- -D warnings'`
  - **unit-verified**: passed with no warnings.
- `git diff --check`
  - **implemented**: no whitespace errors.
- `python scripts/repo_truth.py --repo . --json --check`
  - **implemented**: source inventory check passed; it remains source evidence only.
- `hermes verify --json --port 80`
  - **runtime-verified**: Compose startup completed, nginx readiness returned HTTP 200, and teardown completed cleanly.

## Review disposition

The independent review was fail-closed. Its stale-snapshot claim that the scheduler still executed commands inline does not apply to the current tree; the scheduler had already been routed through durable submission. The valid findings were addressed above. Runtime verification is limited to packaged container startup/readiness. No external provider or Odysseus-node workflow was exercised, so release qualification remains unclaimed.

## Commit boundary

This handoff is intended to accompany the focused hardening commit. No push is authorized.
