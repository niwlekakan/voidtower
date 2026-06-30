#!/usr/bin/env python3
"""
Tailscale MCP Server — manage devices, users, ACLs, and DNS in a Tailscale tailnet.

Setup:
  pip install mcp httpx
  TAILSCALE_API_KEY=tskey-api-... TAILSCALE_TAILNET=example.com python tailscale_server.py

Get an API key: Tailscale admin console → Settings → Keys → Generate access token
TAILSCALE_TAILNET: your tailnet name (e.g. example.com) or "-" to use the default.
Note: requires internet access to reach api.tailscale.com.
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

TAILSCALE_API_KEY = os.environ.get("TAILSCALE_API_KEY", "")
TAILSCALE_TAILNET = os.environ.get("TAILSCALE_TAILNET", "-")
TAILSCALE_BASE = "https://api.tailscale.com"

server = Server("tailscale")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=TAILSCALE_BASE,
            headers={"Authorization": f"Bearer {TAILSCALE_API_KEY}"},
            timeout=30,
        )
    return _client


async def ts_get(path: str) -> Any:
    r = await client().get(path)
    r.raise_for_status()
    return r.json()


async def ts_post(path: str, body: dict) -> Any:
    r = await client().post(path, json=body)
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def ts_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    tn = TAILSCALE_TAILNET
    return [
        types.Tool(
            name="tailscale_list_devices",
            description="List all devices in the tailnet with their IP addresses, OS, and last-seen time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="tailscale_get_device",
            description="Get full details for a specific device by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device ID (from tailscale_list_devices)"},
                },
                "required": ["device_id"],
            },
        ),
        types.Tool(
            name="tailscale_delete_device",
            description="Remove a device from the tailnet (revokes its access)",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device ID"},
                },
                "required": ["device_id"],
            },
        ),
        types.Tool(
            name="tailscale_authorize_device",
            description="Authorize a device that is pending approval (when device authorization is required)",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string"},
                },
                "required": ["device_id"],
            },
        ),
        types.Tool(
            name="tailscale_set_device_tags",
            description="Set ACL tags on a device (e.g. tag:server, tag:production)",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string"},
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tag list (e.g. [\"tag:server\", \"tag:prod\"])",
                    },
                },
                "required": ["device_id", "tags"],
            },
        ),
        types.Tool(
            name="tailscale_list_users",
            description="List all users in the tailnet",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="tailscale_get_acl",
            description="Get the current ACL policy (JSON) for the tailnet",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="tailscale_list_dns_nameservers",
            description="List configured DNS nameservers for the tailnet",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="tailscale_get_dns_preferences",
            description="Get DNS preferences: MagicDNS enabled state and search domains",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="tailscale_list_keys",
            description="List auth keys for the tailnet (reusable, ephemeral, pre-auth keys)",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    tn = TAILSCALE_TAILNET
    try:
        match name:
            case "tailscale_list_devices":
                return _text(await ts_get(f"/api/v2/tailnet/{tn}/devices"))

            case "tailscale_get_device":
                return _text(await ts_get(f"/api/v2/device/{arguments['device_id']}"))

            case "tailscale_delete_device":
                return _text(await ts_delete(f"/api/v2/device/{arguments['device_id']}"))

            case "tailscale_authorize_device":
                return _text(await ts_post(
                    f"/api/v2/device/{arguments['device_id']}/authorized",
                    {"authorized": True},
                ))

            case "tailscale_set_device_tags":
                return _text(await ts_post(
                    f"/api/v2/device/{arguments['device_id']}/tags",
                    {"tags": arguments["tags"]},
                ))

            case "tailscale_list_users":
                return _text(await ts_get(f"/api/v2/tailnet/{tn}/users"))

            case "tailscale_get_acl":
                return _text(await ts_get(f"/api/v2/tailnet/{tn}/acl"))

            case "tailscale_list_dns_nameservers":
                return _text(await ts_get(f"/api/v2/tailnet/{tn}/dns/nameservers"))

            case "tailscale_get_dns_preferences":
                return _text(await ts_get(f"/api/v2/tailnet/{tn}/dns/preferences"))

            case "tailscale_list_keys":
                return _text(await ts_get(f"/api/v2/tailnet/{tn}/keys"))

            case _:
                return _text({"error": f"Unknown tool: {name}"})

    except httpx.HTTPStatusError as e:
        return _text({"error": f"HTTP {e.response.status_code}", "detail": e.response.text})
    except Exception as e:
        return _text({"error": str(e)})


async def main():
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
