# Backups Compatibility Adoption

Date: 2026-08-24
Status: Approved
Scope: J0-01/J0-03 Backups HTTP, local CLI, and scheduled restore-test adoption

## Purpose

Move every Backup mutation caller onto VoidTower's canonical durable operation kernel without
changing the established HTTP payloads or CLI command shapes. The adapter/provider boundary
remains the only place that may create or delete backup configurations, prepare repositories,
run backups, check repositories, or execute restore tests.

The work covers the five Backup execution actions, the advisory HTTP delete plan, all five local
CLI mutation branches, and the scheduled restore-test runner. `GET /api/backups`, `backup list`,
and other read-only callers remain synchronous. No scheduled `backup.run` caller exists in the
current source tree.

## Fixed decisions

- Generalize the existing canonical invocation boundary around a typed principal and ingress.
  Do not build a CLI-only submission path and do not impersonate an HTTP user.
- Model local mutation commands as a trusted local-system principal with local-management
  authority capped at the admin tier. This admits Backup delete while preventing owner-only
  actions if the principal is reused later.
- Model scheduled restore tests as a distinct non-human scheduler principal. Persist actor and
  ingress values that identify scheduler work rather than a human or API token.
- Add explicit local-CLI and scheduler ingress values to the action registry. Registry validation
  must prove the exact admission matrix.
- Preserve normal canonical authorization, planning, policy, approval, idempotency, audit,
  execution, reconciliation, and recovery for both non-HTTP principals.
- Preserve legacy HTTP request bodies, but return durable `202 {"job": ...}` envelopes from all
  five execution routes.
- Produce delete preview from the Backup adapter's canonical `backup.config.delete` plan. The
  preview remains advisory and creates no job or approval.
- Keep frontend work local to the Backups page. Do not add shared Jobs or Approvals UX.
- Keep local CLI mutation command flags unchanged. The CLI prints submission and durable state;
  it no longer prints synchronous provider success as though the command itself performed the
  mutation.
- Retain the scheduled runner's once-per-minute guard and add deterministic durable
  idempotency as the final duplicate boundary.

## Non-goals

- Durable SSE or a shared Jobs/Approvals frontend.
- New Backup actions, schedules, retention behavior, provider features, or request fields.
- Changes to Backup secret handling or restic provider behavior unless an adoption test exposes
  a real adapter-boundary defect.
- Updates, Proxmox, App Vault, CMDB, remote-agent, OIDC, or unrelated scheduler adoption.
- Automatically approving a CLI- or scheduler-submitted operation.
- Pushing the existing local commits or this checkpoint to a remote.

## Invocation model

Replace the HTTP-specific assumption in canonical invocation with a typed invocation context.
The context owns four related decisions that must never be supplied independently by a caller:

1. the action ingress used for registry admission;
2. the persisted canonical actor;
3. the authorization material or trusted local authority; and
4. the idempotency scope.

The supported contexts for this checkpoint are:

| Context | Registry ingress | Persisted actor | Authority | Idempotency scope |
|---|---|---|---|---|
| Session | HTTP | `human`, source `http_session` | Current user role | User ID |
| Bearer | HTTP | `api_token`, source `http_bearer` | Current role plus exact bearer scope | Token ID |
| Local CLI | Local CLI | `system`, ID `voidtower_cli`, source `local_cli` | Trusted local admin ceiling | `v1:local_cli:system:voidtower_cli` |
| Scheduler | Scheduler | `system`, ID `backup_restore_test`, source `scheduler` | Exact registry ingress allowlist | `v1:scheduler:system:backup_restore_test` |

The existing session and bearer behavior remains unchanged. Authorization first verifies that the
selected action declares the context's ingress. Session and bearer contexts then apply the
existing role, AI-exposure, and bearer-scope checks. Local CLI applies the action's canonical role
against its explicit admin ceiling. Scheduler authority comes only from exact ingress admission;
it must not gain a reusable human role or API scope.

Add `System` to VoidWatch's typed actor categories. It matches policy rules whose actor type is
`system` or `*` and defaults to allow only after canonical registry-ingress and local-authority
authorization has succeeded. This makes registry admission the mandatory capability boundary
while retaining policy-rule denial and approval behavior. It must preserve these invariants:

- persisted CLI and scheduler actors are not human or API-token actors;
- policy evaluation still occurs for every submission;
- registry `Always` and policy-derived approvals still create `awaiting_approval` jobs; and
- a scheduler can submit only the actions explicitly admitted for scheduler ingress.

## Action ingress and access matrix

The five durable Backup actions retain their current HTTP policy and gain only the required
non-HTTP admissions:

| Action | HTTP session | HTTP bearer | Local CLI | Scheduler |
|---|---|---|---|---|
| `backup.config.create` | Operator | Denied | Allowed | Denied |
| `backup.config.delete` | Admin | Denied | Allowed | Denied |
| `backup.run` | Operator | `backups:run` | Allowed | Denied |
| `backup.check` | Operator | Denied | Allowed | Denied |
| `backup.restore_test` | Operator | Denied | Allowed | Allowed |

The action registry remains the authoritative admission source. Invocation must reject a durable
action whose metadata does not list the selected ingress even if a caller constructs a context
directly. Registry tests must assert the complete matrix so a broad macro change cannot
accidentally expose other actions to local CLI or scheduler callers.

## Shared Backup target resolution

Create targets the seeded local `system` resource through the
`voidtower.singleton/local/system` alias. The caller authorizes `backup.config.create` before
resolving the singleton or publishing its capability.

Existing configuration actions follow one shared resolution flow:

1. authorize the canonical action for the invocation context;
2. if required, obtain affirmative restic availability;
3. load the Backup configuration from the database/provider boundary using the legacy route ID
   or CLI-selected name;
4. normalize the configuration fields used by the Backup adapter's snapshot contract;
5. observe or resolve an active `backup_config` resource under
   `voidtower.backup_config/local/<config-id>`; and
6. publish the requested capability only after all required evidence exists.

The route ID and CLI name are selection inputs, never canonical resource IDs. CLI name lookup
must happen only after canonical access is checked. Create and delete capability publication does
not depend on restic. Run, check, and restore-test capability publication requires an affirmative
restic probe.

The reusable resolver should live at a boundary callable by HTTP, local CLI, and scheduler code
without constructing fake application state. It may reuse the generic resource observation
primitives, but it must not absorb provider execution or duplicate adapter planning.

## HTTP compatibility routes

The execution routes translate legacy payloads to the adapter's canonical input:

- `POST /api/backups` submits `backup.config.create` against the seeded system singleton using
  the existing create fields and default retention behavior.
- `DELETE /api/backups/:id` submits `backup.config.delete` with unit input.
- `POST /api/backups/:id/run` submits `backup.run` with unit input.
- `POST /api/backups/:id/check` submits `backup.check` with unit input.
- `POST /api/backups/:id/restore-test` submits `backup.restore_test` with unit input.

All five return only `202 {"job": <canonical job summary>}` after accepted submission. They do
not return `ok`, a created config ID, snapshot IDs, check output, restore-test output, or any
provider-level success claim. `backup.run` continues to admit a bearer token with the exact
`backups:run` scope. The other compatibility mutations remain session-only.

`POST /api/backups/:id/delete-plan` performs the same authorization and target resolution as
delete, calls canonical prepare for `backup.config.delete`, and adapts the resulting operation
plan to the existing modal envelope. It does not call submit and therefore creates no job,
approval, attempt, or execution event.

## Local CLI lifecycle

`backup list` remains on the current DB-only read path. Each mutation command initializes the
minimum production-grade operation context:

- database and registry validation;
- the same load-or-create secrets key behavior used by the server;
- the staged adapter registry;
- operation-runtime preparation and recovery; and
- a bounded local worker/reconciler runtime.

The command creates one local CLI invocation context and one UUID-based per-invocation
idempotency key, performs canonical authorization before singleton/config lookup, resolves the
target, and submits exactly once. It prints the job ID immediately after submission.

The wait loop polls the persisted job summary rather than relying on an in-memory completion
channel. It waits through `queued` and `running`. It exits immediately for
`awaiting_approval`, printing the job ID, approval ID, and bounded approval reason without making
a decision. It prints the terminal state plus bounded/redacted result or diagnostic for
`succeeded`, `failed`, `cancelled`, `rejected`, or `expired`. `needs_attention` is reported as an
operator-intervention state and ends the foreground wait even though durable reconciliation may
later advance it.

The wait loop polls at 250 milliseconds and has a fixed 30-minute foreground timeout. Ctrl-C or
that timeout initiates normal local runtime shutdown and exits without directly cancelling the
durable job or invoking the provider. A queued job remains queued; a safely claimed job is
released by the existing runtime shutdown path; an in-flight provider call reaches its durable
checkpoint or lease-based recovery boundary. The command must not fabricate success or failure
when the foreground wait ends.

The result printer emits canonical, already-bounded result data and error fields. It must not
reconstruct raw restic output from provider state.

## Scheduled restore-test flow

The server creates a scheduler invocation context for `backup.restore_test` and authorizes it
before querying scheduled Backup configurations or probing restic. On every existing one-minute
tick it:

1. stops the cycle if canonical scheduler authorization is unavailable;
2. obtains affirmative restic availability;
3. selects enabled configurations with a restore-test schedule;
4. applies the existing cron match and last-run-within-60-seconds guard;
5. observes the normalized Backup config resource and publishes only the restore-test
   capability; and
6. submits `backup.restore_test` through canonical invocation.

The idempotency key contains the configuration ID and the UTC minute window. The scheduler's
fixed idempotency scope makes repeated ticks, concurrent tasks, or a process restart in the same
minute resolve to the same durable job. The runner logs safe job identifiers and submission state,
not synchronous restore-test success. It never waits for or auto-approves a job; the production
operation runtime owns execution.

## Frontend behavior

The Backups page uses the existing shared durable job response type. Create, run, check,
restore-test, and delete notifications say `Submitted (job <id>)`. Closing a form or confirmation
modal means submission was accepted, not that the provider mutation finished. The page may retain
manual refresh and existing busy-state handling, but it must not immediately interpret stale
configuration fields as a job result.

The canonical delete plan remains compatible with `ChangePlanModal`. No job polling, approval
controls, shared job list, or optimistic provider result is added.

## Error handling and safety

- Authentication/ingress/role/scope denial happens before config lookup, restic probing,
  resource observation, or capability writes.
- Missing configurations map to the existing HTTP not-found response or a bounded CLI not-found
  diagnostic only after authorization.
- Missing restic produces feature/capability unavailability and never publishes an available
  run/check/restore-test capability.
- Canonical stale-state, planning, idempotency, and policy errors retain the shared API mapping.
- CLI and scheduler diagnostics use safe job/action/config identifiers and bounded canonical
  messages. They do not print secrets or unbounded provider output.
- The Backup API, CLI, and scheduler contain no direct Backup mutation service call after
  adoption. Direct calls remain only inside the adapter/provider execution boundary.

## Verification

Focused tests must prove:

- the exact Backup ingress, session-role, and bearer-scope matrix;
- local CLI and scheduler actor, ingress, idempotency-scope, and policy behavior;
- unauthorized callers fail before resource reads, restic probes, observation, and capability
  publication;
- create resolves the seeded system singleton and existing actions observe the normalized
  `voidtower.backup_config/local` alias;
- restic-dependent capabilities are unavailable without affirmative evidence;
- delete preview uses canonical prepare and creates no job or approval;
- all five HTTP executions delegate to canonical submit and return `202` job envelopes;
- all five CLI mutation branches submit and wait through the defined durable states;
- CLI approval, interruption, timeout, terminal error, and `needs_attention` branches do not
  invoke providers directly or fabricate completion;
- scheduler replay within one UTC minute returns the same durable job while a later minute gets
  a new key; and
- source inventory explicitly rejects direct create/delete/run/check/restore-test service calls
  from adopted HTTP handler sections, CLI mutation branches, and scheduled runners.

Final verification preserves the handoff baseline: direct `rustfmt --edition 2021` on touched
Rust files, `cargo test --all-targets --all-features`, Clippy with warnings denied, frontend lint
and build, schema-migration ownership, repository hygiene, and `git diff --check`.
