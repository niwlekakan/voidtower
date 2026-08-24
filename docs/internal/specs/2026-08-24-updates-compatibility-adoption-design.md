# Updates Compatibility Adoption

Date: 2026-08-24
Status: Approved
Scope: J0-01/J0-03 Updates HTTP compatibility adoption and page-local job tracking

## Purpose

Move every mapped Updates compatibility caller onto VoidTower's canonical durable operation
kernel. The seven existing update actions retain their current target and provider semantics, but
HTTP handlers may only authorize, resolve or observe canonical resources, prepare advisory plans,
and submit immutable jobs. Provider execution, rollback preparation, restart reconciliation, and
bounded diagnostics remain inside the Updates adapter/provider boundary.

This checkpoint covers the seven `/api/updates/*` execution/check routes and the two legacy
`/api/system/update*` aliases. It also gives the Updates and Settings pages a small shared tracker
for the exact job they submitted. Informational reads remain synchronous. No Update CLI mutation
caller exists in the current source tree.

## Fixed decisions

- Reuse the canonical invocation and compatibility-adoption boundary established by Containers,
  Firewall, Proxy, and Backups.
- Add a focused pool/state-based Updates target resolver rather than duplicating target and
  capability logic in each handler.
- Authorize before provider inspection, resource observation, or capability publication.
- Preserve all seven registered action names, resource kinds, risks, approval policies, retry and
  recovery classes, request bodies, and route paths.
- Return canonical `202 {"job": ...}` envelopes from every execution and check route.
- Keep `dry_run` compatibility requests advisory and adapter-produced. They create no job,
  approval, attempt, event, or provider mutation.
- Retain `GET /api/system/update-check` as a legacy compatibility exception even though it now
  submits a durable read job. New callers should use the canonical POST action boundary.
- Remove the process-local VoidTower and Docker check caches. Synchronous info routes derive image
  status from current provider snapshots.
- Track every submitted update job locally in the initiating page. Checks refresh their relevant
  info after success. Mutations display approval, running, terminal, and reconciliation states
  without adding approval controls or a global Jobs surface.
- No update mutation may claim success merely because the serving process exits or restarts.
- Do not add Update CLI behavior: no Update CLI mutation caller is present.

## Non-goals

- Shared Jobs or Approvals pages, approval decision controls, or durable SSE.
- Proxmox compatibility adoption, cross-domain bypass closure, App Vault image drift, scheduled
  maintenance windows, staggered node updates, plugins, or remote-agent updates.
- New update actions, package managers, rollback strategies, provider commands, or request fields.
- Changing the canonical Updates adapter's established execution or reconciliation semantics
  unless adoption tests expose a boundary defect.
- Replacing the legacy `GET /api/system/update-check` route or removing compatibility paths.
- Pushing local commits to a remote.

## Canonical target resolution

Create `operations::update_adoption` as the shared resolution boundary. It accepts a canonical
credential, action, and target selector and returns the exact active resource whose capability was
proven available.

The target matrix is:

| Actions | Kind | Alias namespace / scope / value | Evidence |
|---|---|---|---|
| `update.voidtower.check`, `apply`, `rollback` | `update_target` | `voidtower.update_target/local/voidtower` | Valid VoidTower snapshot; rollback additionally validates the requested tag through adapter planning |
| `update.odysseus.apply` | `update_target` | `voidtower.update_target/local/odysseus` | Odysseus snapshot reports an installed checkout |
| `update.docker.check` | `docker_engine` | `voidtower.singleton/local/docker` | Docker-engine snapshot succeeds |
| `update.docker.apply` | `container_image` | `docker.container_image/local/<full-container-id>` | Caller selector resolves to one normalized running-container snapshot |
| `update.os.apply` | `update_target` | `voidtower.update_target/local/os` | Supported package-manager snapshot succeeds |

Resolution order is fail-closed:

1. look up and authorize the canonical action for the credential;
2. validate compatibility-only input that can be rejected without provider access;
3. obtain the minimum provider snapshot needed to prove target identity and availability;
4. resolve the seeded target or observe the normalized container-image resource;
5. publish only the selected action capability as available; and
6. call canonical prepare or submit.

Route-provided Docker IDs are selectors, not canonical resource IDs. The resolver uses the
provider's normalized full container ID, name, and image to observe a stable `container_image`
resource under `docker.container_image/local`. A missing container, missing Docker engine,
unsupported package manager, absent Odysseus installation, or unavailable update mode never
publishes a false available capability.

The resolver does not execute checks or mutations and does not duplicate adapter planning.

## HTTP compatibility behavior

### Updates routes

| Route | Canonical action | Compatibility behavior |
|---|---|---|
| `POST /api/updates/voidtower/check` | `update.voidtower.check` | Submit and return `202` job |
| `POST /api/updates/voidtower/apply` | `update.voidtower.apply` | `dry_run` prepares; otherwise submit |
| `POST /api/updates/voidtower/rollback` | `update.voidtower.rollback` | Validate tag, `dry_run` prepares, otherwise submit with `{tag}` |
| `POST /api/updates/odysseus/apply` | `update.odysseus.apply` | Submit and return `202` job |
| `POST /api/updates/docker/check` | `update.docker.check` | Submit and return `202` job |
| `POST /api/updates/docker/:id/apply` | `update.docker.apply` | `dry_run` prepares; otherwise submit |
| `POST /api/updates/os/apply` | `update.os.apply` | `dry_run` prepares; otherwise submit |

All submissions accept an optional existing `Idempotency-Key`; the compatibility bridge generates
a valid unique key when legacy callers omit one. Successful submission never returns provider
output, operation-marker paths, rollback references, or synchronous success fields.

Advisory compatibility responses retain the current modal shape while sourcing `plan` from the
adapter's `PreparedInvocation.operation`. They may also include canonical policy and resource
views. Planning performs no rollback preparation, image pull, package update, restart, or durable
write.

### Legacy system aliases

- `GET /api/system/update-check` resolves the VoidTower target and submits
  `update.voidtower.check`, returning the same `202` job envelope as the Updates route.
- `POST /api/system/update` resolves the VoidTower target and submits
  `update.voidtower.apply`, returning the same envelope.

Both aliases use the Updates resolver and compatibility submit helper. `api/system.rs` no longer
owns update fetching, Git pulls, release downloads, build commands, helper scripts, or restart
execution. Its separate explicit restart route remains outside this slice.

### Synchronous information reads

The five informational endpoints remain synchronous and session-admin-only. They may inspect
provider state but cannot mutate it.

VoidTower Docker and Docker container rows derive status from the relationship between the
running container image ID and the currently available local image ID:

- both IDs present and equal: `up-to-date`;
- both IDs present and different: `update-available`;
- required identity missing: `unknown`.

This removes transient `checking` and cached error state from the backend. A submitted check job
owns check progress and error state durably; after it succeeds, refreshing the info endpoint shows
the newly pulled local-image relationship. Git/binary and OS information continue to come from
their snapshots.

## Frontend job tracking

Add a small reusable frontend utility or hook for tracking one submitted durable job through
`GET /api/jobs/:id`. It is not a shared Jobs UI and persists no second lifecycle model.

The tracker:

- starts from the `DurableJobSummary` returned by submission;
- polls only while the initiating component is mounted and the job is nonterminal;
- tolerates bounded transient fetch failures so self-update restart downtime can recover;
- stops on terminal state, component unmount, or a fixed 20-minute foreground bound;
- treats `awaiting_approval` as a real tracked state and never auto-approves;
- treats `needs_attention` as operator intervention, not success;
- reports the canonical job ID and state in page notifications/status text; and
- invokes a caller-provided refresh callback after conclusive success.

The tracker does not infer completion from HTTP availability, mutate job state, or fabricate a
timeout failure. Reaching the foreground bound stops polling and tells the operator that the job
continues durably.

The Updates page uses typed API methods for all check, apply, rollback, Odysseus, Docker, and OS
submissions. Existing `ChangePlanModal` previews remain advisory; confirmation submits a new
authoritative job. The modal closes after accepted submission, not after provider completion.

The Settings page uses the legacy aliases intentionally to keep those paths exercised. It tracks
their returned jobs rather than treating a request or server restart as proof of success. After a
successful check or update, it refreshes `/api/updates/voidtower` and `/api/system/version` as
appropriate.

All always-approval update mutations normally enter `awaiting_approval`. The page shows the
approval requirement and job ID but does not add an approval decision button. If another existing
client decides the approval while the page remains open, tracking continues into queued/running
and terminal states.

## Error handling and safety

- Authentication, role, and ingress rejection precede provider reads and resource/capability
  writes.
- Compatibility validation errors use existing safe legacy errors where the route contract
  requires them; canonical planning, capability, policy, stale-state, idempotency, and runtime
  failures use the shared stable envelopes.
- Provider absence or unsupported target state fails without publishing capability availability.
- Raw provider error chains, package lists beyond existing bounds, image pull output, helper-script
  paths, and rollback references never cross submission responses.
- Self-update apply/rollback remains a two-step durable operation. Restart initiation is uncertain
  until startup reconciliation proves the expected version or commit.
- Check and apply calls cannot execute in detached API tasks. Only production workers call the
  Updates provider.
- Page polling errors are safe UI state; they never resubmit automatically and never reuse a key
  for different intent.

## Source boundaries

Strengthen Updates source-inventory tests to inspect both `api/updates.rs` and the update sections
of `api/system.rs`.

The adopted handlers must contain canonical credential, resolver, prepare, or submit delegation.
They must not call `update_provider::execute`, `prepare_rollback`, shell/process/file helpers, or
spawn detached work. Low-level provider calls remain permitted only in `updates.rs` and through
the Updates operation adapter.

The source tests also require every mapped compatibility route to remain present in the typed
registry and bound to its exact canonical action.

## Testing and verification

Focused backend tests prove:

- all nine routes retain exact registry action mappings and admin/session-only access;
- authorization occurs before provider evidence and capability publication;
- seeded VoidTower, Odysseus, OS, and Docker-engine aliases resolve to exact resource kinds;
- Docker selectors normalize to a full-ID `container_image` resource;
- provider/capability absence fails closed;
- each execution/check route delegates to canonical submit and returns `202` with the expected
  action;
- all four `dry_run` branches use canonical prepare and create no job or approval;
- rollback input validation happens before provider execution;
- caller idempotency replay and conflict behavior remain canonical;
- info endpoints derive image status without mutable process caches; and
- system and Updates handlers contain no direct execution bypass.

Focused frontend verification covers tracker state classification, terminal handling, approval,
`needs_attention`, transient restart failures, timeout/unmount cleanup, and refresh-after-success
behavior through pure helpers where practical. TypeScript types require every update execution API
to return `DurableJobResponse`.

Final verification preserves the current checkpoint baseline:

- direct `rustfmt --edition 2021` on touched Rust files;
- focused and full `cargo test --all-targets --all-features`;
- `cargo clippy --all-targets --all-features -- -D warnings`;
- frontend `npm run lint` and `npm run build`;
- schema-migration ownership and repository-hygiene scripts; and
- `git diff --check` plus final source/diff review.

## Acceptance criteria

The slice is complete when all seven update actions and all nine compatibility routes reach the
canonical durable boundary; no adopted handler executes providers or owns transient execution
state; previews are adapter-produced and advisory; every submission returns and tracks a durable
job without fabricated completion; read endpoints remain useful and synchronous; direct bypass
inventory passes; the frontend and backend verification matrix is green; and Proxmox, durable SSE,
shared Jobs/Approvals UX, and broader update features remain out of scope.
