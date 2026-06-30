#!/usr/bin/env python3
"""
MinIO MCP Server — manage buckets, objects, and users via the MinIO Console API.

Setup:
  pip install mcp httpx
  MINIO_ACCESS_KEY=minioadmin MINIO_SECRET_KEY=<key> python minio_server.py

Env vars:
  MINIO_CONSOLE_URL  MinIO Console URL (default: http://localhost:9001)
  MINIO_ACCESS_KEY   Access key / root user
  MINIO_SECRET_KEY   Secret key / root password
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

MINIO_CONSOLE_URL = os.environ.get("MINIO_CONSOLE_URL", "http://localhost:9001").rstrip("/")
MINIO_ACCESS_KEY = os.environ.get("MINIO_ACCESS_KEY", "")
MINIO_SECRET_KEY = os.environ.get("MINIO_SECRET_KEY", "")

server = Server("minio")
_client: httpx.AsyncClient | None = None


async def _ensure_auth() -> None:
    global _client
    async with httpx.AsyncClient(base_url=MINIO_CONSOLE_URL, timeout=30) as c:
        r = await c.post("/api/v1/login", json={
            "accessKey": MINIO_ACCESS_KEY,
            "secretKey": MINIO_SECRET_KEY,
        })
        r.raise_for_status()
        token = r.json().get("sessionId", "")
    _client = httpx.AsyncClient(
        base_url=MINIO_CONSOLE_URL,
        headers={"Cookie": f"token={token}"},
        timeout=30,
    )


def client() -> httpx.AsyncClient:
    if _client is None:
        raise RuntimeError("Not authenticated")
    return _client


async def mn_get(path: str, params: dict | None = None) -> Any:
    if _client is None:
        await _ensure_auth()
    r = await client().get(path, params=params)
    if r.status_code == 401:
        await _ensure_auth()
        r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def mn_post(path: str, body: dict | None = None) -> Any:
    if _client is None:
        await _ensure_auth()
    r = await client().post(path, json=body or {})
    if r.status_code == 401:
        await _ensure_auth()
        r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def mn_delete(path: str) -> Any:
    if _client is None:
        await _ensure_auth()
    r = await client().delete(path)
    if r.status_code == 401:
        await _ensure_auth()
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
            name="minio_get_info",
            description="Get MinIO server info: storage usage, bucket count, object count, and server nodes",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="minio_list_buckets",
            description="List all buckets with their size, object count, and access policy",
            inputSchema={
                "type": "object",
                "properties": {
                    "offset": {"type": "integer", "default": 0},
                    "limit": {"type": "integer", "default": 20},
                },
            },
        ),
        types.Tool(
            name="minio_get_bucket",
            description="Get details for a specific bucket including size, versioning status, and access policy",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Bucket name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="minio_create_bucket",
            description="Create a new bucket",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Bucket name (must be globally unique, lowercase, no spaces)"},
                    "versioning": {"type": "boolean", "default": False, "description": "Enable object versioning"},
                    "locking": {"type": "boolean", "default": False, "description": "Enable object locking (requires versioning)"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="minio_delete_bucket",
            description="Delete an empty bucket permanently",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Bucket name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="minio_list_objects",
            description="List objects inside a bucket, optionally filtered by prefix",
            inputSchema={
                "type": "object",
                "properties": {
                    "bucket": {"type": "string", "description": "Bucket name"},
                    "prefix": {"type": "string", "default": "", "description": "Object key prefix to filter by"},
                    "recursive": {"type": "boolean", "default": True, "description": "List recursively into prefixes/folders"},
                },
                "required": ["bucket"],
            },
        ),
        types.Tool(
            name="minio_list_users",
            description="List all MinIO users with their status and assigned policies",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="minio_add_user",
            description="Create a new MinIO user with an access key, secret key, and policy",
            inputSchema={
                "type": "object",
                "properties": {
                    "accessKey": {"type": "string", "description": "Username / access key"},
                    "secretKey": {"type": "string", "description": "Password / secret key (min 8 chars)"},
                    "policies": {
                        "type": "array",
                        "items": {"type": "string"},
                        "default": ["readwrite"],
                        "description": "Policy names to assign (e.g. readwrite, readonly, writeonly, diagnostics)",
                    },
                },
                "required": ["accessKey", "secretKey"],
            },
        ),
        types.Tool(
            name="minio_delete_user",
            description="Delete a MinIO user by access key",
            inputSchema={
                "type": "object",
                "properties": {
                    "accessKey": {"type": "string"},
                },
                "required": ["accessKey"],
            },
        ),
        types.Tool(
            name="minio_list_service_accounts",
            description="List service account keys (application credentials) associated with the current user",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "minio_get_info":
                return _text(await mn_get("/api/v1/admin/info"))

            case "minio_list_buckets":
                return _text(await mn_get("/api/v1/buckets", params={
                    "offset": arguments.get("offset", 0),
                    "limit": arguments.get("limit", 20),
                }))

            case "minio_get_bucket":
                return _text(await mn_get(f"/api/v1/buckets/{arguments['name']}"))

            case "minio_create_bucket":
                return _text(await mn_post("/api/v1/buckets", {
                    "name": arguments["name"],
                    "versioning": {"enabled": arguments.get("versioning", False)},
                    "locking": arguments.get("locking", False),
                }))

            case "minio_delete_bucket":
                return _text(await mn_delete(f"/api/v1/buckets/{arguments['name']}"))

            case "minio_list_objects":
                return _text(await mn_get(f"/api/v1/buckets/{arguments['bucket']}/objects", params={
                    "prefix": arguments.get("prefix", ""),
                    "recursive": str(arguments.get("recursive", True)).lower(),
                    "with_versions": "false",
                }))

            case "minio_list_users":
                return _text(await mn_get("/api/v1/users"))

            case "minio_add_user":
                return _text(await mn_post("/api/v1/users", {
                    "accessKey": arguments["accessKey"],
                    "secretKey": arguments["secretKey"],
                    "groups": [],
                    "policies": arguments.get("policies", ["readwrite"]),
                }))

            case "minio_delete_user":
                return _text(await mn_delete(f"/api/v1/users/{arguments['accessKey']}"))

            case "minio_list_service_accounts":
                return _text(await mn_get("/api/v1/service-accounts"))

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
