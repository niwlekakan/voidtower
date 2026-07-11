# API Tokens & Scopes

API tokens let external tools, scripts, and AI agents authenticate to VoidTower's HTTP API without using a session cookie. Each token carries a fixed set of scopes that limit what it can access.

---

## Creating a token

Go to **VoidTower → Settings → Integrations → API Tokens → New Token**.

| Field | Description |
|---|---|
| **Name** | Label for this token (e.g. `odysseus-voidwatch`, `monitoring-script`) |
| **Scopes** | One or more scopes from the list below |
| **Expiry** | Optional — number of days until the token expires. Leave blank for no expiry. |
| **Secret restriction** | Optional — restrict this token to only specific secret IDs from the secrets manager |

The token value (`vt_` followed by 64 hex characters) is shown **once** on creation — copy it immediately. VoidTower stores only a SHA-256 hash of the token.

---

## Using a token

Pass the token as a Bearer header:

```
Authorization: Bearer vt_<your_token>
```

For the SSE event stream (`/api/integrations/events`), which can't set headers, pass it as a query parameter instead:

```
GET /api/integrations/events?token=vt_<your_token>
```

---

## Scopes

### Read-only

| Scope | What it allows |
|---|---|
| `metrics:read` | Read CPU, RAM, disk, and network metrics |
| `services:read` | List systemd services and their state |
| `containers:read` | List Docker containers and images |
| `containers:logs` | Read container log output |
| `apps:read` | List deployed App Vault applications |
| `backups:read` | List backup jobs and snapshots |
| `alerts:read` | List active alerts and status checks (required for SSE stream) |
| `automation:read` | List automation jobs and run history |
| `timeline:read` | Read the audit timeline |
| `network:read` | List network interfaces and LAN neighbours |
| `files:read` | Browse and read files (read-only) |
| `storage:read` | List storage devices and mount points |
| `proxy:read` | List nginx reverse proxy rules |
| `diagnostics:read` | Run and read system diagnostics checks |
| `secrets:list` | List secret names and descriptions (values are **never** returned) |
| `vms:read` | List KVM and Proxmox virtual machines |
| `tags:read` | List resource tags |

### Action scopes

| Scope | What it allows |
|---|---|
| `services:restart` | Start, stop, and restart systemd services |
| `containers:restart` | Start, stop, and restart Docker containers |
| `apps:deploy` | Deploy applications from the App Vault catalog |
| `apps:restart` | Restart deployed App Vault applications |
| `backups:run` | Trigger a backup job to run now |
| `alerts:ack` | Acknowledge or resolve alerts |
| `automation:run` | Trigger an automation job |
| `proxy:manage` | Add, toggle, and reload nginx proxy rules |

---

## Recommended scope sets

### Voidwatch (read-only baseline)

```
metrics:read  services:read  containers:read  containers:logs
apps:read  backups:read  alerts:read  alerts:ack
automation:read  timeline:read  network:read  storage:read
diagnostics:read  proxy:read  tags:read
```

### Voidwatch with action permissions

Add to the baseline above:

```
services:restart  containers:restart  apps:restart  backups:run  automation:run
```

### Read-only monitoring script

```
metrics:read  alerts:read  services:read  containers:read
```

### Deployment automation

```
apps:read  apps:deploy  containers:read  containers:restart  proxy:read  proxy:manage
```

---

## Capability tiers

`POST /api/integrations/tokens` also accepts a `tier` field as a coarser
shortcut instead of an explicit `scopes` array — useful for splitting a
single god-token AI stack integration into narrower, purpose-built tokens
(gap-analysis P0.6). When `tier` is set, the server derives the token's
scopes from a fixed table; any `scopes` field in the same request is
ignored.

| Tier | Scopes granted |
|---|---|
| `read` | Every read-only scope listed above |
| `deploy` | `apps:read`, `apps:deploy`, `apps:restart` |
| `exec` | `containers:read`, `containers:restart`, `containers:logs`, `services:read`, `services:restart`, `automation:read`, `automation:run` |
| `admin-never` | None — this tier mints a token that structurally cannot reach any admin/owner-gated route, regardless of the minting user's own role |

This is an API-only capability for now — the Settings → Integrations UI
still only exposes the explicit `scopes` picker.

**Migrating an existing `VOIDTOWER_TOKEN`:** if your AI stack currently uses
one broadly-scoped (or legacy unscoped) token for everything, mint
replacement tier tokens for what it actually needs, update the consuming
service's env vars, then revoke the old token — there is no automatic
migration, and the old token is not silently upgraded to a tier.

---

## Secret restriction

When creating a token that needs access to specific secrets (e.g. a script that reads a database password), you can restrict it to only those secret IDs using the **Secret restriction** field. The token will be rejected for `secrets:list` calls to any secret not in the allowed list.

Tokens without a secret restriction (the default) can list all secrets if they have `secrets:list` — but secret values are never returned regardless of scopes.

---

## Revoking a token

Go to **Settings → Integrations → API Tokens** and click **Revoke** next to the token. The revocation is immediate — the token hash is deleted from the database. All active requests using that token will fail on the next call.

---

## Security notes

- Tokens are stored as SHA-256 hashes — VoidTower cannot recover the plaintext value after creation
- Token usage is tracked (`last_used_at` timestamp updated on each authenticated request)
- All token creation and revocation events appear in the audit timeline
- The SSE stream checks the emergency-disable flag — if Odysseus integration is emergency-disabled, `alerts:read` tokens cannot connect to `/api/integrations/events`
- Scopes are enforced by a single middleware for every route a Bearer token can reach (not per-handler) — a route with no listed scope requirement is closed to tokens by default, not implicitly open
