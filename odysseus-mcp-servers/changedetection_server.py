#!/usr/bin/env python3
"""
changedetection.io MCP Server — manage website change monitors.

Setup:
  pip install mcp httpx
  CHANGEDETECTION_URL=http://localhost:5000 CHANGEDETECTION_API_KEY=<key> python changedetection_server.py

Get API key: changedetection.io → Settings → API
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

CHANGEDETECTION_URL = os.environ.get("CHANGEDETECTION_URL", "http://localhost:5000").rstrip("/")
CHANGEDETECTION_API_KEY = os.environ.get("CHANGEDETECTION_API_KEY", "")

server = Server("changedetection")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=CHANGEDETECTION_URL,
            headers={"x-api-key": CHANGEDETECTION_API_KEY},
            timeout=30,
        )
    return _client


async def cd_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def cd_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def cd_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="changedetection_list_watches",
            description="List all watched URLs with their title, last checked time, last changed time, and error state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="changedetection_get_watch",
            description="Get full details for a specific watch including its configuration and history count",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "Watch UUID from changedetection_list_watches"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="changedetection_add_watch",
            description="Add a new URL to monitor for changes",
            inputSchema={
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to monitor"},
                    "title": {"type": "string", "description": "Optional display name"},
                },
                "required": ["url"],
            },
        ),
        types.Tool(
            name="changedetection_delete_watch",
            description="Remove a URL watch permanently",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "Watch UUID"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="changedetection_trigger_check",
            description="Trigger an immediate check for a watched URL instead of waiting for the next scheduled check",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "Watch UUID"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="changedetection_get_history",
            description="Get the list of change timestamps for a watch (each entry is a detected change event)",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "Watch UUID"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="changedetection_get_latest_diff",
            description="Get the latest detected change snapshot/diff for a watch",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "Watch UUID"},
                },
                "required": ["uuid"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "changedetection_list_watches":
                return _text(await cd_get("/api/v1/watch"))

            case "changedetection_get_watch":
                return _text(await cd_get(f"/api/v1/watch/{arguments['uuid']}"))

            case "changedetection_add_watch":
                body: dict = {"url": arguments["url"]}
                if title := arguments.get("title"):
                    body["title"] = title
                return _text(await cd_post("/api/v1/watch", body))

            case "changedetection_delete_watch":
                return _text(await cd_delete(f"/api/v1/watch/{arguments['uuid']}"))

            case "changedetection_trigger_check":
                return _text(await cd_get(f"/api/v1/watch/{arguments['uuid']}/recheck"))

            case "changedetection_get_history":
                return _text(await cd_get(f"/api/v1/watch/{arguments['uuid']}/history"))

            case "changedetection_get_latest_diff":
                return _text(await cd_get(f"/api/v1/watch/{arguments['uuid']}/history/latest"))

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
