#!/usr/bin/env python3
"""
Uptime Kuma MCP Server — manage monitors and status pages on an Uptime Kuma instance.

Setup:
  pip install mcp httpx
  UPTIME_KUMA_URL=http://localhost:3001 UPTIME_KUMA_USERNAME=admin UPTIME_KUMA_PASSWORD=<pw> python uptime_kuma_server.py

Requires Uptime Kuma v2.0.0+ with REST API enabled.
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

UPTIME_KUMA_URL = os.environ.get("UPTIME_KUMA_URL", "http://localhost:3001").rstrip("/")
UPTIME_KUMA_USERNAME = os.environ.get("UPTIME_KUMA_USERNAME", "admin")
UPTIME_KUMA_PASSWORD = os.environ.get("UPTIME_KUMA_PASSWORD", "")

server = Server("uptime-kuma")
_client: httpx.AsyncClient | None = None
_token: str = ""


async def _authenticate() -> str:
    global _token
    async with httpx.AsyncClient(base_url=UPTIME_KUMA_URL, timeout=30) as c:
        r = await c.post("/api/v2/login", json={
            "username": UPTIME_KUMA_USERNAME,
            "password": UPTIME_KUMA_PASSWORD,
        })
        r.raise_for_status()
        _token = r.json()["token"]
    return _token


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=UPTIME_KUMA_URL, timeout=30)
    return _client


async def uk_get(path: str, params: dict | None = None) -> Any:
    global _token
    if not _token:
        await _authenticate()
    r = await client().get(path, params=params, headers={"Authorization": f"Bearer {_token}"})
    if r.status_code == 401:
        await _authenticate()
        r = await client().get(path, params=params, headers={"Authorization": f"Bearer {_token}"})
    r.raise_for_status()
    return r.json()


async def uk_post(path: str, body: dict | None = None) -> Any:
    global _token
    if not _token:
        await _authenticate()
    r = await client().post(path, json=body or {}, headers={"Authorization": f"Bearer {_token}"})
    if r.status_code == 401:
        await _authenticate()
        r = await client().post(path, json=body or {}, headers={"Authorization": f"Bearer {_token}"})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def uk_delete(path: str) -> Any:
    global _token
    if not _token:
        await _authenticate()
    r = await client().delete(path, headers={"Authorization": f"Bearer {_token}"})
    if r.status_code == 401:
        await _authenticate()
        r = await client().delete(path, headers={"Authorization": f"Bearer {_token}"})
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="uptime_kuma_get_monitors",
            description="List all Uptime Kuma monitors with their current up/down status, uptime percentage, and response time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="uptime_kuma_get_monitor",
            description="Get details for a specific monitor including history and configuration",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Monitor ID from uptime_kuma_get_monitors"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="uptime_kuma_add_monitor",
            description="Create a new monitor (HTTP, TCP port, ping, DNS, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "type": {"type": "string", "enum": ["http", "tcp", "ping", "dns", "keyword"], "description": "Monitor type"},
                    "name": {"type": "string", "description": "Display name"},
                    "url": {"type": "string", "description": "URL for http/keyword monitors"},
                    "hostname": {"type": "string", "description": "Hostname for tcp/ping monitors"},
                    "port": {"type": "integer", "description": "Port for tcp monitors"},
                    "interval": {"type": "integer", "default": 60, "description": "Check interval in seconds"},
                },
                "required": ["type", "name"],
            },
        ),
        types.Tool(
            name="uptime_kuma_delete_monitor",
            description="Permanently delete a monitor",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Monitor ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="uptime_kuma_pause_monitor",
            description="Pause monitoring (stop sending checks) for a specific monitor",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="uptime_kuma_resume_monitor",
            description="Resume a paused monitor",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="uptime_kuma_get_heartbeats",
            description="Get recent up/down heartbeat events for a monitor over the last 24 hours",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Monitor ID"},
                    "hours": {"type": "integer", "default": 24},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="uptime_kuma_get_status_pages",
            description="List all public status pages with their slugs and monitored services",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="uptime_kuma_list_notifications",
            description="List all configured notification channels (email, Slack, Telegram, etc.)",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "uptime_kuma_get_monitors":
                return _text(await uk_get("/api/v2/monitor"))

            case "uptime_kuma_get_monitor":
                return _text(await uk_get(f"/api/v2/monitor/{arguments['id']}"))

            case "uptime_kuma_add_monitor":
                body: dict = {
                    "type": arguments["type"],
                    "name": arguments["name"],
                    "interval": arguments.get("interval", 60),
                }
                if url := arguments.get("url"):
                    body["url"] = url
                if hostname := arguments.get("hostname"):
                    body["hostname"] = hostname
                if port := arguments.get("port"):
                    body["port"] = port
                return _text(await uk_post("/api/v2/monitor", body))

            case "uptime_kuma_delete_monitor":
                return _text(await uk_delete(f"/api/v2/monitor/{arguments['id']}"))

            case "uptime_kuma_pause_monitor":
                return _text(await uk_post(f"/api/v2/monitor/{arguments['id']}/pause"))

            case "uptime_kuma_resume_monitor":
                return _text(await uk_post(f"/api/v2/monitor/{arguments['id']}/resume"))

            case "uptime_kuma_get_heartbeats":
                hours = arguments.get("hours", 24)
                return _text(await uk_get(f"/api/v2/monitor/{arguments['id']}/beats", params={"hours": hours}))

            case "uptime_kuma_get_status_pages":
                return _text(await uk_get("/api/v2/status-page"))

            case "uptime_kuma_list_notifications":
                return _text(await uk_get("/api/v2/notification"))

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
