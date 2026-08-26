# Shared Jobs and Approvals UX

Date: 2026-08-26
Status: Approved
Scope: J0-01/J0-03 shared Tower and Void Mode job/approval workflows over bounded HTTP polling

## Purpose

Give operators one cross-domain place to inspect and cancel durable jobs, and give administrators
one exact-record workflow for reviewing, approving, and rejecting pending approvals. The same
canonical records must be available in Tower and Void Mode, and every page-local durable notice
must lead to the shared job detail destination.

This checkpoint uses the existing durable job and approval persistence, serializers, authorization,
worker state machine, and HTTP routes. It does not couple the UI to the later durable-event SSE
checkpoint or create a second frontend operation store.

## Fixed decisions

- Add separate Tower destinations at `/jobs`, `/jobs/:id`, `/approvals`, and `/approvals/:id`.
- Jobs are visible to the existing positive operator allowlist: `owner`, `admin`, and `operator`.
- Approvals are visible only to the existing positive admin allowlist: `owner` and `admin`.
- Backend guards remain authoritative. Frontend route and navigation checks are usability controls,
  not a replacement authorization boundary.
- Add native Void Mode Jobs and Approvals panels. The Approvals panel and navigation entry are
  absent for non-admin roles.
- Use bounded HTTP polling. Durable event SSE, `Last-Event-ID`, cursor recovery, and an integrations
  stream alias remain the immediately following J0 slice.
- Keep page-local durable trackers and their terminal-success provider refresh callbacks. Shared
  pages do not replace or globally replay those callbacks.
- Link every page-local `DurableJobNotice` to `/jobs/:id`. In Void Mode, the same URL opens and
  selects the record in the native Jobs panel.
- Never automatically submit, cancel, approve, reject, retry, or resubmit a mutation.
- Do not rewrite `ChangePlanModal`. Existing compatibility previews remain advisory confirmations,
  not durable approvals.
- Do not add provider actions, CMDB behavior, automatic approval, automatic retry, or a transient
  operation store.
- Do not push local commits.

## Approaches considered

### Recommended: thin shared workflows over existing HTTP contracts

Add complete client contracts, focused polling hooks, reusable safe presentation, separate Tower
pages, and compact native panels. This fits the current backend, preserves the established
authorization split, and can later replace polling with durable SSE without replacing the pages or
their data model.

### Rejected: combined Operations center

Put Jobs and Approvals behind one route with tabs. This reduces navigation entries, but obscures the
admin-only approval boundary and conflicts with the deliberately separate destinations selected for
this slice.

### Rejected: global background job store

Move all job tracking and notifications into a persistent global client store. That could support
cross-page background following, but it duplicates lifecycle and recovery machinery immediately
before the durable SSE checkpoint. Page-local completion callbacks would also be difficult to
preserve without creating a second transient operation model.

## Existing backend contract

The backend already exposes:

- `GET /api/jobs?limit=<n>` for operator sessions;
- `GET /api/jobs/:id` for operator sessions;
- `POST /api/jobs/:id/cancel` for canonical action-authorized credentials, limited to queued or
  running jobs, with bearer callers additionally limited to their own job;
- `GET /api/approvals?status=<status>&limit=<n>` for admin sessions;
- `GET /api/approvals/:id` for admin sessions; and
- `POST /api/approvals/:id/approve` and `/reject` for admin sessions with an optional comment.

`JobSummaryV1` is a complete serialized job despite its legacy name. It contains identity, action,
resource, actor, ingress, state, progress, immutable plan, approval reference, result, typed error,
and timestamps. `ApprovalViewV1` contains the immutable approval identity, associated job, policy
requirement and reason, status, expiry, decision metadata, and timestamps.

The UI does not add unsupported pagination, ownership, or state-transition semantics. The Jobs page
shows at most the newest 50 records returned by the current list endpoint. Operators see the
backend's current all-jobs view. Approval status filtering uses the existing server query.

## Frontend contracts and API client

Add TypeScript shapes that mirror the complete serializers:

- `DurableResourceRef`
- `DurableActorRef` and `DurableActorType`
- `DurablePlanChange`, `DurablePlannedStep`, and `DurableOperationPlan`
- `DurableOperationError`
- `DurableJob`
- `DurableApproval` and `DurableApprovalStatus`
- list, detail, decision, and cancellation response types

Keep `DurableJobSummary` as a compatibility alias for `DurableJob` during this slice. Existing
adopted domain callers can continue using their current annotations while receiving the complete
record. New shared code uses `DurableJob` to avoid implying that the API returns a narrow summary.

Extend `api.operationJobs` with `list`, `get`, and `cancel`. Add `api.approvals` with `list`, `get`,
`approve`, and `reject`. Query construction must omit absent parameters and encode supplied status
values. Decision methods always send an explicit JSON object containing the optional trimmed
comment. Mutation methods are invoked once per operator gesture.

## Polling and read-state model

Create focused hooks over a small reusable bounded-polling primitive:

- `useJobList`
- `useJobDetail`
- `useApprovalList`
- `useApprovalDetail`

Job detail polls every two seconds only while the confirmed job state is `awaiting_approval`,
`queued`, or `running`. Jobs lists poll every five seconds only while they contain at least one of
those states. Approval lists and details poll every five seconds while a confirmed approval is
`pending`. Polling uses the existing 20-minute foreground limit.

The hooks:

- retain the last confirmed record during a transient read failure;
- expose loading, refreshing, stale, and error state separately;
- pause timers while the document is hidden;
- perform a full-record refetch when visibility returns;
- perform a full-record refetch after any polling gap or ambiguous mutation outcome;
- stop timers on terminal job or non-pending approval state, unmount, or foreground deadline; and
- expose manual refresh without extending the original automatic polling deadline.

No hook retries a mutation. Read polling may continue after a failed read until its fixed deadline,
but it never infers a transition from elapsed time or transport recovery. `needs_attention` is not
terminal in the backend state machine, but it leaves automatic frontend following because it needs
manual investigation rather than another automatic action. A later reconciliation may move it to
`succeeded` or `failed`; manual refresh always remains available.

## Shared presentation components

Add reusable components for:

- state label and tone;
- progress current/total/message;
- action, resource, actor, ingress, and timestamps;
- typed plan title, risk, changes, preview, and steps;
- typed error code, safe message, and retryability;
- bounded structured result presentation; and
- stale/loading/empty/forbidden states.

The result renderer is defensive even though server-side redaction is authoritative. It has fixed
limits for nesting depth, object keys, array items, and string length. Keys matching secret-like
names are masked. Truncation is explicit. It uses React text rendering only and never injects HTML.
The UI does not render job input because it is not part of the public job serializer, and it does
not use an unrestricted `JSON.stringify` dump for plans or results.

Plans use their typed fields. Preview text is length-bounded. Errors use only the typed public error
contract. Actor and resource identifiers are shown as identifiers, not interpreted as markup.
Approval comments are plain text with a visible 500-character client-side length limit.

## Tower Jobs workflow

`/jobs` provides:

- newest-first records, initially limited to 50;
- client-side state and action filtering over the loaded window;
- a manual refresh action;
- state, progress, action, resource, actor/ingress, submitted time, and short job identity; and
- links to `/jobs/:id`.

`/jobs/:id` provides the complete safe presentation and a back link to the list. It polls only while
the job is actively followed. A Cancel action appears only for `queued` and `running` states. The
button requires explicit confirmation, disables while the request is in flight, and calls the
selected job's cancellation endpoint once.

On a successful cancellation response, the returned job becomes the confirmed detail. On a
transport error or conflict, the page refetches the selected job before reporting its state. It
does not optimistically label a job cancelled. Awaiting-approval jobs cannot be cancelled through
the current backend contract, so the Jobs page does not offer that action.

`needs_attention` is styled as unresolved non-success and explains that provider state needs manual
investigation. The page does not invent a resolve, retry, or resubmit button.

## Tower Approvals workflow

`/approvals` defaults to pending approvals and supports explicit pending, approved, rejected,
expired, stale, and all filters backed by the existing status query. Each row shows the policy
reason, requirement, expiry or decision time, associated job ID, and status, and links to
`/approvals/:id`.

`/approvals/:id` fetches both the exact approval and its associated job. The job provides the action,
resource, actor, immutable plan, progress, and current state needed for an informed decision. The
page does not reconstruct a plan from current provider state.

Approve and Reject are available only while the confirmed approval status is `pending`. Both act
on the approval ID in the URL, accept an optional plain-text comment, share one in-flight lock, and
are invoked once. The controls identify the decision explicitly and do not treat closing the page
as rejection.

After a decision response, the returned job is stored and both approval and job are refetched. If
the request conflicts because another administrator decided it, the approval expired, or evidence
became stale, the page refetches both records and presents their authoritative states. It never
reports a guessed success. Machine actors cannot access these session-only endpoints and no
automatic approval path is added.

## Navigation and route authorization

Add Jobs and Approvals under the Tower Ops group and the customizable navigation defaults. Nav item
definitions gain positive role metadata rather than scattered label checks. Jobs allow owner,
admin, and operator; Approvals allow owner and admin. Stored navigation configuration may reorder,
rename, or hide an allowed item, but cannot make a role-forbidden item visible.

Add equivalent role metadata to Void Mode dock and command-palette items. Filtering happens after
stored navigation is resolved so a persisted configuration cannot restore a forbidden item.

Add a small authenticated route guard for direct browser visits. It uses the same positive role
sets and renders a clear forbidden state in place; it does not silently redirect a forbidden
approval URL. This is only a client UX boundary; every data request remains protected by backend
authorization.

## Void Mode workflow and deep links

Add compact native Jobs and Approvals panels to `NATIVE_PANEL_REGISTRY`. They reuse the shared
contracts, polling hooks, state/progress primitives, safe result renderer, and mutation functions,
but follow `NativePanelShell` and `NativeRow` conventions rather than embedding Tower pages.

The Jobs panel shows a compact newest-first list and an internal detail state. The Approvals panel
defaults to pending and shows the associated job before enabling a decision. Both panels use a
single list-to-detail transition at every width so selection, back behavior, and deep links have one
deterministic interaction model.

Canonical URLs remain `/jobs/:id` and `/approvals/:id`. When `AiosLayout` is active, a route bridge
recognizes those paths, opens the corresponding native panel, and selects the ID from the URL. The
native panels update the same URL when the selected record changes. This lets bookmarks and
`DurableJobNotice` links behave consistently in both UI modes without introducing a Void-only
identity or operation store.

## Page-local durable notices

`DurableJobNotice` keeps its current label, state, progress indication, and tracking ownership. It
adds a semantic link to `/jobs/:id`. Existing `useDurableJobTracker` and
`useDurableJobBatchTracker` completion behavior remains unchanged: a domain page refreshes provider
reads only after that locally followed job reaches conclusive success.

Opening shared detail never re-registers the job with a domain tracker and never replays the domain
callback. For Proxmox batches, each visible job identity links to its individual shared detail;
batch aggregation remains page-local.

## Error and ambiguity behavior

- Initial `401` follows the existing authenticated-shell behavior.
- `403` renders a role-appropriate forbidden state and no action controls.
- `404` renders a missing-record state without retry or resubmission.
- Transient read failures preserve last confirmed data, mark it stale, and permit manual refresh.
- A polling deadline displays that automatic foreground tracking stopped; durable execution is not
  described as stopped.
- Cancellation conflicts refetch the job and show its current state.
- Approval conflicts refetch the approval and associated job and show their current states.
- Terminal failure, rejection, cancellation, expiry, and `needs_attention` are never styled or
  announced as success.
- Only `succeeded` is conclusive success.

## Testing

### Backend route and state coverage

Add real-router coverage for:

- Jobs list/detail access by owner, admin, and operator, and denial for viewer, guest, demo, member,
  unknown, and unauthenticated callers.
- Approval list/detail/approve/reject access by owner and admin, and denial for every other role and
  unauthenticated callers.
- Cancellation only from queued and running jobs.
- Approval decisions affecting only the selected immutable approval record.
- Approved, rejected, expired, and stale conflicts returning authoritative failure rather than a
  second transition.
- Returned job and approval serializer shapes used by the frontend contracts.

Existing action-registry and worker tests remain authoritative for canonical action authorization,
bearer job ownership, recovery, and state-transition legality.

### Frontend behavior coverage

Add the smallest Vitest, jsdom, and Testing Library setup needed for this shared workflow. Cover:

- active detail/list polling and terminal stopping;
- the fixed foreground deadline;
- hidden-document pause and full refetch on visibility return;
- preservation and stale marking after transient reads;
- a single cancellation request per gesture and detail refetch after ambiguity;
- a single exact-ID approval decision per gesture and approval/job refetch after ambiguity;
- absence of actions in unsupported states and roles;
- safe bounded value rendering and secret-like key masking;
- canonical job-detail links from page-local notices; and
- Tower and Void deep-link selection behavior.

Add source-inventory assertions for Tower routes, positive role-filtered navigation, native panel
registration, and page-local notice linkage so future surface changes cannot silently drop shared
job access.

## Verification gate

Before the implementation commit:

1. Run the focused backend job, approval, route-authorization, state, and source-inventory tests.
2. Run frontend behavior tests.
3. Run `cargo test --all-targets --all-features`.
4. Run `cargo clippy --all-targets --all-features -- -D warnings`.
5. Run `npm run type-check` in `frontend`.
6. Run `npm run lint` in `frontend`.
7. Run `npm run build` in `frontend`.
8. Run migration ownership and repository-hygiene checks.
9. Run `git diff --check` and the staged equivalent.

Repository-wide Rust formatting remains out of scope because the crate has established unrelated
formatting drift. Format only touched Rust files where that does not rewrite user-owned work.

## Acceptance criteria

- Operators can discover, list, inspect, refresh, deep-link, and cancel eligible durable jobs from
  Tower and Void Mode.
- Admins can discover, inspect, approve, and reject the exact immutable approval record from Tower
  and Void Mode with an optional comment.
- Non-admin operators cannot discover or invoke approval UI; backend authorization still rejects
  direct requests.
- Every current page-local durable notice links to the canonical shared job detail destination
  without changing its terminal-success refresh ownership.
- Polling is bounded, visibility-aware, read-only, and unambiguously separate from mutation retry.
- Ambiguous mutation outcomes are resolved by refetching authoritative records, never by optimistic
  success or automatic resubmission.
- Inputs, plans, results, errors, and comments use typed, bounded, redacted-safe presentation.
- No durable SSE, provider action, CMDB behavior, `ChangePlanModal` migration, automatic approval,
  or second transient operation store lands in this slice.
