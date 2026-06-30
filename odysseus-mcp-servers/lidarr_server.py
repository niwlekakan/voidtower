#!/usr/bin/env python3
"""
Lidarr MCP Server — manage music artists, albums, and downloads in Lidarr.

Setup:
  pip install mcp httpx
  LIDARR_URL=http://localhost:8686 LIDARR_API_KEY=<key> python lidarr_server.py

Get an API key: Lidarr → Settings → General → Security → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

LIDARR_URL = os.environ.get("LIDARR_URL", "http://localhost:8686").rstrip("/")
LIDARR_API_KEY = os.environ.get("LIDARR_API_KEY", "")

server = Server("lidarr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{LIDARR_URL}/api/v1",
            headers={"X-Api-Key": LIDARR_API_KEY},
            timeout=30,
        )
    return _client


async def lr_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def lr_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="lidarr_list_artists",
            description="List all artists in Lidarr with their monitored status and album counts",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="lidarr_get_artist",
            description="Get detailed info for a specific artist by their Lidarr ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Lidarr artist ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="lidarr_search_artists",
            description="Search for an artist by name to find their MusicBrainz ID before adding",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Artist name to search"},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="lidarr_add_artist",
            description="Add an artist to Lidarr for monitoring. Use lidarr_search_artists first to get foreignArtistId (MusicBrainz ID), lidarr_get_quality_profiles for qualityProfileId, and lidarr_get_root_folders for rootFolderPath.",
            inputSchema={
                "type": "object",
                "properties": {
                    "foreign_artist_id": {"type": "string", "description": "MusicBrainz artist ID from lidarr_search_artists"},
                    "artist_name": {"type": "string", "description": "Artist name"},
                    "quality_profile_id": {"type": "integer", "description": "Quality profile ID"},
                    "metadata_profile_id": {"type": "integer", "description": "Metadata profile ID (use 1 if unsure)"},
                    "root_folder_path": {"type": "string", "description": "Root folder path"},
                    "monitored": {"type": "boolean", "default": True},
                },
                "required": ["foreign_artist_id", "artist_name", "quality_profile_id", "metadata_profile_id", "root_folder_path"],
            },
        ),
        types.Tool(
            name="lidarr_list_albums",
            description="List all albums for a specific artist",
            inputSchema={
                "type": "object",
                "properties": {
                    "artist_id": {"type": "integer", "description": "Lidarr artist ID"},
                },
                "required": ["artist_id"],
            },
        ),
        types.Tool(
            name="lidarr_get_wanted",
            description="List monitored albums/tracks that are missing and have not been downloaded",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="lidarr_get_queue",
            description="Get the current download queue",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="lidarr_get_quality_profiles",
            description="List available quality profiles (needed when adding an artist)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="lidarr_get_root_folders",
            description="List configured root folders (needed when adding an artist)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="lidarr_get_system_status",
            description="Get Lidarr version, OS, and startup info",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="lidarr_trigger_search",
            description="Trigger an automatic search for all albums by an artist",
            inputSchema={
                "type": "object",
                "properties": {
                    "artist_id": {"type": "integer", "description": "Lidarr artist ID"},
                },
                "required": ["artist_id"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "lidarr_list_artists":
                return _text(await lr_get("/artist"))

            case "lidarr_get_artist":
                return _text(await lr_get(f"/artist/{arguments['id']}"))

            case "lidarr_search_artists":
                return _text(await lr_get("/artist/lookup", params={"term": arguments["query"]}))

            case "lidarr_add_artist":
                return _text(await lr_post("/artist", {
                    "foreignArtistId": arguments["foreign_artist_id"],
                    "artistName": arguments["artist_name"],
                    "qualityProfileId": arguments["quality_profile_id"],
                    "metadataProfileId": arguments["metadata_profile_id"],
                    "rootFolderPath": arguments["root_folder_path"],
                    "monitored": arguments.get("monitored", True),
                    "addOptions": {"searchForMissingAlbums": True},
                }))

            case "lidarr_list_albums":
                return _text(await lr_get("/album", params={"artistId": arguments["artist_id"]}))

            case "lidarr_get_wanted":
                return _text(await lr_get("/wanted/missing", params={
                    "sortKey": "releaseDate",
                    "sortDir": "desc",
                    "pageSize": arguments.get("page_size", 30),
                }))

            case "lidarr_get_queue":
                return _text(await lr_get("/queue"))

            case "lidarr_get_quality_profiles":
                return _text(await lr_get("/qualityprofile"))

            case "lidarr_get_root_folders":
                return _text(await lr_get("/rootfolder"))

            case "lidarr_get_system_status":
                return _text(await lr_get("/system/status"))

            case "lidarr_trigger_search":
                return _text(await lr_post("/command", {
                    "name": "ArtistSearch",
                    "artistId": arguments["artist_id"],
                }))

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
