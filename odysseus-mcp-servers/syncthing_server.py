#!/usr/bin/env python3
"""
Syncthing MCP Server — manage folders, devices, and sync status on a Syncthing instance.

Setup:
  pip install mcp httpx
  SYNCTHING_URL=http://localhost:8384 SYNCTHING_API_KEY=<key> python syncthing_server.py

Get API key: Syncthing Web UI → Actions → Settings → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

SYNCTHING_URL = os.environ.get("SYNCTHING_URL", "http://localhost:8384").rstrip("/")
SYNCTHING_API_KEY = os.environ.get("SYNCTHING_API_KEY", "")

server = Server("syncthing")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=SYNCTHING_URL,
            headers={"X-API-Key": SYNCTHING_API_KEY},
            timeout=30,
        )
    return _client


async def st_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def st_post(path: str, params: dict | None = None, body: dict | None = None) -> Any:
    r = await client().post(path, params=params, json=body)
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="syncthing_get_status",
            description="Get Syncthing system status: version, uptime, goroutines, memory usage, and system name",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="syncthing_get_my_id",
            description="Get the device ID of this Syncthing instance (needed to share with peers)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="syncthing_get_connections",
            description="Get all connected peer devices with their transfer rates, total bytes, and connection address",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="syncthing_list_devices",
            description="List all configured Syncthing peer devices with their names and addresses",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="syncthing_add_device",
            description="Add a new peer device to share folders with",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device ID (from the peer's syncthing_get_my_id)"},
                    "name": {"type": "string", "description": "Display name for the device"},
                    "addresses": {
                        "type": "array",
                        "items": {"type": "string"},
                        "default": ["dynamic"],
                        "description": "Connection addresses, use [\"dynamic\"] for auto-discovery",
                    },
                },
                "required": ["device_id", "name"],
            },
        ),
        types.Tool(
            name="syncthing_list_folders",
            description="List all configured sync folders with their path, shared devices, and sync state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="syncthing_get_folder_status",
            description="Get detailed sync status for a folder: in-sync files, out-of-sync, errors, last scan time",
            inputSchema={
                "type": "object",
                "properties": {
                    "folder_id": {"type": "string", "description": "Folder ID from syncthing_list_folders"},
                },
                "required": ["folder_id"],
            },
        ),
        types.Tool(
            name="syncthing_rescan_folder",
            description="Trigger an immediate rescan of a folder to detect local changes",
            inputSchema={
                "type": "object",
                "properties": {
                    "folder_id": {"type": "string", "description": "Folder ID"},
                    "sub_path": {"type": "string", "description": "Optional sub-path within the folder to rescan"},
                },
                "required": ["folder_id"],
            },
        ),
        types.Tool(
            name="syncthing_pause_device",
            description="Pause syncing with a specific peer device",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device ID"},
                },
                "required": ["device_id"],
            },
        ),
        types.Tool(
            name="syncthing_resume_device",
            description="Resume syncing with a paused peer device",
            inputSchema={
                "type": "object",
                "properties": {
                    "device_id": {"type": "string", "description": "Device ID"},
                },
                "required": ["device_id"],
            },
        ),
        types.Tool(
            name="syncthing_get_errors",
            description="Get recent Syncthing errors and warnings",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "syncthing_get_status":
                return _text(await st_get("/rest/system/status"))

            case "syncthing_get_my_id":
                return _text(await st_get("/rest/system/myid"))

            case "syncthing_get_connections":
                return _text(await st_get("/rest/system/connections"))

            case "syncthing_list_devices":
                return _text(await st_get("/rest/config/devices"))

            case "syncthing_add_device":
                devices = await st_get("/rest/config/devices")
                devices.append({
                    "deviceID": arguments["device_id"],
                    "name": arguments["name"],
                    "addresses": arguments.get("addresses", ["dynamic"]),
                    "autoAcceptFolders": False,
                    "paused": False,
                })
                r = await client().put("/rest/config/devices", json=devices)
                r.raise_for_status()
                return _text({"added": arguments["device_id"]})

            case "syncthing_list_folders":
                return _text(await st_get("/rest/config/folders"))

            case "syncthing_get_folder_status":
                return _text(await st_get("/rest/db/status", params={"folder": arguments["folder_id"]}))

            case "syncthing_rescan_folder":
                params: dict = {"folder": arguments["folder_id"]}
                if sub := arguments.get("sub_path"):
                    params["sub"] = sub
                return _text(await st_post("/rest/db/scan", params=params))

            case "syncthing_pause_device":
                return _text(await st_post("/rest/system/pause", params={"device": arguments["device_id"]}))

            case "syncthing_resume_device":
                return _text(await st_post("/rest/system/resume", params={"device": arguments["device_id"]}))

            case "syncthing_get_errors":
                return _text(await st_get("/rest/system/error"))

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
