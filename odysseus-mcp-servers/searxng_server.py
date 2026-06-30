#!/usr/bin/env python3
"""
SearXNG MCP Server — search the web via a self-hosted SearXNG metasearch engine.

Setup:
  pip install mcp httpx
  SEARXNG_URL=http://localhost:8080 python searxng_server.py

Optional Basic auth: SEARXNG_USER and SEARXNG_PASSWORD env vars.
Enable JSON format in SearXNG settings: search.formats: [html, json]
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

SEARXNG_URL = os.environ.get("SEARXNG_URL", "http://localhost:8080").rstrip("/")
SEARXNG_USER = os.environ.get("SEARXNG_USER", "")
SEARXNG_PASSWORD = os.environ.get("SEARXNG_PASSWORD", "")

server = Server("searxng")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        auth = (SEARXNG_USER, SEARXNG_PASSWORD) if SEARXNG_USER else None
        _client = httpx.AsyncClient(base_url=SEARXNG_URL, auth=auth, timeout=30)
    return _client


async def sx_search(query: str, categories: str, page: int = 1) -> Any:
    r = await client().get("/search", params={
        "q": query,
        "format": "json",
        "categories": categories,
        "language": "auto",
        "pageno": page,
    })
    r.raise_for_status()
    return r.json()


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


def _results(data: Any) -> list[types.TextContent]:
    results = data.get("results", data) if isinstance(data, dict) else data
    return [types.TextContent(type="text", text=json.dumps(results, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="searxng_search",
            description="General web search using the self-hosted SearXNG instance",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "page": {"type": "integer", "default": 1, "description": "Result page number"},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="searxng_search_news",
            description="Search recent news articles via SearXNG",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="searxng_search_files",
            description="Search for downloadable files and documents via SearXNG",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="searxng_search_images",
            description="Search for images via SearXNG",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="searxng_get_config",
            description="Get SearXNG server configuration: enabled engines, categories, and settings",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "searxng_search":
                return _results(await sx_search(arguments["query"], "general", arguments.get("page", 1)))

            case "searxng_search_news":
                return _results(await sx_search(arguments["query"], "news", arguments.get("page", 1)))

            case "searxng_search_files":
                return _results(await sx_search(arguments["query"], "files", arguments.get("page", 1)))

            case "searxng_search_images":
                return _results(await sx_search(arguments["query"], "images", arguments.get("page", 1)))

            case "searxng_get_config":
                r = await client().get("/config")
                r.raise_for_status()
                return _text(r.json())

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
