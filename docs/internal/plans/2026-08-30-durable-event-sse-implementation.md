# Cursor-Resumable Durable Event SSE Implementation Plan

Design: `docs/internal/specs/2026-08-30-durable-event-sse-design.md`
Scope: Final J0 durable-event delivery slice for shared Jobs and Approvals

## 1. Freeze cursor and retained-history behavior in the operation event owner

- Extend `backend/src/operations/events.rs` with a retained sequence-bounds query that returns an
  empty-history high-water of zero and populated minimum/maximum values.
- Keep `list_after` as the only event-envelope row decoder and retain its ascending primary-key
  query and 500-row hard clamp.
- Add focused unit coverage for empty and populated bounds, bounded ordered replay, rollback
  invisibility, and histories with an intentionally deleted sequence.
- Do not add a migration or secondary replay store; the existing sequence primary key owns order.

## 2. Replace canonical transient SSE with a durable protocol core

- Rewrite `backend/src/api/events.rs` around focused cursor parsing, range classification, control
  frame construction, and one bounded delivery loop.
- Accept optional `after` and `Last-Event-ID`, reject malformed/negative values, and select the
  maximum supplied cursor. With neither supplied, start at the current high-water mark.
- Emit `stream.ready`, then ordered `durable_event` frames whose SSE ID equals the complete
  `EventEnvelopeV1.sequence`.
- Read in batches of 100. Await a bounded channel for backpressure, continue immediately after full
  batches, wait briefly after empty reads, and stop when the receiver closes.
- Detect behind-retention, future, and internal sequence gaps. Emit one `stream.gap` frame without
  an SSE ID and close without sending later durable rows.
- Keep 15-second SSE keepalive comments. Database failures close delivery so standard reconnect
  resumes after the last confirmed SSE ID.
- Unit-test cursor parsing/selection, control payloads, gap classification, exact frame IDs/data,
  multi-batch order, and producer shutdown at testable helper boundaries.

## 3. Converge routing, authorization, and the integrations compatibility boundary

- Add one shared stream-credential helper in `api/events.rs` for session cookie and Authorization
  bearer credentials. Reject query-string bearer tokens so credentials cannot leak through URLs.
- Require the positive owner/admin/operator session allowlist. Preserve `alerts:read` for API
  tokens and apply the Odysseus emergency-disable setting to token-backed streams at either durable
  route.
- Route both `/api/events/stream` and `/api/integrations/events` to the same durable handler.
- Rename the current integrations `event_stream` producer to `legacy_event_stream` and mount it at
  `/api/integrations/events/legacy`, retaining its transient `metrics`, `alert`, `audit`, and `ping`
  frames and token emergency-disable behavior.
- Remove the obsolete canonical metrics-threshold/systemd producer rather than preserving or
  persisting those non-durable payloads.
- Update `backend/src/action_registry.rs` so the two durable routes and explicit legacy route have
  complete, unique metadata with operator session and `alerts:read` bearer policy.
- Add a focused real-router test module for exact session roles, unauthenticated access, valid and
  invalid token scopes, emergency disable, live-only startup, replay, Last-Event-ID, alias
  equivalence, and gap frames.

## 4. Add complete frontend durable-event contracts and one connection multiplexer

- Add `DurableEventEnvelope`, `DurableEventActor`, stream-ready, stream-gap, and connection-state
  TypeScript contracts that mirror the Rust wire payloads.
- Add a canonical `api.events.streamUrl(after?)` helper. Shared UI code uses session-cookie
  EventSource auth and never serializes a session secret into the URL.
- Implement a lazy singleton under `frontend/src/operations` that owns only EventSource lifecycle,
  validated sequence state, connection state, and subscriber callbacks.
- Start on the first subscriber and close on the last. Listen for `stream.ready`, `durable_event`,
  `stream.gap`, and transport errors.
- Validate complete envelopes, matching SSE IDs, and strict monotonic sequences. Treat malformed or
  discontinuous frames as a gap.
- Let ordinary EventSource reconnect use `Last-Event-ID`. On an explicit gap, close the old source,
  publish unhealthy state, and reconnect from the reported high-water only after subscribers begin
  authoritative recovery.
- Add a mocked-EventSource test suite covering lifecycle, ready, validated delivery, unrelated
  event names, malformed/mismatched/non-monotonic frames, error recovery, gaps, reset cursors, and
  subscriber cleanup.

## 5. Integrate invalidation with bounded HTTP polling without creating record state

- Extend `useBoundedPolling` with an optional durable invalidation subscription and event predicate.
- Preserve its current initial loading, last-confirmed-data, stale/error, visibility, terminal-state,
  manual-refresh, acceptance, and fixed 20-minute foreground-deadline behavior.
- On `stream.ready`, perform a full HTTP barrier read. Pause the interval only after that read
  succeeds for the subscriber.
- On a relevant validated event, perform a full read. Coalesce bursts into one active read plus one
  pending read so transition clusters cannot create overlapping request storms.
- On disconnect, malformed delivery, or gap, clear stream readiness and schedule the existing
  bounded interval fallback without moving the original foreground deadline.
- On visibility return, force a full read before stream readiness can pause polling again.
- Wire operation hooks with broad identity invalidation: any `job_id` for lists, exact `job_id` for
  detail, any `approval_id` for lists, and exact `approval_id` for detail.
- Do not derive status from event type/payload and do not alter cancellation, approval decision, or
  page-local provider completion behavior.

## 6. Freeze frontend fallback and invalidation behavior

- Extend `useBoundedPolling.test.tsx` for ready barriers, paused intervals, disconnect fallback,
  gap recovery, fixed deadline preservation, event coalescing, and visibility recovery.
- Add focused `useOperationRecords` tests proving exact/broad identity predicates and that unrelated
  events do not refetch selected details.
- Retain and run existing polling, Jobs, Approvals, native-panel, notice, cancellation, and exact
  approval-decision suites as regression evidence.
- Add source inventory assertions for the canonical event client, all four operation-hook
  subscriptions, the two durable routes, and the explicit legacy route.

## 7. Update public contracts and status

- Update `docs/api.md` with first-connect, explicit replay, cursor precedence, ready/durable/gap
  frames, keepalive, backpressure, authorization, and authoritative HTTP recovery.
- Update `docs/api-tokens.md` with `alerts:read`, Authorization-header-only bearer credentials,
  positive session roles, and emergency-disable behavior across durable aliases.
- Update `docs/integrations/odysseus.md`, the runtime manifest, and the integrations UI sample to
  describe durable events by default and the deprecated legacy transient URL.
- Update `ROADMAP.md` from partial to complete only after all backend/frontend verification passes.
- Write an ignored handoff with exact commits, test counts, limitations, and the recommended next
  larger product slice.

## 8. Focused and complete verification

- Run focused Rust event persistence, SSE helper, real-router authorization, registry, and source
  inventory tests while iterating.
- Run focused frontend event-client, polling, operation-hook, Jobs, and Approvals tests.
- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run `npm test`, `npm run type-check`, `npm run lint`, and `npm run build`.
- Run `scripts/check-schema-migration-ownership.sh`, `scripts/check-repository-hygiene.sh`, local
  `gitleaks git --no-banner` when installed, `git diff --check`, and staged diff checks.
- Do not apply repository-wide Rust formatting over unrelated established drift. Format only files
  changed by this slice when the repository-wide formatter would touch unrelated code.

## 9. Final repository checkpoint

- Commit the implementation locally without pushing.
- As required by repository hygiene, remove the tracked design and implementation plan from the
  final implementation tree while preserving their ignored local copies for handoff context.
- Confirm the branch is clean and record the final ahead-of-origin state.

## Acceptance matrix

| Requirement | Evidence |
|---|---|
| Both public durable routes share one ordered history | Alias real-router and source-inventory tests |
| Reconnect never rewinds or silently skips gaps | Cursor selection, resume, and gap tests |
| Delivery is bounded under replay and slow readers | Batch and channel backpressure tests |
| Stream access fails closed | Session-role, token-scope, and emergency-disable tests |
| Jobs/Approvals recover authoritative state | Ready/gap/visibility full-refetch tests |
| HTTP fallback remains bounded | Disconnect and fixed-deadline polling tests |
| Events cannot replay mutations | Existing mutation tests plus invalidation-only source assertions |
| Legacy consumers have an explicit migration path | Legacy route tests and documentation |
| J0 status changes only after full verification | Complete verification gate and roadmap diff |
