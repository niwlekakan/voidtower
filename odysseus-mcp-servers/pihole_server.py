#!/usr/bin/env python3
"""
Pi-hole MCP Server — manage and query a Pi-hole v6 DNS ad blocker.

Setup:
  pip install mcp httpx
  PIHOLE_URL=http://localhost:80 PIHOLE_PASSWORD=<password> python pihole_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

PIHOLE_URL = os.environ.get("PIHOLE_URL", "http://localhost:80").rstrip("/")
PIHOLE_PASSWORD = os.environ.get("PIHOLE_PASSWORD", "")

server = Server("pihole")
_client: httpx.AsyncClient | None = None
_sid: str = ""


async def _authenticate() -> str:
    global _sid
    async with httpx.AsyncClient(base_url=PIHOLE_URL, timeout=30) as c:
        r = await c.post("/api/auth", json={"password": PIHOLE_PASSWORD})
        r.raise_for_status()
        _sid = r.json()["session"]["sid"]
    return _sid


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=PIHOLE_URL, timeout=30)
    return _client


async def ph_get(path: str, params: dict | None = None) -> Any:
    global _sid
    if not _sid:
        await _authenticate()
    r = await client().get(path, params=params, headers={"X-FTL-SID": _sid})
    if r.status_code == 401:
        await _authenticate()
        r = await client().get(path, params=params, headers={"X-FTL-SID": _sid})
    r.raise_for_status()
    return r.json()


async def ph_post(path: str, body: dict | None = None) -> Any:
    global _sid
    if not _sid:
        await _authenticate()
    r = await client().post(path, json=body or {}, headers={"X-FTL-SID": _sid})
    if r.status_code == 401:
        await _authenticate()
        r = await client().post(path, json=body or {}, headers={"X-FTL-SID": _sid})
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
            name="pihole_get_stats",
            description="Get Pi-hole summary stats: total queries today, blocked count, block percentage, unique clients",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="pihole_get_top_domains",
            description="Get the most frequently queried domains",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 20, "description": "Number of domains to return"},
                },
            },
        ),
        types.Tool(
            name="pihole_get_top_blocked",
            description="Get the most frequently blocked domains",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 20},
                },
            },
        ),
        types.Tool(
            name="pihole_list_blocking_status",
            description="Check whether Pi-hole DNS blocking is currently enabled or disabled",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="pihole_enable_blocking",
            description="Enable Pi-hole DNS blocking",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="pihole_disable_blocking",
            description="Temporarily disable Pi-hole DNS blocking for 5 minutes",
            inputSchema={
                "type": "object",
                "properties": {
                    "timer": {"type": "integer", "default": 300, "description": "Seconds to disable blocking (0 = indefinite)"},
                },
            },
        ),
        types.Tool(
            name="pihole_list_clients",
            description="List all clients that have made DNS queries through Pi-hole",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="pihole_get_query_log",
            description="Get recent DNS query log with domain, client, status (blocked/allowed), and reply type",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 100},
                },
            },
        ),
        types.Tool(
            name="pihole_list_groups",
            description="List all Pi-hole client groups with their enabled state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="pihole_list_domains",
            description="List allow/deny list entries with their type and enabled state",
            inputSchema={
                "type": "object",
                "properties": {
                    "type": {"type": "string", "enum": ["allow", "deny"], "description": "Filter by list type (omit for both)"},
                },
            },
        ),
        types.Tool(
            name="pihole_add_domain",
            description="Add a domain to the allowlist or denylist",
            inputSchema={
                "type": "object",
                "properties": {
                    "domain": {"type": "string", "description": "Domain to add (e.g. ads.example.com)"},
                    "type": {"type": "string", "enum": ["allow", "deny"], "default": "deny"},
                    "kind": {"type": "string", "enum": ["exact", "regex"], "default": "exact"},
                    "comment": {"type": "string", "default": ""},
                },
                "required": ["domain"],
            },
        ),
        types.Tool(
            name="pihole_get_ftl_info",
            description="Get Pi-hole FTL (Faster Than Light) DNS resolver status and configuration",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "pihole_get_stats":
                return _text(await ph_get("/api/stats/summary"))

            case "pihole_get_top_domains":
                return _text(await ph_get("/api/stats/top_domains", params={"count": arguments.get("limit", 20)}))

            case "pihole_get_top_blocked":
                return _text(await ph_get("/api/stats/top_blocked", params={"count": arguments.get("limit", 20)}))

            case "pihole_list_blocking_status":
                return _text(await ph_get("/api/dns/blocking"))

            case "pihole_enable_blocking":
                return _text(await ph_post("/api/dns/blocking", {"blocking": True, "timer": None}))

            case "pihole_disable_blocking":
                timer = arguments.get("timer", 300)
                return _text(await ph_post("/api/dns/blocking", {"blocking": False, "timer": timer if timer > 0 else None}))

            case "pihole_list_clients":
                return _text(await ph_get("/api/stats/clients"))

            case "pihole_get_query_log":
                return _text(await ph_get("/api/history", params={"limit": arguments.get("limit", 100)}))

            case "pihole_list_groups":
                return _text(await ph_get("/api/groups"))

            case "pihole_list_domains":
                params = {}
                if t := arguments.get("type"):
                    params["type"] = t
                return _text(await ph_get("/api/domains", params=params or None))

            case "pihole_add_domain":
                dtype = arguments.get("type", "deny")
                kind = arguments.get("kind", "exact")
                return _text(await ph_post(f"/api/domains/{dtype}/{kind}", {
                    "domain": arguments["domain"],
                    "enabled": True,
                    "comment": arguments.get("comment", ""),
                }))

            case "pihole_get_ftl_info":
                return _text(await ph_get("/api/info/ftl"))

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
