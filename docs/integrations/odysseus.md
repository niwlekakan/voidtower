# Odysseus / Voidwatch Integration

This document covers how to connect Odysseus (with the Voidwatch integration) to VoidTower.

> **Architecture note (2026-06-29):** Odysseus is now one optional AI provider inside VoidTower's multi-provider AI orchestrator (`backend/src/ai/`). VoidTower does **not** import or modify Odysseus source — it calls it via HTTP only. You can use OpenAI, Anthropic, or a local LLM instead of, or alongside, Odysseus. Configure providers at **Settings → AI Providers** (`/ai-providers`). The legacy `odysseus.allowed_url` settings key still works as a fallback if no providers are configured in the new table.

## Overview

Voidwatch is the AI operations layer in Odysseus. It connects to VoidTower via a scoped API token and exposes MCP tools that let Odysseus agents safely observe and manage your infrastructure.

VoidTower already ships with Odysseus integration support:
- Scoped API tokens (25 scopes)
- Tool manifest endpoint
- Server-Sent Events for real-time alerts
- Signed webhook inbound support
- Agent action audit trail

---

## Token Setup

### Create a Token

1. Go to **VoidTower → Settings → Integrations → API Tokens**
2. Click **New Token**
3. Name it (e.g. `odysseus-voidwatch`)
4. Select scopes (see below)
5. Optionally set an expiry
6. Click **Create** — copy the token immediately (shown once)

### Recommended Scopes for Voidwatch

**Read-only (safe baseline):**
```
metrics:read
services:read
containers:read
containers:logs
apps:read
backups:read
alerts:read
alerts:ack
automation:read
timeline:read
network:read
storage:read
diagnostics:read
proxy:read
tags:read
secrets:list
vms:read
```

**Add for safe actions:**
```
automation:run
backups:run
containers:restart
apps:restart
```

**Other privileged actions (not API-token scopes):**
- Service restart is available only through the authenticated session API and is not exposed as an API-token scope or integration tool.
- `apps:deploy` and `proxy:manage` are higher-privilege actions; expose them through tokens only if a future policy explicitly permits it.


### Token Revocation

To revoke a token: **Settings → Integrations → API Tokens → Delete**.

All tokens are stored as SHA-256 hashes — the plaintext is never stored after creation.

---

## API Endpoints Used by Voidwatch


The endpoints below are integration endpoints. Service restart is excluded: it is available only through the separately authenticated session API, not through API tokens or Odysseus tools.
Voidwatch uses existing VoidTower API endpoints. No additional backend setup is required.

| Endpoint | Purpose |
|----------|---------|
| `GET /api/auth/me` | Connection test |
| `GET /api/integrations/odysseus/manifest` | Tool manifest |
| `GET /api/metrics/current` | System metrics |
| `GET /api/services` | Service list |
| `GET /api/containers` | Container list |
| `GET /api/containers/{id}/logs` | Container logs |
| `POST /api/containers/{id}/action` | Container control |
| `GET /api/apps/deployed` | Deployed apps |
| `GET /api/apps/{project}/status` | App status |
| `GET /api/apps/{project}/logs` | App logs |
| `GET /api/apps/{project}/compose` | App compose file |
| `POST /api/apps/{project}/restart` | Restart app |
| `GET /api/backups` | Backup list |
| `POST /api/backups/{id}/run` | Run backup |
| `POST /api/backups/{id}/check` | Verify backup |
| `GET /api/alerts` | Alert list |
| `POST /api/alerts/{id}/acknowledge` | Acknowledge alert |
| `POST /api/alerts/{id}/resolve` | Resolve alert |
| `GET /api/automation` | Automation jobs |
| `POST /api/automation/{id}/run` | Run automation |
| `GET /api/audit` | Audit log |
| `GET /api/timeline` | Filtered timeline |
| `GET /api/diagnostics` | Health checks |
| `GET /api/capabilities` | Tool detection |
| `GET /api/proxy` | Proxy rules |
| `GET /api/proxy/nginx/status` | Nginx status |
| `POST /api/proxy/nginx/action` | Nginx control |
| `POST /api/proxy` | Add proxy rule |
| `GET /api/tags` | Resource tags |
| `GET /api/secrets` | Secret list (names only) |
| `GET /api/network/neighbors` | LAN neighbors |
| `GET /api/storage/devices` | Block devices |
| `GET /api/storage/mounts` | Mount points |
| `GET /api/integrations/actions` | Agent audit entries |

---

## Odysseus Config Endpoint

VoidTower exposes a dedicated Odysseus config endpoint:

```
GET  /api/integrations/odysseus/config   → Current Odysseus integration config
POST /api/integrations/odysseus/config   → Update config (enable, URL, webhook secret, emergency disable)
GET  /api/integrations/odysseus/manifest → Public tool manifest (no auth required)
```

**Emergency disable** via config update sets `emergency_disable: true`, which blocks all agent-triggered actions immediately.

---

## Odysseus URL and Webhook Configuration

The `allowed_url` value is the Odysseus base URL used by the legacy AI fallback and the
administrator-only theme synchronization route. It is not an arbitrary server-side fetch target.
VoidTower validates it when saved and revalidates it immediately before each outbound request:

- only `http://` and `https://` URLs are accepted;
- credentials, query strings, fragments, and values longer than 2048 bytes are rejected;
- the hostname is resolved before use and every resolved address must avoid link-local, metadata,
  multicast, documentation, and other special-purpose ranges;
- loopback and RFC1918 addresses are allowed only for a self-hosted Odysseus instance; link-local,
  metadata, and other prohibited ranges remain blocked;
- the resolved addresses are pinned for the request, ambient proxy settings are ignored, and
  redirects are not followed.

Non-empty invalid values return the standard `400 bad_request` error envelope and are not persisted.
Submitting an empty `allowed_url` explicitly clears the stored endpoint. A runtime validation or
request failure returns a bounded generic error; transport diagnostics and the configured URL are
not echoed to the caller.

To configure the URL and inbound webhook credential:

1. In VoidTower: **Settings → Integrations → Odysseus Config**
2. Set **Odysseus base URL** (for example `http://odysseus-host:7000` on a trusted private LAN)
3. Enable **MCP**
4. Click **Regenerate** under **Webhook secret** and copy the returned value immediately. The value
   is shown once; VoidTower stores only its encrypted secret-manager representation.
5. Use **Revoke** to disable the current credential without deleting its metadata. Regenerate to
   issue a replacement.

In Odysseus: **Settings → Integrations → Voidwatch**
- Set `webhook_secret` to the copied one-time value
- Ensure `webhook_enabled: true`

`GET /api/integrations/odysseus/theme` uses the configured base URL and returns `{ "name": "…" }`
when Odysseus responds with a valid bounded theme payload. Non-success, oversized, malformed, or
unreachable upstream responses return a bounded `400 bad_request` response. Redirect responses are
not followed.

### Triggering VoidTower from Odysseus

Inbound webhook requests use HMAC-SHA256 over the exact raw request body. The canonical message is
`timestamp.nonce.raw_body`, where `timestamp` is Unix seconds and `nonce` is a fresh 1–128 byte
ASCII delivery identifier (`A-Z`, `a-z`, `0-9`, `-`, `_`, `.`, or `~`). The signature header is
`sha256=` followed by 64 hexadecimal characters (the verifier accepts either hexadecimal case):

```http
POST /api/integrations/webhooks
X-VoidTower-Timestamp: 1726838400
X-VoidTower-Nonce: 01JABCDEF-webhook-delivery
X-VoidTower-Signature: sha256=<hmac_sha256_hex>
Content-Type: application/json

{ "action": "container.restart", "resource_id": "my-container" }
```

The timestamp must be within ±300 seconds of VoidTower time. A source/nonce pair is accepted only
once; a duplicate returns `409 webhook_replay` without creating another job. Receipts are retained
for 15 minutes. Missing, malformed, stale, tampered, and legacy Bearer-only credentials return the
bounded `401 webhook_authentication_failed` response. The exact raw body is capped at 64 KiB and
must be signed before it is parsed as JSON.

`container.start`, `container.stop`, and `container.restart` use VoidTower's canonical durable
operation path. A normal request returns `202` with the v1 job envelope
`{ "schema_version": 1, "resource_id": "...", "action": "...", "job": ... }`; the returned job is
acceptance, not provider success, and can be followed at `GET /api/jobs/:id`. Add `"dry_run": true`
to receive the canonical plan without creating a job. Unknown actions and durable actions that are
not explicitly webhook-enabled fail closed.

The endpoint also accepts an `automation_id` intent. Include an `Idempotency-Key` when the caller
needs canonical job replay semantics across distinct signed deliveries:

```http
POST /api/integrations/webhooks
X-VoidTower-Timestamp: 1726838400
X-VoidTower-Nonce: 01JABCDEF-automation-delivery
X-VoidTower-Signature: sha256=<hmac_sha256_hex>
Content-Type: application/json
Idempotency-Key: odysseus-run-2026-09-20-1

{ "automation_id": "nightly-check", "dry_run": false }
```

Exactly one of `automation_id` or `action` is required. Unknown JSON fields, ambiguous intents,
missing action resources, empty/oversized identifiers, invalid idempotency keys, and unsupported
actions fail closed with the bounded error envelope. The webhook body is capped at 64 KiB and
requires `Content-Type: application/json`. Automation webhook requests use the same canonical
`automation.run` resource/action/plan/policy/durable-job boundary as HTTP and scheduler submissions.
A normal request returns `202` with the same v1 job envelope, including
`actor_type = "automation"` and `ingress = "webhook"`; the returned job is acceptance, not provider
success, and can be followed at
`GET /api/jobs/:id`. Reusing an idempotency key with the same automation intent replays the same job;
using it for a different intent returns `409 conflict`. Add `"dry_run": true` to receive the
canonical plan and policy preview without creating a job.

`container.start`, `container.stop`, and `container.restart` use the same canonical durable
operation path when the provider resource is available. Signed service webhook mutation inputs
remain explicitly deferred and return `503 feature_unavailable`; the webhook handler does not call
the legacy systemd helper. The separate session-authenticated HTTP route
`POST /api/services/{name}/action` adopts only `service.restart` and returns the canonical `202`
durable-job envelope. It is not an integration endpoint or Odysseus tool, and is not available with API tokens. Unknown actions and durable actions that are not explicitly webhook-enabled
fail closed.

---

## Real-Time Event Stream

Odysseus can also subscribe to the VoidTower SSE event stream:

```
GET /api/integrations/events
Authorization: Bearer <token>
```

Returns a `text/event-stream` of infrastructure events in real time.

The default stream now delivers the durable `EventEnvelopeV1` log. With no cursor it begins at the
current high-water mark. Supply `after=<sequence>` for explicit retained replay (`after=0` for all
retained history); reconnects may send `Last-Event-ID`. Each `durable_event` carries an SSE ID equal
to its ordered database sequence. `stream.ready` confirms the selected cursor, while `stream.gap`
requires a complete authoritative state refresh before reconnect.

For bounded replay without an SSE connection, operators can use the authenticated history envelope:

```
GET /api/events?after=<sequence>&limit=<1..500>
```

The response includes `schema_version`, ordered `events`, `next_cursor`, and retained
`earliest_available`/`latest_available` bounds. A future cursor returns an empty page with
`next_cursor` clamped to `latest_available`; clients must reject any response whose cursor exceeds
that bound and refresh authoritative state after an SSE `stream.gap`. Persisted event records are
decoded through the same bounded v1 read boundary used by both cursor history and resource history:
unknown schema versions or actor types, incomplete/oversized metadata, malformed JSON, invalid negative or
client-unsafe sequence/timestamp values, payloads deeper than 8 levels, arrays or objects with more than 64 items, strings over 4,160 characters, oversized
payloads, and frames that exceed the 128 KiB durable-event limit fail closed. The append boundary and
read boundary enforce the same payload and metadata limits. A malformed persisted row
therefore cannot be sent as an SSE frame or returned through the CMDB resource-history projection.

History replay also requires contiguous sequences from the requested `after` cursor. If retention or
storage corruption causes a returned row to skip a sequence, `GET /api/events` returns `409 conflict`
with `Durable event history is discontinuous; refresh authoritative state.` rather than advancing the
client cursor. The live SSE route continues to emit `stream.gap` for the same discontinuity so clients
close the stream, refresh authoritative state, and reconnect without replaying an action.

Live SSE frames and HTTP history use the same frontend event parser. It accepts only the v1 actor
allowlist and exact event keys, bounds payload JSON, rejects inverted or reason-inconsistent gap bounds,
and refuses a gap that would rewind an accepted cursor. It treats any invalid frame as a recovery gap.
A consumer
must never replay an action after reconnect. Existing consumers of the old `metrics`, `alert`,
`audit`, and `ping` shapes can migrate temporarily to:

```
GET /api/integrations/events/legacy
Authorization: Bearer <token>
```

Both routes require `alerts:read`. Emergency disable rejects token-backed connections.

---

## Security Notes

- Tokens are hashed at rest (SHA-256). Plaintext is never stored after creation.
- The tool manifest (`/api/integrations/odysseus/manifest`) is public — no auth required. It contains no sensitive data.
- All agent-triggered actions are logged with `actor_type = 'agent'` in the audit log.
- Durable container webhook jobs record a typed automation actor and `webhook` ingress; they do not
  inherit a human session role or API-token scopes.
- Emergency disable can be triggered from VoidTower or Odysseus and takes effect immediately.
- The Odysseus integration is disabled by default. Enable explicitly in VoidTower settings.
- VoidTower's outbound event webhooks carry an HMAC-SHA256 signature. Inbound Odysseus action
  requests carry the timestamped raw-body HMAC headers and a fresh nonce; Bearer-only requests,
  stale/tampered signatures, and duplicate nonces are rejected.
