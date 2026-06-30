#!/usr/bin/env python3
"""
Immich MCP Server — manage photos, videos, albums, and people on an Immich instance.

Setup:
  pip install mcp httpx
  IMMICH_URL=http://localhost:2283 IMMICH_API_KEY=<key> python immich_server.py

Get an API key: Immich → Account Settings → API Keys → New API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

IMMICH_URL = os.environ.get("IMMICH_URL", "http://localhost:2283").rstrip("/")
IMMICH_API_KEY = os.environ.get("IMMICH_API_KEY", "")

server = Server("immich")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=IMMICH_URL,
            headers={"x-api-key": IMMICH_API_KEY},
            timeout=30,
        )
    return _client


async def im_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def im_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    return r.json()


async def im_put(path: str, body: dict | None = None) -> Any:
    r = await client().put(path, json=body or {})
    r.raise_for_status()
    return r.json()


async def im_delete(path: str) -> Any:
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
            name="immich_get_server_info",
            description="Get Immich server version, disk usage, and storage statistics",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="immich_get_stats",
            description="Get total counts: photos, videos, users, and disk usage across the server",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="immich_search_assets",
            description="Semantic/AI-powered search across all photos and videos using natural language",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Natural language search (e.g. 'beach sunset', 'dog in snow')"},
                    "size": {"type": "integer", "default": 20, "description": "Results per page"},
                    "page": {"type": "integer", "default": 1},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="immich_search_by_metadata",
            description="Search photos/videos by metadata: filename, location, camera make/model, type, or favorite status",
            inputSchema={
                "type": "object",
                "properties": {
                    "originalFileName": {"type": "string"},
                    "city": {"type": "string"},
                    "country": {"type": "string"},
                    "make": {"type": "string", "description": "Camera manufacturer (e.g. Apple, Canon)"},
                    "model": {"type": "string", "description": "Camera model"},
                    "type": {"type": "string", "enum": ["IMAGE", "VIDEO"]},
                    "isFavorite": {"type": "boolean"},
                    "page": {"type": "integer", "default": 1},
                    "size": {"type": "integer", "default": 20},
                },
            },
        ),
        types.Tool(
            name="immich_get_asset",
            description="Get full details for a single photo or video asset by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Asset UUID from search results"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="immich_list_albums",
            description="List all albums with their asset count and thumbnail",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="immich_get_album",
            description="Get a specific album with its full asset list",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Album UUID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="immich_create_album",
            description="Create a new empty album",
            inputSchema={
                "type": "object",
                "properties": {
                    "albumName": {"type": "string"},
                    "description": {"type": "string"},
                },
                "required": ["albumName"],
            },
        ),
        types.Tool(
            name="immich_add_to_album",
            description="Add one or more assets to an existing album",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Album UUID"},
                    "asset_ids": {"type": "array", "items": {"type": "string"}, "description": "Asset UUIDs to add"},
                },
                "required": ["id", "asset_ids"],
            },
        ),
        types.Tool(
            name="immich_delete_album",
            description="Delete an album (does not delete the assets inside it)",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Album UUID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="immich_list_people",
            description="List all recognized people/faces with their names and asset counts",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="immich_get_memory_lane",
            description="Get 'on this day' assets — photos and videos from previous years on the same date",
            inputSchema={
                "type": "object",
                "properties": {
                    "day": {"type": "integer", "description": "Day of month (1-31)"},
                    "month": {"type": "integer", "description": "Month (1-12)"},
                },
                "required": ["day", "month"],
            },
        ),
        types.Tool(
            name="immich_list_tags",
            description="List all tags defined in Immich",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="immich_list_users",
            description="List all Immich users with their quota and storage usage (admin only)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="immich_run_job",
            description="Trigger a background job: thumbnailGeneration, metadataExtraction, videoConversion, faceDetection, smartSearch, or storageTemplateMigration",
            inputSchema={
                "type": "object",
                "properties": {
                    "jobName": {
                        "type": "string",
                        "enum": ["thumbnailGeneration", "metadataExtraction", "videoConversion", "faceDetection", "smartSearch", "storageTemplateMigration"],
                    },
                    "force": {"type": "boolean", "default": False, "description": "Reprocess already-processed assets"},
                },
                "required": ["jobName"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "immich_get_server_info":
                return _text(await im_get("/api/server-info"))

            case "immich_get_stats":
                return _text(await im_get("/api/server-info/stats"))

            case "immich_search_assets":
                return _text(await im_post("/api/search/smart-search", {
                    "query": arguments["query"],
                    "size": arguments.get("size", 20),
                    "page": arguments.get("page", 1),
                }))

            case "immich_search_by_metadata":
                body = {k: v for k, v in arguments.items() if v is not None}
                body.setdefault("page", 1)
                body.setdefault("size", 20)
                return _text(await im_post("/api/search/metadata", body))

            case "immich_get_asset":
                return _text(await im_get(f"/api/assets/{arguments['id']}"))

            case "immich_list_albums":
                return _text(await im_get("/api/albums"))

            case "immich_get_album":
                return _text(await im_get(f"/api/albums/{arguments['id']}"))

            case "immich_create_album":
                body: dict = {"albumName": arguments["albumName"]}
                if d := arguments.get("description"):
                    body["description"] = d
                return _text(await im_post("/api/albums", body))

            case "immich_add_to_album":
                return _text(await im_put(f"/api/albums/{arguments['id']}/assets", {
                    "ids": arguments["asset_ids"],
                }))

            case "immich_delete_album":
                return _text(await im_delete(f"/api/albums/{arguments['id']}"))

            case "immich_list_people":
                return _text(await im_get("/api/people"))

            case "immich_get_memory_lane":
                return _text(await im_get("/api/assets/memory-lane", params={
                    "day": arguments["day"],
                    "month": arguments["month"],
                }))

            case "immich_list_tags":
                return _text(await im_get("/api/tags"))

            case "immich_list_users":
                return _text(await im_get("/api/users"))

            case "immich_run_job":
                return _text(await im_post(f"/api/jobs/{arguments['jobName']}/command", {
                    "command": "start",
                    "force": arguments.get("force", False),
                }))

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
