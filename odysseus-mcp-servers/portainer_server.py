#!/usr/bin/env python3
"""
Portainer MCP Server — manage Docker environments, stacks, containers, and volumes via Portainer.

Setup:
  pip install mcp httpx
  PORTAINER_URL=http://localhost:9000 PORTAINER_TOKEN=<token> python portainer_server.py

Get a token: Portainer → Account → Access tokens → Add access token
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

PORTAINER_URL = os.environ.get("PORTAINER_URL", "http://localhost:9000").rstrip("/")
PORTAINER_TOKEN = os.environ.get("PORTAINER_TOKEN", "")

server = Server("portainer")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=PORTAINER_URL,
            headers={"X-API-Key": PORTAINER_TOKEN},
            timeout=30,
        )
    return _client


async def pt_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def pt_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
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
            name="portainer_list_endpoints",
            description="List all Docker environments (local socket, remote agents, Kubernetes clusters)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="portainer_get_endpoint",
            description="Get details for a specific Docker environment by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "endpoint_id": {"type": "integer", "description": "Environment ID (from portainer_list_endpoints)"},
                },
                "required": ["endpoint_id"],
            },
        ),
        types.Tool(
            name="portainer_list_stacks",
            description="List all Compose stacks across all environments",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="portainer_get_stack",
            description="Get details for a specific stack by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "stack_id": {"type": "integer", "description": "Stack ID"},
                },
                "required": ["stack_id"],
            },
        ),
        types.Tool(
            name="portainer_get_stack_file",
            description="Get the Docker Compose file content for a stack",
            inputSchema={
                "type": "object",
                "properties": {
                    "stack_id": {"type": "integer", "description": "Stack ID"},
                },
                "required": ["stack_id"],
            },
        ),
        types.Tool(
            name="portainer_redeploy_stack",
            description="Redeploy a git-backed stack (pull latest and restart)",
            inputSchema={
                "type": "object",
                "properties": {
                    "stack_id": {"type": "integer", "description": "Stack ID"},
                    "pull_image": {"type": "boolean", "default": True, "description": "Pull latest images before redeploying"},
                    "prune": {"type": "boolean", "default": False, "description": "Remove services not in compose file"},
                },
                "required": ["stack_id"],
            },
        ),
        types.Tool(
            name="portainer_list_containers",
            description="List all containers in a Docker environment",
            inputSchema={
                "type": "object",
                "properties": {
                    "endpoint_id": {"type": "integer", "description": "Environment ID"},
                    "all": {"type": "boolean", "default": True, "description": "Include stopped containers"},
                },
                "required": ["endpoint_id"],
            },
        ),
        types.Tool(
            name="portainer_get_container_logs",
            description="Get recent log lines from a container",
            inputSchema={
                "type": "object",
                "properties": {
                    "endpoint_id": {"type": "integer", "description": "Environment ID"},
                    "container_id": {"type": "string", "description": "Container ID"},
                    "tail": {"type": "integer", "default": 100},
                },
                "required": ["endpoint_id", "container_id"],
            },
        ),
        types.Tool(
            name="portainer_list_images",
            description="List Docker images in an environment",
            inputSchema={
                "type": "object",
                "properties": {
                    "endpoint_id": {"type": "integer", "description": "Environment ID"},
                },
                "required": ["endpoint_id"],
            },
        ),
        types.Tool(
            name="portainer_list_volumes",
            description="List Docker volumes in an environment",
            inputSchema={
                "type": "object",
                "properties": {
                    "endpoint_id": {"type": "integer", "description": "Environment ID"},
                },
                "required": ["endpoint_id"],
            },
        ),
        types.Tool(
            name="portainer_list_networks",
            description="List Docker networks in an environment",
            inputSchema={
                "type": "object",
                "properties": {
                    "endpoint_id": {"type": "integer", "description": "Environment ID"},
                },
                "required": ["endpoint_id"],
            },
        ),
        types.Tool(
            name="portainer_list_users",
            description="List all Portainer users (admin only)",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "portainer_list_endpoints":
                return _text(await pt_get("/api/endpoints"))

            case "portainer_get_endpoint":
                return _text(await pt_get(f"/api/endpoints/{arguments['endpoint_id']}"))

            case "portainer_list_stacks":
                return _text(await pt_get("/api/stacks"))

            case "portainer_get_stack":
                return _text(await pt_get(f"/api/stacks/{arguments['stack_id']}"))

            case "portainer_get_stack_file":
                return _text(await pt_get(f"/api/stacks/{arguments['stack_id']}/file"))

            case "portainer_redeploy_stack":
                return _text(await pt_post(f"/api/stacks/{arguments['stack_id']}/git/redeploy", {
                    "prune": arguments.get("prune", False),
                    "pullImage": arguments.get("pull_image", True),
                    "env": [],
                }))

            case "portainer_list_containers":
                eid = arguments["endpoint_id"]
                return _text(await pt_get(
                    f"/api/endpoints/{eid}/docker/containers/json",
                    params={"all": str(arguments.get("all", True)).lower()},
                ))

            case "portainer_get_container_logs":
                eid = arguments["endpoint_id"]
                cid = arguments["container_id"]
                tail = arguments.get("tail", 100)
                r = await client().get(
                    f"/api/endpoints/{eid}/docker/containers/{cid}/logs",
                    params={"stdout": "true", "stderr": "true", "tail": tail},
                )
                r.raise_for_status()
                return [types.TextContent(type="text", text=r.text)]

            case "portainer_list_images":
                eid = arguments["endpoint_id"]
                return _text(await pt_get(f"/api/endpoints/{eid}/docker/images/json"))

            case "portainer_list_volumes":
                eid = arguments["endpoint_id"]
                return _text(await pt_get(f"/api/endpoints/{eid}/docker/volumes"))

            case "portainer_list_networks":
                eid = arguments["endpoint_id"]
                return _text(await pt_get(f"/api/endpoints/{eid}/docker/networks"))

            case "portainer_list_users":
                return _text(await pt_get("/api/users"))

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
