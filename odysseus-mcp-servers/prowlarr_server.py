#!/usr/bin/env python3
"""
Prowlarr MCP Server — manage indexers and search across all configured indexers.

Setup:
  pip install mcp httpx
  PROWLARR_URL=http://localhost:9696 PROWLARR_API_KEY=<key> python prowlarr_server.py

Get an API key: Prowlarr → Settings → General → Security → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

PROWLARR_URL = os.environ.get("PROWLARR_URL", "http://localhost:9696").rstrip("/")
PROWLARR_API_KEY = os.environ.get("PROWLARR_API_KEY", "")

server = Server("prowlarr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{PROWLARR_URL}/api/v1",
            headers={"X-Api-Key": PROWLARR_API_KEY},
            timeout=60,
        )
    return _client


async def pw_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def pw_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def pw_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="prowlarr_list_indexers",
            description="List all configured indexers with their enabled status, priority, and capabilities",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="prowlarr_get_indexer",
            description="Get detailed config for a specific indexer by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Prowlarr indexer ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="prowlarr_delete_indexer",
            description="Remove an indexer from Prowlarr",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Prowlarr indexer ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="prowlarr_search",
            description="Search across all enabled indexers for a release. Returns torrent/nzb results.",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search term"},
                    "categories": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "Newznab category IDs to filter (e.g. [2000] for Movies, [5000] for TV). Omit for all.",
                    },
                    "limit": {"type": "integer", "default": 30},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="prowlarr_list_apps",
            description="List connected downstream applications (Sonarr, Radarr, Lidarr, Readarr) synced via Prowlarr",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="prowlarr_sync_indexers",
            description="Push all indexer configs to connected applications (Sonarr, Radarr, etc.)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="prowlarr_get_history",
            description="Get recent search and grab history across all indexers",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="prowlarr_get_system_status",
            description="Get Prowlarr version, OS, and startup info",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="prowlarr_get_tags",
            description="List all tags defined in Prowlarr",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "prowlarr_list_indexers":
                return _text(await pw_get("/indexer"))

            case "prowlarr_get_indexer":
                return _text(await pw_get(f"/indexer/{arguments['id']}"))

            case "prowlarr_delete_indexer":
                return _text(await pw_delete(f"/indexer/{arguments['id']}"))

            case "prowlarr_search":
                params: dict = {
                    "query": arguments["query"],
                    "indexerIds": [-2],
                    "type": "search",
                    "limit": arguments.get("limit", 30),
                }
                if cats := arguments.get("categories"):
                    params["categories"] = cats
                return _text(await pw_get("/search", params=params))

            case "prowlarr_list_apps":
                return _text(await pw_get("/applications"))

            case "prowlarr_sync_indexers":
                return _text(await pw_post("/command", {"name": "ApplicationIndexerSync"}))

            case "prowlarr_get_history":
                return _text(await pw_get("/history", params={"pageSize": arguments.get("page_size", 30)}))

            case "prowlarr_get_system_status":
                return _text(await pw_get("/system/status"))

            case "prowlarr_get_tags":
                return _text(await pw_get("/tag"))

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
