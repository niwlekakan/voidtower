# J0 Cursor-Resumable Durable Event SSE Handoff

Date: 2026-08-30
Status: Complete locally
Implementation commit: `eb8b4f7 feat(operations): add durable event sse`
Design commit: `d5787e4 docs(operations): design durable event sse`
Plan commit: `46da559 docs(operations): plan durable event sse`
Branch state: `dev` is twenty commits ahead of `origin/dev`; nothing was pushed.

## Outcome

J0's durable operation, approval, and event foundation is now complete. `/api/events/stream` and
`/api/integrations/events` share one authenticated cursor-resumable SSE implementation over the
existing ordered `events` table. Shared Tower and Void Mode Jobs/Approvals use the stream only as
an invalidation channel, perform authoritative full HTTP reads after ready/events/gaps, and retain
their bounded visibility-aware polling whenever the stream is not proven ready and gap-free.

The existing integrations metrics/audit/ping feed remains temporarily available at the explicit
deprecated route `/api/integrations/events/legacy`. The old canonical metrics-threshold/systemd
polling stream was removed rather than mixed into durable operation history.

## Durable backend contract

- No new schema, event table, replay buffer, or transient operation store was added.
- `operations::events::bounds` reports retained minimum and maximum sequence; `list_after` remains
  the only ordered `EventEnvelopeV1` decoder.
- A connection without `after` or `Last-Event-ID` starts live at the current high-water mark.
- `after=0` requests all retained history. If both cursor sources exist, the larger cursor wins so
  a reconnect cannot rewind.
- Malformed, negative, and overflowed cursor values return `400 bad_request`.
- A valid connection emits `stream.ready`, then complete `durable_event` envelopes. Each SSE `id`
  exactly matches the envelope's database sequence.
- Replay reads batches of 100 into a bounded 64-frame channel. Slow readers apply backpressure;
  receiver closure terminates the producer.
- Behind-retention, future, and internal discontinuity gaps emit `stream.gap` with retained bounds
  and close before any event crosses the gap.
- Fifteen-second keepalive comments never advance the cursor.
- Owner/admin/operator sessions are allowed; lower and unknown roles fail closed.
- API tokens retain the existing `alerts:read` compatibility scope. The current contract accepts
  bearer credentials only in the Authorization header; query-string bearer tokens are rejected.
  Both credential paths honor emergency disable on either durable URL.
- Bearer middleware identity is distinguished from its temporary session cookie so a token cannot
  bypass emergency disable. Genuine operator sessions retain local recovery access.
- Route metadata and real-router/source-inventory tests freeze the canonical alias, legacy route,
  positive session boundary, bearer scope, cursor behavior, and frames.

## Frontend connection and recovery

- `frontend/src/operations/durableEvents.ts` is one lazy shared EventSource multiplexer. It stores
  only connection/cursor state and subscribers; it does not cache jobs, approvals, mutations,
  plans, or results.
- It validates ready/gap payloads, complete durable envelopes, SSE-ID equality, and strict
  monotonic sequence delivery.
- Ordinary transport failures rely on EventSource `Last-Event-ID` reconnect. Server or locally
  detected gaps close the invalid source, publish unhealthy state, and reconnect from a safe cursor
  only after subscribers have begun authoritative recovery.
- Each operation hook establishes an HTTP barrier read after `stream.ready` before its interval may
  pause.
- Job/approval lists invalidate broadly by identity presence; detail hooks invalidate only for the
  exact selected ID. Associated job detail beside an approval retains its independent exact job
  subscription.
- Relevant event bursts coalesce into one active plus one pending full read.
- Event-read failure clears readiness and resumes bounded polling. Explicit gaps cause immediate
  full reads; disconnects resume the established bounded interval; visibility return always forces
  recovery.
- The original 20-minute foreground HTTP deadline is never extended. Manual refresh remains
  available. Terminal-state and `needs_attention` behavior remains unchanged.
- Events never derive state and never submit, cancel, approve, reject, retry, or replay a mutation.
- Page-local provider trackers retain terminal-success refresh ownership.

## Compatibility and documentation

- The Odysseus manifest now advertises durable ready/event/gap frames, cursor inputs, and the legacy
  URL.
- `docs/api.md`, `docs/api-tokens.md`, `docs/integrations/odysseus.md`, the Integrations quick-start,
  README, and ROADMAP describe the durable default, recovery rule, role/scope boundary, and legacy
  migration.
- ROADMAP now marks J0-01/J0-03 and durable event delivery complete. The next foundation checkpoint
  is the CMDB/Asset Registry.
- The design and implementation plan remain ignored local files at:
  - `docs/internal/specs/2026-08-30-durable-event-sse-design.md`
  - `docs/internal/plans/2026-08-30-durable-event-sse-implementation.md`
  They were committed for review checkpoints and removed from the tracked final tree per repository
  hygiene policy.

## Verification

- `cargo test --all-targets --all-features`: 351 unit tests and 2 golden-path tests passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `npm test`: 25 tests across 11 files passed.
- `npm run type-check`: passed.
- `npm run lint`: passed with zero warnings.
- `npm run build`: passed. Only the existing Vite dynamic-import and large-chunk advisories remain.
- Schema migration ownership: passed.
- Repository hygiene: passed with 541 tracked files checked.
- `git diff --check` and staged diff check: passed.
- Local `gitleaks` was unavailable; the repository's pinned CI gitleaks action remains authoritative.

Repository-wide Rust formatting was not applied because `api/mod.rs` recursively exposes established
unrelated formatting drift. New and focused Rust files were formatted directly; full strict Clippy
passed.

## Recommended next slice: CMDB/Asset Registry vertical foundation

The completed J0 boundaries now support a meaningfully larger slice. Prefer one end-to-end CMDB
foundation rather than another protocol-sized increment:

1. Inventory existing `resources`, aliases, capabilities, nodes, tags, provider reads, and every UI
   place that identifies the same physical/logical asset differently.
2. Specify the canonical asset/resource projection, lifecycle, ownership, discovery provenance,
   merge/alias rules, and stale/retired behavior without duplicating J0 resources.
3. Deliver persistence/projection changes, canonical API list/detail/search, Tower and Void Mode
   inventory/detail UX, durable-event invalidation, and initial Docker/Proxmox/local-node adoption
   together.
4. Keep mutation controls on the existing typed action/job/approval boundary; CMDB records describe
   and route capabilities but do not become a new execution path.
5. Add migration, reconciliation, duplicate-identity, authorization, redaction, source-inventory,
   and complete verification coverage before committing.

This is larger than the preceding closure slices but still has one coherent product outcome: a
canonical inventory users can browse and trust across providers.
