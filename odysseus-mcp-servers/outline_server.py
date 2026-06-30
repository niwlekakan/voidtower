#!/usr/bin/env python3
"""
Outline MCP Server — manage documents and collections in an Outline wiki.

Setup:
  pip install mcp httpx
  OUTLINE_URL=http://localhost:3000 OUTLINE_API_KEY=<key> python outline_server.py

Get an API key: Outline → Settings → API → Create token
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

OUTLINE_URL = os.environ.get("OUTLINE_URL", "http://localhost:3000").rstrip("/")
OUTLINE_API_KEY = os.environ.get("OUTLINE_API_KEY", "")

server = Server("outline")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=OUTLINE_URL,
            headers={
                "Authorization": f"Bearer {OUTLINE_API_KEY}",
                "Content-Type": "application/json",
            },
            timeout=30,
        )
    return _client


async def ol_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    return r.json()


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="outline_search",
            description="Full-text search across all Outline documents",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search term"},
                    "limit": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="outline_search_titles",
            description="Search published document titles only",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "default": 20},
                },
                "required": ["query"],
            },
        ),
        types.Tool(
            name="outline_list_documents",
            description="List all documents, newest first",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 25},
                    "offset": {"type": "integer", "default": 0},
                    "collection_id": {"type": "string", "description": "Scope to a collection"},
                },
            },
        ),
        types.Tool(
            name="outline_get_document",
            description="Get a document's full content and metadata by ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Document ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="outline_create_document",
            description="Create and publish a new document in a collection",
            inputSchema={
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "text": {"type": "string", "description": "Document body in Markdown"},
                    "collection_id": {"type": "string", "description": "ID of the target collection"},
                    "parent_document_id": {"type": "string", "description": "Optional parent document ID for nesting"},
                },
                "required": ["title", "text", "collection_id"],
            },
        ),
        types.Tool(
            name="outline_update_document",
            description="Update the title or content of an existing document",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "title": {"type": "string"},
                    "text": {"type": "string", "description": "New Markdown content"},
                    "publish": {"type": "boolean", "default": True},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="outline_delete_document",
            description="Delete a document permanently",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="outline_list_collections",
            description="List all collections in the workspace",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 25},
                },
            },
        ),
        types.Tool(
            name="outline_get_collection",
            description="Get details and document tree for a collection",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="outline_create_collection",
            description="Create a new collection",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "color": {"type": "string", "description": "Hex color e.g. #FF0000"},
                    "private": {"type": "boolean", "default": False},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="outline_list_users",
            description="List all workspace members",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 25},
                },
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "outline_search":
                data = await ol_post("/api/documents.search", {"query": arguments["query"], "limit": arguments.get("limit", 20)})
                return _text(data.get("data", data))

            case "outline_search_titles":
                data = await ol_post("/api/documents.search", {
                    "query": arguments["query"],
                    "limit": arguments.get("limit", 20),
                    "statusFilter": ["published"],
                })
                return _text(data.get("data", data))

            case "outline_list_documents":
                body: dict = {"limit": arguments.get("limit", 25), "offset": arguments.get("offset", 0), "direction": "DESC"}
                if cid := arguments.get("collection_id"):
                    body["collectionId"] = cid
                data = await ol_post("/api/documents.list", body)
                return _text(data.get("data", data))

            case "outline_get_document":
                data = await ol_post("/api/documents.info", {"id": arguments["id"]})
                return _text(data.get("data", data))

            case "outline_create_document":
                body = {
                    "title": arguments["title"],
                    "text": arguments["text"],
                    "collectionId": arguments["collection_id"],
                    "publish": True,
                }
                if pid := arguments.get("parent_document_id"):
                    body["parentDocumentId"] = pid
                data = await ol_post("/api/documents.create", body)
                return _text(data.get("data", data))

            case "outline_update_document":
                body = {"id": arguments["id"], "publish": arguments.get("publish", True)}
                if t := arguments.get("title"):
                    body["title"] = t
                if tx := arguments.get("text"):
                    body["text"] = tx
                data = await ol_post("/api/documents.update", body)
                return _text(data.get("data", data))

            case "outline_delete_document":
                data = await ol_post("/api/documents.delete", {"id": arguments["id"]})
                return _text(data)

            case "outline_list_collections":
                data = await ol_post("/api/collections.list", {"limit": arguments.get("limit", 25)})
                return _text(data.get("data", data))

            case "outline_get_collection":
                data = await ol_post("/api/collections.info", {"id": arguments["id"]})
                return _text(data.get("data", data))

            case "outline_create_collection":
                body = {"name": arguments["name"], "private": arguments.get("private", False)}
                if d := arguments.get("description"):
                    body["description"] = d
                if c := arguments.get("color"):
                    body["color"] = c
                data = await ol_post("/api/collections.create", body)
                return _text(data.get("data", data))

            case "outline_list_users":
                data = await ol_post("/api/users.list", {"limit": arguments.get("limit", 25)})
                return _text(data.get("data", data))

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
