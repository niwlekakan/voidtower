# VoidTower AIO

**Self-hosted Linux infrastructure management — with an AI operator built in.**

This is the **all-in-one branch** of VoidTower. It ships VoidTower (the infrastructure control plane) together with [Odysseus](https://github.com/niwlekakan/odysseus/tree/odysseus-voidlink) (the AI workspace) and the **Voidwatch** integration that connects them — so an AI agent can inspect and manage your homelab with scoped, audited, policy-controlled access.

Everything else — Jellyfin, Nextcloud, Gitea, Portainer, and 20+ other apps — is **opt-in** from the VoidTower app catalog inside the UI.

> For the standalone VoidTower without AI integration, see the [`main` branch](https://github.com/niwlekakan/voidtower/tree/main).

---

## What's in this branch

| Component | Role | Port |
|---|---|---|
| **VoidTower** | Infrastructure control plane — services, containers, backups, proxies, secrets | 80 / 443 |
| **Odysseus** | AI workspace — chat, agents, MCP tools | 7000 |
| **Voidwatch** | Integration layer — connects Odysseus agents to VoidTower via scoped API + webhooks | built-in |
| **Ollama** *(opt-in)* | Local AI runtime | 11434 |

---

## Quick start — Docker (recommended)

No build required — the image is pulled automatically from GHCR.

```bash
# 1. Clone and configure
git clone -b voidtower-aio https://github.com/niwlekakan/voidtower
cd voidtower
cp .env.example .env
# Edit .env — set ODYSSEUS_ADMIN_PASSWORD at minimum

# 2. Start the full AIO stack
docker compose --profile aio up -d

# 3. With local AI (Ollama) as well
docker compose --profile aio --profile ai up -d
```

Then open `https://localhost` and complete the [first-run setup](#first-run-setup).

### Docker Compose profiles

| Command | What starts |
|---|---|
| `docker compose up -d` | VoidTower only |
| `docker compose --profile aio up -d` | + Odysseus, chromadb, SearXNG, ntfy |
| `docker compose --profile aio --profile ai up -d` | + Ollama local AI |

Homelab apps (Jellyfin, Nextcloud, etc.) are never started by Compose — deploy them from **VoidTower → App Vault**.

The `docker-compose.yml` mounts `/var/run/docker.sock` so VoidTower can manage containers and update itself from the UI. If you prefer not to expose the socket, comment that line out — container management and in-UI updates will be unavailable.

---

## Quick start — system install (bare metal / VM)

```bash
# VoidTower only
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/voidtower-aio/scripts/install.sh \
  | sudo bash

# Full AIO stack
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/voidtower-aio/scripts/install.sh \
  | sudo bash -s -- --all-in-one --pull-model

# Non-interactive with specific model
sudo bash install.sh \
  --all-in-one \
  --ai-model qwen2.5-coder:7b-instruct \
  --pull-model \
  --yes
```

### Installer flags

| Flag | Description |
|---|---|
| `--all-in-one` | Shorthand for `--with-odysseus --with-voidwatch --with-ai` |
| `--with-odysseus` | Install Odysseus AI workspace |
| `--with-voidwatch` | Wire Voidwatch integration (implies `--with-odysseus`) |
| `--with-ai` | Set up Ollama local AI runtime |
| `--ai-model MODEL` | Model to configure (e.g. `qwen2.5-coder:7b-instruct`) |
| `--pull-model` | Pull the model during install |
| `--odysseus-port PORT` | Odysseus port (default: 7000) |
| `--port PORT` | VoidTower port (default: 8743) |
| `--yes` | Non-interactive |
| `--dry-run` | Preview what would happen |
| `--offline` | Skip network calls |
| `--no-mcp` | Skip MCP tool registration |
| `--no-webhooks` | Skip webhook configuration |
| `--no-toolpacks` | Skip toolpack installation |

### Model auto-selection

| RAM | Recommended model |
|---|---|
| ≥ 32 GB | `qwen2.5-coder:14b-instruct` |
| ≥ 16 GB | `qwen2.5-coder:7b-instruct` |
| ≥ 8 GB | `qwen2.5-coder:3b-instruct` |
| < 8 GB | No auto-pull — configure manually or use a remote endpoint |

---

## First-run setup

### Docker path

1. Open `https://localhost` → you will be redirected to `/bootstrap`
2. Enter the bootstrap token (printed in `docker compose logs voidtower` or stored in the `voidtower-config` volume)
3. Create your admin account — the token is consumed
4. Open **Settings → Integrations → API Tokens → New Token**
5. Create a token with the [Voidwatch scopes](#voidwatch-token-scopes)
6. Add it to `.env`:
   ```
   VOIDWATCH_TOKEN=vt_your_token_here
   VOIDWATCH_WEBHOOK_SECRET=  # generate: openssl rand -hex 32
   ```
7. Restart Odysseus:
   ```bash
   docker compose --profile aio restart odysseus
   ```
8. Open `http://localhost:7000` → log in to Odysseus → **Settings → Integrations → Voidwatch** → verify connection shows green

### System install path

Bootstrap credentials are saved to `/root/odysseus-bootstrap-token` and `/root/voidwatch-recovery-info`. The Voidwatch token is created automatically if VoidTower is reachable during install. If it was not (bootstrap not yet complete), run:

```bash
sudo bash scripts/install.sh --with-voidwatch --yes
```

after completing the VoidTower bootstrap.

---

## Configuration

Copy `.env.example` to `.env` before starting. Key variables:

| Variable | Description |
|---|---|
| `ODYSSEUS_ADMIN_PASSWORD` | Odysseus web UI password (required before first start) |
| `ODYSSEUS_ADMIN_USER` | Odysseus admin username (default: `admin`) |
| `VOIDWATCH_TOKEN` | VoidTower API token for Odysseus (set after VoidTower bootstrap) |
| `VOIDWATCH_WEBHOOK_SECRET` | Shared webhook signing secret (generate: `openssl rand -hex 32`) |
| `OLLAMA_BASE_URL` | Override Ollama URL (default: `http://ollama:11434` for Docker, `http://localhost:11434` for bare metal) |
| `ODYSSEUS_PORT` | Host port for Odysseus (default: `7000`) |
| `PUID` / `PGID` | File ownership for Odysseus data mounts (match your host user) |
| `VOIDTOWER_IMAGE` | Pin a specific VoidTower image tag (e.g. `ghcr.io/niwlekakan/voidtower:aio-v1.2.3`). Leave unset to use `aio-latest`. |

---

## Voidwatch — AI ops integration

Voidwatch is the integration layer between Odysseus and VoidTower. It gives AI agents structured, policy-controlled access to your infrastructure.

### What agents can do by default (read-only policy)

- Inspect services, containers, metrics, alerts, backups, network, proxies
- Run diagnostics
- Acknowledge low-severity alerts
- Trigger backup jobs
- Run approved automations
- Summarize infrastructure state

### What requires your confirmation

- Restarting any service or container
- Config edits
- Exposing a service publicly via nginx
- Deploying or removing apps

### What is always blocked

- Shell command execution
- Deleting anything
- Rotating secrets
- Modifying firewall rules
- Touching resources tagged `critical`, `database`, `prod`, or `ai-no-touch`

Policy is configurable in **Odysseus → Settings → Integrations → Voidwatch → Policy** or by editing `/etc/odysseus/voidwatch/policy.json`.

### Emergency disable

```bash
# Immediately block all Voidwatch automation
curl -X POST http://localhost:7000/api/voidwatch/emergency-disable

# Or from VoidTower UI: Settings → Integrations → Disable all AI access
```

### Example agent prompts

Once running, try these in the Odysseus chat:

```
Check my servers and tell me what is unhealthy.
Restart failed non-critical containers only.
Inspect nginx routes and tell me what is publicly exposed.
Run backups on all configured backup jobs.
Investigate why Jellyfin is unhealthy.
Summarize all active alerts from the last 24 hours.
Dry-run an image update for FreshRSS and ask before applying.
Check whether any containers are using outdated images.
```

### Voidwatch token scopes

Minimum scopes for a Voidwatch token:

```
metrics:read  services:read  containers:read  containers:logs
apps:read  backups:read  backups:run  alerts:read  alerts:ack
automation:read  timeline:read  network:read  storage:read
diagnostics:read  proxy:read  tags:read
```

Add for action permissions: `services:restart  containers:restart  apps:restart  automation:run`

---

## Toolpacks

Voidwatch ships 20 pre-built toolpacks for common homelab apps. These define the safe operations Odysseus can perform per-app:

`authentik` · `docker` · `freshrss` · `gitea` · `grafana` · `homeassistant` · `immich` · `jellyfin` · `minio` · `n8n` · `nextcloud` · `nginx` · `ollama` · `open-webui` · `paperless` · `pihole` · `portainer` · `syncthing` · `uptime-kuma` · `vaultwarden`

Toolpacks live in `voidwatch/toolpacks/` inside the Odysseus install. Add your own by dropping a YAML file there — see any existing toolpack as a template.

---

## TrueNAS Scale

Two deployment paths depending on how much access you want. Both store all data on your TrueNAS datasets so nothing is lost across app updates.

> **Docker control disclaimer:** The TrueNAS Custom App UI (Option A) runs containers through its own Kubernetes layer (k3s) and does **not** expose `/var/run/docker.sock` to containers. This means VoidTower's built-in container management panel (start/stop/restart containers, view logs, exec shell) and the in-UI self-update feature will be unavailable — those features require direct Docker socket access. Everything else works normally: the dashboard, services, backups, proxies, secrets, Voidwatch AI integration, and all other pages. If you need container management, use Option B.

---

### Option A — Custom App UI

No SSH required. Uses TrueNAS's built-in app deployment. VoidTower, Odysseus, SearXNG, ChromaDB, and ntfy all start with one click.

**1. Create a dataset**

Go to **Storage → Add Dataset**, create a dataset named `voidtower` on your pool (e.g. `tank/voidtower`). This is where all persistent data will live.

**2. Open Custom App**

Go to **Apps → Discover Apps → Custom App**.

**3. Paste the YAML**

Copy the contents of [`deploy/truenas/custom-app.yml`](deploy/truenas/custom-app.yml) into the YAML editor.

**4. Set environment variables**

In the **Environment Variables** section, add:

| Variable | Value |
|---|---|
| `ODYSSEUS_ADMIN_PASSWORD` | your chosen password |
| `TRUENAS_POOL` | your pool name (e.g. `tank`) |
| `VOIDWATCH_TOKEN` | leave blank — fill in after first login |
| `VOIDWATCH_WEBHOOK_SECRET` | generate: run `openssl rand -hex 32` in a shell |
| `SEARXNG_SECRET` | generate: run `openssl rand -hex 32` in a shell |

**5. Deploy**

Click **Install**. TrueNAS will pull the images and start all services.

**6. First login**

Open `https://<truenas-ip>:8443` in your browser and accept the self-signed certificate warning. You will be redirected to the bootstrap page — find your one-time token in the app logs:

```
Apps → voidtower → Logs → (select voidtower container)
```

Complete the setup wizard to create your admin account.

**7. Wire Voidwatch**

Once logged in:

1. Go to **Settings → Integrations → API Tokens → New Token**
2. Create a token with Voidwatch scopes (see [token scopes](#voidwatch-token-scopes))
3. Go back to **Apps → voidtower → Edit** and set `VOIDWATCH_TOKEN` to the token value
4. Click **Save** — TrueNAS will restart the Odysseus container automatically
5. Open `http://<truenas-ip>:7000` → log in to Odysseus → **Settings → Integrations → Voidwatch** → the status should show green

> **Port note:** VoidTower uses `8443` (HTTPS) and `8080` (HTTP) instead of `443`/`80` to avoid conflicting with the TrueNAS web UI. Odysseus is on port `7000`.

---

### Option B — SSH Docker Compose

Full feature access including container management and self-update. Runs Docker directly on the TrueNAS host, bypassing the k3s layer entirely.

**1. SSH into TrueNAS**

```bash
ssh admin@<truenas-ip>
```

**2. Clone and configure**

```bash
git clone -b voidtower-aio https://github.com/niwlekakan/voidtower /mnt/tank/voidtower-app
cd /mnt/tank/voidtower-app
cp deploy/truenas/.env.example .env
nano .env  # set ODYSSEUS_ADMIN_PASSWORD and TRUENAS_POOL
```

**3. Start the stack**

```bash
docker compose -f deploy/truenas/custom-app.yml up -d
```

**4. Enable Docker socket (optional)**

To unlock container management and self-update, uncomment the socket line in `deploy/truenas/custom-app.yml`:

```yaml
volumes:
  - /var/run/docker.sock:/var/run/docker.sock:ro  # uncomment this line
```

Then restart: `docker compose -f deploy/truenas/custom-app.yml up -d`

**5. First login and Voidwatch setup**

Same as Option A steps 6–7, replacing `Apps → voidtower → Logs` with:

```bash
docker logs voidtower
```

---

### Ollama on TrueNAS

Ollama is commented out in the YAML by default (it's a large download and GPU passthrough requires extra config). To enable it:

**Option A:** Edit the app YAML in TrueNAS and uncomment the `ollama` service block, then save and restart.

**Option B:** Uncomment the `ollama` block in `deploy/truenas/custom-app.yml` and run `docker compose ... up -d` again.

For NVIDIA GPU passthrough on TrueNAS Scale, uncomment the `deploy.resources` block under `ollama` and ensure `nvidia-container-toolkit` is installed on the host. See the [GPU / Ollama](#gpu--ollama) section for full details.

---

## GPU / Ollama

### Docker — NVIDIA

Uncomment the `deploy` block in `docker-compose.yml` under the `ollama` service:

```yaml
deploy:
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]
```

Requires [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) on the host.

### Docker — AMD (ROCm / Vulkan)

Uncomment the `devices` and `environment` block under `ollama`:

```yaml
devices:
  - /dev/dri:/dev/dri
environment:
  - OLLAMA_VULKAN=1
```

### Pulling a model

```bash
# Docker
docker exec ollama ollama pull qwen2.5-coder:7b-instruct

# Bare metal
ollama pull qwen2.5-coder:7b-instruct
```

### Using a remote Ollama instance

Set in `.env` (Docker) or Odysseus `.env` (bare metal):

```
OLLAMA_BASE_URL=http://192.168.1.5:11434
```

---

## Updates

Both installation paths support in-UI updates from **VoidTower → Updates**.

### Docker

The VoidTower image is published to `ghcr.io/niwlekakan/voidtower` on every push to this branch and on release tags. Updates are applied without touching the host.

1. Open **VoidTower → Updates → VoidTower Application**
2. Click **Check for update** — pulls the latest image manifest in the background
3. If a newer image is available, click **Apply update** — VoidTower pulls the image and recreates its own container; the UI reconnects automatically when it comes back up

Requires `/var/run/docker.sock` to be mounted (enabled by default in `docker-compose.yml`).

To pin a release, set `VOIDTOWER_IMAGE=ghcr.io/niwlekakan/voidtower:aio-v1.2.3` in `.env` before restarting. Check available tags at `ghcr.io/niwlekakan/voidtower`.

Companion containers (Odysseus, SearXNG, etc.) are updated from the **Docker Images** section on the same page.

### Bare metal

1. Open **VoidTower → Updates → VoidTower Application**
2. The current commit is compared against the upstream branch — pending commits are listed with authors and dates
3. Click **Apply update** — VoidTower tags the current commit as a rollback point, pulls from GitHub, rebuilds backend and frontend, and restarts
4. If something goes wrong, expand **Rollback points** and click **Roll back** to return to a previous build

The **OS Packages** section on the same page lists available package upgrades and can apply them (apt / pacman / dnf).

---

## Service management

### Docker

```bash
# Status
docker compose ps

# Logs
docker compose logs -f voidtower
docker compose logs -f odysseus
docker compose logs -f ollama

# Restart a service
docker compose restart odysseus

# Stop everything
docker compose --profile aio --profile ai down
```

### Bare metal (systemd)

```bash
systemctl status voidtower odysseus ollama

journalctl -u voidtower -f
journalctl -u odysseus -f
journalctl -u ollama -f

systemctl restart odysseus
```

---

## Uninstall

### Docker

```bash
# Stop and remove containers (preserves volumes)
docker compose --profile aio --profile ai down

# Also remove all data
docker compose --profile aio --profile ai down -v
```

### Bare metal

```bash
# VoidTower only
sudo bash scripts/uninstall.sh

# VoidTower + Odysseus
sudo bash scripts/uninstall.sh --remove-odysseus

# Everything including Ollama and all data
sudo bash scripts/uninstall.sh --all --purge
```

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
| **Backups** | Restic-powered jobs — init, run now, list snapshots, integrity check, dry-run restore test, confidence scoring. |
| **Alerts** | Metric threshold alerts + TCP/HTTP status checks, ack/resolve flow, public `/status` page (no auth). |
| **Automation** | Scheduled shell jobs — cron-style schedules, run history with output, enable/disable toggle. |
| **Secrets** | AES-256-GCM encrypted secrets store — values never appear in list responses, reveal-on-demand with audit logging. |
| **Resource tags** | Create colour-coded tags, assign to services and containers, filter any page by tag. |
| **Timeline** | Global audit timeline — category chips, free-text search, outcome filter, paginated infinite scroll. |
| **Capabilities** | Detect installed tools (Docker, libvirt, WireGuard, restic, nginx, GPU, …) with version strings and install hints. |
| **Diagnostics** | 12 system health checks — config/data dirs, DB, frontend assets, disk space, Docker daemon, nginx config, port bind. |
| **Security** | Session list for all users, revoke individual sessions or all-others, full audit log. |
| **Themes** | 7 built-in themes + custom token editor with live color pickers, 14-param animation editor, randomise button. |
| **Animated backgrounds** | 7 canvas-based presets (Void, Grid, Aurora, Pulse, Noise, Hex, Circuit) + 4 glass levels. |
| **System** | In-app updater — Docker mode: checks GHCR for a newer image, applies with container recreation; bare-metal mode: checks upstream branch commits, pulls latest, rebuilds, and restarts. OS package updates (apt/pacman/dnf) with dry-run. Rollback points for bare-metal installs. |
| **Mobile** | Responsive layout — hamburger sidebar on small screens, touch-friendly targets. |

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
GET  /api/models/download/:id
GET  /api/models/active
POST /api/models/load              { filename }
DELETE /api/models/:filename
POST /api/models/ollama/pull       { model }
GET  /api/models/ollama/pull/:id
POST /api/models/ollama/create     { filename }
GET  /api/models/ollama/create/:id
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
GET  /api/system/version
GET  /api/system/update-check
POST /api/system/restart
POST /api/system/update
```

### Integrations
```
GET  /api/integrations/scopes
GET  /api/integrations/tokens
POST /api/integrations/tokens                    { name, scopes[], expires_days? }
DELETE /api/integrations/tokens/:id
GET  /api/integrations/odysseus/config
POST /api/integrations/odysseus/config           { enabled?, mcp_enabled?, allowed_url?, webhook_secret?, emergency_disable? }
GET  /api/integrations/odysseus/manifest
GET  /api/integrations/events                    SSE stream
POST /api/integrations/webhooks                  { automation_id, dry_run? }
GET  /api/integrations/actions
```

### Voidwatch (Odysseus-side)
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

### Capabilities & diagnostics
```
GET /api/capabilities
GET /api/diagnostics
```

---

## Roles

| Role | Capabilities |
|---|---|
| `owner` | Everything, including deleting other admins |
| `admin` | Create/delete users (not owner), all actions |
| `operator` | Start/stop services, deploy apps, write files, terminal |
| `viewer` | Read-only access to all pages |

---

## License

AGPL-3.0 — see [LICENSE](LICENSE).
