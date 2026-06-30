#!/usr/bin/env python3
"""
Jellyfin MCP Server — manage and query a Jellyfin media server.

Setup:
  pip install mcp httpx
  JELLYFIN_URL=http://localhost:8096 JELLYFIN_API_KEY=<key> python jellyfin_server.py

Get an API key: Jellyfin → Dashboard → API Keys → +
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

JELLYFIN_URL = os.environ.get("JELLYFIN_URL", "http://localhost:8096").rstrip("/")
JELLYFIN_API_KEY = os.environ.get("JELLYFIN_API_KEY", "")

server = Server("jellyfin")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=JELLYFIN_URL,
            headers={
                "Authorization": f'MediaBrowser Token="{JELLYFIN_API_KEY}"',
                "Content-Type": "application/json",
            },
            timeout=30,
        )
    return _client


async def jf_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def jf_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def jf_delete(path: str) -> Any:
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
            name="jellyfin_get_system_info",
            description="Get Jellyfin server info: version, OS, startup time, paths",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="jellyfin_list_libraries",
            description="List all Jellyfin media libraries with their type, path, and item count",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="jellyfin_scan_library",
            description="Trigger a metadata/file scan on a specific library or all libraries",
            inputSchema={
                "type": "object",
                "properties": {
                    "library_id": {"type": "string", "description": "Library ID to scan (omit to scan all libraries)"},
                },
            },
        ),
        types.Tool(
            name="jellyfin_search",
            description="Search for media items (movies, shows, episodes, music, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search term"},
                    "media_type": {
                        "type": "string",
                        "enum": ["Movie", "Series", "Episode", "Audio", "MusicAlbum", "MusicArtist", "Book"],
                        "description": "Filter by media type (omit for all)",
                    },
                    "limit": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="jellyfin_get_latest",
            description="Get recently added items across all libraries or a specific library",
            inputSchema={
                "type": "object",
                "properties": {
                    "library_id": {"type": "string", "description": "Library ID to scope (omit for all)"},
                    "limit": {"type": "integer", "default": 20},
                },
            },
        ),
        types.Tool(
            name="jellyfin_get_sessions",
            description="Get all active playback sessions — who is watching what, stream info, progress",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="jellyfin_stop_session",
            description="Stop (kick) an active playback session",
            inputSchema={
                "type": "object",
                "properties": {
                    "session_id": {"type": "string", "description": "Session ID from jellyfin_get_sessions"},
                    "message": {"type": "string", "description": "Optional message to show the user before stopping"},
                },
                "required": ["session_id"],
            },
        ),
        types.Tool(
            name="jellyfin_list_users",
            description="List all Jellyfin users with their policy and last active time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="jellyfin_create_user",
            description="Create a new Jellyfin user",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Username"},
                    "password": {"type": "string", "description": "Initial password"},
                },
                "required": ["name", "password"],
            },
        ),
        types.Tool(
            name="jellyfin_delete_user",
            description="Delete a Jellyfin user by user ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "string", "description": "User ID from jellyfin_list_users"},
                },
                "required": ["user_id"],
            },
        ),
        types.Tool(
            name="jellyfin_get_activity_log",
            description="Get recent activity log entries (logins, scans, playback, errors)",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 50},
                    "start_index": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="jellyfin_get_scheduled_tasks",
            description="List scheduled tasks (library scans, chapter image extraction, etc.) and their last run status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="jellyfin_run_task",
            description="Trigger a scheduled task immediately",
            inputSchema={
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "description": "Task ID from jellyfin_get_scheduled_tasks"},
                },
                "required": ["task_id"],
            },
        ),
        types.Tool(
            name="jellyfin_get_playback_stats",
            description="Get play count and watch history stats for a media item",
            inputSchema={
                "type": "object",
                "properties": {
                    "item_id": {"type": "string", "description": "Item ID from jellyfin_search or jellyfin_get_latest"},
                },
                "required": ["item_id"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "jellyfin_get_system_info":
                return _text(await jf_get("/System/Info"))

            case "jellyfin_list_libraries":
                data = await jf_get("/Library/VirtualFolders")
                return _text(data)

            case "jellyfin_scan_library":
                lib_id = arguments.get("library_id")
                if lib_id:
                    await jf_post(f"/Items/{lib_id}/Refresh", {"Recursive": True, "MetadataRefreshMode": "Default"})
                else:
                    await jf_post("/Library/Refresh")
                return _text({"triggered": True, "library_id": lib_id or "all"})

            case "jellyfin_search":
                params: dict = {
                    "searchTerm": arguments["query"],
                    "Limit": arguments.get("limit", 20),
                    "Recursive": True,
                    "Fields": "Overview,Genres,ProductionYear",
                }
                if mt := arguments.get("media_type"):
                    params["IncludeItemTypes"] = mt
                data = await jf_get("/Items", params=params)
                return _text(data.get("Items", data))

            case "jellyfin_get_latest":
                params = {"Limit": arguments.get("limit", 20)}
                if lib_id := arguments.get("library_id"):
                    params["ParentId"] = lib_id
                data = await jf_get("/Items/Latest", params=params)
                return _text(data)

            case "jellyfin_get_sessions":
                return _text(await jf_get("/Sessions"))

            case "jellyfin_stop_session":
                sid = arguments["session_id"]
                if msg := arguments.get("message"):
                    await jf_post(f"/Sessions/{sid}/Message", {"Text": msg, "Header": "VoidTower"})
                await jf_delete(f"/Sessions/{sid}/Playing/Stopped")
                return _text({"stopped": sid})

            case "jellyfin_list_users":
                return _text(await jf_get("/Users"))

            case "jellyfin_create_user":
                data = await jf_post("/Users/New", {
                    "Name": arguments["name"],
                    "Password": arguments["password"],
                })
                return _text(data)

            case "jellyfin_delete_user":
                await jf_delete(f"/Users/{arguments['user_id']}")
                return _text({"deleted": arguments["user_id"]})

            case "jellyfin_get_activity_log":
                data = await jf_get("/System/ActivityLog/Entries", params={
                    "Limit": arguments.get("limit", 50),
                    "StartIndex": arguments.get("start_index", 0),
                })
                return _text(data.get("Items", data))

            case "jellyfin_get_scheduled_tasks":
                return _text(await jf_get("/ScheduledTasks"))

            case "jellyfin_run_task":
                await jf_post(f"/ScheduledTasks/Running/{arguments['task_id']}")
                return _text({"triggered": arguments["task_id"]})

            case "jellyfin_get_playback_stats":
                data = await jf_get(f"/Items/{arguments['item_id']}/PlaybackInfo")
                return _text(data)

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
