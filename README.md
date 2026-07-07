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
git clone -b main https://github.com/niwlekakan/voidtower
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
| `docker compose --profile aio up -d` | + Odysseus, ChromaDB, SearXNG, ntfy |
| `docker compose --profile aio --profile ai up -d` | + Ollama local AI |

Homelab apps (Jellyfin, Nextcloud, etc.) are never started by Compose — deploy them from **VoidTower → App Vault**.

The `docker-compose.yml` mounts `/var/run/docker.sock` so VoidTower can manage containers and update itself from the UI. If you prefer not to expose the socket, comment that line out — container management and in-UI updates will be unavailable.

---

## Quick start — system install (bare metal / VM)

```bash
# VoidTower only
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/scripts/install.sh \
  | sudo bash

# Full AIO stack
curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/main/scripts/install.sh \
  | sudo bash -s -- --all-in-one --pull-model

# Non-interactive with specific model
sudo bash install.sh \
  --all-in-one \
  --ai-model qwen2.5-coder:7b-instruct \
  --pull-model \
  --yes
```

For the full installer flag reference, maintenance flags (`--update`, `--reset`, `--repair`, `--uninstall`), model auto-selection, and post-install service management, see [docs/install/all-in-one.md](docs/install/all-in-one.md).

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

For the full scope reference, token creation, secret restrictions, and security notes see [docs/api-tokens.md](docs/api-tokens.md). For the standalone MCP server (use with Claude Desktop or any MCP host), see [docs/integrations/mcp-server.md](docs/integrations/mcp-server.md).

---

## Toolpacks

Voidwatch ships 20 pre-built toolpacks for common homelab apps. These define the safe operations Odysseus can perform per-app:

`authentik` · `docker` · `freshrss` · `gitea` · `grafana` · `homeassistant` · `immich` · `jellyfin` · `minio` · `n8n` · `nextcloud` · `nginx` · `ollama` · `open-webui` · `paperless` · `pihole` · `portainer` · `syncthing` · `uptime-kuma` · `vaultwarden`

Toolpacks live in `voidwatch/toolpacks/` inside the Odysseus install. Add your own by dropping a YAML file there — see any existing toolpack as a template. For adding custom apps to the App Vault catalog, see [docs/app-vault-catalog.md](docs/app-vault-catalog.md).

---

## Networking & reverse proxy

Apps deployed from App Vault are reachable directly via their host port, and optionally via `http://<app>.local:8080` through the bundled nginx-proxy + Pi-hole/AdGuard setup. For the full picture — vt-proxy/nginx-proxy internals, DNS setup (Pi-hole/AdGuard), Traefik/Caddy alternatives, and remote access via Tailscale or WireGuard — see [docs/NETWORKING.md](docs/NETWORKING.md).

To use Authentik as a central identity provider — SSO + MFA login for VoidTower itself, plus an opt-in forward-auth gate for any App Vault app — see [docs/integrations/authentik-sso.md](docs/integrations/authentik-sso.md).

---

## TrueNAS Scale

Two deployment paths depending on how much access you want. See [docs/platforms/truenas.md](docs/platforms/truenas.md) for the full guide including Option A (Custom App UI), Option B (SSH Docker Compose), Ollama setup, Odysseus password reset, service management, updates, and uninstall.

> **Docker control disclaimer:** The TrueNAS Custom App UI (Option A) runs containers through its own Kubernetes layer (k3s) and does **not** expose `/var/run/docker.sock`. Container management, exec shell, and the in-UI self-update will be unavailable. If you need these, use Option B.

---

## Proxmox LXC

Running VoidTower in a Proxmox LXC container is the recommended approach for Proxmox users — it gives you a clean isolated environment with direct Docker access and full container management. See [docs/platforms/proxmox-lxc.md](docs/platforms/proxmox-lxc.md) for the full setup guide.

## Proxmox integration (UI)

VoidTower connects to any Proxmox VE host via its API — browse VMs and LXC containers across all nodes, start/stop/reboot, take and roll back snapshots, view the PBS backup tab, open a noVNC console, and deploy App Vault apps directly to a Proxmox LXC. See [docs/integrations/proxmox.md](docs/integrations/proxmox.md) for setup, token requirements, and the full feature list.

---

## GPU / Ollama

See [docs/gpu.md](docs/gpu.md) for NVIDIA, AMD (ROCm/Vulkan), remote Ollama, and TrueNAS-specific GPU setup. Quick reference:

```bash
# Pull a model (Docker)
docker exec ollama ollama pull qwen2.5-coder:7b-instruct

# Use a remote Ollama instance — set in .env
OLLAMA_BASE_URL=http://192.168.1.5:11434
```

---

## Updates

### Docker

The VoidTower image is published to `ghcr.io/niwlekakan/voidtower` on every push to this branch and on release tags.

1. Open **VoidTower → System → Updates → VoidTower Application**
2. Click **Check for update** — pulls the latest image manifest
3. If a newer image is available, click **Apply update** — VoidTower pulls the image and recreates its own container; the UI reconnects automatically

Requires `/var/run/docker.sock` to be mounted (enabled by default). To pin a release, set `VOIDTOWER_IMAGE=ghcr.io/niwlekakan/voidtower:aio-v1.2.3` in `.env` before restarting.

Companion containers (Odysseus, SearXNG, etc.) are updated from the same page under **Docker Images**.

### Bare metal / LXC

**From the UI:** Open **VoidTower → System → Updates → VoidTower Application** — the current commit is compared against upstream, pending commits are listed, and **Apply update** tags a rollback point, pulls from GitHub, rebuilds, and restarts. The **OS Packages** section handles apt / pacman / dnf updates.

**From the command line:**

```bash
sudo bash scripts/install.sh --update            # latest release
sudo bash scripts/install.sh --update --version v1.2.3
```

### TrueNAS

- **Option A:** TrueNAS shows an update banner when a newer image is available — click **Update** in **Apps → voidtower**.
- **Option B:** `docker pull ghcr.io/niwlekakan/voidtower:aio-latest && docker compose ... up -d`

---

## Admin CLI

The `voidtower` binary has built-in subcommands for user and backup management — they read/write the database directly and exit, without starting the web server.

```bash
# Prefix: Docker → docker exec voidtower voidtower …  |  bare metal → sudo voidtower …

# Users
voidtower user list
voidtower user create --username <name> --password <pw> --role owner|admin|operator|viewer
voidtower user reset-password --username <name> --password <newpw>   # use if locked out — forces password change on next login
voidtower user set-role --username <name> --role <role>
voidtower user delete --username <name>

# Backups
voidtower backup list
voidtower backup create --name <name> --source <path> --repo <resticRepo> [--retention-days N]
voidtower backup run --name <name>            # requires restic on PATH
voidtower backup check --name <name>          # restic check
voidtower backup restore-test --name <name>   # dry-run restore
voidtower backup delete --name <name>         # removes config only, not data on disk
```

`RESTIC_PASSWORD` env var applies the same as scheduled backup jobs (defaults to `changeme` if unset).

---

## Service management

### Docker

```bash
docker compose ps
docker compose logs -f voidtower
docker compose logs -f odysseus
docker compose restart odysseus
docker compose --profile aio --profile ai down
docker compose --profile aio up -d
```

### Bare metal / LXC

```bash
systemctl status voidtower odysseus ollama
journalctl -u voidtower -f
systemctl restart voidtower
systemctl restart odysseus
```

### TrueNAS Option A

Use the TrueNAS UI: **Apps → voidtower → Start / Stop / Restart**. For logs, click the log icon next to each container under **Apps → voidtower → Logs**.

---

## Uninstall

### Docker

```bash
# Stop and remove containers — data and config volumes preserved
docker compose --profile aio --profile ai down

# Also remove all data and config
docker compose --profile aio --profile ai down -v
```

### Bare metal / LXC

```bash
# Interactive — choose what to remove (data, config, system users)
sudo bash scripts/install.sh --uninstall

# Non-interactive full purge
sudo bash scripts/install.sh --uninstall --yes
```

The interactive flow prompts separately for: database (`/var/lib/voidtower`), config and secrets (`/etc/voidtower`), Odysseus data and config, and system users `voidtower` / `odysseus`.

### TrueNAS

See [docs/platforms/truenas.md](docs/platforms/truenas.md#uninstall).

---

## Recovery & maintenance

For recovering admin access, resetting passwords, full resets, reinstalls, and repairs across all deployment types, see [docs/recovery.md](docs/recovery.md).

**Quick reference — locked out of VoidTower:**

```bash
# Docker
docker exec voidtower voidtower user reset-password --username <name> --password <newpassword>

# Bare metal / LXC
sudo voidtower user reset-password --username <name> --password <newpassword>

# Don't know the username?
docker exec voidtower voidtower user list   # Docker
sudo voidtower user list                    # bare metal
```

---

## Features

| Area | What you get |
|---|---|
| **Dashboard** | Customizable widgets — clock, weather, CPU/RAM/disk charts, container summary, alert count. Nine toggleable widgets with drag-to-reorder sections, config persisted per-browser. |
| **Services** | Manage systemd units — start, stop, restart, enable/disable, view logs. Resource tag filtering. |
| **Containers** | Docker container list, start/stop/restart, log viewer, per-container exec shell, compose file editor with staged diff before apply. Resource tag filtering. |
| **App Vault** | 50+ one-click app deployments (Gitea, Nextcloud, Jellyfin, Grafana, Pi-hole, n8n, Ollama, Open WebUI, Home Assistant, Odysseus, and more). Pre-deploy modal shows compose config, env var overrides, and auto-generated secrets before launch. Deploy to Proxmox LXC directly from the catalog. |
| **AI Discover** | Ask the configured LLM to recommend self-hosted apps; results include Docker image names and direct deploy buttons for catalog matches. |
| **Models** | Download GGUF models from URL with popular presets (Content-Type and GGUF magic-byte validated), pull models via Ollama by name, import downloaded GGUFs into Ollama. Live progress bars. |
| **AI workspace** | Iframe-embed any OpenAI-compatible workspace (Odysseus, Open WebUI, etc.). Floating GPU controls panel shows VRAM bar, GPU utilisation %, llama.cpp process list, and one-click unload. |
| **VMs** | KVM/QEMU local VM management via libvirt (`virsh`). Proxmox integration — connect to any Proxmox host via API token, list QEMU VMs and LXC containers, start/stop/reboot with CPU/RAM/uptime stats. |
| **Files** | Full filesystem browser — Monaco editor (25+ language detection), inline image viewer, PDF viewer, new file creation, per-file download, breadcrumb navigation, roots sidebar. |
| **Terminal** | Full PTY browser terminal with shell auto-detection from `/etc/passwd`. SSH session manager — save hosts, connect with one click. |
| **Reverse Proxies** | nginx-backed proxy rule manager — domain + upstream + SSL + optional iframe-embed headers, configs written to the Docker nginx-proxy container's conf.d and reloaded automatically. |
| **Firewall** | UFW rule management — add/delete rules (port, protocol, direction, source CIDR), enable/disable toggle, colour-coded allow/deny columns. |
| **WireGuard** | Peer management — generate Curve25519 keypairs natively, allocate IPs from existing interface subnet, add/remove peers live, client config shown once with copy button. |
| **Storage** | Block device tree, mount manager, fstab editor, format disks, SMART health, software RAID (mdadm) status and creation. Configurable storage paths. |
| **Network** | Real-time interface stats, LAN neighbour table (ARP cache), bandwidth charts. |
| **Backups** | Restic-powered jobs — init, run now, list snapshots, integrity check, dry-run restore test, confidence scoring. |
| **Alerts** | Metric threshold alerts + TCP/HTTP status checks, ack/resolve flow, public `/status` page (no auth). |
| **Automation** | Scheduled shell jobs — cron-style schedules, run history with output, enable/disable toggle. |
| **Secrets** | AES-256-GCM encrypted secrets store — values never appear in list responses, reveal-on-demand with audit logging. |
| **Resource tags** | Create colour-coded tags, assign to services and containers, filter any page by tag. |
| **Timeline** | Global audit timeline — category chips, free-text search, outcome filter, paginated infinite scroll. |
| **Capabilities** | Detect installed tools (Docker, libvirt, WireGuard, restic, nginx, GPU, …) with version strings and install hints. |
| **Diagnostics** | 12 system health checks — config/data dirs, DB, frontend assets, disk space, Docker daemon, nginx config, port bind. |
| **Security** | Session list for all users, revoke individual sessions or all-others, full audit log. TOTP 2FA per-user (see [docs/totp.md](docs/totp.md)). |
| **Themes** | 7 built-in themes + custom token editor with live color pickers, 14-param animation editor, randomise button. |
| **Animated backgrounds** | 7 canvas-based presets (Void, Grid, Aurora, Pulse, Noise, Hex, Circuit) + 4 glass levels. |
| **System** | In-app updater (Docker: GHCR image; bare-metal: upstream branch pull + rebuild). OS package updates (apt/pacman/dnf) with dry-run. Rollback points for bare-metal installs. |
| **Mobile** | Responsive layout — hamburger sidebar on small screens, touch-friendly targets. |

---

## API reference

See [docs/api.md](docs/api.md) for the full endpoint list.

All endpoints require a valid `vt_session` cookie except `/api/auth/*`, `/api/status`, and `/api/system/version`.

---

## Docs

### Install & platforms
- [Installation guide & flags](docs/install/all-in-one.md)
- [TrueNAS Scale](docs/platforms/truenas.md)
- [Proxmox LXC](docs/platforms/proxmox-lxc.md)
- [GPU & Ollama](docs/gpu.md)

### Features
- [App Vault catalog — adding custom apps](docs/app-vault-catalog.md)
- [API tokens & scopes](docs/api-tokens.md)
- [Two-factor authentication (TOTP)](docs/totp.md)

### Integrations
- [Odysseus / Voidwatch](docs/integrations/odysseus.md)
- [Proxmox VE](docs/integrations/proxmox.md)
- [MCP server](docs/integrations/mcp-server.md)
- [Authentik SSO](docs/integrations/authentik-sso.md)
- [Networking & reverse proxy](docs/NETWORKING.md)

### Operations
- [Recovery & maintenance](docs/recovery.md)
- [API reference](docs/api.md)

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
