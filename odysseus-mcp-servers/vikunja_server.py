#!/usr/bin/env python3
"""
Vikunja MCP Server — manage projects, tasks, and labels.

Setup:
  pip install mcp httpx
  VIKUNJA_URL=http://localhost:3456 VIKUNJA_TOKEN=<token> python vikunja_server.py

Get a token: Vikunja → Settings → API Tokens → Create
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

VIKUNJA_URL = os.environ.get("VIKUNJA_URL", "http://localhost:3456").rstrip("/")
VIKUNJA_TOKEN = os.environ.get("VIKUNJA_TOKEN", "")

server = Server("vikunja")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{VIKUNJA_URL}/api/v1",
            headers={"Authorization": f"Bearer {VIKUNJA_TOKEN}"},
            timeout=30,
        )
    return _client


async def vk_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def vk_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    return r.json()


async def vk_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="vikunja_list_projects",
            description="List all projects (task lists)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vikunja_get_project",
            description="Get details for a specific project",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Project ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vikunja_create_project",
            description="Create a new project",
            inputSchema={
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "color": {"type": "string", "description": "Hex color e.g. #FF0000"},
                },
                "required": ["title"],
            },
        ),
        types.Tool(
            name="vikunja_list_tasks",
            description="List tasks in a project",
            inputSchema={
                "type": "object",
                "properties": {
                    "project_id": {"type": "integer"},
                    "page": {"type": "integer", "default": 1},
                    "per_page": {"type": "integer", "default": 25},
                    "done": {"type": "boolean", "description": "Filter by done status (omit for all)"},
                },
                "required": ["project_id"],
            },
        ),
        types.Tool(
            name="vikunja_get_task",
            description="Get a specific task by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Task ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vikunja_create_task",
            description="Create a new task in a project",
            inputSchema={
                "type": "object",
                "properties": {
                    "project_id": {"type": "integer"},
                    "title": {"type": "string"},
                    "description": {"type": "string"},
                    "due_date": {"type": "string", "description": "ISO 8601 datetime e.g. 2024-12-31T00:00:00Z"},
                    "priority": {"type": "integer", "minimum": 0, "maximum": 5, "description": "0=unset 1=low 2=medium 3=high 4=urgent 5=do now"},
                },
                "required": ["project_id", "title"],
            },
        ),
        types.Tool(
            name="vikunja_update_task",
            description="Update a task's title, done state, due date, or priority",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "title": {"type": "string"},
                    "done": {"type": "boolean"},
                    "due_date": {"type": "string", "description": "ISO 8601 datetime"},
                    "priority": {"type": "integer", "minimum": 0, "maximum": 5},
                    "description": {"type": "string"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vikunja_delete_task",
            description="Delete a task permanently",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vikunja_search_tasks",
            description="Search tasks across all projects",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                    "per_page": {"type": "integer", "default": 25},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="vikunja_list_labels",
            description="List all labels defined in Vikunja",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vikunja_get_stats",
            description="Get server info and statistics",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "vikunja_list_projects":
                return _text(await vk_get("/projects"))

            case "vikunja_get_project":
                return _text(await vk_get(f"/projects/{arguments['id']}"))

            case "vikunja_create_project":
                body: dict = {"title": arguments["title"]}
                if d := arguments.get("description"):
                    body["description"] = d
                if c := arguments.get("color"):
                    body["hex_color"] = c.lstrip("#")
                return _text(await vk_post("/projects", body))

            case "vikunja_list_tasks":
                params: dict = {
                    "page": arguments.get("page", 1),
                    "per_page": arguments.get("per_page", 25),
                }
                if "done" in arguments:
                    params["filter_by"] = "done"
                    params["filter_value"] = str(arguments["done"]).lower()
                return _text(await vk_get(f"/projects/{arguments['project_id']}/tasks", params=params))

            case "vikunja_get_task":
                return _text(await vk_get(f"/tasks/{arguments['id']}"))

            case "vikunja_create_task":
                body = {"title": arguments["title"]}
                if d := arguments.get("description"):
                    body["description"] = d
                if dd := arguments.get("due_date"):
                    body["due_date"] = dd
                if p := arguments.get("priority"):
                    body["priority"] = p
                return _text(await vk_post(f"/projects/{arguments['project_id']}/tasks", body))

            case "vikunja_update_task":
                task_id = arguments["id"]
                body = {k: v for k, v in arguments.items() if k != "id" and v is not None}
                return _text(await vk_post(f"/tasks/{task_id}", body))

            case "vikunja_delete_task":
                return _text(await vk_delete(f"/tasks/{arguments['id']}"))

            case "vikunja_search_tasks":
                return _text(await vk_get("/tasks/all", params={
                    "s": arguments["query"],
                    "page": arguments.get("page", 1),
                    "per_page": arguments.get("per_page", 25),
                }))

            case "vikunja_list_labels":
                return _text(await vk_get("/labels"))

            case "vikunja_get_stats":
                return _text(await vk_get("/info"))

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
