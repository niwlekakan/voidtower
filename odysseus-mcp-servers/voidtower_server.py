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
            name="vt_run_automation_job",
            description="Trigger an automation job immediately",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Automation job ID"},
                },
                "required": ["id"],
            },
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

            case "vt_run_automation_job":
                return _text(await vt_post(f"/api/automation/{arguments['id']}/run"))

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
