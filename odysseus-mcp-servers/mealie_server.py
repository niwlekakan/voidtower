#!/usr/bin/env python3
"""
Mealie MCP Server — manage recipes, meal plans, and shopping lists.

Setup:
  pip install mcp httpx
  MEALIE_URL=http://localhost:9000 MEALIE_TOKEN=<token> python mealie_server.py

Get a token: Mealie → Profile → API Tokens → Create
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

MEALIE_URL = os.environ.get("MEALIE_URL", "http://localhost:9000").rstrip("/")
MEALIE_TOKEN = os.environ.get("MEALIE_TOKEN", "")

server = Server("mealie")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=MEALIE_URL,
            headers={"Authorization": f"Bearer {MEALIE_TOKEN}"},
            timeout=30,
        )
    return _client


async def ml_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def ml_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


async def ml_put(path: str, body: dict) -> Any:
    r = await client().put(path, json=body)
    r.raise_for_status()
    return r.json()


async def ml_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="mealie_list_recipes",
            description="List all recipes, newest first",
            inputSchema={
                "type": "object",
                "properties": {
                    "page": {"type": "integer", "default": 1},
                    "per_page": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="mealie_search_recipes",
            description="Search recipes by name or keyword",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                    "per_page": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="mealie_get_recipe",
            description="Get full details of a recipe including ingredients and instructions",
            inputSchema={
                "type": "object",
                "properties": {
                    "slug": {"type": "string", "description": "Recipe slug (URL-safe name from recipe list)"},
                },
                "required": ["slug"],
            },
        ),
        types.Tool(
            name="mealie_create_recipe_from_url",
            description="Import and create a recipe by scraping a URL",
            inputSchema={
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL of the recipe page to scrape"},
                },
                "required": ["url"],
            },
        ),
        types.Tool(
            name="mealie_delete_recipe",
            description="Delete a recipe",
            inputSchema={
                "type": "object",
                "properties": {
                    "slug": {"type": "string"},
                },
                "required": ["slug"],
            },
        ),
        types.Tool(
            name="mealie_get_meal_plan",
            description="Get the meal plan for a date range",
            inputSchema={
                "type": "object",
                "properties": {
                    "start_date": {"type": "string", "description": "Start date YYYY-MM-DD"},
                    "end_date": {"type": "string", "description": "End date YYYY-MM-DD"},
                },
                "required": ["start_date", "end_date"],
            },
        ),
        types.Tool(
            name="mealie_add_to_meal_plan",
            description="Add a recipe or custom entry to the meal plan",
            inputSchema={
                "type": "object",
                "properties": {
                    "date": {"type": "string", "description": "Date YYYY-MM-DD"},
                    "entry_type": {"type": "string", "enum": ["breakfast", "lunch", "dinner", "side"], "default": "dinner"},
                    "recipe_id": {"type": "string", "description": "Recipe ID (from recipe list) — leave empty for a note entry"},
                    "title": {"type": "string", "description": "Custom title if not using a recipe"},
                    "text": {"type": "string", "description": "Optional note"},
                },
                "required": ["date", "entry_type"],
            },
        ),
        types.Tool(
            name="mealie_get_shopping_lists",
            description="Get all shopping lists",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="mealie_add_to_shopping_list",
            description="Add an item to a shopping list",
            inputSchema={
                "type": "object",
                "properties": {
                    "shopping_list_id": {"type": "string", "description": "Shopping list ID from mealie_get_shopping_lists"},
                    "note": {"type": "string", "description": "Item name or free-text note"},
                    "quantity": {"type": "number", "default": 1},
                    "unit": {"type": "string", "description": "Unit of measure e.g. kg, g, cups"},
                },
                "required": ["shopping_list_id", "note"],
            },
        ),
        types.Tool(
            name="mealie_list_categories",
            description="List all recipe categories",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="mealie_list_tags",
            description="List all recipe tags",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="mealie_get_recipe_suggestions",
            description="Get recipe suggestions based on available pantry ingredients",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 10},
                },
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "mealie_list_recipes":
                return _text(await ml_get("/api/recipes", params={
                    "page": arguments.get("page", 1),
                    "perPage": arguments.get("per_page", 25),
                    "orderBy": "createdAt",
                    "orderDirection": "desc",
                }))

            case "mealie_search_recipes":
                return _text(await ml_get("/api/recipes", params={
                    "search": arguments["query"],
                    "page": arguments.get("page", 1),
                    "perPage": arguments.get("per_page", 20),
                }))

            case "mealie_get_recipe":
                return _text(await ml_get(f"/api/recipes/{arguments['slug']}"))

            case "mealie_create_recipe_from_url":
                data = await ml_post("/api/recipes/create-url", {"url": arguments["url"]})
                return _text(data)

            case "mealie_delete_recipe":
                return _text(await ml_delete(f"/api/recipes/{arguments['slug']}"))

            case "mealie_get_meal_plan":
                return _text(await ml_get("/api/groups/mealplans", params={
                    "start_date": arguments["start_date"],
                    "end_date": arguments["end_date"],
                }))

            case "mealie_add_to_meal_plan":
                body: dict = {
                    "date": arguments["date"],
                    "entryType": arguments.get("entry_type", "dinner"),
                }
                if rid := arguments.get("recipe_id"):
                    body["recipeId"] = rid
                if t := arguments.get("title"):
                    body["title"] = t
                if tx := arguments.get("text"):
                    body["text"] = tx
                return _text(await ml_post("/api/groups/mealplans", body))

            case "mealie_get_shopping_lists":
                return _text(await ml_get("/api/groups/shopping/lists"))

            case "mealie_add_to_shopping_list":
                body = {
                    "shoppingListId": arguments["shopping_list_id"],
                    "note": arguments["note"],
                    "quantity": arguments.get("quantity", 1),
                    "isFood": False,
                }
                if u := arguments.get("unit"):
                    body["unit"] = {"name": u}
                return _text(await ml_post("/api/groups/shopping/items", body))

            case "mealie_list_categories":
                return _text(await ml_get("/api/organizers/categories"))

            case "mealie_list_tags":
                return _text(await ml_get("/api/organizers/tags"))

            case "mealie_get_recipe_suggestions":
                return _text(await ml_get("/api/recipes/suggestions", params={"limit": arguments.get("limit", 10)}))

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
