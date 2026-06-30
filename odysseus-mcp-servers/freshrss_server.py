#!/usr/bin/env python3
"""
FreshRSS MCP Server — manage subscriptions and read articles via the Google Reader API.

Setup:
  pip install mcp httpx
  FRESHRSS_URL=http://localhost:80 FRESHRSS_USER=admin FRESHRSS_PASSWORD=<pw> python freshrss_server.py

Enable API: FreshRSS → Settings → Authentication → Allow API access
"""

import asyncio
import os
import json
import time
import urllib.parse
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

FRESHRSS_URL = os.environ.get("FRESHRSS_URL", "http://localhost:80").rstrip("/")
FRESHRSS_USER = os.environ.get("FRESHRSS_USER", "")
FRESHRSS_PASSWORD = os.environ.get("FRESHRSS_PASSWORD", "")

server = Server("freshrss")
_auth_token: str = ""
_client: httpx.AsyncClient | None = None


async def _ensure_auth() -> None:
    global _auth_token, _client
    async with httpx.AsyncClient(base_url=FRESHRSS_URL, timeout=30) as c:
        r = await c.post(
            "/api/greader.php/accounts/ClientLogin",
            content=urllib.parse.urlencode({
                "Email": FRESHRSS_USER,
                "Passwd": FRESHRSS_PASSWORD,
                "service": "reader",
            }),
            headers={"Content-Type": "application/x-www-form-urlencoded"},
        )
        r.raise_for_status()
        for line in r.text.splitlines():
            if line.startswith("Auth="):
                _auth_token = line[5:].strip()
                break
    _client = httpx.AsyncClient(
        base_url=FRESHRSS_URL,
        headers={"Authorization": f"GoogleLogin auth={_auth_token}"},
        timeout=30,
    )


def client() -> httpx.AsyncClient:
    if _client is None:
        raise RuntimeError("Not authenticated")
    return _client


async def _get_action_token() -> str:
    r = await client().get("/api/greader.php/reader/api/0/token")
    r.raise_for_status()
    return r.text.strip()


async def fr_get(path: str, params: dict | None = None) -> Any:
    if not _auth_token:
        await _ensure_auth()
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def fr_post(path: str, data: dict) -> Any:
    if not _auth_token:
        await _ensure_auth()
    action_token = await _get_action_token()
    r = await client().post(
        path,
        content=urllib.parse.urlencode(data),
        headers={
            "Content-Type": "application/x-www-form-urlencoded",
            "T": action_token,
        },
    )
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"result": r.text}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="freshrss_list_subscriptions",
            description="List all RSS feed subscriptions with their title, URL, and unread count",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="freshrss_list_labels",
            description="List all categories/labels used to group subscriptions",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="freshrss_get_unread_count",
            description="Get unread article counts per feed and total unread across all feeds",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="freshrss_get_items",
            description="Get recent articles from all feeds (read and unread)",
            inputSchema={
                "type": "object",
                "properties": {
                    "n": {"type": "integer", "default": 20, "description": "Number of articles to return"},
                },
            },
        ),
        types.Tool(
            name="freshrss_get_unread_items",
            description="Get unread articles across all subscriptions",
            inputSchema={
                "type": "object",
                "properties": {
                    "n": {"type": "integer", "default": 20, "description": "Number of articles to return"},
                },
            },
        ),
        types.Tool(
            name="freshrss_search",
            description="Search articles by keyword across all subscriptions",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search term"},
                    "n": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="freshrss_mark_all_read",
            description="Mark all articles in all feeds as read",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="freshrss_add_subscription",
            description="Subscribe to a new RSS/Atom feed by URL",
            inputSchema={
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "Feed URL to subscribe to"},
                },
                "required": ["url"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "freshrss_list_subscriptions":
                return _text(await fr_get(
                    "/api/greader.php/reader/api/0/subscription/list",
                    params={"output": "json"},
                ))

            case "freshrss_list_labels":
                return _text(await fr_get(
                    "/api/greader.php/reader/api/0/tag/list",
                    params={"output": "json"},
                ))

            case "freshrss_get_unread_count":
                return _text(await fr_get(
                    "/api/greader.php/reader/api/0/unread-count",
                    params={"output": "json"},
                ))

            case "freshrss_get_items":
                return _text(await fr_get(
                    "/api/greader.php/reader/api/0/stream/contents/reading-list",
                    params={"output": "json", "n": arguments.get("n", 20)},
                ))

            case "freshrss_get_unread_items":
                return _text(await fr_get(
                    "/api/greader.php/reader/api/0/stream/contents/user/-/state/com.google/reading-list",
                    params={
                        "output": "json",
                        "xt": "user/-/state/com.google/read",
                        "n": arguments.get("n", 20),
                    },
                ))

            case "freshrss_search":
                return _text(await fr_get(
                    "/api/greader.php/reader/api/0/stream/contents/user/-/state/com.google/reading-list",
                    params={
                        "output": "json",
                        "q": arguments["query"],
                        "n": arguments.get("n", 20),
                    },
                ))

            case "freshrss_mark_all_read":
                ts_us = str(int(time.time() * 1_000_000))
                return _text(await fr_post(
                    "/api/greader.php/reader/api/0/mark-all-as-read",
                    {"s": "user/-/state/com.google/reading-list", "ts": ts_us},
                ))

            case "freshrss_add_subscription":
                return _text(await fr_post(
                    "/api/greader.php/reader/api/0/subscription/quickadd",
                    {"quickadd": arguments["url"]},
                ))

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
