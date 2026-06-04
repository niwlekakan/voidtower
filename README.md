# VoidTower

**Self-hosted Linux infrastructure management — one dashboard to rule your homelab.**

VoidTower is an open-source, local-first server control plane. It gives you a polished web UI for everything you'd normally SSH in to do: containers, services, files, backups, monitoring, reverse proxies, and AI integration. One install script, no cloud dependency, no telemetry.

---

## Features

| Area | What you get |
|---|---|
| **Dashboard** | Customizable widgets — clock, weather, CPU/RAM/disk charts, container summary, alert count. Nine toggleable widgets with drag-to-reorder sections, config persisted per-browser. |
| **Services** | Manage systemd units — start, stop, restart, enable/disable, view logs. Resource tag filtering. |
| **Containers** | Docker container list, start/stop/restart, log viewer, per-container exec shell, compose file editor with staged diff before apply. Resource tag filtering. |
| **App Vault** | 25+ one-click app deployments (Gitea, Nextcloud, Jellyfin, Grafana, Pi-hole, n8n, Ollama, Open WebUI, Home Assistant, Odysseus, and more). Expandable management panel per deployed app with Containers / Compose / Logs tabs. |
| **AI Discover** | Ask the configured LLM to recommend self-hosted apps; results include Docker image names and direct deploy buttons for catalog matches. |
| **Models** | Download GGUF models from URL with popular presets, pull models via Ollama by name, import downloaded GGUFs into Ollama. Live progress bars for all operations. Load a model into llama.cpp in one click. |
| **AI workspace** | Iframe-embed any OpenAI-compatible workspace (Odysseus, Open WebUI, etc.). Floating GPU controls panel shows VRAM bar, GPU utilisation %, llama.cpp process list, and one-click unload. |
| **VMs** | KVM/QEMU local VM management via libvirt (`virsh`). Proxmox integration — connect to any Proxmox host via API token, list QEMU VMs and LXC containers, start/stop/reboot with CPU/RAM/uptime stats. |
| **Files** | Full filesystem browser — Monaco editor (25+ language detection), inline image viewer (PNG/JPG/GIF/WebP/SVG), PDF viewer, new file creation, per-file download, breadcrumb navigation, roots sidebar. |
| **Terminal** | Full PTY browser terminal with shell auto-detection from `/etc/passwd`. SSH session manager — save hosts, connect with one click from a second tab. |
| **Reverse Proxies** | nginx-backed proxy rule manager — domain + upstream + SSL + optional iframe-embed headers, configs written to sites-enabled, validated and reloaded automatically. |
| **Firewall** | UFW rule management — add/delete rules (port, protocol, direction, source CIDR), enable/disable toggle, colour-coded allow/deny columns. |
| **WireGuard** | Peer management — generate Curve25519 keypairs natively, allocate IPs from existing interface subnet, add/remove peers live, client config shown once with copy button. |
| **Storage** | Block device tree, mount manager, fstab editor, format disks, SMART health, software RAID (mdadm) status and creation. Configurable storage paths for containers/VMs/backups. |
| **Network** | Real-time interface stats, LAN neighbour table (ARP cache), bandwidth charts. |
| **Backups** | Restic-powered jobs — init, run now, list snapshots, integrity check, dry-run restore test, confidence scoring (high/medium/low/critical). |
| **Alerts** | Metric threshold alerts + TCP/HTTP status checks, ack/resolve flow, public `/status` page (no auth). |
| **Automation** | Scheduled shell jobs — cron-style schedules (`@hourly`, `*/N` minutes), run history with output, enable/disable toggle. |
| **Secrets** | AES-256-GCM encrypted secrets store — values never appear in list responses, reveal-on-demand with audit logging. |
| **Resource tags** | Create colour-coded tags, assign to services and containers, filter any page by tag. |
| **Timeline** | Global audit timeline — category chips, free-text search, outcome filter, paginated infinite scroll. |
| **Capabilities** | Detect installed tools (Docker, libvirt, WireGuard, restic, nginx, GPU, …) with version strings and install hints. |
| **Diagnostics** | 12 system health checks — config/data dirs, DB, frontend assets, disk space, Docker daemon, nginx config, port bind. `voidtower --doctor` at the CLI. |
| **Security** | Session list for all users, revoke individual sessions or all-others, full audit log. |
| **Themes** | 7 built-in themes + custom token editor with live color pickers, 14-param animation editor, randomise button. |
| **Animated backgrounds** | 7 canvas-based presets (Void, Grid, Aurora, Pulse, Noise, Hex, Circuit) + 4 glass levels (Solid, Blur, Acrylic, Frosted). |
| **System** | In-app updater — checks GitHub for new commits, pulls latest, rebuilds, and restarts automatically. Safe restart button with live reconnect polling. |
| **Mobile** | Responsive layout — hamburger sidebar on small screens, touch-friendly targets. |

---

## Quick install

```bash
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/scripts/install.sh | bash
```

The installer will:

1. Detect your distro and install dependencies (Rust, Node.js, Docker).
2. Build the backend and frontend.
3. Install a systemd service (`voidtower.service`).
4. Prompt for network setup: localhost, mDNS (`hostname.local`), custom hostname, or public domain + nginx.
5. Optionally detect your GPU, download a matching GGUF model, set up llama.cpp, and offer to install Odysseus AI workspace.

**Variables you can override:**

```bash
VT_PORT=8743 VT_DATA_DIR=/var/lib/voidtower bash install.sh
```

---

## Manual setup

### Prerequisites

- Rust 1.75+ (`rustup`)
- Node.js 20+
- Docker (optional — containers / app vault)
- nginx (optional — reverse proxy manager)
- restic (optional — backups)
- nvidia-container-toolkit (optional — GPU passthrough to Ollama)

### Build

```bash
# Backend
cd backend && cargo build --release

# Frontend
cd ../frontend && npm install && npm run build
```

### Run (development)

```bash
bash start-dev.sh
# Frontend HMR dev server (optional, proxies API to :8743)
cd frontend && npm run dev
```

### Configuration

`/etc/voidtower/config.toml` (production) or via environment variables (dev):

```bash
VOIDTOWER_DATA_DIR=backend/dev-data \
VOIDTOWER_CONFIG_DIR=backend/dev-config \
VOIDTOWER_FRONTEND_DIR=frontend/dist \
VOIDTOWER_CATALOG_DIR=app-vault/apps \
./backend/target/release/voidtower
```

---

## First login

1. Open `http://localhost:8743`.
2. You'll be redirected to `/bootstrap`.
3. Enter the **bootstrap token** (printed by the installer, stored in your config dir).
4. Choose a username and password — the owner account is created and the token is consumed.

---

## Pages

| Route | Description |
|---|---|
| `/dashboard` | Customizable widget grid |
| `/services` | systemd service management |
| `/containers` | Docker containers; click a row → detail page |
| `/containers/:id` | Compose editor, exec shell, logs |
| `/apps` | App Vault catalog + Deployed list + AI Discover |
| `/vms` | Local KVM/QEMU VMs + Proxmox integration |
| `/models` | GGUF download, Ollama pull, load into llama.cpp or Ollama |
| `/ai` | Embedded AI workspace + GPU controls panel |
| `/files` | Filesystem browser, Monaco editor, image/PDF viewer |
| `/terminal` | Browser PTY + SSH session manager |
| `/proxies` | nginx reverse proxy rules |
| `/firewall` | UFW rule management |
| `/wireguard` | WireGuard peer management |
| `/storage` | Disks, mounts, fstab, SMART, RAID |
| `/network` | Interface stats + LAN neighbours |
| `/backups` | Restic backup jobs with integrity checks |
| `/alerts` | Active alerts + status checks |
| `/automation` | Scheduled shell jobs |
| `/secrets` | Encrypted secrets store |
| `/tags` | Resource tag management |
| `/timeline` | Global audit timeline |
| `/capabilities` | Detected system capabilities |
| `/diagnostics` | System health checks |
| `/security` | Sessions + audit log |
| `/settings` | Theme, users, AI endpoints, system updater |
| `/status` | **Public** status page (no login required) |

---

## API reference

All endpoints require a valid `vt_session` cookie except `/api/auth/*`, `/api/status`, and `/api/system/version`.

### Auth
```
POST /api/auth/bootstrap   { token, username, password }
POST /api/auth/login       { username, password }
POST /api/auth/logout
GET  /api/auth/me
```

### Metrics
```
GET /api/metrics/current
GET /api/metrics/ws        WebSocket (1 s interval)
```

### Services
```
GET  /api/services
POST /api/services/:name/action   { action: start|stop|restart|enable|disable }
GET  /api/services/:name/logs
```

### Containers
```
GET  /api/containers
GET  /api/containers/images
POST /api/containers/:id/action   { action: start|stop|restart|remove }
GET  /api/containers/:id/logs
GET  /api/containers/:id/exec     WebSocket PTY
GET  /api/containers/:id/compose
POST /api/containers/:id/compose/propose   { content }
POST /api/containers/:id/compose/apply     { content }
```

### App Vault
```
GET  /api/apps/catalog
GET  /api/apps/deployed
POST /api/apps/deploy              { app_id, project_name?, env_overrides? }
POST /api/apps/:name/start|stop|restart|redeploy
GET  /api/apps/:name/compose
POST /api/apps/:name/compose       { content }
GET  /api/apps/:name/logs
GET  /api/apps/:name/status
DELETE /api/apps/:name
```

### Models
```
GET  /api/models
POST /api/models/download          { url, filename? }
GET  /api/models/download/:id      Download progress
GET  /api/models/active
POST /api/models/load              { filename }
DELETE /api/models/:filename
POST /api/models/ollama/pull       { model }
GET  /api/models/ollama/pull/:id   Pull progress
POST /api/models/ollama/create     { filename }   Import GGUF into Ollama
GET  /api/models/ollama/create/:id Import progress
```

### AI / GPU
```
GET  /api/ai/llama
POST /api/ai/llama/unload
```

### VMs
```
GET  /api/vms/local
POST /api/vms/local/action         { name, action }
GET  /api/vms/proxmox/config
POST /api/vms/proxmox/config
GET  /api/vms/proxmox/vms
POST /api/vms/proxmox/action       { vmid, kind, node, action }
POST /api/vms/proxmox/test
```

### Files
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

### Proxy
```
GET  /api/proxy
POST /api/proxy                    { domain, upstream, ssl, allow_embed? }
DELETE /api/proxy/:id
POST /api/proxy/:id/toggle
```

### Firewall
```
GET  /api/firewall
POST /api/firewall/rules           { action, direction, port, protocol, from? }
POST /api/firewall/rules/delete    { rule_number }
POST /api/firewall/action          { action: enable|disable|reload|reset }
```

### WireGuard
```
GET  /api/wireguard
POST /api/wireguard/peers          { name, interface, server_endpoint? }
DELETE /api/wireguard/peers/:id
```

### Storage
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

### Network
```
GET /api/network
GET /api/network/neighbors
```

### Backups
```
GET  /api/backups
POST /api/backups                  { name, source_path, repo_path, password }
POST /api/backups/:id/run
POST /api/backups/:id/check
POST /api/backups/:id/restore-test
DELETE /api/backups/:id
```

### Alerts & status checks
```
GET  /api/alerts?state=&severity=
POST /api/alerts/:id/acknowledge
POST /api/alerts/:id/resolve
DELETE /api/alerts/:id
GET  /api/status-checks
POST /api/status-checks            { name, type, target, interval_secs? }
DELETE /api/status-checks/:id
GET  /status                       Public HTML page
```

### Automation
```
GET  /api/automation
POST /api/automation               { name, command, schedule, enabled? }
PATCH /api/automation/:id
DELETE /api/automation/:id
POST /api/automation/:id/run
GET  /api/automation/:id/runs
```

### Secrets
```
GET  /api/secrets
POST /api/secrets                  { name, description, value }
PATCH /api/secrets/:id
DELETE /api/secrets/:id
GET  /api/secrets/:id/reveal
```

### Tags
```
GET  /api/tags
POST /api/tags                     { name, color }
PATCH /api/tags/:id
DELETE /api/tags/:id
GET  /api/tags/map?type=
POST /api/tags/assign              { tag_id, resource_type, resource_id }
POST /api/tags/unassign            { tag_id, resource_type, resource_id }
```

### Timeline
```
GET /api/timeline?limit=&offset=&category=&outcome=&search=
```

### Users
```
GET  /api/users
POST /api/users                    { username, password, role }
DELETE /api/users/:id
POST /api/users/me/password        { password }
```

### Security
```
GET  /api/security/sessions
POST /api/security/sessions/revoke-others
DELETE /api/security/sessions/:id
```

### System
```
GET  /api/system/version           { commit, branch, commit_date, dirty }
GET  /api/system/update-check      Fetches origin, returns behind/ahead counts
POST /api/system/restart           Graceful restart (responds before exit)
POST /api/system/update            git pull + rebuild + restart
```

### Integrations
```
GET  /api/integrations/scopes                    List all available token scopes
GET  /api/integrations/tokens                    List API tokens (admin+)
POST /api/integrations/tokens                    Create token  { name, scopes[], expires_days? }
DELETE /api/integrations/tokens/:id              Revoke token
GET  /api/integrations/odysseus/config           Get Odysseus integration config
POST /api/integrations/odysseus/config           Update config { enabled?, mcp_enabled?, allowed_url?, regenerate_webhook_secret?, emergency_disable? }
GET  /api/integrations/odysseus/manifest         Tool manifest (public, no auth)
GET  /api/integrations/events                    SSE event stream (Bearer token or ?token=)
POST /api/integrations/webhooks                  Webhook receiver (Bearer webhook-secret)
GET  /api/integrations/actions                   Recent agent-triggered audit entries
```

### Capabilities & diagnostics
```
GET /api/capabilities
GET /api/diagnostics
```

---

## Odysseus integration

VoidTower can act as the infrastructure control plane for [Odysseus](https://github.com/pewdiepie-archdaemon/odysseus), giving AI agents scoped, audited access to your homelab.

### 1 — Enable the integration

Open **Settings → Integrations**, toggle **Enable integration**, and save. While disabled, all API tokens and the event stream are rejected.

### 2 — Create an API token

1. Click **New token** in the Integrations page.
2. Give it a name (e.g. `Odysseus Agent`).
3. Select the scopes you want to grant. Start with the minimum needed:
   - `metrics:read` — dashboards and health checks
   - `services:read` / `services:restart` — service management
   - `containers:read` / `containers:restart` / `containers:logs` — Docker
   - `alerts:read` / `alerts:ack` — alert triage
   - `automation:read` / `automation:run` — trigger automations
4. Set an expiry (90 days recommended) and click **Create token**.
5. Copy the token from the reveal dialog — **it is shown only once**.

Tokens are authenticated via the `Authorization` header:

```bash
curl -H "Authorization: Bearer vt_<your-token>" \
  http://localhost:8743/api/metrics/current
```

All token use is audit-logged under `actor_type = 'agent'` and visible in the **Recent AI-triggered actions** section of the Integrations page.

### 3 — Subscribe to the event stream

The event stream is a standard [Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events) endpoint. It emits:

| Event | When |
|---|---|
| `metrics` | Every metrics broadcast (~1 s) — CPU, RAM totals |
| `alert` | CPU > 90 % or RAM > 90 % threshold crossed |
| `audit` | New audit log entries (polled every 10 s) |
| `ping` | Heartbeat every 10 s |

**Subscribe with curl:**

```bash
curl -N \
  -H "Accept: text/event-stream" \
  -H "Authorization: Bearer vt_<your-token>" \
  http://localhost:8743/api/integrations/events
```

**Subscribe with a query-string token** (useful when the client cannot set custom headers, e.g. browser `EventSource`):

```bash
curl -N "http://localhost:8743/api/integrations/events?token=vt_<your-token>"
```

```js
// Browser EventSource
const es = new EventSource(`/api/integrations/events?token=${TOKEN}`)
es.addEventListener('metrics', e => console.log(JSON.parse(e.data)))
es.addEventListener('alert',   e => console.warn('Threshold crossed', JSON.parse(e.data)))
es.addEventListener('audit',   e => console.log('Audit', JSON.parse(e.data)))
```

The required scope is `alerts:read`. A session cookie is also accepted instead of a token.

### 4 — Configure the webhook secret

The webhook endpoint lets external systems (including Odysseus) trigger VoidTower automations over HTTP.

1. In **Settings → Integrations**, click **Regenerate** next to *Webhook secret*.
2. The full secret is shown only once — copy it now.
3. Use it as a Bearer token in the `Authorization` header of every webhook call.

### 5 — Trigger an automation via webhook

First, find the automation ID from **GET /api/automation** or the Automation page URL.

**Trigger immediately:**

```bash
curl -X POST http://localhost:8743/api/integrations/webhooks \
  -H "Authorization: Bearer <webhook-secret>" \
  -H "Content-Type: application/json" \
  -d '{"automation_id": "<id>"}'
```

**Dry run** (validates the request and logs the attempt without executing):

```bash
curl -X POST http://localhost:8743/api/integrations/webhooks \
  -H "Authorization: Bearer <webhook-secret>" \
  -H "Content-Type: application/json" \
  -d '{"automation_id": "<id>", "dry_run": true}'
```

Response:

```json
{ "ok": true, "dry_run": false, "automation_id": "<id>" }
```

The automation runs in the background. Check its result via:

```bash
curl -H "Authorization: Bearer vt_<token>" \
  http://localhost:8743/api/automation/<id>/runs
```

Webhook calls are audit-logged with `actor_type = 'agent'` and `action = integrations.webhook.automation_trigger`.

### 6 — Fetch the tool manifest

The manifest describes every action available to AI agents — their required scope, risk level, and API endpoint — so Odysseus (or any agent framework) can discover what VoidTower can do without reading this README:

```bash
curl http://localhost:8743/api/integrations/odysseus/manifest
```

This endpoint is public (no authentication required). When the integration is disabled it returns an empty tool list.

### Token scopes reference

| Scope | What it grants |
|---|---|
| `metrics:read` | CPU, RAM, disk and network metrics |
| `services:read` | List systemd services |
| `services:restart` | Start, stop, restart services |
| `containers:read` | List Docker containers and images |
| `containers:restart` | Start, stop, restart containers |
| `containers:logs` | Read container log output |
| `apps:read` | List deployed App Vault apps |
| `apps:deploy` | Deploy apps from the catalog |
| `backups:read` | List backup jobs and snapshots |
| `backups:run` | Trigger a backup job |
| `alerts:read` | List alerts and status checks |
| `alerts:ack` | Acknowledge or resolve alerts |
| `automation:read` | List jobs and run history |
| `automation:run` | Trigger automation jobs |
| `timeline:read` | Read the audit timeline |
| `network:read` | Network interfaces and LAN neighbours |
| `files:read` | Browse and read files |
| `storage:read` | Storage devices and mounts |

### Emergency disable

If you need to immediately cut all AI agent access without revoking individual tokens, click **Disable all AI access** in the Integrations page. This blocks every API token and SSE connection instantly. Tokens are preserved and can be re-activated by clicking **Re-enable**.

---

## Roles

| Role | Capabilities |
|---|---|
| `owner` | Everything, including deleting other admins |
| `admin` | Create/delete users (not owner), all actions |
| `operator` | Start/stop services, deploy apps, write files, terminal |
| `viewer` | Read-only access to all pages |

---

## Upgrading

Use the in-app updater in **Settings → System**, or manually:

```bash
cd /opt/voidtower
git pull origin main
cd backend && cargo build --release
cd ../frontend && npm ci && npm run build
sudo systemctl restart voidtower
```

---

## Uninstalling

```bash
sudo systemctl disable --now voidtower
sudo rm /etc/systemd/system/voidtower.service
sudo rm -rf /var/lib/voidtower
sudo rm -rf /opt/voidtower
sudo systemctl daemon-reload
```

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The project follows AGPL-3.0 — contributions welcome.

Bug reports and feature requests: [GitHub Issues](https://github.com/niwlekakan/voidtower/issues).

---

## License

AGPL-3.0 — see [LICENSE](LICENSE).
