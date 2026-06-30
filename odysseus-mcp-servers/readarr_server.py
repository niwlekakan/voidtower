#!/usr/bin/env python3
"""
Readarr MCP Server — manage books and authors in Readarr.

Setup:
  pip install mcp httpx
  READARR_URL=http://localhost:8787 READARR_API_KEY=<key> python readarr_server.py

Get an API key: Readarr → Settings → General → Security → API Key
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

READARR_URL = os.environ.get("READARR_URL", "http://localhost:8787").rstrip("/")
READARR_API_KEY = os.environ.get("READARR_API_KEY", "")

server = Server("readarr")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{READARR_URL}/api/v1",
            headers={"X-Api-Key": READARR_API_KEY},
            timeout=30,
        )
    return _client


async def rd_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def rd_post(path: str, body: dict | None = None) -> Any:
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
            name="readarr_list_authors",
            description="List all authors in Readarr with their monitored status and book counts",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="readarr_get_author",
            description="Get detailed info for a specific author by their Readarr ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Readarr author ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="readarr_search_books",
            description="Search for a book or author by name to find Goodreads/ISBN IDs before adding",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Book title or author name to search"},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="readarr_add_book",
            description="Add a book to Readarr for monitoring and downloading. Use readarr_search_books first to get the foreignBookId and author details.",
            inputSchema={
                "type": "object",
                "properties": {
                    "foreign_book_id": {"type": "string", "description": "Goodreads book ID from readarr_search_books"},
                    "title": {"type": "string", "description": "Book title"},
                    "author_id": {"type": "integer", "description": "Readarr author ID (if author already exists in Readarr)"},
                    "quality_profile_id": {"type": "integer", "description": "Quality profile ID from readarr_get_quality_profiles"},
                    "root_folder_path": {"type": "string", "description": "Root folder path from readarr_get_root_folders"},
                    "monitored": {"type": "boolean", "default": True},
                },
                "required": ["foreign_book_id", "title", "quality_profile_id", "root_folder_path"],
            },
        ),
        types.Tool(
            name="readarr_list_books",
            description="List all books for a specific author",
            inputSchema={
                "type": "object",
                "properties": {
                    "author_id": {"type": "integer", "description": "Readarr author ID"},
                },
                "required": ["author_id"],
            },
        ),
        types.Tool(
            name="readarr_get_wanted",
            description="List monitored books that are missing and have not been downloaded",
            inputSchema={
                "type": "object",
                "properties": {
                    "page_size": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="readarr_get_queue",
            description="Get the current download queue",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="readarr_get_quality_profiles",
            description="List available quality profiles (needed when adding a book)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="readarr_get_root_folders",
            description="List configured root folders (needed when adding a book)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="readarr_get_system_status",
            description="Get Readarr version, OS, and startup info",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "readarr_list_authors":
                return _text(await rd_get("/author"))

            case "readarr_get_author":
                return _text(await rd_get(f"/author/{arguments['id']}"))

            case "readarr_search_books":
                return _text(await rd_get("/book/lookup", params={"term": arguments["query"]}))

            case "readarr_add_book":
                body: dict = {
                    "foreignBookId": arguments["foreign_book_id"],
                    "title": arguments["title"],
                    "qualityProfileId": arguments["quality_profile_id"],
                    "rootFolderPath": arguments["root_folder_path"],
                    "monitored": arguments.get("monitored", True),
                    "addOptions": {"searchForNewBook": True},
                }
                if author_id := arguments.get("author_id"):
                    body["authorId"] = author_id
                return _text(await rd_post("/book", body))

            case "readarr_list_books":
                return _text(await rd_get("/book", params={"authorId": arguments["author_id"]}))

            case "readarr_get_wanted":
                return _text(await rd_get("/wanted/missing", params={
                    "sortKey": "releaseDate",
                    "sortDir": "desc",
                    "pageSize": arguments.get("page_size", 30),
                }))

            case "readarr_get_queue":
                return _text(await rd_get("/queue"))

            case "readarr_get_quality_profiles":
                return _text(await rd_get("/qualityprofile"))

            case "readarr_get_root_folders":
                return _text(await rd_get("/rootfolder"))

            case "readarr_get_system_status":
                return _text(await rd_get("/system/status"))

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
