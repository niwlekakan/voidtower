#!/usr/bin/env python3
"""
Bazarr MCP Server — manage subtitles for movies and TV shows in Bazarr.

Setup:
  pip install mcp httpx
  BAZARR_URL=http://localhost:6767 BAZARR_API_KEY=<key> python bazarr_server.py

Get an API key: Bazarr → Settings → General → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

BAZARR_URL = os.environ.get("BAZARR_URL", "http://localhost:6767").rstrip("/")
BAZARR_API_KEY = os.environ.get("BAZARR_API_KEY", "")

server = Server("bazarr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{BAZARR_URL}/api",
            headers={"X-API-KEY": BAZARR_API_KEY},
            timeout=30,
        )
    return _client


async def bz_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def bz_post(path: str, body: dict | None = None) -> Any:
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
            name="bazarr_get_status",
            description="Get Bazarr system status, version, and health info",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="bazarr_list_series",
            description="List all TV series tracked by Bazarr with missing subtitle counts per language",
            inputSchema={
                "type": "object",
                "properties": {
                    "start": {"type": "integer", "default": 0},
                    "length": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="bazarr_list_movies",
            description="List all movies tracked by Bazarr with missing subtitle counts per language",
            inputSchema={
                "type": "object",
                "properties": {
                    "start": {"type": "integer", "default": 0},
                    "length": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="bazarr_get_wanted_episodes",
            description="List TV episodes that are missing subtitles in their configured languages",
            inputSchema={
                "type": "object",
                "properties": {
                    "start": {"type": "integer", "default": 0},
                    "length": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="bazarr_get_wanted_movies",
            description="List movies that are missing subtitles in their configured languages",
            inputSchema={
                "type": "object",
                "properties": {
                    "start": {"type": "integer", "default": 0},
                    "length": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="bazarr_list_providers",
            description="List all configured subtitle providers and their enabled/working status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="bazarr_search_subtitles_episode",
            description="Search for and download subtitles for a specific TV episode",
            inputSchema={
                "type": "object",
                "properties": {
                    "episode_id": {"type": "integer", "description": "Bazarr episode ID"},
                    "language": {"type": "string", "description": "Language code (e.g. 'en', 'fr', 'de')"},
                    "providers": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Provider names to search (omit to use all enabled providers)",
                    },
                },
                "required": ["episode_id", "language"],
            },
        ),
        types.Tool(
            name="bazarr_search_subtitles_movie",
            description="Search for and download subtitles for a specific movie",
            inputSchema={
                "type": "object",
                "properties": {
                    "movie_id": {"type": "integer", "description": "Bazarr movie ID"},
                    "language": {"type": "string", "description": "Language code (e.g. 'en', 'fr', 'de')"},
                    "providers": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Provider names to search (omit to use all enabled providers)",
                    },
                },
                "required": ["movie_id", "language"],
            },
        ),
        types.Tool(
            name="bazarr_get_history_episodes",
            description="Get recent subtitle download history for TV episodes",
            inputSchema={
                "type": "object",
                "properties": {
                    "start": {"type": "integer", "default": 0},
                    "length": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="bazarr_get_history_movies",
            description="Get recent subtitle download history for movies",
            inputSchema={
                "type": "object",
                "properties": {
                    "start": {"type": "integer", "default": 0},
                    "length": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="bazarr_get_languages",
            description="List all subtitle languages supported by Bazarr with their codes",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "bazarr_get_status":
                return _text(await bz_get("/system/status"))

            case "bazarr_list_series":
                return _text(await bz_get("/series", params={
                    "start": arguments.get("start", 0),
                    "length": arguments.get("length", 30),
                }))

            case "bazarr_list_movies":
                return _text(await bz_get("/movies", params={
                    "start": arguments.get("start", 0),
                    "length": arguments.get("length", 30),
                }))

            case "bazarr_get_wanted_episodes":
                return _text(await bz_get("/episodes/wanted", params={
                    "start": arguments.get("start", 0),
                    "length": arguments.get("length", 30),
                }))

            case "bazarr_get_wanted_movies":
                return _text(await bz_get("/movies/wanted", params={
                    "start": arguments.get("start", 0),
                    "length": arguments.get("length", 30),
                }))

            case "bazarr_list_providers":
                return _text(await bz_get("/providers"))

            case "bazarr_search_subtitles_episode":
                body: dict = {
                    "episodeid": arguments["episode_id"],
                    "language": arguments["language"],
                }
                if providers := arguments.get("providers"):
                    body["providers"] = providers
                return _text(await bz_post("/providers/episodes", body))

            case "bazarr_search_subtitles_movie":
                body = {
                    "movieid": arguments["movie_id"],
                    "language": arguments["language"],
                }
                if providers := arguments.get("providers"):
                    body["providers"] = providers
                return _text(await bz_post("/providers/movies", body))

            case "bazarr_get_history_episodes":
                return _text(await bz_get("/history/episodes", params={
                    "start": arguments.get("start", 0),
                    "length": arguments.get("length", 30),
                }))

            case "bazarr_get_history_movies":
                return _text(await bz_get("/history/movies", params={
                    "start": arguments.get("start", 0),
                    "length": arguments.get("length", 30),
                }))

            case "bazarr_get_languages":
                return _text(await bz_get("/languages"))

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
