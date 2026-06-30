#!/usr/bin/env python3
"""
AdGuard Home MCP Server — manage and query an AdGuard Home DNS filter.

Setup:
  pip install mcp httpx
  ADGUARD_URL=http://localhost:3000 ADGUARD_USERNAME=admin ADGUARD_PASSWORD=<pw> python adguardhome_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

ADGUARD_URL = os.environ.get("ADGUARD_URL", "http://localhost:3000").rstrip("/")
ADGUARD_USERNAME = os.environ.get("ADGUARD_USERNAME", "admin")
ADGUARD_PASSWORD = os.environ.get("ADGUARD_PASSWORD", "")

server = Server("adguardhome")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=ADGUARD_URL,
            auth=(ADGUARD_USERNAME, ADGUARD_PASSWORD),
            timeout=30,
        )
    return _client


async def ag_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def ag_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="adguard_get_status",
            description="Get AdGuard Home running status, version, DNS addresses, and filtering state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="adguard_get_stats",
            description="Get AdGuard Home DNS stats: total queries, blocked count, safebrowsing hits, top domains/clients",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="adguard_get_query_log",
            description="Get recent DNS query log with domain, client IP, answer, and block reason",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 50},
                },
            },
        ),
        types.Tool(
            name="adguard_list_filtering",
            description="List all enabled filter lists with their URL, name, and rule count",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="adguard_refresh_filters",
            description="Force refresh of all filter lists from their upstream URLs",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="adguard_add_filter",
            description="Add a new filter list by URL",
            inputSchema={
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "Filter list URL"},
                    "name": {"type": "string", "description": "Display name for the filter"},
                    "whitelist": {"type": "boolean", "default": False, "description": "True to add as allowlist"},
                },
                "required": ["url", "name"],
            },
        ),
        types.Tool(
            name="adguard_toggle_filtering",
            description="Enable or disable DNS filtering globally",
            inputSchema={
                "type": "object",
                "properties": {
                    "enabled": {"type": "boolean"},
                    "interval": {"type": "integer", "default": 24, "description": "Filter update interval in hours"},
                },
                "required": ["enabled"],
            },
        ),
        types.Tool(
            name="adguard_list_clients",
            description="List all configured AdGuard Home clients with their settings and assigned groups",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="adguard_add_client",
            description="Add a new AdGuard Home client configuration",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Client display name"},
                    "ids": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Client identifiers: IP addresses, MAC addresses, or CIDRs",
                    },
                    "use_global_settings": {"type": "boolean", "default": True},
                },
                "required": ["name", "ids"],
            },
        ),
        types.Tool(
            name="adguard_list_blocked_services",
            description="List all available blocked service categories (social media, gaming, streaming, etc.)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="adguard_get_dhcp",
            description="Get DHCP server status, current leases, and configuration",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "adguard_get_status":
                return _text(await ag_get("/control/status"))

            case "adguard_get_stats":
                return _text(await ag_get("/control/stats"))

            case "adguard_get_query_log":
                return _text(await ag_get("/control/querylog", params={"limit": arguments.get("limit", 50)}))

            case "adguard_list_filtering":
                return _text(await ag_get("/control/filtering/status"))

            case "adguard_refresh_filters":
                return _text(await ag_post("/control/filtering/refresh", {"whitelist": False}))

            case "adguard_add_filter":
                return _text(await ag_post("/control/filtering/add", {
                    "url": arguments["url"],
                    "name": arguments["name"],
                    "whitelist": arguments.get("whitelist", False),
                }))

            case "adguard_toggle_filtering":
                return _text(await ag_post("/control/filtering/config", {
                    "enabled": arguments["enabled"],
                    "interval": arguments.get("interval", 24),
                }))

            case "adguard_list_clients":
                return _text(await ag_get("/control/clients"))

            case "adguard_add_client":
                return _text(await ag_post("/control/clients/add", {
                    "name": arguments["name"],
                    "ids": arguments["ids"],
                    "use_global_settings": arguments.get("use_global_settings", True),
                    "filtering_enabled": True,
                    "safebrowsing_enabled": False,
                    "parental_enabled": False,
                    "safesearch_enabled": False,
                    "use_global_blocked_services": True,
                    "blocked_services": [],
                    "tags": [],
                }))

            case "adguard_list_blocked_services":
                return _text(await ag_get("/control/blocked_services/all"))

            case "adguard_get_dhcp":
                return _text(await ag_get("/control/dhcp/status"))

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
