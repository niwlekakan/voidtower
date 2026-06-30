#!/usr/bin/env python3
"""
Open WebUI MCP Server — manage models, users, and chats in Open WebUI.

Setup:
  pip install mcp httpx
  OPENWEBUI_URL=http://localhost:3000 OPENWEBUI_TOKEN=<jwt> python open_webui_server.py

Get a token: Open WebUI → Settings → Account → API Keys → Create new secret key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

OPENWEBUI_URL = os.environ.get("OPENWEBUI_URL", "http://localhost:3000").rstrip("/")
OPENWEBUI_TOKEN = os.environ.get("OPENWEBUI_TOKEN", "")

server = Server("open-webui")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=OPENWEBUI_URL,
            headers={"Authorization": f"Bearer {OPENWEBUI_TOKEN}"},
            timeout=30,
        )
    return _client


async def ow_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def ow_delete(path: str) -> Any:
    r = await client().delete(path)
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
            name="openwebui_list_models",
            description="List all models available in Open WebUI across all configured providers",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="openwebui_list_users",
            description="List all users (admin only)",
            inputSchema={
                "type": "object",
                "properties": {
                    "skip": {"type": "integer", "default": 0},
                    "limit": {"type": "integer", "default": 50},
                },
            },
        ),
        types.Tool(
            name="openwebui_get_user",
            description="Get details for a specific user by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "string", "description": "User ID"},
                },
                "required": ["user_id"],
            },
        ),
        types.Tool(
            name="openwebui_list_chats",
            description="List recent chats for the authenticated user",
            inputSchema={
                "type": "object",
                "properties": {
                    "page": {"type": "integer", "default": 1},
                },
            },
        ),
        types.Tool(
            name="openwebui_get_chat",
            description="Get full contents of a specific chat by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "chat_id": {"type": "string", "description": "Chat ID"},
                },
                "required": ["chat_id"],
            },
        ),
        types.Tool(
            name="openwebui_delete_chat",
            description="Delete a chat by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "chat_id": {"type": "string", "description": "Chat ID to delete"},
                },
                "required": ["chat_id"],
            },
        ),
        types.Tool(
            name="openwebui_list_knowledge",
            description="List all knowledge bases (RAG document collections)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="openwebui_list_tools",
            description="List all installed tools and functions",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "openwebui_list_models":
                return _text(await ow_get("/api/models"))

            case "openwebui_list_users":
                return _text(await ow_get("/api/v1/users/", params={
                    "skip": arguments.get("skip", 0),
                    "limit": arguments.get("limit", 50),
                }))

            case "openwebui_get_user":
                return _text(await ow_get(f"/api/v1/users/{arguments['user_id']}"))

            case "openwebui_list_chats":
                return _text(await ow_get("/api/v1/chats/", params={"page": arguments.get("page", 1)}))

            case "openwebui_get_chat":
                return _text(await ow_get(f"/api/v1/chats/{arguments['chat_id']}"))

            case "openwebui_delete_chat":
                return _text(await ow_delete(f"/api/v1/chats/{arguments['chat_id']}"))

            case "openwebui_list_knowledge":
                return _text(await ow_get("/api/v1/knowledge/"))

            case "openwebui_list_tools":
                return _text(await ow_get("/api/v1/tools/"))

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
