# Canonical Mutation API

Date: 2026-08-21
Status: Approved for implementation planning
Scope: J0 canonical planning, submission, idempotency, and job cancellation boundary

## Purpose

Expose the staged durable-operation kernel through one authenticated HTTP boundary for all 51
registered durable actions. The boundary must derive every authoritative execution and security
property on the server, create immutable durable jobs, and return stable contracts to human and
machine clients.

This checkpoint does not convert compatibility routes or CLI callers. It proves the canonical
boundary before any existing mutation source is moved onto it.

## Fixed decisions

- Use a focused `operations::invocation` service shared by planning and submission handlers.
- Keep the existing typed action registry as the security and execution authority.
- Expose an advisory plan endpoint and a submission endpoint that independently repeats the same
  authoritative planning pipeline. A caller never submits a plan or plan digest.
- Authenticate session and bearer credentials through server-owned context. Never trust actor,
  ingress, role, scope, or credential claims from JSON or headers other than the bearer credential
  itself.
- Treat API-token requests as machine-capable ingress. They require both an action bearer scope and
  `AiExposure::Callable`; a bearer scope alone does not make an action AI-callable.
- Keep session access for all 51 durable actions, subject to each action's minimum role. Bearer
  access remains narrower and does not broaden any compatibility route's existing bearer policy.
- Require explicit `available` capability state. Missing, `unknown`, or `unavailable` capability
  state fails closed before adapter planning.
- Recheck the resource, capability, and provider fingerprint after planning. A change during
  planning returns a stale-state error rather than submitting a mixed snapshot.
- Derive policy through Voidwatch on every plan and submission. Registry `Always` approval can
  strengthen but never weaken the Voidwatch verdict.
- Persist a rejected durable job for a policy-denied submission, preserving its audit identity.
- Make idempotency identify caller intent, not the observed provider snapshot. Identical retries
  return the original job before new provider reads.
- Implement cancellation only through `worker::request_cancellation`. Never abort an in-flight
  provider future or invent a terminal outcome.
- Return bounded, stable JSON error and success envelopes. Raw adapter/provider errors do not cross
  the HTTP boundary.

## Non-goals

- Converting the six compatibility domains or any CLI mutation.
- Enabling Compose compatibility apply.
- Persisting reusable plan tokens or adding a plan table.
- Durable SSE conversion or frontend Jobs/Approvals work.
- Adding new durable actions, resources, capabilities, providers, or mutation semantics.
- Adding an AI credential type, remote agent, plugin execution ingress, or automation ingress.
- Changing approval decisions, worker retry/recovery semantics, or runtime configuration.
- Refactoring legacy bearer-session compatibility outside what stable token identity requires.
- OpenID Connect dependency work.

## Approaches considered

### Central invocation service — selected

Both HTTP handlers call one service that owns action lookup, authorization, resource and
capability validation, adapter dispatch, planning, revalidation, and policy derivation. Submission
adds idempotency and durable persistence around the same result.

This produces one fail-closed path that can later serve compatibility routes and CLI callers
without moving HTTP-specific extractors into adapters.

### Handler-local orchestration — rejected

Putting the pipeline directly in Axum handlers would reduce the initial file count but duplicate
authorization, error mapping, and planning rules. Compatibility-route adoption would then either
call HTTP code or reimplement the boundary.

### Persisted plan token — rejected

A server-side plan record could bind preview and submission exactly, but it adds schema, expiry,
cleanup, replay, and authorization semantics. The durable job is already the permanent immutable
record. Replanning at submission gives a current, exact plan without a second lifecycle.

## Public HTTP contract

### Advisory planning

`POST /api/resources/:resource_id/actions/:action/plan`

The request body is:

```json
{
  "input": {}
}
```

The outer object uses strict unknown-field rejection. Action input remains tagged by the action
path and is validated by the selected typed adapter. The caller cannot include plan, risk,
approval, retry, recovery, actor, ingress, concurrency, resource revision, fingerprint, or policy
fields.

The response is `200 OK`:

```json
{
  "plan": {
    "action": "container.restart",
    "resource": {},
    "input_schema_id": "container.restart.input.v1",
    "result_schema_id": "container.restart.result.v1",
    "operation": {},
    "policy": {
      "outcome": "allow",
      "reason": null
    }
  }
}
```

`policy.outcome` is `allow`, `require_approval`, or `deny`. The endpoint has no durable side
effect, does not reserve the provider snapshot, and does not guarantee that a later submission
will see the same plan or verdict.

The planning endpoint does not accept or require an idempotency key.

### Durable submission

`POST /api/resources/:resource_id/actions/:action`

The body is identical to the planning body. The request requires an `Idempotency-Key` header.
Accepted keys are 1–128 ASCII characters and match:

```text
[A-Za-z0-9][A-Za-z0-9._:-]{0,127}
```

Queued and approval-gated jobs return `202 Accepted`:

```json
{
  "schema_version": 1,
  "resource_id": "...",
  "action": "...",
  "job": {}
}
```

A policy denial atomically creates a terminal `rejected` job and returns `403 Forbidden`:

```json
{
  "error": {
    "code": "policy_denied",
    "message": "The operation was denied by policy.",
    "job_id": "..."
  }
}
```

The rejected job is available through the normal job detail surface to an authorized caller.
Repeating an identical denied request returns the same rejected job reference and the same HTTP
classification.

### Job cancellation

`POST /api/jobs/:id/cancel`

The body is empty. An accepted request returns `202 Accepted` with the current job envelope.

- `queued` becomes `cancelled` atomically;
- `running` remains `running` with cancellation intent recorded;
- cancellation at a later safe checkpoint uses the existing worker semantics; and
- all other states return `409 invalid_job_state`.

Approval-gated work is rejected through the approval API rather than cancelled through this
route. The cancellation handler calls only `worker::request_cancellation` and then reloads the
job. It owns no task handle, provider cancellation token, or completion channel.

## Stable response and error envelope

Canonical endpoint failures use:

```json
{
  "error": {
    "code": "stable_code",
    "message": "Safe bounded explanation.",
    "job_id": null
  }
}
```

`job_id` is omitted unless a durable job exists. The initial stable codes are:

| Status | Code | Meaning |
|---:|---|---|
| 400 | `invalid_request` | Invalid outer JSON or forbidden caller-owned metadata |
| 400 | `invalid_idempotency_key` | Missing or malformed submission key |
| 401 | `unauthorized` | No valid session or bearer context |
| 403 | `forbidden` | Session role or credential kind is not allowed |
| 403 | `insufficient_scope` | Bearer scope does not permit the action |
| 403 | `ai_exposure_denied` | Machine-capable ingress selected a non-callable action |
| 403 | `policy_denied` | Policy rejected a submission; includes `job_id` |
| 404 | `unknown_action` | No callable durable HTTP action matches the path |
| 404 | `resource_not_found` | Canonical resource does not exist or is retired |
| 404 | `job_not_found` | Cancellation target does not exist |
| 409 | `resource_kind_mismatch` | Action and resource kinds do not match |
| 409 | `capability_unavailable` | Capability is missing, unknown, or unavailable |
| 409 | `stale_state` | Resource, capability, or fingerprint changed during planning |
| 409 | `idempotency_conflict` | The scoped key belongs to different intent |
| 409 | `invalid_job_state` | Job cannot accept cancellation in its current state |
| 422 | `planning_rejected` | Typed adapter rejected the requested input or provider state |
| 503 | `operation_runtime_unavailable` | Required adapter/runtime boundary is unavailable |
| 500 | `internal_error` | Safe fallback for an unexpected internal failure |

Messages are constants or bounded redacted explanations. Adapter/provider error chains may be
logged only through a bounded redaction helper with safe action/resource identifiers; they are
never returned directly.

## Authentication and authorization

### Credential context

Bearer middleware gains a server-owned context containing the stable API-token ID, owning user
ID, and declared scopes. It continues to support the legacy temporary session internally, but the
canonical handlers inspect bearer context first so a token request can never be mistaken for a
human session.

The invocation service receives one of:

- human session: current user ID and role; or
- API token: stable token ID, current owner user ID and role, and current declared scopes.

The durable actor snapshot is:

- `ActorType::Human`, user ID, source `http_session`; or
- `ActorType::ApiToken`, token ID, source `http_bearer`.

The persisted ingress is correspondingly `http_session` or `http_bearer`. Neither value is read
from the request body.

### Registry changes

Each durable `ActionMetadata` record gains explicit canonical access metadata:

- minimum session role;
- bearer policy (`Denied` or an exact scope); and
- existing AI exposure classification.

The generic plan, submit, and cancellation routes declare that authorization is action-scoped.
Route metadata remains maximally classified for risk and approval, while the selected action
record supplies the exact role, bearer, AI, and execution rules.

Action access initially preserves compatibility-route exposure:

- Containers lifecycle actions retain operator plus `containers:restart`; Compose apply remains
  admin/session-only.
- Backup create/run/check/restore retain operator roles, deletion retains admin, and only run
  retains bearer scope `backups:run`.
- Firewall actions remain admin/session-only.
- Proxy actions remain admin plus bearer scope `proxy:manage`.
- Updates and Proxmox actions remain admin/session-only.

Only actions with an explicit bearer scope are marked `AiExposure::Callable` for the canonical
boundary. Bearer-denied actions remain `AiExposure::None`.

Registry startup validation proves:

- every durable HTTP action has complete canonical access metadata;
- direct actions cannot accidentally acquire canonical access metadata;
- each mapped compatibility route is at least as restrictive as its selected action for role,
  scope, risk, and approval;
- `AiExposure::Callable` durable actions have an exact non-public bearer scope;
- bearer-denied durable actions are not AI-callable;
- generic canonical routes are marked action-scoped; and
- unknown, direct, misbound, or partially classified actions fail closed.

### Cancellation authorization

The handler loads the job only to select its registered action and authorize the request before
calling the cancellation repository function.

- A session needs the action's current minimum role.
- A bearer token must be the same stable token that submitted the job and must still satisfy the
  action's scope and AI-exposure rules.
- Jobs created by other bearer tokens cannot be cancelled by token credentials.

Human administrators retain operational cancellation according to the action role; cancellation
does not require the original human submitter.

## Canonical invocation service

`operations::invocation` owns transport-independent types and the pipeline. Axum code is limited
to extraction, body/header validation, response status, and conversion to stable API errors.

### Planning pipeline

1. Look up the action and require `ActionExecution::DurableJob` plus HTTP ingress.
2. Authorize current credential kind, role, bearer scope, and AI exposure.
3. Resolve the canonical resource and require active lifecycle.
4. Require the action's declared resource kind to equal the resource kind.
5. Read the exact `(resource_id, action)` capability and require `available`.
6. Select the adapter exclusively through `AdapterRegistry::for_action`.
7. Call the adapter's side-effect-free typed `plan` with action, resource, and caller input.
8. Validate plan schema version, bounded redacted fields, non-empty fingerprint and steps, plan
   risk against registry risk, and per-step retry/recovery against action metadata.
9. Re-read the canonical resource and capability. Require the same resource revision and an
   available capability record that has not changed since the first read.
10. Ask the adapter for a fresh external fingerprint and require equality with the plan.
11. Evaluate Voidwatch with the derived actor kind, canonical action, resource kind, and resource
    UUID.
12. Combine the verdict with registry approval metadata and return a typed `PreparedInvocation`.

The second resource/capability/fingerprint observation detects state changes during planning.
Worker preflight remains authoritative for changes after submission.

### Policy derivation

The service maps Voidwatch and action metadata as follows:

| Action metadata / Voidwatch verdict | Submission policy |
|---|---|
| Read-result job / `Allow` | `Allow` |
| Mutating `RiskLadder` / `Allow` | `Allow` |
| Mutating `Always` / `Allow` | `RequireApproval` |
| Any / `RequireApproval` | `RequireApproval` |
| Any / `Deny` | `Deny` |
| Any / `AllowRequireSnapshot` | `RequireApproval` with a fail-closed snapshot reason |

Snapshot-required verdicts are conservatively approval-gated because the current plan contract
does not carry a registry-verifiable snapshot guarantee. A later contract may make a typed
snapshot precondition sufficient; this checkpoint does not infer it from human-readable step
names.

Approval expiry is a single server constant of 15 minutes from submission. The reason combines a
safe stable policy explanation with the action's registry requirement. Caller-provided approval
or mode fields are rejected by the strict outer request shape.

## Idempotency

The server derives the scope:

```text
v1:http_session:human:<user-id>
v1:http_bearer:api_token:<token-id>
```

The request digest is canonical JSON over:

```json
{
  "schema_version": 1,
  "action": "...",
  "resource_id": "...",
  "input": {}
}
```

Resource revision, plan, provider fingerprint, policy verdict, and time are deliberately excluded
from caller intent.

After action authorization and key validation, submission checks `(scope, key)` before resolving
or observing the provider:

- matching digest returns the original job without replanning;
- different digest returns `409 idempotency_conflict`; and
- no match continues through the full planning pipeline.

The transactional `jobs::submit` path calculates or verifies the same full intent digest before
insertion. Its unique database constraint resolves concurrent first submissions: one insert wins,
and the other returns that job if its digest matches. It never treats identical input sent to a
different action or resource as an identical request.

Changing the meaning of `jobs.request_digest` requires no migration because its SQL type and
constraint remain unchanged. Existing non-HTTP test scopes cannot collide with the new versioned
HTTP scopes.

## Immutability, bounds, and redaction

- Submission persists exactly the validated adapter `OperationPlanV1` and its fingerprint.
- Action, resource revision, actor, ingress, concurrency key, retry, recovery, policy, and
  idempotency scope are server-derived snapshots.
- The input body is bounded at 64 KiB before JSON extraction. Existing typed adapters continue to
  require secret references rather than raw secret values.
- Plan titles, changes, previews, fingerprints, step names, and policy reasons use explicit count
  and string bounds before persistence or response.
- Common credential patterns and known secret values are redacted before a diagnostic is logged or
  stored. Canonical input and plan tests include representative bearer, password, and secret
  values.
- The API never returns provider stdout/stderr or an `anyhow` display chain.

## Cancellation semantics

The endpoint does not add cancellation behavior to the worker. It exposes the semantics already
proved by the production lifecycle:

- queued cancellation atomically cancels pending steps and appends event/audit history;
- running cancellation records intent and appends event/audit history;
- provider execution already in flight reaches a real persistence checkpoint;
- the next safe preflight observes cancellation; and
- a real final-step outcome wins over a late cancellation request.

Repository cancellation errors become a typed not-found or invalid-state result so the handler
never parses error strings. The returned job is reloaded from SQLite after the transaction.

## Components and file boundaries

- `operations/invocation.rs`: credential-neutral authorization input, prepared invocation,
  planning, policy mapping, bounds, idempotency scope/digest, and typed failures.
- `api/actions.rs`: strict request type, session/bearer context extraction, idempotency header,
  handlers, status selection, and stable envelopes.
- `api/bearer_auth.rs` and `auth/mod.rs`: stable non-secret API-token identity in request context.
- `action_registry.rs` and `operations/registry.rs`: action-scoped canonical access metadata and
  exhaustive validation.
- `operations/jobs.rs`: full-intent digest/replay semantics and typed idempotency conflict.
- `operations/worker.rs`: typed cancellation errors without changing cancellation behavior.
- `api/jobs.rs`: authenticated cancellation handler while retaining existing list/detail routes.
- `api/mod.rs`: mount the three routes and retain middleware ordering.
- `error.rs`: structured details needed for canonical failures without changing unrelated legacy
  response shapes.

No compatibility domain handler changes in this checkpoint.

## Concurrency and failure behavior

- Advisory planning is not serialized and reserves nothing.
- Submission can race with external state changes; double observation rejects changes during
  planning, and worker preflight rejects changes after persistence.
- Two concurrent identical submissions may both plan, but the database idempotency constraint
  yields one durable job.
- A concurrent conflicting key reuse yields one job and one stable conflict.
- Database failure before commit creates no visible job or event.
- Database failure after an external side effect remains worker/reconciler territory; the HTTP
  request is never coupled to completion.
- Runtime/adaptor registry absence fails before HTTP serving in production and maps to a safe 503
  only in defensive test/injected states.

## Testing

### Registry and authorization

- Every durable action has complete role, bearer, AI, resource, schema, retry, recovery, and
  adapter metadata.
- Every action is exercised through session roles below, at, and above its threshold.
- Every action is exercised with absent, wrong, and correct bearer scopes.
- Bearer-denied and non-AI-callable actions fail before the fake adapter observes a call.
- Unknown, direct, non-HTTP, mismatched-resource, and partially classified actions fail closed.
- Real-router probes cover unauthenticated session and bearer cases for plan, submit, and cancel.

### Planning and policy

- Raw requests cannot supply authoritative metadata.
- Fake adapters prove plan calls are side-effect-free and submission persists the exact plan.
- Missing, unknown, and unavailable capabilities reject before planning.
- Resource revision, capability observation, and fingerprint changes during planning reject as
  stale.
- Voidwatch mode × risk × actor matrices cover Observer, Assisted, Trusted, YOLO, unconfigured,
  read-result, mutate, destructive, and irreversible cases.
- Every `ApprovalPolicy::Always` action remains approval-gated even when Voidwatch allows it.
- Snapshot-required verdicts cannot become direct `Allow`.
- Policy denial creates one rejected durable job with safe event/audit data.

### Idempotency and cancellation

- Identical replay returns the original queued, approval-gated, terminal, or rejected job without
  another adapter call.
- Same key with different action, resource, or input conflicts.
- Different actors and session/bearer ingress do not collide.
- Concurrent identical submissions yield one job; concurrent conflicting submissions yield one
  job and one conflict.
- Queued and running cancellation use existing repository behavior.
- Awaiting-approval, terminal, missing, and unauthorized job cancellation return stable errors.
- Cancellation during a held fake provider call records intent without aborting that future.

### Bounds, redaction, and lifecycle

- Invalid and oversized keys, bodies, plans, fields, and diagnostics fail safely.
- Known secrets and common credential patterns are absent from responses, logs captured by tests,
  jobs, events, and audit rows.
- A real-router test with a prepared production runtime submits a fake durable action, observes
  `202`, and reaches a durable worker result without an in-memory completion channel.
- An unavailable/incomplete runtime cannot serve the canonical route successfully.

## Verification and publication

Run focused tests while iterating, then the complete repository matrix:

- targeted Rust formatting and `cargo fmt --check`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- `cargo test --all-targets --all-features`;
- frontend lint and production build;
- schema migration ownership and repository hygiene checks;
- Compose rendering, shell/YAML checks, and `git diff --check`;
- the real Docker/App Vault/restic golden path; and
- proportional mutation testing for authorization, policy mapping, idempotency, and cancellation.

Update `ROADMAP.md` only after the checkpoint passes. Keep this specification, its implementation
plan, and its successor handoff ignored and untracked. Publish one focused tracked commit, push
`dev`, and confirm all six CI jobs attach to and pass on the exact new `HEAD`.
