#!/usr/bin/env python3
"""
Paperless-ngx MCP Server — search, manage, and tag documents.

Setup:
  pip install mcp httpx
  PAPERLESS_URL=http://localhost:8000 PAPERLESS_TOKEN=<token> python paperless_server.py

Get a token: Paperless → Settings → API → Generate token (or use /api/token/ POST with username/password)
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

PAPERLESS_URL = os.environ.get("PAPERLESS_URL", "http://localhost:8000").rstrip("/")
PAPERLESS_TOKEN = os.environ.get("PAPERLESS_TOKEN", "")

server = Server("paperless")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=PAPERLESS_URL,
            headers={"Authorization": f"Token {PAPERLESS_TOKEN}"},
            timeout=30,
        )
    return _client


async def pl_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def pl_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    return r.json()


async def pl_patch(path: str, body: dict) -> Any:
    r = await client().patch(path, json=body)
    r.raise_for_status()
    return r.json()


async def pl_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="paperless_search_documents",
            description="Full-text and title search across all documents",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="paperless_list_documents",
            description="List documents ordered by creation date (newest first)",
            inputSchema={
                "type": "object",
                "properties": {
                    "page": {"type": "integer", "default": 1},
                    "page_size": {"type": "integer", "default": 25},
                    "tag_id": {"type": "integer", "description": "Filter by tag ID"},
                    "correspondent_id": {"type": "integer", "description": "Filter by correspondent ID"},
                    "document_type_id": {"type": "integer", "description": "Filter by document type ID"},
                },
            },
        ),
        types.Tool(
            name="paperless_get_document",
            description="Get metadata for a specific document",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Document ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="paperless_get_document_content",
            description="Get the extracted text content of a document",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="paperless_update_document",
            description="Update document metadata (title, correspondent, tags, type, date)",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "title": {"type": "string"},
                    "correspondent": {"type": "integer", "description": "Correspondent ID"},
                    "document_type": {"type": "integer", "description": "Document type ID"},
                    "tags": {"type": "array", "items": {"type": "integer"}, "description": "Tag IDs"},
                    "created_date": {"type": "string", "description": "ISO date e.g. 2024-01-15"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="paperless_delete_document",
            description="Permanently delete a document",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="paperless_list_tags",
            description="List all document tags",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="paperless_create_tag",
            description="Create a new document tag",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "color": {"type": "string", "description": "Hex color e.g. #FF0000"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="paperless_list_correspondents",
            description="List all correspondents (senders/recipients)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="paperless_create_correspondent",
            description="Create a new correspondent",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="paperless_list_document_types",
            description="List all document types",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="paperless_get_stats",
            description="Get statistics: document count, inbox count, character count",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "paperless_search_documents":
                return _text(await pl_get("/api/documents/", params={
                    "search": arguments["query"],
                    "page": arguments.get("page", 1),
                    "page_size": arguments.get("page_size", 20),
                    "ordering": "-created",
                }))

            case "paperless_list_documents":
                params: dict = {
                    "page": arguments.get("page", 1),
                    "page_size": arguments.get("page_size", 25),
                    "ordering": "-created",
                }
                if t := arguments.get("tag_id"):
                    params["tags__id__in"] = t
                if c := arguments.get("correspondent_id"):
                    params["correspondent__id"] = c
                if dt := arguments.get("document_type_id"):
                    params["document_type__id"] = dt
                return _text(await pl_get("/api/documents/", params=params))

            case "paperless_get_document":
                return _text(await pl_get(f"/api/documents/{arguments['id']}/"))

            case "paperless_get_document_content":
                r = await client().get(f"/api/documents/{arguments['id']}/content/")
                r.raise_for_status()
                return [types.TextContent(type="text", text=r.text)]

            case "paperless_update_document":
                doc_id = arguments.pop("id")
                body = {k: v for k, v in arguments.items() if v is not None}
                return _text(await pl_patch(f"/api/documents/{doc_id}/", body))

            case "paperless_delete_document":
                return _text(await pl_delete(f"/api/documents/{arguments['id']}/"))

            case "paperless_list_tags":
                return _text(await pl_get("/api/tags/", params={"page_size": 100}))

            case "paperless_create_tag":
                body = {"name": arguments["name"]}
                if c := arguments.get("color"):
                    body["color"] = c
                return _text(await pl_post("/api/tags/", body))

            case "paperless_list_correspondents":
                return _text(await pl_get("/api/correspondents/", params={"page_size": 100}))

            case "paperless_create_correspondent":
                return _text(await pl_post("/api/correspondents/", {"name": arguments["name"]}))

            case "paperless_list_document_types":
                return _text(await pl_get("/api/document_types/", params={"page_size": 100}))

            case "paperless_get_stats":
                return _text(await pl_get("/api/statistics/"))

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
