#!/usr/bin/env python3
"""
Kavita MCP Server — manage libraries, series, and reading progress on a Kavita instance.

Setup:
  pip install mcp httpx
  KAVITA_URL=http://localhost:5000 KAVITA_API_KEY=<key> python kavita_server.py

Get an API key: Kavita → User Menu → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

KAVITA_URL = os.environ.get("KAVITA_URL", "http://localhost:5000").rstrip("/")
KAVITA_API_KEY = os.environ.get("KAVITA_API_KEY", "")

server = Server("kavita")
_client: httpx.AsyncClient | None = None
_jwt_token: str = ""


async def _ensure_auth() -> None:
    global _client, _jwt_token
    if _jwt_token:
        return
    async with httpx.AsyncClient(base_url=KAVITA_URL, timeout=30) as c:
        r = await c.post(
            "/api/Plugin/authenticate",
            params={"apiKey": KAVITA_API_KEY, "pluginName": "voidtower"},
        )
        r.raise_for_status()
        _jwt_token = r.json().get("token", "")
    _client = None  # reset so client() picks up the token


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=KAVITA_URL,
            headers={"Authorization": f"Bearer {_jwt_token}"},
            timeout=30,
        )
    return _client


async def kv_get(path: str, params: dict | None = None) -> Any:
    await _ensure_auth()
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def kv_post(path: str, params: dict | None = None, body: dict | None = None) -> Any:
    await _ensure_auth()
    r = await client().post(path, params=params, json=body or {})
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
            name="kavita_get_server_info",
            description="Get Kavita server version, OS, and .NET runtime version",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="kavita_list_libraries",
            description="List all Kavita libraries with their type (manga, comics, books) and folder paths",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="kavita_scan_library",
            description="Trigger a file scan on a specific library to pick up new/changed files",
            inputSchema={
                "type": "object",
                "properties": {
                    "libraryId": {"type": "integer", "description": "Library ID from kavita_list_libraries"},
                    "force": {"type": "boolean", "default": False, "description": "Force full rescan even if files appear unchanged"},
                },
                "required": ["libraryId"],
            },
        ),
        types.Tool(
            name="kavita_list_series",
            description="List series in a library with pagination",
            inputSchema={
                "type": "object",
                "properties": {
                    "libraryId": {"type": "integer", "description": "Library ID (omit for all libraries)"},
                    "pageNumber": {"type": "integer", "default": 1},
                    "pageSize": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="kavita_get_series",
            description="Get metadata for a specific series including genre, summary, and volume count",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Series ID from kavita_list_series"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="kavita_search",
            description="Search series, collections, and tags by keyword",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search term"},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="kavita_list_collections",
            description="List all reading collections (curated groups of series)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="kavita_get_reading_progress",
            description="Get reading progress for a specific series",
            inputSchema={
                "type": "object",
                "properties": {
                    "seriesId": {"type": "integer", "description": "Series ID"},
                },
                "required": ["seriesId"],
            },
        ),
        types.Tool(
            name="kavita_list_users",
            description="List all Kavita users (admin only)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="kavita_get_stats",
            description="Get server-wide reading statistics: total read time, pages read, series count",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "kavita_get_server_info":
                return _text(await kv_get("/api/Server/server-info"))

            case "kavita_list_libraries":
                return _text(await kv_get("/api/Library/libraries"))

            case "kavita_scan_library":
                return _text(await kv_post("/api/Library/scan", params={
                    "libraryId": arguments["libraryId"],
                    "force": str(arguments.get("force", False)).lower(),
                }))

            case "kavita_list_series":
                params: dict = {
                    "pageNumber": arguments.get("pageNumber", 1),
                    "pageSize": arguments.get("pageSize", 30),
                }
                if lib := arguments.get("libraryId"):
                    params["libraryId"] = lib
                return _text(await kv_get("/api/Series", params=params))

            case "kavita_get_series":
                return _text(await kv_get(f"/api/Series/{arguments['id']}"))

            case "kavita_search":
                return _text(await kv_get("/api/Search/search", params={"queryString": arguments["query"]}))

            case "kavita_list_collections":
                return _text(await kv_get("/api/Collection"))

            case "kavita_get_reading_progress":
                return _text(await kv_get("/api/Reader/reading-list-item", params={"seriesId": arguments["seriesId"]}))

            case "kavita_list_users":
                return _text(await kv_get("/api/Users/users"))

            case "kavita_get_stats":
                return _text(await kv_get("/api/Statistics/server/stats"))

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
