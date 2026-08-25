# Cross-Domain Durable-Operation Bypass Closure

Date: 2026-08-25
Status: Approved by standing recommendation authority
Scope: J0-01/J0-03 six-domain bypass closure and public asynchronous mutation contract

## Purpose

Close the remaining ways an adopted Containers, Firewall, Proxy, Updates, Backups, or Proxmox
operation can be invoked without its canonical durable job, even when the caller lives outside the
domain's primary compatibility module. Replace acceptance-as-success behavior in every current
six-domain frontend surface, establish one executable repository-wide source inventory, and publish
the asynchronous API contract.

This checkpoint does not invent durable actions for unrelated domains. It records the remaining
App Vault, model, settings, service, and automation mutations as exact bounded exceptions so new
cross-domain bypasses fail CI instead of silently expanding that legacy surface.

## Fixed decisions

- Preserve the 51-action six-domain runtime registry. Reuse existing durable actions instead of
  adding App Vault, model, service, settings, or automation lifecycle actions in this slice.
- Expand the adopted compatibility inventory from 46 to 48 routes by adding App Vault proxy
  exposure and the container-action branch of the Odysseus webhook route.
- Add a typed webhook invocation context. It has a stable automation actor and idempotency scope,
  enters only actions that explicitly declare webhook ingress, and derives policy through the same
  canonical invocation service as HTTP, CLI, and scheduler callers.
- Map webhook container start, stop, and restart to `container.{start,stop,restart}`. Service and
  arbitrary automation branches retain their existing direct contracts and remain outside the six
  adopted domains.
- Convert `POST /api/apps/:project_name/expose` to `proxy.rule.create`. It returns the canonical
  `202 {job}` response and no longer writes `proxy_configs`, nginx configuration, or audit success
  from the handler.
- Make `POST /api/apps/open-ui` an informational compatibility lookup. It may return a direct URL,
  an existing embed URL, and whether an existing proxy is usable, but may not create/update proxy
  rows, write nginx files, reload nginx, open firewall ports, or spawn mutation work.
- Keep Proxmox VNC ticket creation as the existing explicit ephemeral exception.
- Keep compatibility dry runs as advisory adapter plans. Confirmation submits a durable job; it is
  not provider execution and does not constitute approval of a future unrelated request.
- Every current frontend caller of an adopted mutation must register the returned job, show its
  canonical state/ID, and refresh provider data only after conclusive success. It may not infer
  success from `202`, network recovery, or a later read.
- Do not push local commits.

## Approaches considered

### Recommended: bounded closure plus executable exception ledger

Adopt the two cross-domain callers already expressible by existing actions, remove the hidden
open-UI mutation, follow jobs in all current six-domain surfaces, and freeze every deferred direct
provider call in an exact source inventory. This closes demonstrated J0 bypasses without smuggling
several new product domains into one security checkpoint.

### Rejected: inventory only

Add tests and documentation without changing callers. This would make the remaining bypasses
visible but would retain direct Docker execution from the webhook and direct nginx execution from
App Vault exposure.

### Rejected: adopt every provider call now

Create durable actions for all App Vault and model Compose lifecycle operations, AI proxy settings,
service control, and arbitrary automation. That is the long-term direction, but each needs its own
resource identity, typed input, recovery, approval, frontend, and compatibility design. Combining
them here would make the closure boundary unreviewable.

## Backend architecture

### Webhook invocation context

`operations::invocation::InvocationContext` gains a `Webhook` variant carrying a stable source ID.
It maps to `ActionIngress::Webhook`, `ActorType::Automation`, `ActorKind::Automation`, a stable
`v1:webhook:automation:<source>` idempotency scope, and an `odysseus_webhook` source label. It does
not inherit a human role or bearer scopes. Authorization succeeds only when the selected durable
action explicitly includes webhook ingress and is AI-callable.

The three durable container lifecycle actions add webhook ingress without broadening their HTTP
session or bearer policies. The webhook handler authenticates the secret first, selects the exact
canonical action, authorizes before Docker evidence or resource observation, resolves the requested
container through the same normalized container evidence as the compatibility route, and either:

- returns a canonical advisory plan for `dry_run: true`; or
- submits with the incoming `Idempotency-Key`, or a generated compatibility key when absent.

The canonical invocation service owns Voidwatch evaluation, approval state, immutable planning,
job persistence, audit/event history, and policy-denied job identity. The container branch removes
its duplicate manual policy evaluation, direct provider call, and success audit. The service and
automation branches retain their existing behavior and metadata.

### Shared container resolution

The normalized container selector logic in `api::containers` becomes a focused reusable resolver.
It accepts a typed invocation context and selected action, authorizes before checking Docker, lists
read-only provider evidence, canonicalizes exact name/full ID/unambiguous short ID, observes the
`docker.container/local-engine/<full-id>` resource, and publishes only the selected capability.
Both the session/bearer compatibility route and webhook branch use it.

### App Vault proxy exposure

The proxy module exposes a narrow internal submission helper that accepts an already-authenticated
credential, headers, and canonical `CreateRequest`. It authorizes, verifies provider availability,
resolves the seeded reverse-proxy service, canonicalizes the input, and submits or plans. The normal
proxy create route and App Vault exposure route use the same helper.

App Vault exposure still verifies the project and derives its configured local upstream. It then
submits `proxy.rule.create`; it does not persist a proxy ID or report exposure success before the
worker completes. The obsolete `create_proxy_record` route-side mutation helper is removed.

### Read-only app URL lookup

The app-UI endpoint computes the browser-facing direct URL and reads an existing embed proxy row.
When an existing enabled row has an allocated embed port and its upstream matches the requested app
port, it returns that embed URL. Otherwise it returns no embed URL and the frontend falls back to
the existing backend embed proxy. No reconciliation or repair is hidden in this read-shaped call.
Creating a public reverse proxy remains the explicit App Vault exposure action.

## Executable repository inventory

A central source-boundary test owns four exact inventories:

1. the 48 adopted route keys and their registered canonical actions;
2. the handler/helper source regions that must reach canonical prepare or submit;
3. direct six-domain provider mutation symbols and their permitted adapter callsites; and
4. deferred cross-domain exception callsites with a fixed reason and exact count.

The deferred exception ledger is limited to:

- App Vault and model Compose lifecycle handlers, which need App Vault/model resource actions;
- AI proxy settings, which combine settings, Compose port bindings, nginx, and firewall changes;
- direct service and arbitrary automation webhook branches, which are not six-domain actions; and
- the explicit ephemeral Proxmox VNC handler.

The test rejects a missing expected entry, a new callsite, an increased count, a provider mutation
inside an adopted handler, route/action drift, handler-side audit success, detached mutation work,
or a frontend adopted caller that does not use durable job tracking. Existing focused domain tests
remain useful defense in depth.

## Frontend behavior

The existing page-local `useDurableJobTracker` and `DurableJobNotice` are reused. The main
Containers, Container Detail, Firewall, Proxies, Backups, and Dashboard callers plus native
Containers, Firewall, Proxies, and Backups panels register accepted jobs. Existing Updates,
Settings, Proxmox, VMs, and native Proxmox/VM surfaces remain on their current trackers.

Each surface follows these rules:

- request acceptance changes copy to “submitted,” never “completed” or provider-success language;
- busy state covers submission, while the tracker owns later lifecycle presentation;
- provider lists refresh only in the tracker's conclusive-success callback;
- awaiting approval, rejected, failed, cancelled, expired, and `needs_attention` remain visible;
- polling timeout changes only foreground presentation and never resubmits or cancels the job; and
- one page-local tracker may replace a prior foreground notice, but it never changes durable state.

The native panels correct their stale route/request shapes while adopting the typed API client.
The Dashboard restore-test widget follows the same submitted job instead of refreshing immediately.

## Public asynchronous contract

`docs/api.md` gains one authoritative Durable Operations section covering:

- resource discovery and capability availability;
- canonical plan and submit endpoints;
- `Idempotency-Key` grammar, replay, and conflict semantics;
- `202 {job}` acceptance and every stable job state;
- immutable plans, approval binding, cancellation, retry/recovery, and `needs_attention`;
- job list/detail and approval list/detail/approve/reject endpoints;
- compatibility dry runs and the grouped 48-route adoption boundary;
- redaction and bounded result/error guarantees; and
- explicit synchronous/deferred exceptions.

The documentation must not claim that durable SSE or shared Jobs/Approvals frontend navigation has
landed. Event history is documented as readable; resumable SSE remains the next delivery.

## Error handling and security

- Webhook authentication precedes action selection side effects, provider reads, resource writes,
  capability publication, planning, and submission.
- Unknown or non-webhook durable actions fail closed.
- Policy denial uses the canonical structured error and job reference; the handler does not write a
  conflicting success audit.
- Provider diagnostics, Compose content, secret material, webhook secrets, and raw output remain
  bounded/redacted under existing adapter rules.
- App exposure returns acceptance rather than a guessed proxy ID.
- Read-only app URL lookup tolerates absent/stale proxy rows by returning the existing safe fallback.
- Deferred exceptions are not described as durable or approval-protected in public docs.

## Testing and verification

Focused backend coverage proves:

- webhook denial precedes Docker evidence and observation;
- webhook container dry runs create no job and direct calls create exactly one durable job;
- canonical policy/approval and idempotency apply to webhook submissions;
- service and automation webhook behavior is unchanged;
- App Vault exposure creates no proxy row/config before worker execution;
- app URL lookup performs no database/provider mutation;
- the 48-route registry and source inventories are exact; and
- every deferred exception remains explicit and count-bounded.

Frontend static inventory plus TypeScript/lint/build prove all current six-domain mutation callers
register durable jobs. Full backend tests, strict Clippy, schema ownership, repository hygiene,
tracked secret scanning, diff checks, and the real Docker/App Vault/restic golden path remain final
checkpoint gates where the local environment supports them.

## Non-goals

- New durable App Vault, model, settings, service, automation, local VM/LXC, storage, WireGuard, or
  file-management actions.
- Shared Jobs/Approvals pages, global job navigation, approval controls, or durable SSE.
- Changing the Proxmox VNC, container exec/log stream, or informational provider-read contracts.
- Removing compatibility advisory plans before the shared approval UX exists.
- CMDB, remote-agent, placement, mobile, OIDC dependency, or unrelated refactoring work.

## Completion criteria

This slice is complete when the two reusable cross-domain callers submit durable jobs, open-UI is
read-only, every current six-domain frontend caller follows acceptance to a terminal state, the
central inventory fails on any unlisted bypass, the public asynchronous contract is truthful, the
roadmap records bypass closure as complete, and the full local verification matrix passes.
