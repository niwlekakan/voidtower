# Shared Jobs and Approvals UX Implementation Plan

Design: `docs/internal/specs/2026-08-26-shared-jobs-approvals-ux-design.md`
Scope: J0-01/J0-03 shared Tower and Void Mode job/approval workflows over bounded HTTP polling

## 1. Freeze backend route, authorization, and serializer behavior

- Add focused real-router tests around `backend/src/api/jobs.rs` and
  `backend/src/api/approvals.rs`, reusing the established test application/session helpers.
- Prove the positive Jobs allowlist is exactly owner/admin/operator and the Approvals allowlist is
  exactly owner/admin for list and detail routes.
- Prove cancellation accepts only queued/running jobs and does not add awaiting-approval
  cancellation behavior.
- Prove approval decisions act on the requested immutable approval ID and that decided, expired,
  and stale records return conflicts rather than making a second transition.
- Assert the JSON shapes for the complete job and approval views used by the frontend.
- Keep canonical action authorization, bearer ownership, worker transitions, and reconciliation in
  their existing authoritative tests rather than duplicating those implementations in API code.

## 2. Add the minimal frontend behavior-test harness

- Add Vitest, jsdom, Testing Library, and jest-dom development dependencies to
  `frontend/package.json` and the lockfile.
- Add a non-watch `test` script and focused Vitest setup/configuration without changing Vite's
  production behavior.
- Provide small test builders for complete durable job and approval records and reset fake timers,
  mocked visibility, routing, API calls, and Zustand stores between tests.
- Keep the harness limited to shared Jobs/Approvals behavior; do not attempt a repository-wide
  frontend test migration.

## 3. Complete TypeScript contracts and API methods

- Expand `frontend/src/api/types.ts` with typed resource, actor, plan, step, error, complete job,
  approval, list, detail, cancellation, and decision contracts matching the Rust serializers.
- Preserve `DurableJobSummary` as a compatibility alias so existing adopted pages remain source
  compatible while shared code uses `DurableJob`.
- Extend `api.operationJobs` in `frontend/src/api/client.ts` with bounded `list`, `get`, and `cancel`
  methods.
- Add `api.approvals.list/get/approve/reject`, omitting absent query parameters and sending one
  explicit JSON decision body with the optional trimmed comment.
- Add contract/client tests for URL encoding, query omission, response typing fixtures, and exact
  decision endpoints.

## 4. Implement bounded polling hooks

- Add a focused visibility-aware bounded polling primitive under `frontend/src/hooks` that tracks
  initial loading, background refresh, last confirmed data, stale reads, error, and deadline state.
- Build `useJobList`, `useJobDetail`, `useApprovalList`, and `useApprovalDetail` on that primitive.
- Poll active job detail every two seconds and active-containing lists every five seconds; poll
  pending approval records every five seconds.
- Pause while hidden, refetch the complete record on visibility return or after a read gap, and stop
  after the fixed 20-minute foreground deadline without implying durable execution stopped.
- Leave `needs_attention` outside automatic following while preserving manual refresh.
- Add fake-timer tests for scheduling, terminal stopping, cleanup, visibility, stale preservation,
  full refetch, and fixed-deadline behavior.

## 5. Build safe shared operation presentation

- Add a focused `frontend/src/components/operations` module for job/approval state badges, progress,
  identity metadata, typed plan display, public error display, and shared loading/stale/empty/
  forbidden states.
- Add a bounded structured-value renderer with fixed depth/key/item/string limits, explicit
  truncation, and secret-like key masking.
- Use React text rendering only. Do not add raw HTML, unrestricted arbitrary JSON dumps, or job
  input presentation.
- Keep existing durable-job tone/label helpers source compatible or move them behind a shared
  module with compatibility exports for `useDurableJobTracker` and `DurableJobNotice`.
- Add renderer tests for every bound, key masking, non-secret scalar output, and hostile strings.

## 6. Add Tower Jobs list and detail workflows

- Add Jobs list/detail page components under `frontend/src/pages` and register `/jobs` and
  `/jobs/:id` in `frontend/src/App.tsx`.
- Show the newest 50 confirmed records with state/action filters, manual refresh, progress, resource,
  actor/ingress, submitted time, and canonical detail links.
- Render complete safe detail from the typed job contract.
- Offer explicit-confirmation cancellation only for queued/running records, lock the action while
  in flight, and call the selected ID exactly once.
- On success, accept the returned job. On transport error or conflict, refetch detail before
  presenting the authoritative state; never optimistically mark cancellation successful.
- Add page tests for loading/empty/error/stale states, filtering, state-specific actions,
  single-shot cancellation, ambiguous-result refetch, and `needs_attention` guidance.

## 7. Add Tower Approvals list and exact-record decision workflow

- Add Approvals list/detail page components and register `/approvals` and `/approvals/:id`.
- Default the list to pending and support explicit pending/approved/rejected/expired/stale/all
  server-backed filters.
- Load the selected approval and associated job, then render policy reason, requirement, expiry,
  actor/resource, immutable plan, and current job state before decision controls.
- Offer Approve and Reject only for a confirmed pending record. Share one in-flight lock, enforce a
  visible 500-character plain-text comment limit, and submit the URL's exact approval ID once.
- Refetch both approval and job after success, transport ambiguity, or conflict; present expiry,
  staleness, or another administrator's decision as the authoritative result.
- Add tests for filters, exact ID use, role/state action absence, comment handling, single-shot
  decisions, and post-decision/ambiguity refetch.

## 8. Add positive role-aware Tower navigation and routes

- Add one shared frontend role helper with exact operator and admin allowlists matching
  `backend/src/api/role_guard.rs`.
- Add a `RequireRole` route wrapper that renders a clear forbidden state for a direct disallowed
  visit without claiming to be a security boundary.
- Protect Jobs routes with the operator allowlist and Approvals routes with the admin allowlist.
- Add Jobs and Approvals under the Tower Ops group in `frontend/src/components/layout/Sidebar.tsx`
  and `frontend/src/store/navConfig.ts`.
- Attach role metadata to navigation definitions and apply it after persisted customization is
  resolved so stored configuration cannot restore a forbidden item.
- Add role-matrix tests for route and navigation visibility across all frontend roles.

## 9. Add native Void Mode Jobs and Approvals panels

- Add `frontend/src/aios/panels/jobs.tsx` and `approvals.tsx` using the shared contracts, hooks,
  safe presentation, and mutation behavior with `NativePanelShell`/`NativeRow` conventions.
- Use one deterministic list-to-detail transition at every panel width with explicit back behavior.
- Register both panels in `NATIVE_PANEL_REGISTRY`; add role-aware dock, command-palette, icon/label,
  and customizable navigation definitions.
- Filter Jobs to owner/admin/operator and Approvals to owner/admin after persisted navigation is
  resolved.
- Add an `AiosLayout` route bridge for `/jobs/:id` and `/approvals/:id` that opens the corresponding
  panel and lets the panel selection follow and update the canonical URL.
- Add tests for role visibility, list/detail transition, canonical URL selection, opening from a
  deep link, and exact native mutation behavior.

## 10. Link every page-local durable notice to shared detail

- Change `frontend/src/components/ui/DurableJobNotice.tsx` so the displayed job identity is a
  semantic `/jobs/:id` link while retaining state, label, tone, spinner, and local tracking.
- Make the link use the canonical route in Tower and rely on the Void route bridge in Void Mode.
- Preserve `useDurableJobTracker` and `useDurableJobBatchTracker` terminal-success callbacks and
  deadlines unchanged.
- Give visible Proxmox batch job identities individual shared detail links without moving batch
  aggregation into a global store.
- Add tests proving navigation does not resubmit, re-track, or replay domain completion callbacks.

## 11. Freeze shared-workflow source coverage and update public status

- Extend the executable source inventory in `backend/src/operations/registry.rs` or a focused
  frontend-boundary test module.
- Assert the four Tower routes, exact role-filtered Tower navigation entries, native panel
  registration, role-filtered Void entries, canonical deep-link bridge, and linked page-local
  durable notice.
- Retain the existing six-domain tracker/notice inventory and ensure shared detail linkage cannot be
  silently removed.
- Update `ROADMAP.md`, `README.md`, and `docs/api.md` to mark shared Jobs/Approvals UX complete while
  leaving cursor-resumable durable SSE as the next J0 checkpoint.
- Document that list windows are bounded, HTTP polling is temporary, cancellation remains
  queued/running only, and approval decisions require owner/admin human sessions.

## 12. Focused and complete verification

- Run focused backend job, approval, role-guard, state, route, serializer, and source-inventory
  tests while iterating.
- Run focused frontend API, hook, component, page, navigation, and native-panel tests.
- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run frontend tests, type-check, lint, and production build.
- Run `scripts/check-schema-migration-ownership.sh`, `scripts/check-repository-hygiene.sh`,
  `gitleaks git --no-banner` when the local binary is installed (otherwise record that exact
  environmental limitation), and `git diff --check` plus staged diff checks.
- Do not run a repository-wide Rust formatter over unrelated pre-existing drift.
- Commit the implementation locally without pushing.
- Remove the tracked design and plan from the final implementation tree while preserving their
  ignored local copies, then write an ignored handoff with exact verification and the next bounded
  durable-SSE slice.

## Acceptance matrix

| Requirement | Evidence |
|---|---|
| Operators can inspect and cancel eligible jobs | Jobs routes/pages/panels, cancellation tests |
| Admins decide exact immutable approvals | Approval detail workflow and exact-ID conflict tests |
| Role boundaries fail closed in every surface | Backend real-router matrix and frontend role tests |
| Polling cannot mutate or run forever | Hook fake-timer/visibility/deadline tests |
| Ambiguity never becomes optimistic success | Cancellation/decision refetch tests |
| Results and plans stay bounded and redacted-safe | Structured renderer and hostile-value tests |
| Tower and Void share canonical deep links | Route bridge and notice-link tests |
| Local provider refresh ownership is preserved | Existing tracker callbacks and regression tests |
| Durable SSE remains the next separate slice | Roadmap/docs scope assertions |
