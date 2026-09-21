# Updates Compatibility Adoption Implementation Plan

Design: `docs/internal/specs/2026-08-24-updates-compatibility-adoption-design.md`
Scope: J0-01/J0-03 Updates HTTP compatibility adoption and page-local job tracking

## 1. Add shared Updates target adoption

- Add `operations::update_adoption` with typed selectors for VoidTower, Odysseus, Docker engine,
  Docker container image, and operating system.
- Authorize the canonical action before every provider snapshot or capability write.
- Resolve the seeded `update_target` and `docker_engine` aliases only after affirmative evidence.
- Normalize Docker selectors through the provider snapshot and observe `container_image` using the
  full ID under `docker.container_image/local`.
- Publish only the selected available capability and return typed compatibility-safe errors.
- Add focused tests for alias/kind resolution, Docker normalization, unavailable evidence, and
  authorization-before-observation.

## 2. Adopt `/api/updates/*` checks and mutations

- Replace admin-handler-local credentials with the canonical credential context.
- Route VoidTower check/apply/rollback, Odysseus apply, Docker check/apply, and OS apply through
  the shared resolver and `operation_adoption` prepare/submit helpers.
- Preserve legacy bodies and translate rollback to canonical `{tag}` input and all other actions
  to `{}`.
- Produce the four legacy `dry_run` modal responses from adapter plans without submitting.
- Accept caller idempotency headers and return only canonical `202` responses with the versioned job
  envelope for execution.
- Remove direct rollback preparation/execution, detached check tasks, and process-local caches.
- Derive Docker and VoidTower image status directly from synchronous snapshots.
- Add handler tests for action mapping, `202` responses, prepare-only behavior, and no inline
  provider execution.

## 3. Adopt the legacy system aliases

- Replace `GET /api/system/update-check` implementation with canonical
  `update.voidtower.check` submission through the Updates resolver.
- Replace `POST /api/system/update` helper-script execution with canonical
  `update.voidtower.apply` submission.
- Preserve the separate system version and restart behavior.
- Remove now-unused update-only GitHub, Git/process, install-directory, and helper-script code from
  `api/system.rs` without changing version reporting.
- Add tests and source inventory proving both aliases delegate and cannot execute update providers
  or scripts.

## 4. Strengthen Updates boundary tests

- Expand the existing source-inventory test to inspect both Updates and System handlers.
- Require canonical credential, target-resolution, prepare, and submit calls in their exact
  sections.
- Explicitly forbid direct `execute`, `prepare_rollback`, detached tasks, command construction,
  update script writes, and process-local execution status.
- Assert all nine typed route mappings retain their exact seven canonical actions and policy.
- Keep adapter/provider execution and reconciliation tests unchanged unless a failing adoption test
  exposes an actual boundary defect.

## 5. Add typed frontend job tracking

- Extend the frontend API client with typed methods for all Updates and system-alias submissions,
  advisory plans, synchronous info reads, and `GET /api/jobs/:id`.
- Add a focused hook/helper that tracks one returned durable job, classifies canonical states,
  tolerates restart downtime, stops safely on unmount or the 20-minute foreground bound, and
  refreshes caller data only after success.
- Keep canonical job state as the only lifecycle source; do not mirror backend execution state in
  an independent frontend state machine.
- Add pure helper tests only if the current frontend test setup supports them without new
  infrastructure; otherwise enforce behavior through strict types, lint, and build.

## 6. Convert Updates and Settings interactions

- Update every Updates page check, apply, rollback, Odysseus, Docker, and OS handler to consume a
  `DurableJobResponse` and register the returned job with the local tracker.
- Keep adapter-produced `ChangePlanModal` previews and close them after accepted submission.
- Replace synchronous success/output/restart polling claims with canonical job/state messages.
- Refresh VoidTower/Docker/OS information after successful jobs.
- Update Settings to keep exercising `/api/system/update-check` and `/api/system/update`, track the
  resulting jobs, and refresh version/Update info on success.
- Surface approval, terminal failure, expiration, cancellation, `needs_attention`, and foreground
  tracking timeout without adding approval controls or global job navigation.

## 7. Reconcile roadmap and internal handoff

- Update the tracked J0 status and compatibility-route evidence in `ROADMAP.md`, leaving Proxmox,
  durable SSE, shared frontend UX, and bypass closure explicitly outstanding.
- Create the ignored successor handoff with exact files, verification results, commit/publication
  state, and Proxmox as the next bounded slice.
- Do not add Update CLI claims because source inventory proves no such caller exists.

## 8. Verify, review, and commit

- Format only touched Rust files with direct `rustfmt --edition 2021`.
- Run focused Updates resolver, API, adapter, registry, and source-inventory tests.
- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run frontend `npm run lint` and `npm run build`.
- Run schema-migration ownership, repository hygiene, and `git diff --check`.
- Review the complete diff for authorization ordering, provider bypasses, false completion,
  unbounded output, stale cache state, unrelated churn, and route compatibility.
- Remove the tracked internal design/plan from the public implementation checkpoint while keeping
  their ignored local copies, matching the previous domain-adoption workflow.
- Commit one focused Updates adoption checkpoint. Do not push.

## Acceptance matrix

| Requirement | Primary evidence |
|---|---|
| Exact target identity and capability | Resolver alias/provider tests |
| Authorization before host observation | Denial-order tests and source inventory |
| Nine routes use seven durable actions | Registry and handler response tests |
| Advisory plans remain side-effect free | Prepare-only job/approval count tests |
| No API provider execution | Cross-file source inventory |
| Accurate status without transient caches | Snapshot-to-status tests |
| Frontend follows canonical jobs | Typed APIs, tracker state coverage, lint/build |
| Self-update never fabricates success | Restart-tolerant tracker plus adapter reconciliation |
| No scope expansion | Diff review and explicit deferred list |
| Full checkpoint health | Backend, frontend, schema, hygiene, and diff gates |
