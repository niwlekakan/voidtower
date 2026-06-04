# Odysseus / Voidwatch Integration

This document covers how to connect Odysseus (with the Voidwatch integration) to VoidTower.

## Overview

Voidwatch is the AI operations layer in Odysseus. It connects to VoidTower via a scoped API token and exposes MCP tools that let Odysseus agents safely observe and manage your infrastructure.

VoidTower already ships with Odysseus integration support:
- Scoped API tokens (25 scopes)
- Tool manifest endpoint
- Server-Sent Events for real-time alerts
- Signed webhook outbound support
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

**Add for higher-privilege actions (use with caution):**
```
services:restart
apps:deploy
proxy:manage
```

### Token Revocation

To revoke a token: **Settings → Integrations → API Tokens → Delete**.

All tokens are stored as SHA-256 hashes — the plaintext is never stored after creation.

---

## API Endpoints Used by Voidwatch

Voidwatch uses existing VoidTower API endpoints. No additional backend setup is required.

| Endpoint | Purpose |
|----------|---------|
| `GET /api/auth/me` | Connection test |
| `GET /api/integrations/odysseus/manifest` | Tool manifest |
| `GET /api/metrics/current` | System metrics |
| `GET /api/services` | Service list |
| `POST /api/services/{name}/action` | Service control |
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
POST /api/integrations/odysseus/config   → Update config (enable, disable, webhook secret, emergency disable)
GET  /api/integrations/odysseus/manifest → Public tool manifest (no auth required)
```

**Emergency disable** via config update sets `emergency_disable: true`, which blocks all agent-triggered actions immediately.

---

## Webhook Configuration

To send events from VoidTower to Odysseus:

1. In VoidTower: **Settings → Integrations → Odysseus Config**
2. Set **Allowed Odysseus URL** (e.g. `http://odysseus-host:7000`)
3. Enable **MCP**
4. Note the **Webhook Secret** (or regenerate it)

In Odysseus: **Settings → Integrations → Voidwatch**
- Set `webhook_secret` to the same value
- Ensure `webhook_enabled: true`

VoidTower sends `POST` to `{odysseus_url}/api/voidwatch/webhook` with:
```
X-VoidTower-Signature: sha256=<hmac>
Content-Type: application/json

{
  "event_type": "service_failed",
  "timestamp": 1234567890,
  "resource": { ... }
}
```

Event types: `node_down`, `high_cpu`, `high_memory`, `disk_nearly_full`, `service_failed`, `container_unhealthy`, `backup_failed`, `certificate_expiring`, `suspicious_login`, `automation_completed`, `security_finding`, `app_deployment_failed`, `config_drift_detected`.

---

## Real-Time Event Stream

Odysseus can also subscribe to the VoidTower SSE event stream:

```
GET /api/integrations/events
Authorization: Bearer <token>
```

Returns a `text/event-stream` of infrastructure events in real time.

---

## Security Notes

- Tokens are hashed at rest (SHA-256). Plaintext is never stored after creation.
- The tool manifest (`/api/integrations/odysseus/manifest`) is public — no auth required. It contains no sensitive data.
- All agent-triggered actions are logged with `actor_type = 'agent'` in the audit log.
- Emergency disable can be triggered from VoidTower or Odysseus and takes effect immediately.
- The Odysseus integration is disabled by default. Enable explicitly in VoidTower settings.
- Webhook payloads must carry a valid HMAC-SHA256 signature or they are rejected (HTTP 401).
