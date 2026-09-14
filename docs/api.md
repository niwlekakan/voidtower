# API Reference

`backend/contracts/api-v1-envelope-contract.json` is the source-owned contract artifact for the versioned canonical operation responses and the shared frontend generated artifact. The current API version is `1`; clients may omit `x-voidtower-api-version` for compatibility, while an explicit unsupported, malformed, or duplicate value receives `406` with the bounded `unsupported_api_version` envelope. Successful responses echo `x-voidtower-api-version: 1`.

Canonical action requests use the same JSON body for planning and submission:

```
POST /api/resources/:id/actions/:action/plan   { "input": { ... } }
POST /api/resources/:id/actions/:action        { "input": { ... } }
```

Both requests send `Content-Type: application/json` and the API-version header. Submission additionally requires `Idempotency-Key`; it is 1–128 ASCII characters matching `[A-Za-z0-9][A-Za-z0-9._:-]{0,127}`. Clients reject blank or oversized resource/action path values and invalid idempotency keys before making a request. The plan response is `200` and the submission response is `202`; both use the source-owned versioned envelopes below. Unknown JSON body fields and malformed JSON fail with bounded `400 { "error": { "code": "invalid_request", "message": "The request body is invalid." } }`.

Web-client session and recovery behavior:

- Every API request includes credentials and the current API-version header. A `401` response for the request’s current auth session immediately clears the in-memory authenticated user; stale responses from an older auth session are ignored. The route guard then returns the browser to `/login` rather than leaving protected screens mounted.
- A `406` `unsupported_api_version` response is parsed only when its bounded `supported_versions` list is valid. The client error exposes that list for a compatibility prompt; malformed negotiation bodies fall back to the generic bounded API error.
- Durable SSE is an invalidation/history channel, not authoritative state. The client validates event schema, sequence, and SSE ID; transport failure reconnects from the last accepted cursor, while a server or local gap triggers an authoritative HTTP read and reconnects from the server high-water mark.
- After reconnect or gap recovery, operation views use the authoritative read as their ready barrier. Bounded polling remains the fallback while disconnected and resumes only within its foreground deadline.

`backend/src/action_registry.rs` is the authoritative security inventory for every registered
route and structured AI/automation action. Each route explicitly declares its session policy,
concrete credential mechanism, bearer policy, risk class, approval policy, and AI exposure.
Unknown bearer routes remain denied. Handler checks remain defense in depth. MCP's handlers expect
API-token bearer credentials, but the global bearer policy deliberately remains denied pending a
separate audited correction; S0-03 does not broaden that access path. The two embed-router paths
explicitly retain their existing unscoped bearer-to-session bridge because that sub-router does not
mount scope enforcement.

---

## Durable operations

VoidTower's Containers, Firewall, Proxy, Updates, Backups, and Proxmox operation families use one
asynchronous mutation contract. Discover the canonical resource and its currently available
capabilities before planning or submitting an action:

```
GET  /api/resources
GET  /api/resources/:id
GET  /api/resources/:id/capabilities
POST /api/resources/:id/actions/:action/plan   { "input": { ... } }
POST /api/resources/:id/actions/:action        { "input": { ... } }
GET  /api/jobs?limit=50
GET  /api/jobs/:id
POST /api/jobs/:id/cancel
GET  /api/approvals?status=&limit=50
GET  /api/approvals/:id
POST /api/approvals/:id/approve                { "comment": "..." }
POST /api/approvals/:id/reject                 { "comment": "..." }
GET  /api/events?after=&limit=
```

Planning returns `200` and has no durable side effect. Canonical submission requires an
`Idempotency-Key` header and returns `202 { "job": ... }` when the request is accepted. Keys are
1–128 ASCII characters, begin with an alphanumeric character, and thereafter may also contain
`.`, `_`, `:`, or `-`. Reusing a key with the same credential and identical intent returns the same
job; reusing it for different intent returns `409 idempotency_conflict`. Adopted compatibility
routes accept the same header but generate a request-scoped key when older callers omit it.

A returned job is an acceptance record, not evidence that the provider mutation succeeded. Stable
states are `awaiting_approval`, `queued`, `running`, `succeeded`, `failed`, `cancelled`,
`needs_attention`, `rejected`, and `expired`. `needs_attention` is deliberately non-terminal while
the reconciler resolves an uncertain provider outcome. Plans are immutable and approval decisions
remain bound to the exact job and observed resource revision. Cancellation is cooperative and is
accepted only while a job is queued or running. Retry and recovery policy come from the registered
action and plan; callers must not resubmit while following a job.

Policy denial returns `403 policy_denied` with the rejected durable `job_id`. Other canonical errors
use `{ "error": { "code", "message", "job_id"? } }`. Persisted plans, results, events, and errors
use bounded, redacted representations; credentials and staged secret contents are never returned.

The adopted compatibility inventory contains 48 route keys: App Vault exposure (1), Odysseus
container webhooks (1), Backups (5), Containers/Compose apply (2), Firewall (3), Proxy/nginx (6),
Updates/system update (9), and Proxmox plus legacy Proxmox VM routes (21). Each mapped durable
branch returns `202 { "job": ... }`; a compatibility `dry_run: true` returns an advisory plan and
creates no job. App Vault/model Compose lifecycle, AI proxy-settings orchestration, service and
arbitrary-automation webhook actions, and ephemeral Proxmox VNC ticket creation remain synchronous
exceptions because they do not yet have matching durable actions. The current web clients follow
submitted jobs locally and link to shared job detail. Owner/admin/operator sessions can list and
inspect the newest 50 jobs in Tower or Void Mode; cancellation is offered only for queued/running
records. Owner/admin sessions can list and decide exact immutable approvals with an optional
comment. These shared workflows use bounded, visibility-aware HTTP polling and never retry a
mutation automatically. `/api/events` exposes durable history; `/api/events/stream` and
`/api/integrations/events` expose the same cursor-resumable durable SSE stream. Shared operation
views use it only to invalidate authoritative HTTP reads and retain bounded polling as fallback.

---

## Auth

```
POST /api/auth/bootstrap   { token, username, password }
POST /api/auth/login       { username, password }
POST /api/auth/logout
GET  /api/auth/me
```

## Metrics

```
GET /api/metrics/current
GET /api/metrics/ws        WebSocket (1 s interval)
```

## Durable events

```
GET /api/events                         Ordered retained history (`after`, `limit`)
GET /api/events/stream                  Cursor-resumable SSE
GET /api/integrations/events            Exact durable SSE alias
GET /api/integrations/events/legacy     Deprecated transient metrics/audit feed
```

The durable stream accepts `after=<non-negative sequence>` and the standard `Last-Event-ID`
header. If both are present, the greater value wins so reconnect cannot rewind. A connection with
neither cursor starts at the current high-water mark; use `after=0` to request all retained events.

After validating the cursor, the server emits:

```text
event: stream.ready
data: {"cursor":42,"high_water":47}

id: 43
event: durable_event
data: {"sequence":43,"event_id":"...","schema_version":1,"event_type":"job.running.v1",...}
```

`durable_event` data is the complete `EventEnvelopeV1`, and its SSE `id` always equals
`sequence`. Delivery is ordered in bounded batches with bounded backpressure. Keepalive comments do
not advance the cursor.

If retained history cannot satisfy a cursor, or a sequence discontinuity is detected, the server
emits `stream.gap` with `reason`, `requested_after`, `earliest_available`, and `latest_available`,
then closes. Clients must refetch complete authoritative resources before reconnecting at a known
high-water mark. They must never infer or replay a mutation from an event.

Owner, admin, and operator sessions may connect. API tokens require `alerts:read`; browser
`EventSource` clients may pass the token as `?token=` when they cannot set an Authorization header.
Emergency disable rejects token-backed streams on either durable URL while leaving local session
recovery available.

## Services

```
GET  /api/services
POST /api/services/:name/action   { action: start|stop|restart|enable|disable }
GET  /api/services/:name/logs
```

## Containers

```
GET  /api/containers
GET  /api/containers/images
POST /api/containers/:id/action   { action: start|stop|restart|remove, dry_run?: boolean }
GET  /api/containers/:id/logs
GET  /api/containers/:id/exec     WebSocket PTY
GET  /api/containers/:id/compose
POST /api/containers/:id/compose/propose   { path, content }
POST /api/containers/:id/compose/apply     { path?, content }
```

Container mutations submit through the durable operation boundary. A normal action/apply returns
`202 { "job": ... }`; `dry_run: true` returns the current advisory plan without creating a job.
Callers may provide `Idempotency-Key`; compatibility callers that omit it receive legacy
at-most-once-per-request behavior through a generated key. Compose content is validated and staged
as a controlled opaque artifact before the job is submitted—the handler never applies it directly.

## App Vault

```
GET  /api/apps/catalog
GET  /api/apps/deployed
POST /api/apps/deploy              { app_id, project_name?, env_overrides? }
POST /api/apps/:name/start|stop|restart|redeploy
GET  /api/apps/:name/compose
POST /api/apps/:name/compose       { content }
POST /api/apps/:name/expose        { domain, ssl?, allow_embed? }
POST /api/apps/open-ui             { project_name, primary_port }
GET  /api/apps/:name/logs
GET  /api/apps/:name/status
DELETE /api/apps/:name
```

App exposure submits `proxy.rule.create` and returns a durable job. `open-ui` is a read-only lookup:
it returns an existing valid embed proxy when available and never creates proxy, nginx, database,
or firewall state. Its response includes `proxy_available`; `proxy_created` is always `false`.

## Models

```
GET  /api/models
POST /api/models/download          { url, filename? }
GET  /api/models/download/:id      Admin or owner session; Bearer denied
GET  /api/models/active
POST /api/models/load              { filename }
DELETE /api/models/:filename
POST /api/models/ollama/pull       { model }
GET  /api/models/ollama/pull/:id   Admin or owner session; Bearer denied
POST /api/models/ollama/create     { filename }
GET  /api/models/ollama/create/:id Admin or owner session; Bearer denied
```

The legacy model mutation POST endpoints (`/api/models/load`, `/api/models/llama-config`,
`/api/models/ollama-config`, and `/api/models/ollama/create`) authenticate first and
return `503 feature_unavailable` until canonical operation adapters exist. They do not
write compose files, invoke Docker, or mutate provider state.

The public OpenAI-compatible proxy remains available at:

```
POST /v1/chat/completions       { model, messages, ... }
```

It forwards inference requests to the local llama.cpp server and does not implicitly
switch or reload models. Model selection/loading must be performed through a future
canonical operation boundary; callers should target the model currently served by the
configured local runtime.

## AI / GPU

```
GET  /api/ai/llama
POST /api/ai/llama/unload
POST /api/ai/ask              { query, context?, provider_id? }   → SSE stream
GET  /api/ai/context
```

## AI Providers

```
GET    /api/ai/providers
POST   /api/ai/providers                { kind, name, base_url?, api_key_ref?, api_key_value?, model?, priority? }
PUT    /api/ai/providers/:id            { name?, enabled?, base_url?, model?, priority?, api_key_value? }
DELETE /api/ai/providers/:id
GET    /api/ai/providers/:id/health
```

Valid `kind` values: `odysseus` · `openai` · `anthropic` · `local`.  
The orchestrator picks the enabled provider with the lowest `priority` number. Pass `provider_id` in `/api/ai/ask` to pin a specific provider for that request.  
API key values are stored in the `settings` table under the name given in `api_key_ref`.

## VMs

```
GET  /api/vms/local
POST /api/vms/local/action         { name, action }
GET  /api/vms/proxmox/config
POST /api/vms/proxmox/config
GET  /api/vms/proxmox/vms
POST /api/vms/proxmox/action       { vmid, kind, node, action }
POST /api/vms/proxmox/test
```

The three legacy Proxmox configuration/action/test mutations return durable jobs. Local libvirt VM
actions remain synchronous because no local-VM durable action is registered.

## Files

```
GET  /api/files/roots
GET  /api/files/list?path=
GET  /api/files/read?path=
GET  /api/files/raw?path=
POST /api/files/write              { path, content }
POST /api/files/mkdir              { path }
DELETE /api/files/delete?path=
POST /api/files/rename             { from, to }
```

Filesystem mutation routes authenticate an owner/admin session and currently return
`503 { "error": { "code": "feature_unavailable", "message": "filesystem mutations require a canonical operation adapter" } }`.
They never write, create, rename, or delete host paths until a typed canonical action adapter exists.
Read-only file listing, reading, and raw serving remain separate paths.

## Plugins and repository mods

```
GET  /api/plugins
POST /api/plugins                    { url }
PATCH /api/plugins/:id               { enabled? }
DELETE /api/plugins/:id
GET  /api/mods
POST /api/mods/config                { url, branch }
POST /api/mods/fetch
GET  /api/mods/diff
POST /api/mods/apply
POST /api/mods/rollback
```

Plugin install/update/uninstall and repository-mod fetch/apply/rollback routes authenticate an
owner/admin session and return the same bounded `503 feature_unavailable` response until canonical
operation adapters exist. They do not download archives, alter the plugin database/filesystem,
run Git commands, merge, or reset the host directly. The separate `POST /api/mods/config` route only
stores the selected source settings and remains a bounded configuration mutation; it does not fetch
or apply a repository. Status, configuration, and diff reads remain available where registered.

## Proxy

```
GET  /api/proxy
POST /api/proxy                    { domain, upstream, ssl, allow_embed? }
DELETE /api/proxy/:id
PUT  /api/proxy/:id
POST /api/proxy/:id/toggle
POST /api/proxy/nginx/action       { action: start|stop|restart|reload }
```

Proxy mutations return durable jobs. The nginx `test` action is an informational synchronous read.

## Firewall

```
GET  /api/firewall
POST /api/firewall/rules           { action, direction?, port?, proto?, from?, comment?, dry_run? }
POST /api/firewall/rules/delete    { num }
POST /api/firewall/action          { action: enable|disable|reload|reset }
```

Firewall mutations return durable jobs; `dry_run: true` returns an advisory plan.

## WireGuard

```
GET  /api/wireguard
POST /api/wireguard/peers          { name, interface, server_endpoint? }
DELETE /api/wireguard/peers/:id
```

WireGuard peer mutations authenticate an owner/admin session and return bounded `503 feature_unavailable`
until a canonical WireGuard resource/action adapter exists. They do not invoke `wg`, write interface
configuration, or mutate peer records from the compatibility handler. The authenticated read path remains
available for status inspection.

## Storage

```
GET  /api/storage/devices
GET  /api/storage/mounts
POST /api/storage/mount
POST /api/storage/umount
GET  /api/storage/fstab
POST /api/storage/fstab
DELETE /api/storage/fstab/:idx
GET  /api/storage/smart/:dev
GET  /api/storage/raid
POST /api/storage/raid/create
POST /api/storage/raid/stop
POST /api/storage/format
GET  /api/storage/paths
POST /api/storage/paths
```

Storage mutations, including mount/umount, fstab, RAID, format, and storage-path settings, authenticate
an owner/admin session and return bounded `503 feature_unavailable` until a canonical storage adapter
exists. They do not invoke host storage commands or write settings from compatibility handlers. Device,
mount, fstab, SMART, RAID, and configured-path reads remain separate authenticated projections.

## Network

```
GET /api/network
GET /api/network/neighbors
```

## Backups

```
GET  /api/backups
POST /api/backups                  { name, source_path, repo_path, password }
POST /api/backups/:id/run
POST /api/backups/:id/check
POST /api/backups/:id/restore-test
DELETE /api/backups/:id
```

All five Backup mutation/check routes return durable jobs. `check` and `restore-test` are durable
read operations and do not require approval.

## Alerts & status checks

```
GET  /api/alerts?state=&severity=
POST /api/alerts/:id/acknowledge
POST /api/alerts/:id/resolve
DELETE /api/alerts/:id
GET  /api/status-checks
POST /api/status-checks            { name, type, target, interval_secs? }
DELETE /api/status-checks/:id
GET  /status                       Public HTML page (no auth)
```

## Automation

```
GET  /api/automation
POST /api/automation               { name, command, schedule, enabled? }
PATCH /api/automation/:id
DELETE /api/automation/:id
POST /api/automation/:id/run
GET  /api/automation/:id/runs
```

## Secrets

```
GET  /api/secrets
POST /api/secrets                  { name, description, value }
PATCH /api/secrets/:id
DELETE /api/secrets/:id
GET  /api/secrets/:id/reveal
```

## Tags

```
GET  /api/tags
POST /api/tags                     { name, color }
PATCH /api/tags/:id
DELETE /api/tags/:id
GET  /api/tags/map?type=
POST /api/tags/assign              { tag_id, resource_type, resource_id }
POST /api/tags/unassign            { tag_id, resource_type, resource_id }
```

## Timeline

```
GET /api/timeline?limit=&offset=&category=&outcome=&search=
```

## Users

```
GET  /api/users
POST /api/users                    { username, password, role }
DELETE /api/users/:id
POST /api/users/me/password        { password }
```

## Security

```
GET  /api/security/sessions
POST /api/security/sessions/revoke-others
DELETE /api/security/sessions/:id
```

## Updates

```
POST /api/updates/docker/check
POST /api/updates/docker/:id/apply
POST /api/updates/odysseus/apply
POST /api/updates/os/apply
POST /api/updates/voidtower/check
POST /api/updates/voidtower/apply
POST /api/updates/voidtower/rollback
GET  /api/system/update-check
POST /api/system/update
```

These compatibility routes return durable jobs, including the slow check operations. The GET
`/api/system/update-check` alias is retained for compatibility; new callers should use the POST
VoidTower check action and follow its returned job.

## System

```
GET  /api/system/version       Authenticated session or diagnostics:read Bearer
GET  /api/system/update-check
POST /api/system/restart
POST /api/system/update
```

`POST /api/system/restart` authenticates an owner/admin session, then returns `503
feature_unavailable` until a canonical system lifecycle adapter exists. It does not write a restart
script, spawn a process, signal the server, or otherwise mutate the host directly. System updates and
update checks remain on their separately adopted durable operation paths.

## Integrations

```
GET  /api/integrations/scopes
GET  /api/integrations/tokens
POST /api/integrations/tokens                    { name, scopes[], expires_days? }
DELETE /api/integrations/tokens/:id
GET  /api/integrations/odysseus/config
POST /api/integrations/odysseus/config           { enabled?, mcp_enabled?, allowed_url?, webhook_secret?, emergency_disable? }
GET  /api/integrations/odysseus/manifest
GET  /api/integrations/events                    Durable cursor-resumable SSE alias
GET  /api/integrations/events/legacy             Deprecated metrics/audit SSE
POST /api/integrations/webhooks                  { automation_id?, action?, resource_id?, dry_run? }
GET  /api/integrations/actions
```

Webhook `container.start`, `container.stop`, and `container.restart` actions return a durable job;
their dry runs return a canonical plan. `service.*` and `automation_id` webhook requests retain the
legacy synchronous `{ "ok": true, ... }` response until matching durable actions are introduced.

## Voidwatch (Odysseus-side)

```
GET  /api/voidwatch/config
POST /api/voidwatch/config         { enabled, base_url, api_token, webhook_secret, auto_action_policy }
POST /api/voidwatch/emergency-disable
POST /api/voidwatch/test
GET  /api/voidwatch/manifest
GET  /api/voidwatch/toolpacks
GET  /api/voidwatch/actions
POST /api/voidwatch/webhook
```

## Capabilities & diagnostics

```
GET /api/capabilities   Authenticated session or diagnostics:read Bearer
GET /api/diagnostics    Admin/owner session or diagnostics:read Bearer
```
