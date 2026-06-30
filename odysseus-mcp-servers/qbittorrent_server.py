#!/usr/bin/env python3
"""
qBittorrent MCP Server — manage torrents on a qBittorrent instance.

Setup:
  pip install mcp httpx
  QB_URL=http://localhost:8080 QB_USERNAME=admin QB_PASSWORD=<pw> python qbittorrent_server.py

Enable Web UI: qBittorrent → Tools → Options → Web UI → Enable
"""

import asyncio
import os
import json
import urllib.parse
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

QB_URL = os.environ.get("QB_URL", "http://localhost:8080").rstrip("/")
QB_USERNAME = os.environ.get("QB_USERNAME", "admin")
QB_PASSWORD = os.environ.get("QB_PASSWORD", "")

server = Server("qbittorrent")
_client: httpx.AsyncClient | None = None


async def _ensure_auth() -> None:
    global _client
    c = httpx.AsyncClient(base_url=QB_URL, timeout=30)
    r = await c.post(
        "/api/v2/auth/login",
        content=urllib.parse.urlencode({"username": QB_USERNAME, "password": QB_PASSWORD}),
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    r.raise_for_status()
    _client = httpx.AsyncClient(
        base_url=QB_URL,
        cookies=r.cookies,
        timeout=30,
    )
    await c.aclose()


def client() -> httpx.AsyncClient:
    if _client is None:
        raise RuntimeError("Not authenticated — call _ensure_auth() first")
    return _client


async def qb_get(path: str, params: dict | None = None) -> Any:
    if _client is None:
        await _ensure_auth()
    r = await client().get(path, params=params)
    if r.status_code == 403:
        await _ensure_auth()
        r = await client().get(path, params=params)
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return r.text


async def qb_post_form(path: str, data: dict) -> Any:
    if _client is None:
        await _ensure_auth()
    r = await client().post(
        path,
        content=urllib.parse.urlencode(data),
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    if r.status_code == 403:
        await _ensure_auth()
        r = await client().post(
            path,
            content=urllib.parse.urlencode(data),
            headers={"Content-Type": "application/x-www-form-urlencoded"},
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
            name="qb_get_version",
            description="Get the qBittorrent application version",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="qb_list_torrents",
            description="List all torrents with their name, size, progress, state, download/upload speed, and category",
            inputSchema={
                "type": "object",
                "properties": {
                    "filter": {
                        "type": "string",
                        "enum": ["all", "downloading", "seeding", "completed", "paused", "active", "inactive", "stalled"],
                        "default": "all",
                    },
                    "category": {"type": "string", "description": "Filter by category name"},
                },
            },
        ),
        types.Tool(
            name="qb_get_torrent",
            description="Get detailed properties for a specific torrent by its hash",
            inputSchema={
                "type": "object",
                "properties": {
                    "hash": {"type": "string", "description": "Torrent hash from qb_list_torrents"},
                },
                "required": ["hash"],
            },
        ),
        types.Tool(
            name="qb_add_torrent_url",
            description="Add a new torrent by magnet link or .torrent URL",
            inputSchema={
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "Magnet link or URL to a .torrent file"},
                    "savepath": {"type": "string", "description": "Download directory path"},
                    "category": {"type": "string", "description": "Category to assign"},
                },
                "required": ["url"],
            },
        ),
        types.Tool(
            name="qb_pause_torrent",
            description="Pause one or all torrents. Use hash='all' to pause everything.",
            inputSchema={
                "type": "object",
                "properties": {
                    "hash": {"type": "string", "description": "Torrent hash, or 'all' to pause all"},
                },
                "required": ["hash"],
            },
        ),
        types.Tool(
            name="qb_resume_torrent",
            description="Resume one or all paused torrents. Use hash='all' to resume everything.",
            inputSchema={
                "type": "object",
                "properties": {
                    "hash": {"type": "string", "description": "Torrent hash, or 'all' to resume all"},
                },
                "required": ["hash"],
            },
        ),
        types.Tool(
            name="qb_delete_torrent",
            description="Delete a torrent, optionally also deleting its downloaded files",
            inputSchema={
                "type": "object",
                "properties": {
                    "hash": {"type": "string", "description": "Torrent hash"},
                    "delete_files": {"type": "boolean", "default": False, "description": "Also delete downloaded files from disk"},
                },
                "required": ["hash"],
            },
        ),
        types.Tool(
            name="qb_get_categories",
            description="List all torrent categories with their save paths",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="qb_set_category",
            description="Assign a category to a torrent",
            inputSchema={
                "type": "object",
                "properties": {
                    "hash": {"type": "string"},
                    "category": {"type": "string", "description": "Category name (must already exist)"},
                },
                "required": ["hash", "category"],
            },
        ),
        types.Tool(
            name="qb_get_transfer_info",
            description="Get global transfer stats: current download/upload speed, session totals, and speed limits",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="qb_get_main_data",
            description="Get a full snapshot of all torrents and server state in one call",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="qb_set_speed_limits",
            description="Set global download and/or upload speed limits in bytes per second (0 = unlimited)",
            inputSchema={
                "type": "object",
                "properties": {
                    "download_limit": {"type": "integer", "description": "Download limit in bytes/s (0 = unlimited)"},
                    "upload_limit": {"type": "integer", "description": "Upload limit in bytes/s (0 = unlimited)"},
                },
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "qb_get_version":
                return _text(await qb_get("/api/v2/app/version"))

            case "qb_list_torrents":
                params: dict = {
                    "filter": arguments.get("filter", "all"),
                    "sort": "added_on",
                    "reverse": "true",
                }
                if cat := arguments.get("category"):
                    params["category"] = cat
                return _text(await qb_get("/api/v2/torrents/info", params=params))

            case "qb_get_torrent":
                return _text(await qb_get("/api/v2/torrents/properties", params={"hash": arguments["hash"]}))

            case "qb_add_torrent_url":
                data: dict = {"urls": arguments["url"]}
                if sp := arguments.get("savepath"):
                    data["savepath"] = sp
                if cat := arguments.get("category"):
                    data["category"] = cat
                return _text(await qb_post_form("/api/v2/torrents/add", data))

            case "qb_pause_torrent":
                return _text(await qb_post_form("/api/v2/torrents/pause", {"hashes": arguments["hash"]}))

            case "qb_resume_torrent":
                return _text(await qb_post_form("/api/v2/torrents/resume", {"hashes": arguments["hash"]}))

            case "qb_delete_torrent":
                return _text(await qb_post_form("/api/v2/torrents/delete", {
                    "hashes": arguments["hash"],
                    "deleteFiles": "true" if arguments.get("delete_files") else "false",
                }))

            case "qb_get_categories":
                return _text(await qb_get("/api/v2/torrents/categories"))

            case "qb_set_category":
                return _text(await qb_post_form("/api/v2/torrents/setCategory", {
                    "hashes": arguments["hash"],
                    "category": arguments["category"],
                }))

            case "qb_get_transfer_info":
                return _text(await qb_get("/api/v2/transfer/info"))

            case "qb_get_main_data":
                return _text(await qb_get("/api/v2/sync/maindata", params={"rid": "0"}))

            case "qb_set_speed_limits":
                results = {}
                if "download_limit" in arguments:
                    results["download"] = await qb_post_form(
                        "/api/v2/transfer/setDownloadLimit",
                        {"limit": str(arguments["download_limit"])},
                    )
                if "upload_limit" in arguments:
                    results["upload"] = await qb_post_form(
                        "/api/v2/transfer/setUploadLimit",
                        {"limit": str(arguments["upload_limit"])},
                    )
                return _text(results)

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
