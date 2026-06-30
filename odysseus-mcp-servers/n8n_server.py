#!/usr/bin/env python3
"""
n8n MCP Server — manage and trigger n8n automation workflows.

Setup:
  pip install mcp httpx
  N8N_URL=http://localhost:5678 N8N_API_KEY=<key> python n8n_server.py

Get an API key: n8n → Settings → API → Create an API key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

N8N_URL = os.environ.get("N8N_URL", "http://localhost:5678").rstrip("/")
N8N_API_KEY = os.environ.get("N8N_API_KEY", "")

server = Server("n8n")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{N8N_URL}/api/v1",
            headers={"X-N8N-API-KEY": N8N_API_KEY},
            timeout=30,
        )
    return _client


async def n8_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def n8_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def n8_patch(path: str, body: dict) -> Any:
    r = await client().patch(path, json=body)
    r.raise_for_status()
    return r.json()


async def n8_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="n8n_list_workflows",
            description="List all workflows with their active/inactive status",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 25},
                    "active": {"type": "boolean", "description": "Filter by active state (omit for all)"},
                    "tags": {"type": "string", "description": "Comma-separated tag names to filter by"},
                },
            },
        ),
        types.Tool(
            name="n8n_get_workflow",
            description="Get full details and node definitions for a workflow",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Workflow ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="n8n_activate_workflow",
            description="Activate a workflow so it runs on its trigger",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="n8n_deactivate_workflow",
            description="Deactivate a workflow (stops it from running on its trigger)",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="n8n_execute_workflow",
            description="Trigger a manual execution of a workflow",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "data": {"type": "object", "description": "Optional input data to pass to the workflow"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="n8n_list_executions",
            description="List recent workflow executions with their status",
            inputSchema={
                "type": "object",
                "properties": {
                    "workflow_id": {"type": "string", "description": "Filter by workflow ID (omit for all)"},
                    "status": {"type": "string", "enum": ["success", "error", "running", "waiting"], "description": "Filter by status"},
                    "limit": {"type": "integer", "default": 20},
                },
            },
        ),
        types.Tool(
            name="n8n_get_execution",
            description="Get details of a specific execution",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Execution ID"},
                    "include_data": {"type": "boolean", "default": False, "description": "Include full node I/O data (can be large)"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="n8n_delete_execution",
            description="Delete an execution record",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="n8n_list_credentials",
            description="List credential names and types (values are never returned)",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="n8n_list_tags",
            description="List all workflow tags",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "n8n_list_workflows":
                params: dict = {"limit": arguments.get("limit", 25)}
                if "active" in arguments:
                    params["active"] = str(arguments["active"]).lower()
                if tags := arguments.get("tags"):
                    params["tags"] = tags
                return _text(await n8_get("/workflows", params=params))

            case "n8n_get_workflow":
                return _text(await n8_get(f"/workflows/{arguments['id']}"))

            case "n8n_activate_workflow":
                return _text(await n8_patch(f"/workflows/{arguments['id']}", {"active": True}))

            case "n8n_deactivate_workflow":
                return _text(await n8_patch(f"/workflows/{arguments['id']}", {"active": False}))

            case "n8n_execute_workflow":
                body: dict = {}
                if data := arguments.get("data"):
                    body = data
                return _text(await n8_post(f"/workflows/{arguments['id']}/run", body))

            case "n8n_list_executions":
                params = {"limit": arguments.get("limit", 20), "includeData": False}
                if wid := arguments.get("workflow_id"):
                    params["workflowId"] = wid
                if s := arguments.get("status"):
                    params["status"] = s
                return _text(await n8_get("/executions", params=params))

            case "n8n_get_execution":
                params = {"includeData": str(arguments.get("include_data", False)).lower()}
                return _text(await n8_get(f"/executions/{arguments['id']}", params=params))

            case "n8n_delete_execution":
                return _text(await n8_delete(f"/executions/{arguments['id']}"))

            case "n8n_list_credentials":
                return _text(await n8_get("/credentials", params={"limit": arguments.get("limit", 25)}))

            case "n8n_list_tags":
                return _text(await n8_get("/tags"))

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
