#!/usr/bin/env python3
"""
Navidrome MCP Server — browse and manage a Navidrome music server via OpenSubsonic API.

Setup:
  pip install mcp httpx
  NAVIDROME_URL=http://localhost:4533 NAVIDROME_USER=admin NAVIDROME_PASSWORD=<pw> python navidrome_server.py
"""

import asyncio
import hashlib
import os
import json
import secrets
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

NAVIDROME_URL = os.environ.get("NAVIDROME_URL", "http://localhost:4533").rstrip("/")
NAVIDROME_USER = os.environ.get("NAVIDROME_USER", "")
NAVIDROME_PASSWORD = os.environ.get("NAVIDROME_PASSWORD", "")

server = Server("navidrome")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=NAVIDROME_URL, timeout=30)
    return _client


def _auth_params() -> dict:
    salt = secrets.token_hex(3)  # 6 chars
    token = hashlib.md5(f"{NAVIDROME_PASSWORD}{salt}".encode()).hexdigest()
    return {"u": NAVIDROME_USER, "t": token, "s": salt, "v": "1.16.1", "c": "voidtower", "f": "json"}


async def nd_get(endpoint: str, extra: dict | None = None) -> Any:
    params = {**_auth_params(), **(extra or {})}
    r = await client().get(f"/rest/{endpoint}", params=params)
    r.raise_for_status()
    data = r.json()
    # OpenSubsonic wraps everything in "subsonic-response"
    resp = data.get("subsonic-response", data)
    if resp.get("status") == "failed":
        raise RuntimeError(resp.get("error", {}).get("message", "Unknown error"))
    return resp


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="navidrome_get_status",
            description="Ping the Navidrome server to verify connectivity and get server version",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="navidrome_get_artists",
            description="Get all artists grouped by first letter index",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="navidrome_get_artist",
            description="Get a specific artist with their full album list",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Artist ID from navidrome_get_artists"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="navidrome_get_album",
            description="Get a specific album with its full track list",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Album ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="navidrome_get_albums",
            description="Get a list of albums sorted by type: newest, recent, frequent, highest, starred, alphabeticalByName, or random",
            inputSchema={
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["newest", "recent", "frequent", "highest", "starred", "alphabeticalByName", "random"],
                        "default": "newest",
                    },
                    "size": {"type": "integer", "default": 20, "description": "Number of albums (max 500)"},
                    "offset": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="navidrome_search",
            description="Search for artists, albums, and songs by keyword",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "artistCount": {"type": "integer", "default": 5},
                    "albumCount": {"type": "integer", "default": 10},
                    "songCount": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="navidrome_get_playlists",
            description="List all playlists accessible to the current user",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="navidrome_get_playlist",
            description="Get a specific playlist with its full track list",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Playlist ID from navidrome_get_playlists"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="navidrome_create_playlist",
            description="Create a new empty playlist",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Playlist name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="navidrome_get_starred",
            description="Get all starred/favorited artists, albums, and songs for the current user",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="navidrome_get_now_playing",
            description="Get currently active playback streams across all users",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="navidrome_get_song",
            description="Get details for a specific song/track by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Song ID"},
                },
                "required": ["id"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "navidrome_get_status":
                return _text(await nd_get("ping"))

            case "navidrome_get_artists":
                return _text(await nd_get("getArtists"))

            case "navidrome_get_artist":
                return _text(await nd_get("getArtist", {"id": arguments["id"]}))

            case "navidrome_get_album":
                return _text(await nd_get("getAlbum", {"id": arguments["id"]}))

            case "navidrome_get_albums":
                return _text(await nd_get("getAlbumList2", {
                    "type": arguments.get("type", "newest"),
                    "size": arguments.get("size", 20),
                    "offset": arguments.get("offset", 0),
                }))

            case "navidrome_search":
                return _text(await nd_get("search3", {
                    "query": arguments["query"],
                    "artistCount": arguments.get("artistCount", 5),
                    "albumCount": arguments.get("albumCount", 10),
                    "songCount": arguments.get("songCount", 20),
                }))

            case "navidrome_get_playlists":
                return _text(await nd_get("getPlaylists"))

            case "navidrome_get_playlist":
                return _text(await nd_get("getPlaylist", {"id": arguments["id"]}))

            case "navidrome_create_playlist":
                return _text(await nd_get("createPlaylist", {"name": arguments["name"]}))

            case "navidrome_get_starred":
                return _text(await nd_get("getStarred2"))

            case "navidrome_get_now_playing":
                return _text(await nd_get("getNowPlaying"))

            case "navidrome_get_song":
                return _text(await nd_get("getSong", {"id": arguments["id"]}))

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
