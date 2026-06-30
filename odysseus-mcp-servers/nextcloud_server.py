#!/usr/bin/env python3
"""
Nextcloud MCP Server — manage users, groups, shares, and files in Nextcloud.

Setup:
  pip install mcp httpx
  NEXTCLOUD_URL=http://localhost NEXTCLOUD_USER=admin NEXTCLOUD_PASSWORD=<pw> python nextcloud_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

NEXTCLOUD_URL = os.environ.get("NEXTCLOUD_URL", "http://localhost").rstrip("/")
NEXTCLOUD_USER = os.environ.get("NEXTCLOUD_USER", "admin")
NEXTCLOUD_PASSWORD = os.environ.get("NEXTCLOUD_PASSWORD", "")

OCS_HEADERS = {"OCS-APIRequest": "true", "Accept": "application/json"}

server = Server("nextcloud")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=NEXTCLOUD_URL,
            auth=(NEXTCLOUD_USER, NEXTCLOUD_PASSWORD),
            headers=OCS_HEADERS,
            timeout=30,
        )
    return _client


async def nc_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def nc_post(path: str, data: dict | None = None) -> Any:
    r = await client().post(path, data=data or {})
    r.raise_for_status()
    return r.json()


async def nc_put(path: str) -> Any:
    r = await client().put(path)
    r.raise_for_status()
    return r.json()


async def nc_propfind(path: str) -> str:
    r = await client().request("PROPFIND", path, headers={"Depth": "1"})
    r.raise_for_status()
    return r.text


def _ocs(data: Any) -> list[types.TextContent]:
    # Unwrap OCS envelope if present
    if isinstance(data, dict) and "ocs" in data:
        data = data["ocs"].get("data", data["ocs"])
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="nextcloud_get_status",
            description="Get Nextcloud server status: version, installed state, maintenance mode",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="nextcloud_get_capabilities",
            description="Get server capabilities: enabled features, app versions, quota limits",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="nextcloud_list_users",
            description="List all users on the Nextcloud instance",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 25},
                    "offset": {"type": "integer", "default": 0},
                    "search": {"type": "string"},
                },
            },
        ),
        types.Tool(
            name="nextcloud_get_user",
            description="Get details for a specific user including quota and storage usage",
            inputSchema={
                "type": "object",
                "properties": {
                    "userid": {"type": "string", "description": "Nextcloud username"},
                },
                "required": ["userid"],
            },
        ),
        types.Tool(
            name="nextcloud_create_user",
            description="Create a new Nextcloud user",
            inputSchema={
                "type": "object",
                "properties": {
                    "userid": {"type": "string"},
                    "password": {"type": "string"},
                    "displayName": {"type": "string"},
                    "email": {"type": "string"},
                },
                "required": ["userid", "password"],
            },
        ),
        types.Tool(
            name="nextcloud_disable_user",
            description="Disable a Nextcloud user (prevents login without deleting)",
            inputSchema={
                "type": "object",
                "properties": {
                    "userid": {"type": "string"},
                },
                "required": ["userid"],
            },
        ),
        types.Tool(
            name="nextcloud_enable_user",
            description="Re-enable a disabled Nextcloud user",
            inputSchema={
                "type": "object",
                "properties": {
                    "userid": {"type": "string"},
                },
                "required": ["userid"],
            },
        ),
        types.Tool(
            name="nextcloud_list_groups",
            description="List all groups on the Nextcloud instance",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="nextcloud_list_apps",
            description="List installed and enabled Nextcloud apps",
            inputSchema={
                "type": "object",
                "properties": {
                    "filter": {"type": "string", "enum": ["enabled", "disabled"], "default": "enabled"},
                },
            },
        ),
        types.Tool(
            name="nextcloud_list_shares",
            description="List active shares created by the authenticated user",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="nextcloud_create_public_link",
            description="Create a public share link for a file or folder",
            inputSchema={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Path to the file or folder (e.g. /Documents/report.pdf)"},
                    "permissions": {"type": "integer", "default": 1, "description": "1=read, 3=read+write"},
                },
                "required": ["path"],
            },
        ),
        types.Tool(
            name="nextcloud_list_files",
            description="List files and folders at a path using WebDAV",
            inputSchema={
                "type": "object",
                "properties": {
                    "path": {"type": "string", "default": "/", "description": "Directory path (relative to user root)"},
                },
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "nextcloud_get_status":
                return _text(await nc_get("/status.php"))

            case "nextcloud_get_capabilities":
                return _ocs(await nc_get("/ocs/v2.php/cloud/capabilities"))

            case "nextcloud_list_users":
                params: dict = {
                    "limit": arguments.get("limit", 25),
                    "offset": arguments.get("offset", 0),
                }
                if s := arguments.get("search"):
                    params["search"] = s
                return _ocs(await nc_get("/ocs/v2.php/cloud/users", params=params))

            case "nextcloud_get_user":
                return _ocs(await nc_get(f"/ocs/v2.php/cloud/users/{arguments['userid']}"))

            case "nextcloud_create_user":
                data: dict = {"userid": arguments["userid"], "password": arguments["password"]}
                if dn := arguments.get("displayName"):
                    data["displayName"] = dn
                if em := arguments.get("email"):
                    data["email"] = em
                return _ocs(await nc_post("/ocs/v2.php/cloud/users", data))

            case "nextcloud_disable_user":
                return _ocs(await nc_put(f"/ocs/v2.php/cloud/users/{arguments['userid']}/disable"))

            case "nextcloud_enable_user":
                return _ocs(await nc_put(f"/ocs/v2.php/cloud/users/{arguments['userid']}/enable"))

            case "nextcloud_list_groups":
                return _ocs(await nc_get("/ocs/v2.php/cloud/groups"))

            case "nextcloud_list_apps":
                return _ocs(await nc_get("/ocs/v2.php/cloud/apps", params={"filter": arguments.get("filter", "enabled")}))

            case "nextcloud_list_shares":
                return _ocs(await nc_get("/ocs/v2.php/apps/files_sharing/api/v1/shares"))

            case "nextcloud_create_public_link":
                return _ocs(await nc_post("/ocs/v2.php/apps/files_sharing/api/v1/shares", {
                    "path": arguments["path"],
                    "shareType": "3",
                    "permissions": str(arguments.get("permissions", 1)),
                }))

            case "nextcloud_list_files":
                path = arguments.get("path", "/").lstrip("/")
                xml = await nc_propfind(f"/remote.php/dav/files/{NEXTCLOUD_USER}/{path}")
                return [types.TextContent(type="text", text=xml)]

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
