# Backups Compatibility Adoption Implementation Plan

Design: `docs/internal/specs/2026-08-24-backups-compatibility-adoption-design.md`
Scope: J0-01/J0-03 Backups HTTP, local CLI, and scheduled restore-test adoption

## 1. Generalize canonical invocation ingress

- Add `LocalCli` and `Scheduler` to `ActionIngress` and give only the five Backup actions the
  exact admissions fixed by the design.
- Replace the HTTP-only credential abstraction with a typed invocation context that derives its
  registry ingress, actor, authority, and idempotency scope atomically.
- Preserve session role and bearer role/scope/AI checks. Give local CLI an explicit admin ceiling
  and scheduler authority only through exact action-ingress admission.
- Add `System` to VoidWatch actor policy evaluation, matching `system` and wildcard rules and
  defaulting to allow only after canonical authorization.
- Change prepare and submit to accept any declared durable ingress rather than filtering every
  call through `ActionIngress::Http`.
- Add table-driven tests for exact ingress admissions, actor encoding, scope construction,
  role/scope denial, scheduler denial on every non-scheduled action, and normal policy/approval.

## 2. Add shared Backup target adoption helpers

- Add a small pool-based helper that authorizes before resolving the seeded local system
  singleton and publishes create capability without a restic probe.
- Add a config helper that authorizes first, probes restic only for run/check/restore-test, loads
  a normalized config by ID or by name, observes `backup_config` under
  `voidtower.backup_config/local/<config-id>`, then publishes the requested capability.
- Keep route IDs and names as lookup values rather than canonical resource IDs.
- Return typed compatibility errors for HTTP and anyhow diagnostics for CLI/scheduler adapters
  without leaking provider output.
- Add tests for authorization ordering, aliases, missing config, missing restic, and capability
  evidence.

## 3. Adopt Backup HTTP compatibility routes

- Change create, delete, run, check, and restore-test handlers to derive the canonical invocation
  context, resolve/observe through the shared helper, and submit through operation adoption.
- Add `HeaderMap` idempotency handling and the optional bearer extension only to `backup.run`.
- Translate create's legacy body to `BackupConfigInput`; use unit input for existing-config
  actions.
- Replace handwritten delete preview with canonical prepare and keep the existing modal envelope.
- Return `202 {"job": ...}` for all executions and no provider success fields.
- Extend route/action metadata if needed so only run accepts `backups:run` bearer credentials.
- Add handler/source tests proving every execution delegates to canonical submit, delete-plan
  delegates to prepare only, and no direct Backup mutation calls remain.

## 4. Update the Backups frontend

- Use the existing `DurableJobResponse` type for all five mutations.
- Report `Submitted (job <id>)` for create, run, check, restore-test, and delete.
- Stop interpreting synchronous status/message/provider fields and stop claiming immediate
  creation or deletion success.
- Preserve the current form, confirmation modal, read-only list, tags, and manual refresh.
- Run focused TypeScript lint/build after the page change.

## 5. Adopt local CLI mutations

- Separate DB-only management commands so users and `backup list` retain the early lightweight
  path while Backup mutations receive configuration and operation context.
- Extract server secrets-key loading into a shared helper and construct the same staged adapter
  registry for CLI mutations.
- Prepare and start a bounded local operation runtime, submit exactly once with the local CLI
  context and per-invocation UUID key, and print the job ID immediately.
- Resolve create through the system singleton and existing configs by name only after
  authorization.
- Add a persisted-state wait helper with 250 ms polling and a 30-minute timeout. Handle approval,
  terminal, needs-attention, Ctrl-C, and timeout exactly as the design specifies.
- Shut down the local runtime on every exit path without directly cancelling the job or invoking
  Backup services.
- Add focused tests around context setup, state classification, bounded output, and source
  inventory for all five branches.

## 6. Adopt scheduled restore tests

- Replace the direct restore-test task with a scheduler-context submission helper.
- Authorize before the scheduled-config query or restic probe.
- Preserve the cron and last-run-within-60-seconds guard.
- Observe each due config and submit `backup.restore_test` with a key containing the config ID and
  UTC minute window under the fixed scheduler scope.
- Log safe job ID/state data only and let the already-running production worker execute.
- Add idempotency tests for same-window replay and later-window submission, plus a source test
  forbidding direct scheduled provider execution.

## 7. Strengthen Backup boundary inventory

- Expand `compatibility_and_cli_callers_use_the_typed_backup_service_boundary` to inspect each
  adopted HTTP handler, every mutation CLI branch, and the scheduler section.
- Require canonical authorize/prepare/submit delegation in the relevant sections.
- Explicitly forbid `create_config`, `delete_config`, `prepare_config_repository`,
  `run_config_backup`, `check_config`, and `restore_test_config` outside the Backup
  adapter/provider execution boundary.
- Keep read-only `list_configs`, `get_config`, confidence, cron, and availability observations
  allowed only where the design permits them.

## 8. Verify and commit the checkpoint

- Format only touched Rust files with direct `rustfmt --edition 2021`.
- Run focused registry, invocation, Backup adapter, API, CLI, scheduler, and frontend checks while
  iterating.
- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run frontend `npm run lint` and `npm run build`.
- Run schema-migration ownership, repository hygiene, and `git diff --check`.
- Review the complete diff for authorization-before-observation, provider bypasses, unsafe
  output, unrelated churn, and legacy request compatibility.
- Commit one focused Backups adoption checkpoint. Do not push.

## Acceptance matrix

| Requirement | Primary evidence |
|---|---|
| Exact HTTP/CLI/scheduler authority | Registry and invocation context matrices |
| Authorization precedes evidence reads | Shared resolver behavior and source tests |
| Durable HTTP mutation responses | Handler tests and frontend typed consumption |
| Advisory delete preview | Prepare-only test with unchanged job/approval counts |
| CLI submit/wait/recovery behavior | State-classification and runtime shutdown tests |
| Scheduler duplicate boundary | Deterministic minute-window replay test |
| No direct Backup execution bypass | Expanded adapter source inventory |
| No provider success leakage | Response/CLI assertions and bounded result tests |
| Full checkpoint health | Backend, frontend, schema, hygiene, and diff gates |
