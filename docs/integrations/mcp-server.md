# VoidTower MCP Server

`odysseus-mcp-servers/voidtower_server.py` is a standalone Python process that exposes VoidTower's infrastructure management capabilities as MCP tools. It runs via stdio transport and can be registered in any MCP-compatible AI host — including Odysseus, Claude Desktop, or any application that supports the Model Context Protocol.

This is separate from the Voidwatch webhook/SSE integration. Use this when you want an AI model to call VoidTower tools directly via MCP rather than through Odysseus's Voidwatch layer.

---

## Setup

**Prerequisites:** Python 3.10+, a VoidTower API token with appropriate scopes.

```bash
cd odysseus-mcp-servers
pip install mcp httpx
```

**Required environment variables:**

| Variable | Description |
|---|---|
| `VOIDTOWER_URL` | VoidTower base URL (default: `http://localhost:8743`) |
| `VOIDTOWER_TOKEN` | API token with the scopes the tools need (see below) |

**Run manually:**

```bash
VOIDTOWER_URL=http://192.168.1.10:8743 \
VOIDTOWER_TOKEN=vt_your_token_here \
python voidtower_server.py
```

---

## Which servers to add

**Only add servers for apps you actually have deployed.** Each server is an independent process — there is no bundle. Register `voidtower_server.py` always (it covers the host itself), then add app-specific servers one by one for whatever is running.

---

## Registering in Claude Desktop or Cursor / VS Code

All servers go into **one JSON file** as separate keys under `mcpServers`. Each key spawns an independent process.

- **Claude Desktop** — `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows)
- **Cursor** — `.cursor/mcp.json` in your project, or `~/.cursor/mcp.json` globally
- **VS Code** — workspace `.vscode/mcp.json` or user settings

```json
{
  "mcpServers": {
    "voidtower": {
      "command": "python",
      "args": ["/path/to/odysseus-mcp-servers/voidtower_server.py"],
      "env": {
        "VOIDTOWER_URL": "http://192.168.1.10:8743",
        "VOIDTOWER_TOKEN": "vt_your_token_here"
      }
    },
    "jellyfin": {
      "command": "python",
      "args": ["/path/to/odysseus-mcp-servers/jellyfin_server.py"],
      "env": {
        "JELLYFIN_URL": "http://192.168.1.10:8096",
        "JELLYFIN_API_KEY": "your_jellyfin_api_key"
      }
    },
    "sonarr": {
      "command": "python",
      "args": ["/path/to/odysseus-mcp-servers/sonarr_server.py"],
      "env": {
        "SONARR_URL": "http://192.168.1.10:8989",
        "SONARR_API_KEY": "your_sonarr_api_key"
      }
    }
  }
}
```

Add as many entries as you have deployed apps. Restart Claude Desktop / reload the window after editing.

---

## Registering in Odysseus

Odysseus adds each server as a **separate entry** via the UI — one Add per server.

For each server you want:

1. Open Odysseus → **Settings → MCP Servers → Add**
2. Set **Transport** to `stdio`
3. Set **Command** to `python /path/to/odysseus-mcp-servers/<name>_server.py`
4. Add the required environment variables for that server (see table below)
5. Save — Odysseus starts the process and lists its tools immediately

Repeat for each app server. There is no limit on how many you can add.

---

## Registering in Open WebUI

Open WebUI does not speak MCP stdio natively — use [`mcpo`](https://github.com/open-webui/mcpo) to bridge each server to an OpenAPI HTTP endpoint. Each server needs its **own port**.

```bash
pip install mcpo

# One mcpo process per server, on sequential ports
VOIDTOWER_URL=http://192.168.1.10:8743 VOIDTOWER_TOKEN=vt_xxx \
  mcpo --port 8810 -- python /path/to/odysseus-mcp-servers/voidtower_server.py &

JELLYFIN_URL=http://192.168.1.10:8096 JELLYFIN_API_KEY=xxx \
  mcpo --port 8811 -- python /path/to/odysseus-mcp-servers/jellyfin_server.py &

SONARR_URL=http://192.168.1.10:8989 SONARR_API_KEY=xxx \
  mcpo --port 8812 -- python /path/to/odysseus-mcp-servers/sonarr_server.py &
```

Then in Open WebUI, for each server:

1. **Workspace → Tools → +**
2. Click **Import from URL**
3. Enter `http://localhost:8811/openapi.json` (adjust port per server)
4. Save

> **Tip:** Put the mcpo launch commands in a startup script (systemd service or `docker-compose` with a sidecar container). Port range 8810–8849 is suggested — well away from VoidTower's embed proxy range (8800–8899 is already in use, so use ports outside that or a separate range).

> **Security:** mcpo endpoints have no auth — the app credentials are baked into the environment. Keep them on loopback or behind a VPN.

---

## Available tools

### Read-only tools

| Tool | Scopes needed | Description |
|---|---|---|
| `vt_get_metrics` | `metrics:read` | Current CPU, RAM, disk, network, top processes |
| `vt_list_services` | `services:read` | All systemd services and their state |
| `vt_get_service_logs` | `services:read` | Recent log lines from a systemd service |
| `vt_list_containers` | `containers:read` | All Docker containers with state and ports |
| `vt_get_container_logs` | `containers:logs` | Recent log lines from a container (default: 100) |
| `vt_list_alerts` | `alerts:read` | Active, acknowledged, or resolved alerts |
| `vt_get_status_summary` | `alerts:read` | High-level health: active alerts count, failing checks |
| `vt_list_status_checks` | `alerts:read` | All HTTP/TCP status checks and their up/down state |
| `vt_list_deployed_apps` | `apps:read` | Apps deployed from the App Vault |
| `vt_list_app_catalog` | `apps:read` | All apps available in the App Vault catalog |
| `vt_get_app_status` | `apps:read` | Status and container health of a deployed app |
| `vt_get_app_logs` | `apps:read` | Recent logs from a deployed app |
| `vt_get_app_compose` | `apps:read` | Read the Docker Compose file for a deployed app |
| `vt_list_backups` | `backups:read` | Backup configs and their last run status |
| `vt_list_automations` | `automation:read` | Automation jobs with schedule and last status |
| `vt_get_timeline` | `timeline:read` | Filtered activity timeline — supports category and search |
| `vt_get_audit_log` | `timeline:read` | Recent audit log entries (up to 100) |
| `vt_list_proxies` | `proxy:read` | All nginx reverse proxy rules |
| `vt_list_firewall_rules` | `network:read` | Active firewall rules (ufw/firewalld/iptables) |
| `vt_list_wireguard_peers` | `network:read` | WireGuard peers and connection status |
| `vt_get_storage` | `storage:read` | Block devices, mount points, disk health |
| `vt_get_network_neighbors` | `network:read` | Devices on the local network (ARP/LAN scan) |
| `vt_list_vms` | `vms:read` | Proxmox VMs and LXC containers across all configured hosts |
| `vt_list_secrets` | `secrets:list` | Secret names and descriptions (values never returned) |
| `vt_list_tags` | `tags:read` | All resource tags |
| `vt_list_users` | `metrics:read` | All VoidTower users |
| `vt_run_diagnostics` | `diagnostics:read` | Run the system diagnostics check suite |
| `vt_get_capabilities` | `diagnostics:read` | Installed tools: Docker, nginx, WireGuard, GPU, restic, etc. |

### Action tools

| Tool | Scopes needed | Description |
|---|---|---|
| `vt_control_service` | `services:restart` | Start, stop, restart, enable, or disable a systemd service |
| `vt_control_container` | `containers:restart` | Start, stop, restart, or remove a Docker container |
| `vt_control_app` | `apps:restart` | Start, stop, restart, or redeploy a deployed App Vault app |
| `vt_update_app_compose` | `apps:deploy` | Replace the Docker Compose file for a deployed app |
| `vt_deploy_app` | `apps:deploy` | Deploy an app from the App Vault catalog (supports `env_overrides`) |
| `vt_remove_app` | `apps:deploy` | Remove a deployed app (config only — data on disk is not deleted) |
| `vt_toggle_proxy` | `proxy:manage` | Enable or disable an nginx proxy rule |
| `vt_create_proxy` | `proxy:manage` | Create a new nginx reverse proxy rule |
| `vt_run_backup` | `backups:run` | Trigger an immediate backup for a backup job |
| `vt_run_automation_job` | `automation:run` | Trigger an automation job and wait for output |
| `vt_control_vm` | `vms:read` | Start, stop, reboot, or shutdown a Proxmox VM or LXC |
| `vt_acknowledge_alert` | `alerts:ack` | Acknowledge an active alert |
| `vt_resolve_alert` | `alerts:ack` | Mark an alert as resolved |

---

## Recommended token scopes

For a read-only MCP session:

```
metrics:read  services:read  containers:read  containers:logs
apps:read  backups:read  alerts:read  automation:read
timeline:read  network:read  storage:read  diagnostics:read
proxy:read  tags:read  vms:read  secrets:list
```

For full control:

```
metrics:read  services:read  services:restart
containers:read  containers:logs  containers:restart
apps:read  apps:deploy  apps:restart
backups:read  backups:run
alerts:read  alerts:ack
automation:read  automation:run
timeline:read  network:read  storage:read  diagnostics:read
proxy:read  proxy:manage  tags:read  vms:read  secrets:list
```

See [docs/api-tokens.md](../api-tokens.md) for how to create a token with these scopes.

---

## App-specific MCP servers

`voidtower_server.py` gives AI agents control over VoidTower itself. For apps *deployed by* VoidTower, separate server files talk directly to each app's own API.

| File | App | Auth env vars |
|---|---|---|
| **Arr stack** | | |
| `sonarr_server.py` | Sonarr TV manager | `SONARR_URL`, `SONARR_API_KEY` |
| `radarr_server.py` | Radarr movie manager | `RADARR_URL`, `RADARR_API_KEY` |
| `lidarr_server.py` | Lidarr music manager | `LIDARR_URL`, `LIDARR_API_KEY` |
| `readarr_server.py` | Readarr book manager | `READARR_URL`, `READARR_API_KEY` |
| `prowlarr_server.py` | Prowlarr indexer manager | `PROWLARR_URL`, `PROWLARR_API_KEY` |
| `bazarr_server.py` | Bazarr subtitle manager | `BAZARR_URL`, `BAZARR_API_KEY` |
| `jellyseerr_server.py` | Jellyseerr request manager | `JELLYSEERR_URL`, `JELLYSEERR_API_KEY` |
| **Media** | | |
| `jellyfin_server.py` | Jellyfin media server | `JELLYFIN_URL`, `JELLYFIN_API_KEY` |
| `immich_server.py` | Immich photo management | `IMMICH_URL`, `IMMICH_API_KEY` |
| `navidrome_server.py` | Navidrome music server | `NAVIDROME_URL`, `NAVIDROME_USER`, `NAVIDROME_PASSWORD` |
| `kavita_server.py` | Kavita comic/book reader | `KAVITA_URL`, `KAVITA_API_KEY` |
| `freshrss_server.py` | FreshRSS feed reader | `FRESHRSS_URL`, `FRESHRSS_USER`, `FRESHRSS_PASSWORD` |
| `qbittorrent_server.py` | qBittorrent torrent client | `QB_URL`, `QB_USERNAME`, `QB_PASSWORD` |
| **Productivity & collaboration** | | |
| `outline_server.py` | Outline wiki | `OUTLINE_URL`, `OUTLINE_API_KEY` |
| `paperless_server.py` | Paperless-ngx documents | `PAPERLESS_URL`, `PAPERLESS_TOKEN` |
| `mealie_server.py` | Mealie recipe manager | `MEALIE_URL`, `MEALIE_TOKEN` |
| `vikunja_server.py` | Vikunja task manager | `VIKUNJA_URL`, `VIKUNJA_TOKEN` |
| `n8n_server.py` | n8n workflow automation | `N8N_URL`, `N8N_API_KEY` |
| `homeassistant_server.py` | Home Assistant | `HA_URL`, `HA_TOKEN` |
| `matrix_synapse_server.py` | Matrix Synapse chat | `SYNAPSE_URL`, `SYNAPSE_ADMIN_TOKEN` |
| `gitea_server.py` | Gitea git hosting | `GITEA_URL`, `GITEA_TOKEN` |
| `changedetection_server.py` | changedetection.io | `CHANGEDETECTION_URL`, `CHANGEDETECTION_API_KEY` |
| **AI & ML** | | |
| `ollama_server.py` | Ollama LLM runtime | `OLLAMA_URL` |
| `llama_cpp_server.py` | llama.cpp server | `LLAMACPP_URL` |
| `open_webui_server.py` | Open WebUI | `OPENWEBUI_URL`, `OPENWEBUI_TOKEN` |
| `comfyui_server.py` | ComfyUI image generation | `COMFYUI_URL` |
| **Monitoring & network** | | |
| `pihole_server.py` | Pi-hole DNS filter | `PIHOLE_URL`, `PIHOLE_PASSWORD` |
| `adguardhome_server.py` | AdGuard Home DNS filter | `ADGUARD_URL`, `ADGUARD_USERNAME`, `ADGUARD_PASSWORD` |
| `uptime_kuma_server.py` | Uptime Kuma monitor | `UPTIME_KUMA_URL`, `UPTIME_KUMA_USERNAME`, `UPTIME_KUMA_PASSWORD` |
| `grafana_server.py` | Grafana dashboards | `GRAFANA_URL`, `GRAFANA_API_KEY` |
| `syncthing_server.py` | Syncthing file sync | `SYNCTHING_URL`, `SYNCTHING_API_KEY` |
| `wireguard_easy_server.py` | WireGuard Easy VPN | `WG_EASY_URL`, `WG_EASY_PASSWORD` |
| `tailscale_server.py` | Tailscale mesh VPN | `TAILSCALE_API_KEY`, `TAILSCALE_TAILNET` |
| **Infrastructure** | | |
| `portainer_server.py` | Portainer container mgmt | `PORTAINER_URL`, `PORTAINER_TOKEN` |
| `authentik_server.py` | Authentik identity provider | `AUTHENTIK_URL`, `AUTHENTIK_TOKEN` |
| `nextcloud_server.py` | Nextcloud file platform | `NEXTCLOUD_URL`, `NEXTCLOUD_USER`, `NEXTCLOUD_PASSWORD` |
| `minio_server.py` | MinIO object storage | `MINIO_CONSOLE_URL`, `MINIO_ACCESS_KEY`, `MINIO_SECRET_KEY` |
| `vaultwarden_server.py` | Vaultwarden password manager | `VAULTWARDEN_URL`, `VAULTWARDEN_ADMIN_TOKEN` |
| `searxng_server.py` | SearXNG metasearch | `SEARXNG_URL` |
| `stirling_pdf_server.py` | Stirling-PDF processor | `STIRLING_URL` |

### Adding a new app server

Each server file follows the same pattern:

```python
BASE_URL = os.environ.get("MYAPP_URL", "http://localhost:XXXX")
API_KEY  = os.environ.get("MYAPP_API_KEY", "")
server   = Server("myapp")
# implement list_tools() + call_tool() using the app's REST API
```

Register it as a second MCP server entry in your host (Odysseus, Claude Desktop, etc.) alongside `voidtower_server.py`.

---

## Difference from Voidwatch

| | MCP server | Voidwatch |
|---|---|---|
| Transport | MCP stdio (any MCP host) | HTTP webhooks + SSE (Odysseus only) |
| Auth | API token in env var | Signed webhook secret |
| Policy enforcement | None — token scopes only | Voidwatch policy engine |
| Audit trail | VoidTower audit log (actor: token) | VoidTower audit log (actor: `odysseus`) |
| Real-time alerts | Not supported | SSE stream pushed to Odysseus |
| Use case | Direct model tool calls, Claude Desktop, custom AI apps | Odysseus agent workflows with policy control |
