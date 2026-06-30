#!/usr/bin/env python3
"""
WireGuard Easy MCP Server — manage WireGuard VPN peers via WireGuard Easy v14+.

Setup:
  pip install mcp httpx
  WG_EASY_URL=http://localhost:51821 WG_EASY_PASSWORD=<password> python wireguard_easy_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

WG_EASY_URL = os.environ.get("WG_EASY_URL", "http://localhost:51821").rstrip("/")
WG_EASY_PASSWORD = os.environ.get("WG_EASY_PASSWORD", "")

server = Server("wireguard-easy")
_client: httpx.AsyncClient | None = None
_cookies: dict = {}


async def _authenticate() -> None:
    global _cookies
    async with httpx.AsyncClient(base_url=WG_EASY_URL, timeout=30) as c:
        r = await c.post("/api/session", json={"password": WG_EASY_PASSWORD})
        r.raise_for_status()
        _cookies = dict(r.cookies)


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=WG_EASY_URL, timeout=30)
    return _client


async def wg_get(path: str) -> Any:
    global _cookies
    if not _cookies:
        await _authenticate()
    r = await client().get(path, cookies=_cookies)
    if r.status_code == 401:
        await _authenticate()
        r = await client().get(path, cookies=_cookies)
    r.raise_for_status()
    return r.json()


async def wg_post(path: str, body: dict | None = None) -> Any:
    global _cookies
    if not _cookies:
        await _authenticate()
    r = await client().post(path, json=body or {}, cookies=_cookies)
    if r.status_code == 401:
        await _authenticate()
        r = await client().post(path, json=body or {}, cookies=_cookies)
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def wg_delete(path: str) -> Any:
    global _cookies
    if not _cookies:
        await _authenticate()
    r = await client().delete(path, cookies=_cookies)
    if r.status_code == 401:
        await _authenticate()
        r = await client().delete(path, cookies=_cookies)
    r.raise_for_status()
    return {"status": r.status_code}


async def wg_get_text(path: str) -> str:
    global _cookies
    if not _cookies:
        await _authenticate()
    r = await client().get(path, cookies=_cookies)
    if r.status_code == 401:
        await _authenticate()
        r = await client().get(path, cookies=_cookies)
    r.raise_for_status()
    return r.text


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2) if not isinstance(data, str) else data)]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="wg_easy_list_clients",
            description="List all WireGuard VPN peers with their name, IP address, enabled state, and transfer statistics",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="wg_easy_create_client",
            description="Create a new WireGuard VPN peer",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Display name for the peer (e.g. 'laptop', 'phone')"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="wg_easy_delete_client",
            description="Permanently delete a WireGuard VPN peer",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Client ID from wg_easy_list_clients"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="wg_easy_enable_client",
            description="Enable a disabled WireGuard VPN peer so it can connect",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Client ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="wg_easy_disable_client",
            description="Disable a WireGuard VPN peer to block it from connecting without deleting it",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Client ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="wg_easy_get_client_config",
            description="Get the WireGuard .conf configuration file content for a peer (to share with the device)",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Client ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="wg_easy_get_client_qr",
            description="Get the QR code SVG for a WireGuard peer (scan with mobile WireGuard app to configure)",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Client ID"},
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
            case "wg_easy_list_clients":
                return _text(await wg_get("/api/wireguard/client"))

            case "wg_easy_create_client":
                return _text(await wg_post("/api/wireguard/client", {"name": arguments["name"]}))

            case "wg_easy_delete_client":
                return _text(await wg_delete(f"/api/wireguard/client/{arguments['id']}"))

            case "wg_easy_enable_client":
                return _text(await wg_post(f"/api/wireguard/client/{arguments['id']}/enable"))

            case "wg_easy_disable_client":
                return _text(await wg_post(f"/api/wireguard/client/{arguments['id']}/disable"))

            case "wg_easy_get_client_config":
                text = await wg_get_text(f"/api/wireguard/client/{arguments['id']}/configuration")
                return [types.TextContent(type="text", text=text)]

            case "wg_easy_get_client_qr":
                svg = await wg_get_text(f"/api/wireguard/client/{arguments['id']}/qrcode.svg")
                return [types.TextContent(type="text", text=svg)]

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
