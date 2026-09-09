# Handoff: M1-03 automation durable-operation convergence

- **Date:** 2026-09-09
- **Branch:** `dev`
- **Commit:** `1a7ba36 converge automation ingress on durable operations`
- **Parent outcome:** M1-03 automation execution uses the canonical typed operation boundary.
- **Status:** integration-verified for the bounded source/test slice; runtime-verified and release-qualified are not claimed.

## Implemented checkpoints

1. **Durable contract and registry** (`implemented`)
   - Added the `automation` operation adapter and registered it in the adapter registry.
   - Registered `automation_job`/`automation` metadata and `POST /api/automation/:id/run` as a canonical operation route.
   - Added scheduler ingress metadata while retaining HTTP and webhook ingress.
   - Removed the obsolete combined webhook/automation ingress declaration.

2. **HTTP and webhook submission** (`integration-verified`)
   - `run_now` now resolves the canonical resource and submits `automation.run` through operation adoption, including request idempotency headers.
   - The Odysseus automation webhook path resolves the same resource and uses dry-run planning or durable submission; it no longer executes a shell command inline.
   - Automation policy authorization remains before resource observation.

3. **Scheduler convergence** (`integration-verified`)
   - The scheduler now observes the automation resource and submits a canonical durable job with a stable current-slot idempotency key.
   - The worker/adapter owns command execution and durable `automation_runs` updates.

4. **Adapter execution and replay** (`unit-verified`)
   - Added bounded command execution with `kill_on_drop`, plan fingerprinting that does not expose the command, durable run recording, output redaction/bounding, and completed-run replay by operation job ID.

## Exact evidence

The reproducible batch manifest is:

`docs/internal/evidence/2026-09-09-m1-03-automation-convergence/manifest.json`

The final batch report is:

`docs/internal/evidence/2026-09-09-m1-03-automation-convergence/final-report-v3/evidence.json`

The batch completed every listed command with exit 0:

- `python scripts/repo_truth.py --repo . --json --check`
- HTTP canonical-job/idempotency test
- adapter execution/replay test
- scheduler canonical-job/current-slot replay test
- `docker run ... cargo test --all-features` — 479 unit tests and 2 golden-path tests passed
- `git diff --check`

Additional final checks:

- `docker run ... cargo clippy --all-targets --all-features -- -D warnings` — exit 0.
- `repo_truth.py --json --check` run twice after the final edit; source-derived reports were equal (`structured_action_count=67`, `route_metadata_count=371`).

## Tests added or changed

- `api::operation_workflows_tests::automation_run_uses_canonical_job_and_replays_by_idempotency_key`
- `api::operation_workflows_tests::automation_scheduler_submits_canonical_job_and_replays_current_slot`
- `operations::adapters::automation::tests::execute_step_persists_result_and_replays_completed_run`
- `operations::adapters::automation::tests::plan_fingerprints_command_without_exposing_it`
- Registry/action inventory assertions updated for the new durable action and scheduler ingress.

## Limitations and blockers

- No live HTTP server, external provider, Odysseus node, or production scheduler process was started; runtime-verified and release-qualified labels are intentionally not claimed.
- Rust formatting was not independently run because the available container toolchain lacks the `cargo-fmt` component. `git diff --check`, full tests, and clippy passed.
- The evidence report directory is local runner output and may be ignored by repository policy; reproduce it with the manifest above.
- The pre-existing staged changes in `backend/src/agent/mod.rs` and `backend/src/agent/state.rs` were preserved and are not part of commit `1a7ba36`.

## Next dependency-ready slice

Exercise the canonical automation route through a running local server and durable worker, including webhook authentication, scheduler-to-worker completion, and runtime audit/event read-back without external cloud dependencies.
