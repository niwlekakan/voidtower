#!/usr/bin/env python3
"""
Authentik MCP Server — manage users, groups, applications, and flows in Authentik.

Setup:
  pip install mcp httpx
  AUTHENTIK_URL=http://localhost:9000 AUTHENTIK_TOKEN=<token> python authentik_server.py

Get a token: Authentik Admin UI → Directory → Tokens → Create (intent: API)
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

AUTHENTIK_URL = os.environ.get("AUTHENTIK_URL", "http://localhost:9000").rstrip("/")
AUTHENTIK_TOKEN = os.environ.get("AUTHENTIK_TOKEN", "")

server = Server("authentik")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=AUTHENTIK_URL,
            headers={"Authorization": f"Bearer {AUTHENTIK_TOKEN}"},
            timeout=30,
        )
    return _client


async def ak_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def ak_post(path: str, body: dict) -> Any:
    r = await client().post(path, json=body)
    r.raise_for_status()
    return r.json()


async def ak_patch(path: str, body: dict) -> Any:
    r = await client().patch(path, json=body)
    r.raise_for_status()
    return r.json()


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="authentik_get_system_info",
            description="Get Authentik system metrics, worker status, and version info",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="authentik_list_users",
            description="List users with optional search filter",
            inputSchema={
                "type": "object",
                "properties": {
                    "search": {"type": "string", "description": "Filter by username or email"},
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="authentik_get_user",
            description="Get details for a specific user by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "integer", "description": "User ID"},
                },
                "required": ["user_id"],
            },
        ),
        types.Tool(
            name="authentik_create_user",
            description="Create a new Authentik user",
            inputSchema={
                "type": "object",
                "properties": {
                    "username": {"type": "string"},
                    "name": {"type": "string", "description": "Display name"},
                    "email": {"type": "string"},
                    "is_active": {"type": "boolean", "default": True},
                    "path": {"type": "string", "default": "users"},
                },
                "required": ["username", "name", "email"],
            },
        ),
        types.Tool(
            name="authentik_update_user",
            description="Update a user's active status or group memberships",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "integer"},
                    "is_active": {"type": "boolean"},
                    "name": {"type": "string"},
                    "email": {"type": "string"},
                },
                "required": ["user_id"],
            },
        ),
        types.Tool(
            name="authentik_reset_user_password",
            description="Set a new password for a user",
            inputSchema={
                "type": "object",
                "properties": {
                    "user_id": {"type": "integer"},
                    "password": {"type": "string"},
                },
                "required": ["user_id", "password"],
            },
        ),
        types.Tool(
            name="authentik_list_groups",
            description="List all groups with member counts",
            inputSchema={
                "type": "object",
                "properties": {
                    "search": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="authentik_get_group",
            description="Get details for a specific group by primary key (UUID)",
            inputSchema={
                "type": "object",
                "properties": {
                    "pk": {"type": "string", "description": "Group UUID"},
                },
                "required": ["pk"],
            },
        ),
        types.Tool(
            name="authentik_create_group",
            description="Create a new group",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "is_superuser": {"type": "boolean", "default": False},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="authentik_list_applications",
            description="List all configured applications (SSO apps, proxy apps, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "search": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="authentik_get_application",
            description="Get details for a specific application by slug",
            inputSchema={
                "type": "object",
                "properties": {
                    "slug": {"type": "string", "description": "Application slug"},
                },
                "required": ["slug"],
            },
        ),
        types.Tool(
            name="authentik_list_providers",
            description="List all authentication providers (OAuth2, SAML, LDAP, proxy, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="authentik_list_flows",
            description="List all authentication/enrollment/recovery flows",
            inputSchema={
                "type": "object",
                "properties": {
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 25},
                },
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "authentik_get_system_info":
                return _text(await ak_get("/api/v3/admin/system/"))

            case "authentik_list_users":
                params: dict = {"page": arguments.get("page", 1), "page_size": arguments.get("page_size", 25)}
                if s := arguments.get("search"):
                    params["search"] = s
                return _text(await ak_get("/api/v3/core/users/", params=params))

            case "authentik_get_user":
                return _text(await ak_get(f"/api/v3/core/users/{arguments['user_id']}/"))

            case "authentik_create_user":
                return _text(await ak_post("/api/v3/core/users/", {
                    "username": arguments["username"],
                    "name": arguments["name"],
                    "email": arguments["email"],
                    "is_active": arguments.get("is_active", True),
                    "groups": [],
                    "path": arguments.get("path", "users"),
                }))

            case "authentik_update_user":
                body: dict = {}
                for field in ("is_active", "name", "email"):
                    if field in arguments:
                        body[field] = arguments[field]
                return _text(await ak_patch(f"/api/v3/core/users/{arguments['user_id']}/", body))

            case "authentik_reset_user_password":
                return _text(await ak_post(
                    f"/api/v3/core/users/{arguments['user_id']}/set_password/",
                    {"password": arguments["password"]},
                ))

            case "authentik_list_groups":
                params = {"page": arguments.get("page", 1), "page_size": arguments.get("page_size", 25)}
                if s := arguments.get("search"):
                    params["search"] = s
                return _text(await ak_get("/api/v3/core/groups/", params=params))

            case "authentik_get_group":
                return _text(await ak_get(f"/api/v3/core/groups/{arguments['pk']}/"))

            case "authentik_create_group":
                return _text(await ak_post("/api/v3/core/groups/", {
                    "name": arguments["name"],
                    "is_superuser": arguments.get("is_superuser", False),
                    "users": [],
                }))

            case "authentik_list_applications":
                params = {"page": arguments.get("page", 1), "page_size": arguments.get("page_size", 25)}
                if s := arguments.get("search"):
                    params["search"] = s
                return _text(await ak_get("/api/v3/core/applications/", params=params))

            case "authentik_get_application":
                return _text(await ak_get(f"/api/v3/core/applications/{arguments['slug']}/"))

            case "authentik_list_providers":
                return _text(await ak_get("/api/v3/providers/all/", params={
                    "page": arguments.get("page", 1),
                    "page_size": arguments.get("page_size", 25),
                }))

            case "authentik_list_flows":
                return _text(await ak_get("/api/v3/flows/instances/", params={
                    "page": arguments.get("page", 1),
                    "page_size": arguments.get("page_size", 25),
                }))

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
