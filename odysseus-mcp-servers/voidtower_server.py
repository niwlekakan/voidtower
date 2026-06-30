#!/usr/bin/env python3
"""
VoidTower MCP Server — gives Odysseus AI tools to manage VoidTower infrastructure.

Setup:
  pip install mcp httpx
  VOIDTOWER_URL=http://localhost:8743 VOIDTOWER_TOKEN=<api-token> python voidtower_server.py

Register in Odysseus:
  Settings → MCP Servers → Add → Command: python /path/to/voidtower_server.py
  Env: VOIDTOWER_URL, VOIDTOWER_TOKEN
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

VOIDTOWER_URL = os.environ.get("VOIDTOWER_URL", "http://localhost:8743").rstrip("/")
VOIDTOWER_TOKEN = os.environ.get("VOIDTOWER_TOKEN", "")

server = Server("voidtower")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=VOIDTOWER_URL,
            headers={"Authorization": f"Bearer {VOIDTOWER_TOKEN}"},
            timeout=30,
        )
    return _client


async def vt_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def vt_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    return r.json()


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="vt_get_metrics",
            description="Get current system metrics: CPU, RAM, disk, network, top processes",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_services",
            description="List all systemd services with their status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_control_service",
            description="Start, stop, restart, enable, or disable a systemd service",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Service name (e.g. nginx)"},
                    "action": {"type": "string", "enum": ["start", "stop", "restart", "enable", "disable"]},
                },
                "required": ["name", "action"],
            },
        ),
        types.Tool(
            name="vt_list_containers",
            description="List all Docker containers with state and ports",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_control_container",
            description="Start, stop, restart, or remove a Docker container",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Container ID or short ID"},
                    "action": {"type": "string", "enum": ["start", "stop", "restart", "remove"]},
                },
                "required": ["id", "action"],
            },
        ),
        types.Tool(
            name="vt_list_alerts",
            description="List active infrastructure alerts",
            inputSchema={
                "type": "object",
                "properties": {
                    "state": {"type": "string", "enum": ["active", "acknowledged", "resolved"], "default": "active"},
                },
            },
        ),
        types.Tool(
            name="vt_list_deployed_apps",
            description="List apps deployed from the App Vault",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_deploy_app",
            description="Deploy an app from the App Vault catalog",
            inputSchema={
                "type": "object",
                "properties": {
                    "app_id": {"type": "string", "description": "App ID from the catalog (e.g. 'nextcloud')"},
                    "project_name": {"type": "string", "description": "Optional Docker Compose project name"},
                    "env_overrides": {
                        "type": "object",
                        "description": "Optional env var overrides (key/value pairs, e.g. {\"ADMIN_PASSWORD\": \"secret\"})",
                        "additionalProperties": {"type": "string"},
                    },
                },
                "required": ["app_id"],
            },
        ),
        types.Tool(
            name="vt_create_proxy",
            description="Create an nginx reverse proxy rule",
            inputSchema={
                "type": "object",
                "properties": {
                    "domain": {"type": "string", "description": "Domain name (e.g. app.example.com)"},
                    "upstream": {"type": "string", "description": "Upstream URL (e.g. http://localhost:8080)"},
                    "ssl": {"type": "boolean", "default": False},
                    "allow_embed": {"type": "boolean", "default": True},
                },
                "required": ["domain", "upstream"],
            },
        ),
        types.Tool(
            name="vt_list_backups",
            description="List backup configurations and their last run status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_run_backup",
            description="Trigger an immediate backup for a configured backup job",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Backup config ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_get_audit_log",
            description="Fetch recent audit log entries",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 20, "maximum": 100},
                    "offset": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="vt_list_users",
            description="List all VoidTower users",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_run_diagnostics",
            description="Run the VoidTower diagnostics check suite",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_timeline",
            description="Get a filtered activity timeline of all actions in VoidTower",
            inputSchema={
                "type": "object",
                "properties": {
                    "category": {"type": "string", "description": "Filter by category: auth, containers, services, backups, networking, etc."},
                    "search": {"type": "string", "description": "Free-text search"},
                    "limit": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="vt_list_wireguard_peers",
            description="List WireGuard VPN peers and their connection status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_automations",
            description="List all configured automation jobs with their schedule, last status, and last run time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_run_automation_job",
            description="Trigger an automation job immediately and wait for its output",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Automation job ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_get_container_logs",
            description="Get recent log lines from a Docker container",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Container ID or short ID"},
                    "tail": {"type": "integer", "default": 100, "description": "Number of lines to return"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_acknowledge_alert",
            description="Acknowledge an active infrastructure alert",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Alert ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_resolve_alert",
            description="Mark an infrastructure alert as resolved",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Alert ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_get_capabilities",
            description="Detect which tools are installed and available on this system (Docker, nginx, WireGuard, GPU, restic, etc.)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_storage",
            description="List block storage devices, mount points, and disk health",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_network_neighbors",
            description="List devices on the local network (ARP/LAN scan)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_secrets",
            description="List secret names and descriptions (values are never returned)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_status_summary",
            description="Get a high-level health summary: active alerts, failing status checks, and recent failures",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_service_logs",
            description="Get recent log lines from a systemd service",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Service name (e.g. nginx)"},
                    "tail": {"type": "integer", "default": 100, "description": "Number of lines to return"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_list_proxies",
            description="List all nginx reverse proxy rules with their domain, upstream, SSL status, and enabled state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_toggle_proxy",
            description="Enable or disable a specific nginx reverse proxy rule",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Proxy rule ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_list_firewall_rules",
            description="List all active firewall rules (ufw/firewalld/iptables depending on what is installed)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_app_catalog",
            description="List all apps available in the App Vault catalog (installable one-click apps)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_app_status",
            description="Get the current status and container health of a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name (as shown in deployed apps list)"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_get_app_logs",
            description="Get recent log lines from a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                    "tail": {"type": "integer", "default": 100, "description": "Number of lines to return"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_control_app",
            description="Start, stop, restart, or redeploy a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                    "action": {"type": "string", "enum": ["start", "stop", "restart", "redeploy"]},
                },
                "required": ["name", "action"],
            },
        ),
        types.Tool(
            name="vt_get_app_compose",
            description="Read the Docker Compose file for a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_update_app_compose",
            description="Replace the Docker Compose file for a deployed App Vault application. Changes take effect on next redeploy.",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                    "content": {"type": "string", "description": "Full YAML content of the new compose file"},
                },
                "required": ["name", "content"],
            },
        ),
        types.Tool(
            name="vt_remove_app",
            description="Remove a deployed App Vault application (removes config only — data on disk is not deleted)",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_list_vms",
            description="List all Proxmox virtual machines and LXC containers across all configured Proxmox hosts",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_control_vm",
            description="Start, stop, reboot, or shutdown a Proxmox VM or LXC container",
            inputSchema={
                "type": "object",
                "properties": {
                    "vmid": {"type": "integer", "description": "VM or container ID (e.g. 100)"},
                    "kind": {"type": "string", "enum": ["qemu", "lxc"], "description": "VM type: qemu for VMs, lxc for containers"},
                    "node": {"type": "string", "description": "Proxmox node name"},
                    "action": {"type": "string", "enum": ["start", "stop", "reboot", "shutdown"]},
                },
                "required": ["vmid", "kind", "node", "action"],
            },
        ),
        types.Tool(
            name="vt_list_status_checks",
            description="List all HTTP/TCP status checks and their current up/down state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_tags",
            description="List all resource tags defined in VoidTower",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "vt_get_metrics":
                return _text(await vt_get("/api/metrics/current"))

            case "vt_list_services":
                return _text(await vt_get("/api/services"))

            case "vt_control_service":
                svc = arguments["name"]
                return _text(await vt_post(f"/api/services/{svc}/action", {"action": arguments["action"]}))

            case "vt_list_containers":
                return _text(await vt_get("/api/containers"))

            case "vt_control_container":
                cid = arguments["id"]
                return _text(await vt_post(f"/api/containers/{cid}/action", {"action": arguments["action"]}))

            case "vt_list_alerts":
                state = arguments.get("state", "active")
                return _text(await vt_get("/api/alerts", params={"state": state}))

            case "vt_list_deployed_apps":
                return _text(await vt_get("/api/apps/deployed"))

            case "vt_deploy_app":
                return _text(await vt_post("/api/apps/deploy", {
                    "app_id": arguments["app_id"],
                    "project_name": arguments.get("project_name"),
                    "env_overrides": arguments.get("env_overrides"),
                }))

            case "vt_create_proxy":
                return _text(await vt_post("/api/proxy", {
                    "domain": arguments["domain"],
                    "upstream": arguments["upstream"],
                    "ssl": arguments.get("ssl", False),
                    "allow_embed": arguments.get("allow_embed", True),
                }))

            case "vt_list_backups":
                return _text(await vt_get("/api/backups"))

            case "vt_run_backup":
                return _text(await vt_post(f"/api/backups/{arguments['id']}/run"))

            case "vt_get_audit_log":
                limit = arguments.get("limit", 20)
                offset = arguments.get("offset", 0)
                return _text(await vt_get("/api/audit", params={"limit": limit, "offset": offset}))

            case "vt_list_users":
                return _text(await vt_get("/api/users"))

            case "vt_run_diagnostics":
                return _text(await vt_get("/api/diagnostics"))

            case "vt_get_timeline":
                params: dict = {"limit": arguments.get("limit", 30)}
                if cat := arguments.get("category"):
                    params["category"] = cat
                if s := arguments.get("search"):
                    params["search"] = s
                return _text(await vt_get("/api/timeline", params=params))

            case "vt_list_wireguard_peers":
                return _text(await vt_get("/api/wireguard"))

            case "vt_list_automations":
                return _text(await vt_get("/api/automation"))

            case "vt_run_automation_job":
                return _text(await vt_post(f"/api/automation/{arguments['id']}/run"))

            case "vt_get_container_logs":
                tail = arguments.get("tail", 100)
                data = await vt_get(f"/api/containers/{arguments['id']}/logs", params={"tail": tail})
                lines = data.get("lines", [])
                return [types.TextContent(type="text", text="\n".join(lines))]

            case "vt_acknowledge_alert":
                return _text(await vt_post(f"/api/alerts/{arguments['id']}/acknowledge"))

            case "vt_resolve_alert":
                return _text(await vt_post(f"/api/alerts/{arguments['id']}/resolve"))

            case "vt_get_capabilities":
                return _text(await vt_get("/api/capabilities"))

            case "vt_get_storage":
                devices, mounts = await asyncio.gather(
                    vt_get("/api/storage/devices"),
                    vt_get("/api/storage/mounts"),
                )
                return _text({"devices": devices, "mounts": mounts})

            case "vt_get_network_neighbors":
                return _text(await vt_get("/api/network/neighbors"))

            case "vt_list_secrets":
                return _text(await vt_get("/api/secrets"))

            case "vt_get_status_summary":
                alerts, checks = await asyncio.gather(
                    vt_get("/api/alerts", params={"state": "active"}),
                    vt_get("/api/status-checks"),
                )
                alert_list = alerts.get("alerts", alerts) if isinstance(alerts, dict) else alerts
                check_list = checks.get("checks", checks) if isinstance(checks, dict) else checks
                failing = [c for c in check_list if isinstance(c, dict) and c.get("last_status") == "down"]
                return _text({
                    "active_alerts": len(alert_list),
                    "alerts": alert_list,
                    "failing_checks": len(failing),
                    "failing": failing,
                })

            case "vt_get_service_logs":
                tail = arguments.get("tail", 100)
                data = await vt_get(f"/api/services/{arguments['name']}/logs", params={"tail": tail})
                lines = data.get("lines", []) if isinstance(data, dict) else data
                return [types.TextContent(type="text", text="\n".join(lines) if isinstance(lines, list) else str(lines))]

            case "vt_list_proxies":
                return _text(await vt_get("/api/proxy"))

            case "vt_toggle_proxy":
                return _text(await vt_post(f"/api/proxy/{arguments['id']}/toggle"))

            case "vt_list_firewall_rules":
                return _text(await vt_get("/api/firewall"))

            case "vt_list_app_catalog":
                return _text(await vt_get("/api/apps/catalog"))

            case "vt_get_app_status":
                return _text(await vt_get(f"/api/apps/{arguments['name']}/status"))

            case "vt_get_app_logs":
                tail = arguments.get("tail", 100)
                data = await vt_get(f"/api/apps/{arguments['name']}/logs", params={"tail": tail})
                lines = data.get("lines", []) if isinstance(data, dict) else data
                return [types.TextContent(type="text", text="\n".join(lines) if isinstance(lines, list) else str(lines))]

            case "vt_control_app":
                name = arguments["name"]
                action = arguments["action"]
                return _text(await vt_post(f"/api/apps/{name}/{action}"))

            case "vt_get_app_compose":
                return _text(await vt_get(f"/api/apps/{arguments['name']}/compose"))

            case "vt_update_app_compose":
                return _text(await vt_post(f"/api/apps/{arguments['name']}/compose", {"content": arguments["content"]}))

            case "vt_remove_app":
                r = await client().delete(f"/api/apps/{arguments['name']}")
                r.raise_for_status()
                return _text({"removed": arguments["name"]})

            case "vt_list_vms":
                return _text(await vt_get("/api/vms/proxmox/vms"))

            case "vt_control_vm":
                return _text(await vt_post("/api/vms/proxmox/action", {
                    "vmid": arguments["vmid"],
                    "kind": arguments["kind"],
                    "node": arguments["node"],
                    "action": arguments["action"],
                }))

            case "vt_list_status_checks":
                return _text(await vt_get("/api/status-checks"))

            case "vt_list_tags":
                return _text(await vt_get("/api/tags"))

            case _:
                return _text({"error": f"Unknown tool: {name}"})

    except httpx.HTTPStatusError as e:
        return _text({"error": f"HTTP {e.response.status_code}", "detail": e.response.text})
    except Exception as e:
        return _text({"error": str(e)})


# ─── Entry point ─────────────────────────────────────────────────────────────

async def main():
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
