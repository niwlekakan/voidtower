#!/usr/bin/env python3
"""
Matrix Synapse MCP Server — admin tools for a Synapse homeserver.

Setup:
  pip install mcp httpx
  SYNAPSE_URL=http://localhost:8008 SYNAPSE_ADMIN_TOKEN=<token> python matrix_synapse_server.py

Get an admin token: log in with an admin user via Element or curl, copy the access_token.
The account must have server admin privileges (set via register_new_matrix_user or Synapse admin API).
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

SYNAPSE_URL = os.environ.get("SYNAPSE_URL", "http://localhost:8008").rstrip("/")
SYNAPSE_ADMIN_TOKEN = os.environ.get("SYNAPSE_ADMIN_TOKEN", "")

server = Server("matrix_synapse")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=SYNAPSE_URL,
            headers={"Authorization": f"Bearer {SYNAPSE_ADMIN_TOKEN}"},
            timeout=30,
        )
    return _client


async def sy_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def sy_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def sy_delete(path: str, body: dict | None = None) -> Any:
    r = await client().request("DELETE", path, json=body or {})
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
            name="synapse_get_server_info",
            description="Get Synapse server version and federation status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="synapse_list_users",
            description="List all local users on the homeserver",
            inputSchema={
                "type": "object",
                "properties": {
                    "from_index": {"type": "integer", "default": 0},
                    "limit": {"type": "integer", "default": 25},
                    "guests": {"type": "boolean", "default": False},
                    "deactivated": {"type": "boolean", "default": False},
                    "search_term": {"type": "string", "description": "Filter by username or display name"},
                },
            },
        ),
        types.Tool(
            name="synapse_get_user",
            description="Get details for a specific Matrix user",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "string", "description": "Full Matrix user ID e.g. @alice:example.com"},
                },
                "required": ["user_id"],
            },
        ),
        types.Tool(
            name="synapse_deactivate_user",
            description="Deactivate a user account (optionally erase their data)",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "string", "description": "Full Matrix user ID"},
                    "erase": {"type": "boolean", "default": False, "description": "If true, erases all messages and media for this user"},
                },
                "required": ["user_id"],
            },
        ),
        types.Tool(
            name="synapse_reset_user_password",
            description="Reset a user's password",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "string"},
                    "new_password": {"type": "string"},
                    "logout_devices": {"type": "boolean", "default": True},
                },
                "required": ["user_id", "new_password"],
            },
        ),
        types.Tool(
            name="synapse_list_rooms",
            description="List all rooms on the homeserver with member counts and aliases",
            inputSchema={
                "type": "object",
                "properties": {
                    "from_index": {"type": "integer", "default": 0},
                    "limit": {"type": "integer", "default": 25},
                    "search_term": {"type": "string", "description": "Filter by room name or alias"},
                    "order_by": {"type": "string", "enum": ["name", "canonical_alias", "joined_members", "joined_local_members", "version", "creator", "state_events"], "default": "joined_members"},
                },
            },
        ),
        types.Tool(
            name="synapse_get_room",
            description="Get details for a specific Matrix room",
            inputSchema={
                "type": "object",
                "properties": {
                    "room_id": {"type": "string", "description": "Room ID e.g. !abc123:example.com"},
                },
                "required": ["room_id"],
            },
        ),
        types.Tool(
            name="synapse_get_room_members",
            description="List all members in a room",
            inputSchema={
                "type": "object",
                "properties": {
                    "room_id": {"type": "string"},
                },
                "required": ["room_id"],
            },
        ),
        types.Tool(
            name="synapse_delete_room",
            description="Shut down and purge a room from the homeserver",
            inputSchema={
                "type": "object",
                "properties": {
                    "room_id": {"type": "string"},
                    "purge": {"type": "boolean", "default": True, "description": "Remove all room data from the database"},
                    "block": {"type": "boolean", "default": False, "description": "Block the room from being rejoined"},
                    "message": {"type": "string", "description": "Message to send to users before deletion"},
                },
                "required": ["room_id"],
            },
        ),
        types.Tool(
            name="synapse_purge_history",
            description="Delete old messages from a room up to a given timestamp",
            inputSchema={
                "type": "object",
                "properties": {
                    "room_id": {"type": "string"},
                    "purge_up_to_ts": {"type": "integer", "description": "Unix timestamp in milliseconds — messages before this are deleted"},
                    "delete_local_events": {"type": "boolean", "default": False},
                },
                "required": ["room_id", "purge_up_to_ts"],
            },
        ),
        types.Tool(
            name="synapse_list_media_by_user",
            description="List media usage statistics per user",
            inputSchema={
                "type": "object",
                "properties": {
                    "from_index": {"type": "integer", "default": 0},
                    "limit": {"type": "integer", "default": 25},
                    "order_by": {"type": "string", "enum": ["user_id", "displayname", "media_count", "media_length"], "default": "media_length"},
                },
            },
        ),
        types.Tool(
            name="synapse_get_background_tasks",
            description="Get the status of Synapse background update tasks",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "synapse_get_server_info":
                version = await sy_get("/_synapse/admin/v1/server_version")
                return _text(version)

            case "synapse_list_users":
                params: dict = {
                    "from": arguments.get("from_index", 0),
                    "limit": arguments.get("limit", 25),
                    "guests": str(arguments.get("guests", False)).lower(),
                    "deactivated": str(arguments.get("deactivated", False)).lower(),
                }
                if s := arguments.get("search_term"):
                    params["name"] = s
                return _text(await sy_get("/_synapse/admin/v2/users", params=params))

            case "synapse_get_user":
                return _text(await sy_get(f"/_synapse/admin/v2/users/{arguments['user_id']}"))

            case "synapse_deactivate_user":
                return _text(await sy_post(
                    f"/_synapse/admin/v1/deactivate/{arguments['user_id']}",
                    {"erase": arguments.get("erase", False)},
                ))

            case "synapse_reset_user_password":
                return _text(await sy_post(
                    f"/_synapse/admin/v1/reset_password/{arguments['user_id']}",
                    {
                        "new_password": arguments["new_password"],
                        "logout_devices": arguments.get("logout_devices", True),
                    },
                ))

            case "synapse_list_rooms":
                params = {
                    "from": arguments.get("from_index", 0),
                    "limit": arguments.get("limit", 25),
                    "order_by": arguments.get("order_by", "joined_members"),
                }
                if s := arguments.get("search_term"):
                    params["search_term"] = s
                return _text(await sy_get("/_synapse/admin/v1/rooms", params=params))

            case "synapse_get_room":
                return _text(await sy_get(f"/_synapse/admin/v1/rooms/{arguments['room_id']}"))

            case "synapse_get_room_members":
                return _text(await sy_get(f"/_synapse/admin/v1/rooms/{arguments['room_id']}/members"))

            case "synapse_delete_room":
                body: dict = {
                    "purge": arguments.get("purge", True),
                    "block": arguments.get("block", False),
                }
                if msg := arguments.get("message"):
                    body["message"] = msg
                return _text(await sy_delete(f"/_synapse/admin/v2/rooms/{arguments['room_id']}", body))

            case "synapse_purge_history":
                return _text(await sy_post(
                    f"/_synapse/admin/v1/purge_history/{arguments['room_id']}",
                    {
                        "purge_up_to_ts": arguments["purge_up_to_ts"],
                        "delete_local_events": arguments.get("delete_local_events", False),
                    },
                ))

            case "synapse_list_media_by_user":
                params = {
                    "from": arguments.get("from_index", 0),
                    "limit": arguments.get("limit", 25),
                    "order_by": arguments.get("order_by", "media_length"),
                }
                return _text(await sy_get("/_synapse/admin/v1/statistics/users/media", params=params))

            case "synapse_get_background_tasks":
                return _text(await sy_get("/_synapse/admin/v1/background_updates/status"))

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
