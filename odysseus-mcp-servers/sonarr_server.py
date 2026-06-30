#!/usr/bin/env python3
"""
Sonarr MCP Server — manage TV shows, episodes, and download queue in Sonarr.

Setup:
  pip install mcp httpx
  SONARR_URL=http://localhost:8989 SONARR_API_KEY=<key> python sonarr_server.py

Get an API key: Sonarr → Settings → General → Security → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

SONARR_URL = os.environ.get("SONARR_URL", "http://localhost:8989").rstrip("/")
SONARR_API_KEY = os.environ.get("SONARR_API_KEY", "")

server = Server("sonarr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{SONARR_URL}/api/v3",
            headers={"X-Api-Key": SONARR_API_KEY},
            timeout=30,
        )
    return _client


async def sr_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def sr_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def sr_delete(path: str, params: dict | None = None) -> Any:
    r = await client().delete(path, params=params)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="sonarr_list_series",
            description="List all TV series in Sonarr with their monitored status, episode stats, and path",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="sonarr_get_series",
            description="Get detailed info for a specific series by its Sonarr ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Sonarr series ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="sonarr_search_series",
            description="Search for a TV series by name to find its TVDB ID before adding",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Series name to search"},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="sonarr_add_series",
            description="Add a TV series to Sonarr for monitoring and downloading. Use sonarr_search_series first to get tvdbId, sonarr_get_quality_profiles for qualityProfileId, and sonarr_get_root_folders for rootFolderPath.",
            inputSchema={
                "type": "object",
                "properties": {
                    "tvdb_id": {"type": "integer", "description": "TVDB series ID from sonarr_search_series"},
                    "title": {"type": "string", "description": "Series title"},
                    "quality_profile_id": {"type": "integer", "description": "Quality profile ID from sonarr_get_quality_profiles"},
                    "root_folder_path": {"type": "string", "description": "Root folder path from sonarr_get_root_folders"},
                    "monitored": {"type": "boolean", "default": True},
                    "season_folder": {"type": "boolean", "default": True},
                },
                "required": ["tvdb_id", "title", "quality_profile_id", "root_folder_path"],
            },
        ),
        types.Tool(
            name="sonarr_delete_series",
            description="Remove a series from Sonarr",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Sonarr series ID"},
                    "delete_files": {"type": "boolean", "default": False, "description": "Also delete files from disk"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="sonarr_list_episodes",
            description="List all episodes for a series, showing which are monitored and have files",
            inputSchema={
                "type": "object",
                "properties": {
                    "series_id": {"type": "integer", "description": "Sonarr series ID"},
                },
                "required": ["series_id"],
            },
        ),
        types.Tool(
            name="sonarr_get_queue",
            description="Get the current download queue — what's downloading, pending, or failed",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="sonarr_get_wanted",
            description="List monitored episodes that are missing and have not been downloaded",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="sonarr_trigger_search",
            description="Trigger an automatic search for all episodes of a series",
            inputSchema={
                "type": "object",
                "properties": {
                    "series_id": {"type": "integer", "description": "Sonarr series ID"},
                },
                "required": ["series_id"],
            },
        ),
        types.Tool(
            name="sonarr_get_quality_profiles",
            description="List available quality profiles (needed when adding a series)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="sonarr_get_root_folders",
            description="List configured root folders (needed when adding a series)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="sonarr_get_history",
            description="Get recent download history — grabbed, imported, failed events",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="sonarr_get_system_status",
            description="Get Sonarr version, OS, database info, and startup time",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "sonarr_list_series":
                return _text(await sr_get("/series"))

            case "sonarr_get_series":
                return _text(await sr_get(f"/series/{arguments['id']}"))

            case "sonarr_search_series":
                return _text(await sr_get("/series/lookup", params={"term": arguments["query"]}))

            case "sonarr_add_series":
                return _text(await sr_post("/series", {
                    "tvdbId": arguments["tvdb_id"],
                    "title": arguments["title"],
                    "qualityProfileId": arguments["quality_profile_id"],
                    "rootFolderPath": arguments["root_folder_path"],
                    "monitored": arguments.get("monitored", True),
                    "seasonFolder": arguments.get("season_folder", True),
                    "addOptions": {"searchForMissingEpisodes": True},
                }))

            case "sonarr_delete_series":
                return _text(await sr_delete(
                    f"/series/{arguments['id']}",
                    params={"deleteFiles": str(arguments.get("delete_files", False)).lower()},
                ))

            case "sonarr_list_episodes":
                return _text(await sr_get("/episode", params={"seriesId": arguments["series_id"]}))

            case "sonarr_get_queue":
                return _text(await sr_get("/queue", params={"includeUnknownSeriesItems": "false"}))

            case "sonarr_get_wanted":
                return _text(await sr_get("/wanted/missing", params={
                    "sortKey": "airDateUtc",
                    "sortDir": "desc",
                    "pageSize": arguments.get("page_size", 30),
                }))

            case "sonarr_trigger_search":
                return _text(await sr_post("/command", {
                    "name": "SeriesSearch",
                    "seriesId": arguments["series_id"],
                }))

            case "sonarr_get_quality_profiles":
                return _text(await sr_get("/qualityprofile"))

            case "sonarr_get_root_folders":
                return _text(await sr_get("/rootfolder"))

            case "sonarr_get_history":
                return _text(await sr_get("/history", params={"pageSize": arguments.get("page_size", 30)}))

            case "sonarr_get_system_status":
                return _text(await sr_get("/system/status"))

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
