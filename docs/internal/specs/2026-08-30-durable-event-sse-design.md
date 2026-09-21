# Cursor-Resumable Durable Event SSE

Date: 2026-08-30
Status: Approved
Scope: Final J0 durable-event delivery slice for shared Jobs and Approvals

## Purpose

Replace VoidTower's two transient SSE implementations with one cursor-resumable stream over the
existing durable `events` table. The canonical `/api/events/stream` route and the Odysseus-facing
`/api/integrations/events` route must expose the same ordered event history, cursor semantics,
authorization, and wire representation. Shared Jobs and Approvals may then use the stream as an
invalidation channel while continuing to treat their HTTP list and detail resources as
authoritative.

This slice completes live delivery for the existing J0 operation model. It does not create another
event table, an in-memory replay history, a client-side operation store, or any automatic mutation
retry. It does not turn transient metrics snapshots into durable operation events.

## Fixed decisions

- `/api/events/stream` becomes the canonical durable SSE route.
- `/api/integrations/events` becomes an exact delivery alias for the canonical durable route.
- The current integrations metrics/audit/ping stream moves to
  `/api/integrations/events/legacy` as an explicitly transitional compatibility route.
- The current `/api/events/stream` metrics-threshold/systemd polling implementation is removed. Its
  transient event shapes are not mixed into the durable operation stream.
- A first connection with neither `after` nor `Last-Event-ID` starts at the current durable
  high-water mark and receives only later events.
- A caller requests retained replay explicitly with `after`; `after=0` requests the complete
  retained history.
- If both cursor sources are present, the effective cursor is the greater valid value. A browser
  reconnect therefore cannot rewind to the original query cursor after it has received newer SSE
  event IDs.
- Durable frames reuse `EventEnvelopeV1` without a second public event representation.
- SSE is notification and invalidation transport only. Jobs, approvals, and their mutation
  responses remain authoritative HTTP resources.
- Bounded HTTP polling remains the frontend fallback whenever the stream is not connected, has not
  completed its ready barrier, or reports a cursor gap.
- No event causes the client to submit, cancel, approve, reject, retry, or resubmit a mutation.
- Do not push local commits.

## Approaches considered

### Recommended: one durable delivery core with an explicit legacy route

Build one authenticated, cursor-aware durable stream implementation and mount it at both public
durable URLs. Keep only the existing integrations transient feed at a clearly named legacy route.
This makes the default contract unambiguous, prevents alias drift, and preserves a bounded
migration path for integrations that still consume `metrics`, `alert`, `audit`, or `ping` frames.

### Rejected: select durable or legacy behavior with a query flag

A `mode=durable` or `legacy=true` parameter would preserve one route, but it would leave external
consumers dependent on a mixed endpoint whose default semantics are difficult to evolve and
document. It would also make route tests and the Odysseus manifest less clear.

### Rejected: persist transient metrics and audit polling output

Writing every metrics sample, threshold repeat, or audit projection into the operation event log
would produce a high-volume history with different retention and privacy needs. It would expand J0
beyond operation transitions and duplicate existing metrics and audit persistence. Those domains
can adopt typed durable events later if a separate approved contract requires them.

## Existing source of truth

Migration `0002_operation_contracts.sql` owns the append-only `events` table. Its autoincrement
`sequence` is the ordered cursor. Each committed row serializes as `EventEnvelopeV1`, containing:

- `sequence`, `event_id`, `schema_version`, `event_type`, and `occurred_at`;
- optional typed actor, resource, job, and approval references;
- correlation and optional causation identity; and
- a bounded, redacted payload written by the operation transaction.

`operations::events::append` writes an event in the same transaction as its state transition, so a
rolled-back transition does not become observable. `list_after` already returns committed rows in
ascending sequence order with a clamped upper bound. The SSE implementation extends this module
with cursor-bound reads; it does not introduce another persistence owner.

There is currently no automatic event-retention deletion. Gap handling is still required so the
wire contract remains safe if retention is introduced later, an operator restores a database, or
history is otherwise discontinuous.

## Durable stream request contract

Both durable routes accept:

- a session cookie or bearer credential in the `Authorization` header; query-string bearer tokens
  are rejected, including for browser `EventSource` clients that cannot set custom headers;
- optional `after=<non-negative sequence>`; and
- the standard optional `Last-Event-ID: <non-negative sequence>` header.

Missing cursors select the current maximum committed sequence. An empty table has high-water mark
zero. When either cursor is supplied, the effective cursor is the maximum supplied value. Empty,
negative, non-integer, or overflowed cursor values return `400 bad_request` before the SSE response
begins.

The server reads the retained minimum and maximum sequence before accepting replay:

- zero is valid when the first retained sequence is one;
- a cursor below `minimum - 1` is a behind-retention gap;
- a cursor above the current maximum is a future-cursor gap; and
- an empty table accepts only cursor zero.

A valid connection begins with a `stream.ready` control event containing the effective cursor and
current high-water mark. The server then emits every committed event whose sequence is greater than
the cursor, in ascending order, before following new commits.

## SSE wire contract

Durable rows use one representation:

```text
id: <EventEnvelopeV1.sequence>
event: durable_event
data: <complete JSON-serialized EventEnvelopeV1>
```

The `id` and envelope `sequence` must match. The client rejects malformed frames, mismatched IDs,
and non-monotonic delivery as a local stream gap. Event type remains inside the envelope so clients
subscribe to one stable SSE event name rather than registering every operation event name.

Control frames do not claim durable identities and therefore carry no SSE `id`:

- `stream.ready` contains `{ "cursor", "high_water" }`;
- `stream.gap` contains `{ "reason", "requested_after", "earliest_available",
  "latest_available" }`.

Keepalive comments are sent every 15 seconds and do not advance the cursor. The server may include
an SSE retry hint, but correctness does not depend on a particular browser reconnect delay.

## Ordered delivery and backpressure

The delivery task reads at most 100 rows at a time. A full batch is followed immediately by the
next bounded read until the cursor catches the high-water mark; an empty read waits for the fixed
short follow interval before checking again. This is database polling over durable history, not
the legacy metrics/systemd polling stream.

Frames pass through a bounded channel. Sending awaits channel capacity, so a slow client applies
backpressure instead of causing an unbounded allocation or dropping a durable frame. Receiver
closure cancels the delivery task. Database read failure terminates the connection; reconnect and
the last delivered SSE ID resume from the last confirmed row.

Before each row is sent, its sequence must equal the previous cursor plus one. A discontinuity
emits `stream.gap` and closes the stream. No later row is delivered across that gap. Although SQLite
rollback does not publish an event, the runtime does not assume sequence continuity across deleted
or restored history.

## Cursor-gap recovery

An invalid retained range is communicated as `stream.gap` and the connection closes without a
`stream.ready` frame. The frame includes the current retained bounds so a client can distinguish a
behind-retention cursor from a future cursor.

The VoidTower browser client responds by:

1. marking the stream unhealthy and keeping or resuming bounded HTTP polling;
2. refetching each subscribed job or approval list/detail in full;
3. discarding the invalid local cursor;
4. creating a fresh EventSource at the reported latest high-water mark; and
5. considering the stream healthy only after the replacement connection emits `stream.ready` and
   its authoritative refetch barrier completes.

External integrations must follow the same principle for their own resources: recover full state,
then reconnect from a known high-water mark. Neither the server nor client synthesizes missed
resource transitions from later events.

## Authorization and compatibility policy

Human session access to the durable stream uses the same positive operation visibility boundary as
shared Jobs: owner, admin, or operator. Viewer and unknown roles are denied. The API token
compatibility boundary remains `alerts:read` for this slice so existing Odysseus tokens can migrate
without a new capability rollout.

Session cookies and the Authorization Bearer scheme are parsed by one shared authentication helper.
The previously proposed query-string bearer-token compatibility path is superseded: query-string tokens are
rejected to prevent credential leakage through URLs, logs, and referrers. Durable event delivery
and cursor handling are implemented once and mounted at both durable routes; the integrations alias
does not own a second history loop
or event transformation.

Emergency disable rejects API-token stream connections on either durable URL and on the legacy
integrations URL. Human session access remains available for local recovery and inspection. This
prevents an integration from bypassing emergency disable by changing from the integrations alias
to the canonical path.

The legacy route keeps the current integrations transient frame names and shapes: `metrics`,
`alert`, `audit`, and `ping`. Its manifest and documentation mark it deprecated and separate from
the durable cursor contract. The removed canonical `high_cpu`, `high_memory`, `disk_nearly_full`,
and `service_failed` feed does not receive another alias because no frontend consumer or documented
integration contract uses it.

## Frontend connection multiplexer

Add one small browser durable-event client shared by the Jobs and Approvals hooks. It owns only:

- the active EventSource;
- connection state (`connecting`, `ready`, `disconnected`, or `gap`);
- the last validated sequence for the current connection; and
- subscriptions to validated envelopes and connection-state changes.

It does not cache jobs, approvals, plans, results, or mutation state. It starts lazily when the
first operation hook subscribes and closes when the final subscriber leaves. Browser session-cookie
authentication is used; frontend code never puts a session credential in the URL.

The client validates ready, durable, and gap frames. `ready` alone does not pause fallback polling.
Each subscriber first completes an authoritative HTTP refetch; only then is that subscriber
stream-ready. A disconnect, parse failure, sequence mismatch, or gap clears readiness immediately.
Native EventSource reconnect supplies `Last-Event-ID` after ordinary transport failures. An
explicit gap closes that instance and creates a new connection from the server-reported latest
cursor only after full-record recovery has started.

## Jobs and Approvals invalidation flow

The existing bounded-polling primitive gains an optional invalidation source and relevant-event
predicate. Its confirmed-data, stale-read, visibility, foreground-deadline, manual-refresh, and
mutation-acceptance semantics remain intact.

Relevant durable events are intentionally broad:

- a job list refetches for any envelope with a `job_id`;
- a job detail refetches when `job_id` matches the selected job;
- an approval list refetches for any envelope with an `approval_id`;
- an approval detail refetches when `approval_id` matches the selected approval; and
- the existing associated `useJobDetail` subscription refreshes the job shown beside an approval.

Events are invalidations, so predicates do not derive state from `event_type` or payload. Bursts are
coalesced into at most one active refetch plus one pending refetch, preventing simultaneous reads
from repeated transition events. A successful full read replaces the confirmed record. A failed
read preserves prior confirmed data and marks it stale exactly as bounded polling does today.

While a subscriber is stream-ready, its interval timer is paused. A stream error or gap schedules
the existing two-second job-detail or five-second list/approval fallback without extending the
original 20-minute foreground deadline. Returning from a hidden document always forces a full
refetch before readiness is restored because browser delivery may have been throttled.

The foreground deadline bounds automatic HTTP following, not the stream connection or durable
execution. Manual refresh remains available after the deadline. Terminal records still stop
unneeded fallback polling; a relevant durable invalidation may nevertheless refetch them because a
later reconciliation can change `needs_attention` and related records.

## Mutation behavior

Cancellation and approval decisions remain single-shot operations invoked only by an explicit user
gesture. A successful mutation response is accepted immediately. Ambiguous errors and conflicts
still refetch authoritative detail. An event arriving during a mutation may coalesce a read, but it
never retries or replays the mutation and never causes optimistic success.

Provider-page local trackers retain terminal-success refresh ownership. Shared SSE invalidation
does not register a durable job with those trackers or replay a provider completion callback.

## Error behavior

- Authentication and role failures occur before the SSE response and use existing structured HTTP
  errors.
- Invalid cursors return `400 bad_request`.
- Retention/future/discontinuity gaps use `stream.gap`, close delivery, and require full-record
  recovery.
- Database and transport failures close the stream; the browser resumes bounded HTTP polling while
  EventSource reconnects.
- Malformed or non-monotonic client frames are treated as gaps, never as state transitions.
- Read failures preserve the last confirmed job or approval and mark it stale.
- No stream condition disables manual refresh or creates a mutation control.

## Backend implementation boundaries

- Extend `operations/events.rs` with retained bounds and focused cursor helpers.
- Replace `api/events.rs` transient polling with shared durable authentication, cursor parsing, and
  delivery.
- Route `/api/integrations/events` to the shared durable handler.
- Keep only the legacy integrations producer in `api/integrations.rs`, mounted at the new legacy
  path.
- Update action-registry metadata so both durable routes freeze the operator-session and
  `alerts:read` bearer boundary, and register the legacy route explicitly.
- Do not add a schema migration unless implementation reveals a missing index required by measured
  query behavior; the existing primary-key order supports `sequence > ? ORDER BY sequence`.

## Documentation

Update:

- `docs/api.md` with durable request, frame, cursor, gap, and alias behavior;
- `docs/api-tokens.md` with durable stream authentication, role/scope, and emergency-disable
  behavior;
- `docs/integrations/odysseus.md` and the manifest with the durable default and legacy migration
  route; and
- `ROADMAP.md` to mark durable event delivery complete only after the full verification gate
  passes.

Documentation must not describe SSE as a source of authoritative job state or imply that a client
may replay mutations after reconnect.

## Testing

### Durable persistence and cursor helpers

- Retained bounds are correct for empty and populated histories.
- Ordered bounded listing remains resumable.
- Cursor selection covers absent sources, explicit zero, header-only, query-only, and the maximum
  of both sources.
- Invalid, negative, and overflowed cursors fail closed.
- Behind-retention, future, and internal discontinuity gaps are detected.

### Real router and delivery

- Owner, admin, and operator sessions can connect; viewer, unauthenticated, and unknown roles
  cannot.
- An `alerts:read` token can connect; missing-scope, invalid, revoked, and emergency-disabled token
  connections cannot.
- Canonical and integrations durable routes emit identical ready and durable frames for the same
  cursor.
- No-cursor startup is live-only; `after=0` replays retained history.
- `Last-Event-ID` resumes after the last delivered row and combines monotonically with `after`.
- SSE IDs equal envelope sequences and delivery remains ordered across multiple bounded batches.
- Gap frames contain retained bounds and no durable event is emitted across a gap.
- A slow/closed receiver remains bounded and terminates producer work.
- The legacy integrations route retains its documented transient event names and emergency-disable
  policy.

### Frontend event client and hooks

- The shared client validates ready, durable, malformed, non-monotonic, error, and gap behavior.
- Each operation hook performs a full-read barrier before pausing fallback polling.
- Relevant events refetch the correct list/detail; unrelated identities do not.
- Event bursts coalesce reads.
- Disconnect and gap resume the existing bounded intervals without extending their deadline.
- Gap and visibility recovery perform authoritative full refetches.
- Existing stale-read preservation, terminal-state polling, manual refresh, mutation acceptance,
  cancellation, and exact approval-decision tests continue to pass.
- Source inventory freezes both durable routes, the legacy route, positive role metadata, aliasing,
  and shared Jobs/Approvals subscriptions.

### Complete verification gate

- `cargo test --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- focused frontend tests and then `npm test`
- `npm run type-check`
- `npm run lint`
- `npm run build`
- migration ownership and repository-hygiene checks
- `git diff --check` and staged diff checks
- the repository's pinned CI secret scan, with local `gitleaks` used when available

## Acceptance criteria

The slice is complete when both durable URLs deliver the same ordered, cursor-resumable
`EventEnvelopeV1` stream; cursor gaps are explicit and recover through authoritative full reads;
Jobs and Approvals use durable invalidations while retaining bounded polling as a safe fallback;
legacy transient integrations consumers have a documented migration route; no event can replay a
mutation; and the complete verification gate passes.
