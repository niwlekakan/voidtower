#!/usr/bin/env python3
"""
Jellyseerr MCP Server — manage media requests, users, and issues in Jellyseerr.

Setup:
  pip install mcp httpx
  JELLYSEERR_URL=http://localhost:5055 JELLYSEERR_API_KEY=<key> python jellyseerr_server.py

Get an API key: Jellyseerr → Settings → General → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

JELLYSEERR_URL = os.environ.get("JELLYSEERR_URL", "http://localhost:5055").rstrip("/")
JELLYSEERR_API_KEY = os.environ.get("JELLYSEERR_API_KEY", "")

server = Server("jellyseerr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{JELLYSEERR_URL}/api/v1",
            headers={"X-Api-Key": JELLYSEERR_API_KEY},
            timeout=30,
        )
    return _client


async def js_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def js_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def js_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="jellyseerr_get_status",
            description="Get Jellyseerr server status, version, and total request/user counts",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="jellyseerr_list_requests",
            description="List media requests with their approval status. Use filter to scope by status.",
            inputSchema={
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "string",
                        "enum": ["all", "approved", "pending", "declined", "available", "processing"],
                        "default": "all",
                        "description": "Filter requests by status",
                    },
                    "take": {"type": "integer", "default": 20},
                    "skip": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="jellyseerr_get_request",
            description="Get details for a specific media request by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Request ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="jellyseerr_approve_request",
            description="Approve a pending media request so it gets sent to Radarr/Sonarr",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Request ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="jellyseerr_decline_request",
            description="Decline a pending media request",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Request ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="jellyseerr_delete_request",
            description="Delete a media request entirely",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Request ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="jellyseerr_search",
            description="Search for movies and TV shows to check availability or request status",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Movie or show title"},
                    "page": {"type": "integer", "default": 1},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="jellyseerr_list_users",
            description="List all Jellyseerr users with their request counts and permissions",
            inputSchema={
                "type": "object",
                "properties": {
                    "take": {"type": "integer", "default": 20},
                    "skip": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="jellyseerr_get_user",
            description="Get details and request history for a specific user",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "User ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="jellyseerr_list_issues",
            description="List reported issues (playback problems, wrong audio, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "take": {"type": "integer", "default": 20},
                    "skip": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="jellyseerr_resolve_issue",
            description="Mark a reported issue as resolved",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Issue ID"},
                },
                "required": ["id"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "jellyseerr_get_status":
                return _text(await js_get("/status"))

            case "jellyseerr_list_requests":
                return _text(await js_get("/request", params={
                    "filter": arguments.get("filter", "all"),
                    "take": arguments.get("take", 20),
                    "skip": arguments.get("skip", 0),
                }))

            case "jellyseerr_get_request":
                return _text(await js_get(f"/request/{arguments['id']}"))

            case "jellyseerr_approve_request":
                return _text(await js_post(f"/request/{arguments['id']}/approve"))

            case "jellyseerr_decline_request":
                return _text(await js_post(f"/request/{arguments['id']}/decline"))

            case "jellyseerr_delete_request":
                return _text(await js_delete(f"/request/{arguments['id']}"))

            case "jellyseerr_search":
                return _text(await js_get("/search", params={
                    "query": arguments["query"],
                    "page": arguments.get("page", 1),
                }))

            case "jellyseerr_list_users":
                return _text(await js_get("/user", params={
                    "take": arguments.get("take", 20),
                    "skip": arguments.get("skip", 0),
                }))

            case "jellyseerr_get_user":
                return _text(await js_get(f"/user/{arguments['id']}"))

            case "jellyseerr_list_issues":
                return _text(await js_get("/issue", params={
                    "take": arguments.get("take", 20),
                    "skip": arguments.get("skip", 0),
                }))

            case "jellyseerr_resolve_issue":
                return _text(await js_post(f"/issue/{arguments['id']}/resolved"))

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
