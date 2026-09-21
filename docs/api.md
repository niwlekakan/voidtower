# API Reference

`backend/contracts/api-v1-envelope-contract.json` is the source-owned contract artifact for the versioned canonical operation responses and the shared frontend generated artifact. The current API version is `1`; clients may omit `x-voidtower-api-version` for compatibility, while an explicit unsupported, malformed, or duplicate value receives `406` with the bounded `unsupported_api_version` envelope. Successful responses echo `x-voidtower-api-version: 1`.

Compatibility mutation source boundary:

- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check` is the credential-safe developer/CI check over production Rust under `backend/src/api`; CI adds `--base "$BASE_COMMIT"` from a pull-request base or a protected-branch push baseline; unprotected push approval contexts are rejected. It validates Rust syntax through `rustfmt`, resolves the bounded import/alias and inline module/`impl` forms covered by its fixtures, excludes selected `#[cfg(test)]` items, and recognizes the covered provider/filesystem/process call shapes rather than textual comments or strings. Local runs default to `HEAD` only in a Git checkout. A checkout without a resolvable Git base fails closed. Exit `0` means the inventory is clean, exit `1` means unknown findings were emitted, and exit `2` means parser, source-layout, registry, Git-approval, or evidence setup failed.
- The enforcement workflow is `.github/workflows/compatibility-enforcement.yml`. Its `pull_request_target` job checks out the approved base into `trusted-verifier/`, checks out the PR head (including fork heads) into `candidate-source/` as inert data, and executes only the base verifier against the candidate. Both checkouts pin `actions/checkout` to a reviewed commit and disable persisted credentials; the candidate checkout explicitly opts into fork materialization but is never executed. If the approved base does not contain the verifier artifact, the job fails closed and maintainers must bootstrap the artifact on a protected branch; the normal `ci.yml` job runs fixtures but does not execute PR-controlled verifier code as an enforcement decision.
- Canonical delegation is proven only by the exact resolved targets `operation_adoption::{submit,submit_with_key,prepare}`, their `super::`/`crate::api::` forms, or `crate::operations::invocation::{submit,prepare}`; broad `crate::operations::`, `crate::networking::`, `crate::cmdb::`, and `super::support` prefixes are not trusted. Reviewed CMDB/support service calls and local proxy helpers use explicit exact identities. Same-module helper propagation excludes nested functions, closures, and async blocks and ignores imported canonical names shadowed by parameters, destructuring/`if let`/`for`/`match` bindings, locals, or closure parameters; untrusted mutation aliases become unknown instead of inheriting a module-prefix proof. Unsupported re-exports, wildcard imports, UFCS/qualified associated mutation dispatch (including filesystem `File` mutators and typed receivers), and helper names that merely end in `prepare_or_submit` also fail closed. This is still not compiler-grade Rust call resolution, so unsupported syntax/provenance must remain an explicit source-check failure rather than a safety claim.
- The parser contract is intentionally bounded and executable: `scripts.test_compatibility_mutation_inventory.test_rust_mutation_syntax_contract_is_explicit_and_never_silent` covers qualified calls, UFCS, generic calls, borrowed typed receivers, mutation function values, helper-returned receivers, and exact canonical delegation. Each form must emit either its recognized marker or an explicit unsupported/provenance marker; a form that emits no marker is a test failure. This contract is not a claim to resolve arbitrary Rust semantics.
- Deferred compatibility exceptions use registry identities and checkout-local SHA-256 body evidence for deterministic metadata, then compare both the complete active registry and every evidence-bound function with the protected Git base supplied by CI. Updating a registry entry, body, or local digest together therefore fails closed until the change is present in an already-approved base commit; a new or moved exception has no approved base. The checker reports only file, function, line, marker, classification, bounded reason metadata, and the redacted base commit ID; it is not provider, runtime, browser, or release qualification.

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
Unknown bearer routes remain denied. Handler checks remain defense in depth. MCP's two handlers are
explicitly `Unscoped` in the registry so a valid token can reach the protocol authentication
handler; individual tools then enforce their exact declared MCP scope at the canonical invocation
boundary. The two embed-router paths explicitly retain their existing unscoped bearer-to-session
bridge because that sub-router does not mount scope enforcement.

## Built-in MCP and Studio tool boundary

Built-in MCP access is disabled unless the `odysseus.mcp_enabled` setting is `true`. When enabled,
clients use an API-token bearer credential with the following protocol endpoints:

```
GET  /api/mcp
POST /api/mcp/message
```

`GET /api/mcp` returns a short-lived SSE endpoint event pointing at `/api/mcp/message`. The POST
body is JSON-RPC 2.0 (`jsonrpc`, `id`, `method`, and optional `params`); malformed JSON returns a
bounded `400` JSON-RPC error, while unknown request fields, an unsupported JSON-RPC version,
unknown `tools/call` parameter or tool-argument fields, invalid identifier types, explicit `null` arguments, and non-object parameters are rejected
without dispatch. The message and Studio invoke bodies are capped at 64 KiB. Protocol errors use
JSON-RPC error codes; tool and policy failures are returned as a bounded `result` with `isError:
true` and never bypass the shared invocation boundary. Authentication runs before JSON parsing.

The currently approved built-in mutation is `container.start`. It requires the `containers:restart`
token scope, a canonical container resource ID, and a caller-supplied `request_id`. The handler
submits through the typed action registry and durable operation kernel, so the response is a
serialized job summary and repeating the same request ID replays the existing job. Unknown tools,
wrong scopes, unknown resources, policy denials, and invalid typed arguments fail closed before a
provider mutation. Tool output is passed through the shared AI redaction choke point.

The Studio tool panel uses the same tool registry and invocation choke point with a session cookie:

```
GET  /api/studio/mcp/tools
POST /api/studio/mcp/invoke   { "name": "...", "arguments": { ... } }
```

Studio invocation rejects malformed JSON with `400 bad_request`, unsupported content types with
`415 unsupported_media_type`, and unknown request fields with the standard `422
unprocessable_entity` validation envelope. It returns `{ "ok": false, "error": "..." }` for
bounded tool/policy failures. Session authorization does not make an unapproved action callable.
`POST /api/ai/ask` is a provider-streaming chat endpoint only; its prompt explicitly does not
execute mutation tools. AI mutation requests must use an approved typed MCP/Studio action and are
not inferred from chat text.

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

All durable-operation responses negotiate API version `1` through the optional
`x-voidtower-api-version` request header. A successful response echoes that version. Job and
approval reads use source-owned v1 envelopes: job lists return
`{ "schema_version": 1, "jobs": [...] }`, approval lists return
`{ "schema_version": 1, "approvals": [...] }`, and an individual approval returns
`{ "schema_version": 1, "approval": {...} }`. Job detail and idempotency lookup use the v1 read
envelope; accepted submission, cancellation, approval, and rejection use the v1 job envelope with
`resource_id`, `action`, and `job`. The checked-in examples and generated frontend artifact come
from `backend/contracts/api-v1-envelope-contract.json`; run `node scripts/generate-api-contract.mjs
--check` to detect drift. A repeated decision for an approval that is no longer pending returns
the bounded `409 approval_conflict` envelope and never exposes internal decision diagnostics.

Planning returns `200` and has no durable side effect. Canonical submission requires an
`Idempotency-Key` header and returns `202` with the same v1 job envelope when the request is accepted. Keys are
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

The adopted compatibility inventory is executable and validated by
`backend/src/operations/registry.rs`; it must not be copied into documentation as a mutable route
count. Each mapped durable branch returns `202` with the same v1 job envelope as canonical
submissions: `{ "schema_version": 1, "resource_id": "...", "action": "...", "job": ... }`.
A compatibility `dry_run: true` returns an advisory plan and creates no job. App Vault/model Compose lifecycle, AI proxy-settings
orchestration, service webhook actions, and ephemeral Proxmox VNC ticket creation remain synchronous
exceptions because they do not yet have matching durable actions. Automation webhook actions use the
canonical `automation.run` durable action described below. The
current web clients follow submitted jobs locally and link to shared job detail. Owner/admin/operator
sessions can list and inspect the newest 50 jobs in Tower or Void Mode; cancellation is offered only
for queued/running records. Owner/admin sessions can list and decide exact immutable approvals with
an optional comment of at most 500 characters. These shared workflows use bounded, visibility-aware HTTP polling and never
retry a mutation automatically. `/api/events` exposes durable history; `/api/events/stream` and
`/api/integrations/events` expose the same cursor-resumable durable SSE stream. Shared operation
views use it only to invalidate authoritative HTTP reads and retain bounded polling as fallback.

### Compatibility source-boundary check

The repository also enforces the compatibility boundary from production source rather than a copied
route total. Run `python3 scripts/compatibility_mutation_inventory.py --repo . --check` from the
repository root. The report contains only file, function, line, marker, classification, and reason
metadata; it never emits source lines, request bodies, URLs, or credential values. Canonical adapter
delegation, authentication-first deferred handlers, read-only probes, the inference-only model
proxy, the explicit Proxmox VNC exception, and local synchronous exceptions are classified separately.
Unknown provider or destructive callsites fail the check. Generic notification-webhook delivery is
classified as separate outbound-egress work and is not counted as canonical mutation convergence.
The checker validates each allowlisted function against the source digest and file recorded in
`scripts/compatibility_mutation_exception_evidence.json`, and against the protected Git base passed
with `--base`; stale, missing, extra, changed, new, or out-of-checkout evidence fails closed.
Unsupported macros, path aliases, source forms, and syntax are not assumed safe. Exact canonical
adapter call shapes are required; a similarly named local helper, a private or public re-export, an
unknown wildcard import, a closure/async-block-local unavailable error, local shadowing, or
UFCS/qualified associated mutation dispatch cannot authorize a provider/filesystem/process marker.
Rustfmt is installed by CI before the parser runs. A legitimate exception-body change must land in
an approved base first, followed by its regenerated evidence digest; the base comparison is the
approval boundary rather than a self-updatable checkout manifest.

---

## AI Studio generation boundary

The AI Studio generation routes are authenticated compatibility seams, but they do not yet have canonical AI/media resource and action adapters. Until those adapters exist, they authenticate the session first and return a bounded `503` response:

```
POST /api/studio/image/generate
POST /api/studio/tts/generate
POST /api/studio/stt/transcribe

503 { "error": { "code": "feature_unavailable", "message": "... canonical operation adapter" } }
```

Unauthenticated requests receive `401` before request-body validation or provider access. The routes do not contact SD WebUI, ComfyUI, Kokoro, or Whisper, and do not write generated media while this boundary is deferred. Read-only Studio status, gallery listing, media serving, and deletion remain separate routes; their availability does not imply generation support.

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

Owner, admin, and operator sessions may connect. API tokens require `alerts:read` and must be sent
in the Authorization header (for example, `Bearer ***`); query-string tokens are rejected and are never accepted
as an EventSource fallback. Emergency disable rejects token-backed streams on either durable URL
while leaving local session recovery available.

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
GET  /api/containers/:id/exec     Interactive shell (currently unavailable)
GET  /api/containers/:id/compose
POST /api/containers/:id/compose/propose   { path, content }
POST /api/containers/:id/compose/apply     { path?, content }
```

Container mutations submit through the durable operation boundary. A normal action/apply returns
`202` with the v1 job envelope `{ "schema_version": 1, "resource_id": "...", "action": "...", "job": ... }`;
`dry_run: true` returns the current advisory plan without creating a job.
Callers may provide `Idempotency-Key`; compatibility callers that omit it receive legacy
at-most-once-per-request behavior through a generated key. Compose content is validated and staged
as a controlled opaque artifact before the job is submitted—the handler never applies it directly.

Interactive container execution is intentionally fail-closed: an authenticated operator receives
`503 { "error": { "code": "feature_unavailable", "message": "interactive container shells require the canonical operation adapter" } }`.
The endpoint does not open a WebSocket or invoke `docker exec`. Container logs remain read-only.

## Terminal and SSH sessions

```
GET /api/terminal/ws                 Local shell (currently unavailable)
GET /api/terminal/ssh/sessions       SSH session metadata and encrypted credential reference CRUD
GET /api/terminal/ssh/ws?session_id= Interactive SSH shell (currently unavailable)
GET /api/terminal/local/sessions     Local session metadata CRUD
```

The local and SSH interactive shell endpoints authenticate first and return bounded `503
feature_unavailable` until canonical shell/session action adapters exist. They do not spawn a local
shell, open an SSH connection, or expose a WebSocket upgrade. Session metadata endpoints remain
available; SSH passwords are stored through the encrypted secret reference boundary and are never
returned in responses. This limitation is intentional and is not runtime or release qualification.

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

App Vault read responses use the standard `{ "error": { "code", "message" } }` envelope on failure.
Member sessions can only read and proxy their own deployed apps; requests for another member's app use the same not-found envelope as a missing app. Recognized application roles are enforced before Docker or filesystem access, and host-wide external-stack discovery is limited to owner/admin/operator sessions. Compose reads return `{ "content" }` only, are capped at 256 KiB, and
redact values on sensitive keys such as passwords, secrets, tokens, and API keys. The self-access response at
`/api/members/me/access` uses a separate DTO that omits administrator-only drive host paths. Deployed-app and
external-stack responses do not expose host compose paths or storage roots. App logs are capped at 64 KiB by UTF-8-safe
bytes, and Docker status/log failures return a bounded `503` instead of a successful empty/fallback
response; Docker command output is drained concurrently, capped at 64 KiB per stream, and terminated
after 30 seconds; Docker external discovery is capped at 1 MiB of container metadata and uses the same
timeout. `open-ui` validates the recognized application-role and member-ownership boundary, requires the requested port
to match the stored app, and rejects malformed Host authorities before constructing its URL. The embed
proxy preserves the incoming query string, disables redirects, removes cookies and hop-by-hop/frame-policy
headers, rejects non-success upstream responses and oversized paths/queries, and caps successful bodies at 4 MiB.
Compatibility mutation routes fail closed at the role boundary: deploy and custom deploy accept owner/admin/operator/member sessions, with member custom deploy additionally requiring `member_settings.can_deploy_custom`; adopt, convert, pull, and cancel require an operator-capable session. Host-wide external discovery requires owner/admin/operator. These routes still stop at the canonical operation-adapter boundary before provider mutation.

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

`POST /api/ai/llama/unload` authenticates an owner/admin session but currently returns `503 feature_unavailable` until a canonical AI process lifecycle adapter exists. It does not signal or terminate host processes. `GET /api/ai/llama` remains a read-only status projection.

The model lifecycle mutation family (`POST /api/models/download`, `DELETE /api/models/:filename`,
`POST /api/models/load`, `POST /api/models/llama-config`, `POST /api/models/ollama-config`,
`POST /api/models/ollama/pull`, and `POST /api/models/ollama/create`) authenticates an owner/admin
session first and then returns bounded `503 feature_unavailable` until canonical model/resource
operation adapters exist. These compatibility handlers do not create files, delete model files,
spawn detached work, call llama.cpp/Ollama, write compose configuration, invoke Docker, or emit
mutation success audit. Their existing status/progress implementations remain unreachable from
these mutation handlers; callers must not retry an uncertain request as a new mutation.


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
API key values are encrypted in the secret manager and referenced by canonical secrets.id; plaintext keys are never returned by provider APIs. Provider names/models and base URLs are bounded; only credential-free HTTP(S) URLs are accepted, and local/private/link-local/metadata and other special targets are rejected. Before every external provider health, completion, or streaming request, the hostname is resolved and every result is checked; the safe address is pinned for that request and redirects are disabled. The explicitly local provider may use loopback or RFC1918 targets for a local LLM, but still pins DNS and rejects metadata/link-local/special targets. This closes hostname-based DNS rebinding for external provider clients. Health failures return a generic provider health check failed message. External provider and Docker/App Vault runtime qualification remains a separate gate.

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
Authentication is performed before request-body or query validation, so unauthenticated malformed requests
receive the bounded `401 unauthorized` envelope rather than an extractor `400`/`422`. They never write,
create, rename, or delete host paths until a typed canonical action adapter exists. Read-only file listing,
reading, and raw serving remain separate paths.

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
operation adapters exist. Authentication precedes body parsing on deferred mutation routes, including
malformed requests, which receive `401 unauthorized` before any `400`/`422` extractor response. They do
not download archives, alter the plugin database/filesystem, run Git commands, merge, or reset the host
directly. The separate `POST /api/mods/config` route only stores the selected source settings and remains
a bounded configuration mutation; it does not fetch or apply a repository. Status, configuration, and diff
reads remain available where registered.

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
until a canonical WireGuard resource/action adapter exists. Authentication occurs before body parsing, so
malformed unauthenticated calls receive `401 unauthorized` rather than `400`/`422`. They do not invoke `wg`,
write interface configuration, or mutate peer records from the compatibility handler. The authenticated read
path remains available for status inspection.

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
exists. Authentication precedes request-body parsing, so malformed unauthenticated requests receive `401
unauthorized` rather than `400`/`422`. They do not invoke host storage commands or write settings from
compatibility handlers. Device, mount, fstab, SMART, RAID, and configured-path reads remain separate
authenticated projections.

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
POST /api/automation               { name, description?, command, schedule?, timeout_secs?, enabled? }
PATCH /api/automation/:id          { name?, description?, command?, schedule?, timeout_secs?, enabled? }
DELETE /api/automation/:id
POST /api/automation/:id/run
GET  /api/automation/:id/runs?limit=
```

Automation list and run-history responses include commands and captured output, so they require an
operator session (owner, admin, or operator). Viewer/member-style sessions are rejected with
`403 forbidden`. Run-history `limit` must be between 1 and 200.

Create/update bodies require `Content-Type: application/json`, reject unknown fields. Names are required and capped at 200 characters,
commands are required and capped at 8192 characters, descriptions are capped at 4000 characters,
and `timeout_secs` must be between 1 and 3600. Supported schedules are `@minutely`, `@hourly`,
`@daily`, `@midnight`, `@weekly`, `@monthly`, or `*/N` with an optional `min`/`minutes` unit where
N is 1–1440. Invalid schedules and timeouts return the bounded `bad_request` error envelope.

`POST /api/automation/:id/run` and scheduler submissions converge on the canonical
`automation.run` durable-job path; the HTTP response is job acceptance, not provider execution.

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
POST /api/integrations/odysseus/config           { enabled?, mcp_enabled?, allowed_url?, regenerate_webhook_secret?, revoke_webhook_secret?, emergency_disable? }
GET  /api/integrations/odysseus/manifest
GET  /api/integrations/events                    Durable cursor-resumable SSE alias
GET  /api/integrations/events/legacy             Deprecated metrics/audit SSE
POST /api/integrations/webhooks                  { automation_id?, action?, resource_id?, dry_run? }
GET  /api/integrations/actions
```

Webhook `container.start`, `container.stop`, and `container.restart` actions return a durable job;
their dry runs return a canonical plan. `automation_id` webhook requests also return a durable
`automation.run` job with the same dry-run and idempotency contract. `service.*` webhook requests
remain explicitly deferred: policy-denied requests return `403 policy_denied`, while an allowlisted
request returns `503 feature_unavailable` until a matching durable action is introduced.

Inbound webhook authentication is a signed-request contract, not a Bearer token. Send all three
headers below and compute the lowercase hexadecimal HMAC-SHA256 over the exact UTF-8 request body
using the canonical message `timestamp.nonce.raw_body`:

```
X-VoidTower-Timestamp: 1726838400
X-VoidTower-Nonce: 01JABCDEF-webhook-delivery
X-VoidTower-Signature: sha256=<hmac_sha256_hex>
Content-Type: application/json
```

The timestamp is Unix seconds and must be within ±300 seconds of VoidTower time. Nonces are 1–128
ASCII bytes restricted to letters, digits, `-`, `_`, `.`, or `~`. A source/nonce pair is accepted
only once; a duplicate delivery returns `409 webhook_replay` and creates no second job. Replay
receipts are retained for 15 minutes. Missing, malformed, stale, tampered, or Bearer-only
credentials return the bounded `401 webhook_authentication_failed` envelope. The raw body remains
capped at 64 KiB; oversized bodies return `413 payload_too_large`, and JSON parsing happens only
after the signature and replay checks.

`Idempotency-Key` remains an independent canonical job key: a new signed delivery with the same
key and unchanged intent replays the existing job, while a changed intent returns `409 conflict`.

The inbound webhook credential is stored encrypted in the secret manager and is never returned by
the GET config route or included in the status hint. An explicit regeneration request returns the
new credential once in `webhook_secret`; clients must display/store it immediately and must not
persist it in application settings. `revoke_webhook_secret: true` disables the current credential
without deleting its metadata. Legacy `odysseus.webhook_secret` settings are migrated at startup
transactionally; if migration fails, the legacy value remains for recovery and startup fails closed
rather than silently disabling signed authentication.

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
