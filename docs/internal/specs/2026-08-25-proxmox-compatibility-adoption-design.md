# Proxmox Compatibility Adoption

Date: 2026-08-25
Status: Approved by standing recommendation authority
Scope: J0-01/J0-03 Proxmox compatibility adoption and page-local durable job tracking

## Purpose

Move all 21 mapped Proxmox compatibility routes onto VoidTower's canonical durable operation
kernel without weakening their existing admin/session-only policy. The routes cover 20 durable
actions across the multi-host Proxmox API and the legacy `/api/vms/proxmox/*` aliases. Mutation
handlers may authenticate, translate compatibility input, stage controlled artifacts or opaque
secret references, resolve proven canonical targets, prepare advisory plans, and submit immutable
jobs. They may not execute Proxmox HTTP mutations, write authoritative host configuration, or
claim provider success.

Informational reads remain synchronous. VNC ticket creation remains an explicitly ephemeral,
audited direct action and is not mapped to a durable job. Existing frontend surfaces follow the
jobs they submit locally; shared Jobs/Approvals navigation and durable SSE remain later work.

## Fixed decisions

- Reuse `operation_adoption` and the canonical invocation service used by the other five domains.
- Add a focused `operations::proxmox_adoption` resolver with an injectable evidence provider.
- Authorize the selected canonical action before database lookup, secret/file staging, provider
  evidence, resource observation, or capability publication.
- Resolve or observe exactly one `system`, `proxmox_host`, `proxmox_guest`, `proxmox_storage`, or
  `proxmox_disk` resource and publish only the selected action capability.
- Use minimum read-only Proxmox evidence to normalize guest kind/node and prove storage or disk
  identity. Provider mutations remain adapter-only.
- Preserve all route paths, HTTP methods, admin/session-only access, action mappings, action input
  meaning, risk, approval, retry, and reconciliation metadata.
- Return canonical `202` responses with the versioned job envelope
  `{ "schema_version": 1, "resource_id": "...", "action": "...", "job": ... }` for execution
  and read-result jobs.
- Keep existing compatibility dry runs but source their modal plan from canonical adapter planning.
  They create no job, approval, task, artifact mutation, or provider mutation.
- Stage uploaded files beneath the configured `proxmox-uploads` directory and pass only a bounded
  filename reference to the durable input.
- Encrypt compatibility tokens into candidate secret records and pass only `token_secret_id`.
  Raw token ID/secret material never enters job input, plans, events, logs, errors, or responses.
- Preserve the legacy single-host configuration as a compatibility projection. A stable legacy
  host selector uses the current settings/secret storage, while the adapter remains capable of
  configuring ordinary multi-host records through the canonical action.
- Preserve App Vault's manual bootstrap workflow without persisting Compose YAML: the browser
  constructs the bootstrap script locally, the durable LXC job creates the container, and the
  reconciled job result returns the allocated VMID.
- Use the existing single-job tracker for individual actions and a small page-local batch tracker
  for bulk VM actions. Neither is a shared Jobs or Approvals product surface.
- Do not push local commits.

## Non-goals

- New Proxmox features, VM creation wizard, migration, network/firewall management, PBS-native
  APIs, remote console redesign, or role-tier changes.
- Changing local libvirt or standalone local-LXC mutation paths.
- Shared Jobs/Approvals pages, approval controls, durable SSE, cursor recovery, or global job
  navigation.
- Persisting App Vault Compose YAML, bootstrap scripts, raw credentials, VNC tickets, or upload
  contents in durable job JSON.
- Refactoring every synchronous Proxmox read around a new universal client.
- Cross-domain bypass closure beyond the Proxmox and legacy Proxmox handler files.

## Approaches considered

### Recommended: focused evidence resolver and controlled staging

Add one adoption module that owns authorization-first resolution and provider evidence, keep read
routes intact, and make only the adapter changes needed for legacy configuration, LXC result
continuity, and staged uploads. This follows the proven domain-adoption pattern, supports direct
compatibility API calls without requiring a prior inventory read, and keeps the slice bounded.

### Rejected: alias-only adoption

Resolve only resources previously observed by inventory GET routes. This is smaller, but direct
mutation calls would fail until a user opened the matching page, legacy VM inventory does not
currently observe canonical aliases, and stale aliases could publish capabilities without current
provider evidence.

### Rejected: full Proxmox client rewrite

Move all reads, monitoring, VNC, and adapter traffic through a new shared client before adoption.
This would reduce duplication eventually, but it combines a large read-path rewrite with the
security-critical mutation conversion and materially increases regression risk.

## Route and action matrix

| Compatibility route | Canonical action |
|---|---|
| `POST /api/proxmox/hosts` | `proxmox.host.create` |
| `DELETE /api/proxmox/hosts/:host_id` | `proxmox.host.delete` |
| `POST /api/vms/proxmox/config` | `proxmox.host.configure` |
| `POST /api/vms/proxmox/test` | `proxmox.host.test` |
| `POST /api/vms/proxmox/action` | selected `proxmox.guest.{start,stop,shutdown,reboot,suspend,resume}` |
| `POST /api/proxmox/:host_id/vms/:vmid/start` | `proxmox.guest.start` |
| `POST /api/proxmox/:host_id/vms/:vmid/stop` | `proxmox.guest.stop` |
| `POST /api/proxmox/:host_id/vms/:vmid/shutdown` | `proxmox.guest.shutdown` |
| `POST /api/proxmox/:host_id/vms/:vmid/reboot` | `proxmox.guest.reboot` |
| `POST /api/proxmox/:host_id/vms/:vmid/reset` | `proxmox.guest.reset` |
| `POST /api/proxmox/:host_id/vms/:vmid/suspend` | `proxmox.guest.suspend` |
| `POST /api/proxmox/:host_id/vms/:vmid/resume` | `proxmox.guest.resume` |
| `POST /api/proxmox/:host_id/vms/:vmid/snapshot` | `proxmox.snapshot.create` |
| `POST /api/proxmox/:host_id/vms/:vmid/rollback/:snapname` | `proxmox.snapshot.rollback` |
| `DELETE /api/proxmox/:host_id/vms/:vmid/snapshot/:snapname` | `proxmox.snapshot.delete` |
| `POST /api/proxmox/:host_id/vms/:vmid/disk-passthrough` | `proxmox.disk.attach` |
| `POST /api/proxmox/:host_id/lxc/deploy` | `proxmox.lxc.deploy` |
| `POST /api/proxmox/:host_id/nodes/:node/storage/:storage/content` | `proxmox.storage.upload` |
| `DELETE /api/proxmox/:host_id/nodes/:node/storage/:storage/content` | `proxmox.storage.delete` |
| `POST /api/proxmox/:host_id/nodes/:node/disks/wipe` | `proxmox.disk.wipe` |
| `POST /api/proxmox/:host_id/nodes/:node/disks/init` | `proxmox.disk.initialize` |

All informational GET routes, `POST .../vncproxy`, local libvirt routes, and local LXC routes remain
outside this matrix.

## Canonical target resolution

`operations::proxmox_adoption` accepts a canonical credential, action, and typed selector. Its
resolution order is fixed:

1. look up and authorize the exact canonical action;
2. validate selector structure that requires no provider access;
3. load the configured host without exposing or persisting its token;
4. obtain the minimum read-only provider evidence for external targets;
5. resolve a seeded resource or observe the normalized provider identity;
6. publish only `(resource_id, selected_action)` as available; and
7. return the resource and normalized target data needed to translate the compatibility input.

The target matrix is:

| Actions | Resource identity | Evidence |
|---|---|---|
| host create/configure | seeded `system` singleton | authorized local configuration target |
| host delete/test/LXC deploy | `voidtower.proxmox_host/local/<host-id>` | configured host row or stable legacy configuration |
| guest lifecycle/snapshot/disk attach | `proxmox.guest/<host-id>/<node>/<kind>:<vmid>` | exact guest status read; kind normalized to `qemu` or `lxc` |
| storage upload/delete | `proxmox.storage/<host-id>/<node>/<storage>` | exact storage status read |
| disk wipe/initialize | `proxmox.disk/<host-id>/<node>/<device-path>` | exact match in the provider disk inventory |

The resolver rejects missing hosts, guests, storage, disks, invalid kind/node mismatches, LXC hard
reset or disk attach, and provider failures without publishing capability availability. Provider
diagnostics are pattern-redacted and bounded before becoming compatibility errors.

## Compatibility input and planning

Handlers translate legacy bodies into the adapter's strict canonical inputs:

- guest lifecycle uses `{}`;
- snapshot create uses `{name, description}`, rollback/delete use `{name}`;
- disk attach uses `{disk_path, bus}`;
- LXC deploy uses `{node, hostname, ostemplate, cores, memory, storage, disk_gb}`;
- storage upload uses `{content, staged_file}` and delete uses `{volid}`;
- disk wipe uses `{}` and initialize uses `{fstype, name, raidlevel}`;
- host create/configure use public host fields plus `token_secret_id`, never raw token material;
- host delete/test use `{}`.

Dry-run compatibility responses retain `{dry_run: true, plan}` while taking the plan, risk, steps,
and fingerprint from `PreparedInvocation`. The response may include canonical policy/resource
views. Submission accepts an existing `Idempotency-Key`; the bridge generates a valid unique key
when legacy callers omit one.

## Secret and file staging

Host-create and legacy-config handlers authenticate and authorize before storing credentials. A
token is encrypted with the instance key into a generated candidate secret whose name is explicitly
marked as Proxmox staging. The job input contains only its UUID. The adapter copies the encrypted
value to the established destination name at execution. A deterministic follow-up step performs
best-effort deletion only for compatibility-owned staging names; a failed submission removes the
candidate immediately.

Storage upload authenticates, resolves the exact storage target, and then streams multipart data
to a generated file below `<data_dir>/proxmox-uploads`. It accepts only the established `content`
field and one filename field, enforces the adapter's 16 GiB bound while streaming, rejects path
components and duplicate/unknown fields, and passes only the generated filename to planning and
submission. A failed submission removes the staged file. After successful task reconciliation, a
deterministic follow-up step performs best-effort removal. Uncertain work retains the file until the
provider outcome is known, and cleanup failure does not rewrite a proven provider success as a
failed upload.

Neither staging operation is provider execution. Candidate names, controlled filenames, content
digests, and secret versions may enter fingerprints; raw bytes and decrypted values may not.

## Legacy configuration bridge

The legacy settings API remains readable and token-redacted. `proxmox.host.configure` uses a stable
legacy host selector. The adapter reads existing settings/`proxmox_legacy_token` state for its
fingerprint and applies the configuration only inside the worker. Existing plaintext token settings
remain readable as legacy credential state until the next configure job commits an encrypted
replacement; that job then removes the plaintext setting.
The legacy test route observes the same stable host resource and submits `proxmox.host.test`.

Ordinary multi-host create/delete behavior continues to use `proxmox_hosts` and
`proxmox_token_<host-id>`. Canonical configure behavior for ordinary host IDs is retained; only the
stable legacy selector takes the settings compatibility branch.

## LXC deployment result continuity

The durable LXC operation preserves the current VM defaults: DHCP on `vmbr0`, nesting enabled,
on-boot enabled, and immediate start. It obtains the VMID during provider execution. Its bounded
external task reference stores the raw UPID plus the safe allocated VMID. Reconciliation extracts
the UPID for task status and includes the VMID in the successful job result.

Compose YAML and the bootstrap script stay in browser memory. After the canonical job succeeds,
the App Vault modal reads the VMID from the job result and presents the locally generated script.
Awaiting approval, failure, expiration, cancellation, and `needs_attention` never reveal the script
as if deployment completed.

## Frontend job following

Typed API methods return `DurableJobResponse` for every mapped execution route and retain the
existing advisory-plan unions. The job summary type exposes safe progress, result, and error fields
already returned by the backend.

The Tower Proxmox page, native Proxmox panel, legacy VMs page/panel, host modals, storage/disk
modals, snapshot flows, and App Vault LXC modal register accepted jobs with page-local tracking.
They show canonical job IDs and states, refresh provider reads only after conclusive success, and
never infer success from request acceptance or later HTTP availability.

Bulk VM start/stop submits one job per target and follows the complete returned set with a bounded
batch tracker. The UI reports partial terminal outcomes accurately and refreshes after all jobs are
terminal. It does not auto-approve, cancel, retry, or create a synthetic server-side batch job.

## Error handling and safety

- Authentication and action authorization precede provider reads, resource/capability writes, and
  controlled staging.
- Compatibility validation failures use bounded existing `AppError` responses; invocation,
  capability, policy, stale-state, idempotency, and runtime failures use canonical envelopes.
- Provider error bodies, auth headers, decrypted tokens, fingerprints containing secrets, upload
  bytes, VNC tickets, bootstrap contents, and raw task diagnostics never cross job responses.
- Only the operation adapter performs Proxmox mutation HTTP calls or authoritative configuration
  writes.
- UPIDs remain external operation identities and are reconciled without replay after timeout or
  restart.
- Snapshot deletion, disk wipe, and disk initialization remain always-approval in every mode.
- The shutdown route selects `proxmox.guest.shutdown`; it is no longer implemented as an alias that
  silently records the stop action.

## Source boundaries

Source-inventory tests inspect the mutation sections of `api/proxmox.rs` and the legacy Proxmox
sections of `api/vms.rs`. Adopted handlers must contain canonical credential, resolver, prepare, or
submit delegation and must not contain provider `.send()`, `pve_post`, direct host/settings/secret
mutation SQL, audit-success claims, or raw updater-style detached work. Synchronous reads and the
ephemeral VNC handler may continue to use the Proxmox client.

The registry test remains authoritative for the exact 21-route/20-action matrix and proves route
policy is at least as restrictive as action policy.

## Testing and verification

Focused backend tests prove:

- authorization denial occurs before evidence, observation, capability publication, or staging;
- system and configured-host resolution uses exact kinds and aliases;
- guest, storage, and disk selectors normalize from read-only provider evidence;
- absent or mismatched evidence never publishes a capability;
- all mapped routes select the exact action and return canonical `202` jobs;
- dry runs are adapter-produced and create no job or approval;
- raw compatibility tokens are encrypted and absent from job/plan/error/audit state;
- multipart uploads are path-safe, bounded, staged, and absent from handler-side provider calls;
- legacy configuration changes only when its worker executes;
- LXC reconciliation returns its safe VMID without persisting Compose or bootstrap content;
- UPID success/failure/timeout reconciliation never replays provider mutation; and
- mutation handler source contains no direct Proxmox execution bypass.

Frontend verification uses strict TypeScript, lint, and production build. Pure tracker tests are
added only if the existing frontend setup supports them without new infrastructure.

Final gates are direct `rustfmt --edition 2021` on touched Rust files,
`cargo test --all-targets --all-features`, strict Clippy, frontend type-check/lint/build, schema
migration ownership, repository hygiene, `git diff --check`, and a final authorization/secret/
provider-bypass/false-completion review.

## Acceptance criteria

The slice is complete when all 21 mapped Proxmox routes reach their exact canonical durable action;
all dry runs are adapter-produced; no adopted handler executes provider mutations or writes
authoritative configuration; tokens and uploads cross the boundary only through controlled opaque
references; UPID reconciliation remains restart-safe; every frontend caller consumes job state
without false completion; VNC and reads remain compatible; all verification gates pass; and shared
Jobs/Approvals, durable SSE, and cross-domain bypass closure remain explicitly deferred.
