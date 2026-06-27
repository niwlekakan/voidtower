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

## Registering in Odysseus

1. Open Odysseus → **Settings → MCP Servers → Add**
2. Set **Transport** to `stdio`
3. Set **Command** to `python /path/to/odysseus-mcp-servers/voidtower_server.py`
4. Add environment variables:
   - `VOIDTOWER_URL` = your VoidTower URL
   - `VOIDTOWER_TOKEN` = your API token
5. Save — Odysseus will start the process and list the available tools

---

## Registering in Claude Desktop

Add to `claude_desktop_config.json`:

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
    }
  }
}
```

---

## Available tools

### Read-only tools

| Tool | Scopes needed | Description |
|---|---|---|
| `vt_get_metrics` | `metrics:read` | Current CPU, RAM, disk, network, top processes |
| `vt_list_services` | `services:read` | All systemd services and their state |
| `vt_list_containers` | `containers:read` | All Docker containers with state and ports |
| `vt_get_container_logs` | `containers:logs` | Recent log lines from a container (default: 100) |
| `vt_list_alerts` | `alerts:read` | Active, acknowledged, or resolved alerts |
| `vt_list_deployed_apps` | `apps:read` | Apps deployed from the App Vault |
| `vt_list_backups` | `backups:read` | Backup configs and their last run status |
| `vt_get_audit_log` | `timeline:read` | Recent audit log entries (up to 100) |
| `vt_list_users` | `metrics:read` | All VoidTower users |
| `vt_run_diagnostics` | `diagnostics:read` | Run the system diagnostics check suite |
| `vt_get_timeline` | `timeline:read` | Filtered activity timeline — supports category and search |
| `vt_list_wireguard_peers` | `network:read` | WireGuard peers and connection status |
| `vt_list_automations` | `automation:read` | Automation jobs with schedule and last status |
| `vt_get_capabilities` | `diagnostics:read` | Installed tools: Docker, nginx, WireGuard, GPU, restic, etc. |
| `vt_get_storage` | `storage:read` | Block devices, mount points, disk health |
| `vt_get_network_neighbors` | `network:read` | Devices on the local network (ARP/LAN scan) |
| `vt_list_secrets` | `secrets:list` | Secret names and descriptions (values never returned) |
| `vt_get_status_summary` | `alerts:read` | High-level health: active alerts, failing checks |

### Action tools

| Tool | Scopes needed | Description |
|---|---|---|
| `vt_control_service` | `services:restart` | Start, stop, restart, enable, or disable a systemd service |
| `vt_control_container` | `containers:restart` | Start, stop, restart, or remove a Docker container |
| `vt_deploy_app` | `apps:deploy` | Deploy an app from the App Vault catalog |
| `vt_create_proxy` | `proxy:manage` | Create an nginx reverse proxy rule |
| `vt_run_backup` | `backups:run` | Trigger an immediate backup for a backup job |
| `vt_run_automation_job` | `automation:run` | Trigger an automation job and wait for output |
| `vt_acknowledge_alert` | `alerts:ack` | Acknowledge an active alert |
| `vt_resolve_alert` | `alerts:ack` | Mark an alert as resolved |

---

## Recommended token scopes

For a read-only MCP session:

```
metrics:read  services:read  containers:read  containers:logs
apps:read  backups:read  alerts:read  automation:read
timeline:read  network:read  storage:read  diagnostics:read
secrets:list
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
proxy:read  proxy:manage  secrets:list
```

See [docs/api-tokens.md](../api-tokens.md) for how to create a token with these scopes.

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
