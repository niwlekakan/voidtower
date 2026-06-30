#!/usr/bin/env python3
"""
Radarr MCP Server — manage movies and download queue in Radarr.

Setup:
  pip install mcp httpx
  RADARR_URL=http://localhost:7878 RADARR_API_KEY=<key> python radarr_server.py

Get an API key: Radarr → Settings → General → Security → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

RADARR_URL = os.environ.get("RADARR_URL", "http://localhost:7878").rstrip("/")
RADARR_API_KEY = os.environ.get("RADARR_API_KEY", "")

server = Server("radarr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{RADARR_URL}/api/v3",
            headers={"X-Api-Key": RADARR_API_KEY},
            timeout=30,
        )
    return _client


async def rr_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def rr_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def rr_delete(path: str, params: dict | None = None) -> Any:
    r = await client().delete(path, params=params)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="radarr_list_movies",
            description="List all movies in Radarr with their monitored status, availability, and file info",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="radarr_get_movie",
            description="Get detailed info for a specific movie by its Radarr ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Radarr movie ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="radarr_search_movies",
            description="Search for a movie by name to find its TMDB ID before adding",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Movie title to search"},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="radarr_add_movie",
            description="Add a movie to Radarr for monitoring and downloading. Use radarr_search_movies first to get tmdbId, radarr_get_quality_profiles for qualityProfileId, and radarr_get_root_folders for rootFolderPath.",
            inputSchema={
                "type": "object",
                "properties": {
                    "tmdb_id": {"type": "integer", "description": "TMDB movie ID from radarr_search_movies"},
                    "title": {"type": "string", "description": "Movie title"},
                    "quality_profile_id": {"type": "integer", "description": "Quality profile ID from radarr_get_quality_profiles"},
                    "root_folder_path": {"type": "string", "description": "Root folder path from radarr_get_root_folders"},
                    "monitored": {"type": "boolean", "default": True},
                    "minimum_availability": {
                        "type": "string",
                        "enum": ["announced", "inCinemas", "released", "preDB"],
                        "default": "announced",
                    },
                },
                "required": ["tmdb_id", "title", "quality_profile_id", "root_folder_path"],
            },
        ),
        types.Tool(
            name="radarr_delete_movie",
            description="Remove a movie from Radarr",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Radarr movie ID"},
                    "delete_files": {"type": "boolean", "default": False, "description": "Also delete files from disk"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="radarr_get_queue",
            description="Get the current download queue — what's downloading, pending, or failed",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="radarr_get_wanted",
            description="List monitored movies that are missing and have not been downloaded",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="radarr_trigger_search",
            description="Trigger an automatic search for one or more movies",
            inputSchema={
                "type": "object",
                "properties": {
                    "movie_ids": {
                        "type": "array",
                        "items": {"type": "integer"},
                        "description": "List of Radarr movie IDs to search for",
                    },
                },
                "required": ["movie_ids"],
            },
        ),
        types.Tool(
            name="radarr_get_quality_profiles",
            description="List available quality profiles (needed when adding a movie)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="radarr_get_root_folders",
            description="List configured root folders (needed when adding a movie)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="radarr_get_history",
            description="Get recent download history — grabbed, imported, failed events",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="radarr_get_system_status",
            description="Get Radarr version, OS, database info, and startup time",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "radarr_list_movies":
                return _text(await rr_get("/movie"))

            case "radarr_get_movie":
                return _text(await rr_get(f"/movie/{arguments['id']}"))

            case "radarr_search_movies":
                return _text(await rr_get("/movie/lookup", params={"term": arguments["query"]}))

            case "radarr_add_movie":
                return _text(await rr_post("/movie", {
                    "tmdbId": arguments["tmdb_id"],
                    "title": arguments["title"],
                    "qualityProfileId": arguments["quality_profile_id"],
                    "rootFolderPath": arguments["root_folder_path"],
                    "monitored": arguments.get("monitored", True),
                    "minimumAvailability": arguments.get("minimum_availability", "announced"),
                    "addOptions": {"searchForMovie": True},
                }))

            case "radarr_delete_movie":
                return _text(await rr_delete(
                    f"/movie/{arguments['id']}",
                    params={"deleteFiles": str(arguments.get("delete_files", False)).lower()},
                ))

            case "radarr_get_queue":
                return _text(await rr_get("/queue"))

            case "radarr_get_wanted":
                return _text(await rr_get("/wanted/missing", params={
                    "sortKey": "inCinemas",
                    "sortDir": "desc",
                    "pageSize": arguments.get("page_size", 30),
                }))

            case "radarr_trigger_search":
                return _text(await rr_post("/command", {
                    "name": "MoviesSearch",
                    "movieIds": arguments["movie_ids"],
                }))

            case "radarr_get_quality_profiles":
                return _text(await rr_get("/qualityprofile"))

            case "radarr_get_root_folders":
                return _text(await rr_get("/rootfolder"))

            case "radarr_get_history":
                return _text(await rr_get("/history", params={"pageSize": arguments.get("page_size", 30)}))

            case "radarr_get_system_status":
                return _text(await rr_get("/system/status"))

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
