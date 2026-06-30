#!/usr/bin/env python3
"""
Vaultwarden MCP Server — admin management for a Vaultwarden (Bitwarden-compatible) instance.

Setup:
  pip install mcp httpx
  VAULTWARDEN_URL=http://localhost VAULTWARDEN_ADMIN_TOKEN=<token> python vaultwarden_server.py

Set admin token: ADMIN_TOKEN env var in your Vaultwarden container config.
Note: Vaultwarden admin API is limited to user/org admin tasks — vault item contents are never exposed.
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

VAULTWARDEN_URL = os.environ.get("VAULTWARDEN_URL", "http://localhost").rstrip("/")
VAULTWARDEN_ADMIN_TOKEN = os.environ.get("VAULTWARDEN_ADMIN_TOKEN", "")

server = Server("vaultwarden")
_client: httpx.AsyncClient | None = None
_session_cookie: str | None = None


async def get_client() -> httpx.AsyncClient:
    global _client, _session_cookie
    if _client is None:
        _client = httpx.AsyncClient(base_url=VAULTWARDEN_URL, timeout=30)
    if not _session_cookie:
        r = await _client.post("/admin/", data={"token": VAULTWARDEN_ADMIN_TOKEN})
        r.raise_for_status()
        _session_cookie = "; ".join(f"{k}={v}" for k, v in r.cookies.items())
    return _client


def _admin_headers() -> dict:
    return {"Cookie": _session_cookie or "", "Content-Type": "application/json"}


async def vw_get(path: str) -> Any:
    c = await get_client()
    r = await c.get(path, headers=_admin_headers())
    r.raise_for_status()
    return r.json()


async def vw_post(path: str, body: dict | None = None) -> Any:
    c = await get_client()
    r = await c.post(path, json=body or {}, headers=_admin_headers())
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def vw_delete(path: str) -> Any:
    c = await get_client()
    r = await c.delete(path, headers=_admin_headers())
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="vaultwarden_list_users",
            description="List all registered Vaultwarden users with vault stats (item counts, 2FA status)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vaultwarden_get_user",
            description="Get details for a specific user by UUID",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "User UUID (from vaultwarden_list_users)"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="vaultwarden_invite_user",
            description="Send an invitation email to a new user",
            inputSchema={
                "type": "object",
                "properties": {
                    "email": {"type": "string", "description": "Email address to invite"},
                },
                "required": ["email"],
            },
        ),
        types.Tool(
            name="vaultwarden_deactivate_user",
            description="Deactivate a user (prevents login without deleting vault)",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "User UUID"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="vaultwarden_reactivate_user",
            description="Reactivate a previously deactivated user",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "User UUID"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="vaultwarden_delete_user",
            description="Permanently delete a user and their vault",
            inputSchema={
                "type": "object",
                "properties": {
                    "uuid": {"type": "string", "description": "User UUID"},
                },
                "required": ["uuid"],
            },
        ),
        types.Tool(
            name="vaultwarden_list_organizations",
            description="List all organizations (shared vaults) on this instance",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vaultwarden_get_config",
            description="Get current Vaultwarden server configuration (non-sensitive settings)",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "vaultwarden_list_users":
                return _text(await vw_get("/admin/users"))

            case "vaultwarden_get_user":
                return _text(await vw_get(f"/admin/users/{arguments['uuid']}"))

            case "vaultwarden_invite_user":
                return _text(await vw_post("/admin/invite", {"email": arguments["email"]}))

            case "vaultwarden_deactivate_user":
                return _text(await vw_post(f"/admin/users/{arguments['uuid']}/deactivate"))

            case "vaultwarden_reactivate_user":
                return _text(await vw_post(f"/admin/users/{arguments['uuid']}/activate"))

            case "vaultwarden_delete_user":
                return _text(await vw_delete(f"/admin/users/{arguments['uuid']}"))

            case "vaultwarden_list_organizations":
                return _text(await vw_get("/admin/organizations"))

            case "vaultwarden_get_config":
                return _text(await vw_get("/admin/config"))

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
