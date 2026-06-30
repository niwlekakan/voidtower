#!/usr/bin/env python3
"""
Stirling-PDF MCP Server — perform PDF operations via a self-hosted Stirling-PDF instance.

Setup:
  pip install mcp httpx
  STIRLING_URL=http://localhost:8080 python stirling_pdf_server.py

File input: all tools that process PDFs accept a local file path. The file is read
from disk and sent as multipart form data. Output is saved to a path you specify.
"""

import asyncio
import base64
import os
import json
from pathlib import Path
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

STIRLING_URL = os.environ.get("STIRLING_URL", "http://localhost:8080").rstrip("/")
STIRLING_USER = os.environ.get("STIRLING_USER", "")
STIRLING_PASSWORD = os.environ.get("STIRLING_PASSWORD", "")

server = Server("stirling-pdf")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        auth = (STIRLING_USER, STIRLING_PASSWORD) if STIRLING_USER else None
        _client = httpx.AsyncClient(base_url=STIRLING_URL, auth=auth, timeout=120)
    return _client


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


def _read_file(path: str) -> bytes:
    return Path(path).expanduser().resolve().read_bytes()


async def _pdf_op(endpoint: str, input_path: str, output_path: str, extra: dict | None = None) -> dict:
    data = extra or {}
    files = {"fileInput": (Path(input_path).name, _read_file(input_path), "application/pdf")}
    r = await client().post(endpoint, data=data, files=files)
    r.raise_for_status()
    Path(output_path).write_bytes(r.content)
    return {"output": output_path, "bytes": len(r.content)}


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="stirling_get_info",
            description="Get Stirling-PDF server status and version",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="stirling_list_endpoints",
            description="List all available PDF operation endpoints with their parameters",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="stirling_compress_pdf",
            description="Compress a PDF file to reduce its size. Input and output are local file paths on the server running this MCP.",
            inputSchema={
                "type": "object",
                "properties": {
                    "input_path": {"type": "string", "description": "Local path to the input PDF"},
                    "output_path": {"type": "string", "description": "Local path to save the compressed PDF"},
                    "optimize_level": {"type": "integer", "default": 3, "description": "Compression level 1 (low) to 9 (maximum)"},
                },
                "required": ["input_path", "output_path"],
            },
        ),
        types.Tool(
            name="stirling_merge_pdfs",
            description="Merge multiple PDF files into one. All input paths must be local paths accessible to this MCP server.",
            inputSchema={
                "type": "object",
                "properties": {
                    "input_paths": {"type": "array", "items": {"type": "string"}, "description": "List of local PDF file paths to merge in order"},
                    "output_path": {"type": "string", "description": "Local path to save the merged PDF"},
                },
                "required": ["input_paths", "output_path"],
            },
        ),
        types.Tool(
            name="stirling_pdf_to_images",
            description="Convert PDF pages to image files",
            inputSchema={
                "type": "object",
                "properties": {
                    "input_path": {"type": "string", "description": "Local path to input PDF"},
                    "output_path": {"type": "string", "description": "Local path to save output (zip of images)"},
                    "image_format": {"type": "string", "enum": ["png", "jpeg"], "default": "png"},
                },
                "required": ["input_path", "output_path"],
            },
        ),
        types.Tool(
            name="stirling_rotate_pdf",
            description="Rotate all pages in a PDF by 90, 180, or 270 degrees",
            inputSchema={
                "type": "object",
                "properties": {
                    "input_path": {"type": "string"},
                    "output_path": {"type": "string"},
                    "angle": {"type": "integer", "enum": [90, 180, 270], "default": 90},
                },
                "required": ["input_path", "output_path"],
            },
        ),
        types.Tool(
            name="stirling_remove_pages",
            description="Remove specific pages from a PDF",
            inputSchema={
                "type": "object",
                "properties": {
                    "input_path": {"type": "string"},
                    "output_path": {"type": "string"},
                    "page_numbers": {"type": "string", "description": "Pages to remove, e.g. '1,3-5,8'"},
                },
                "required": ["input_path", "output_path", "page_numbers"],
            },
        ),
        types.Tool(
            name="stirling_extract_text",
            description="Extract all text content from a PDF file",
            inputSchema={
                "type": "object",
                "properties": {
                    "input_path": {"type": "string", "description": "Local path to input PDF"},
                },
                "required": ["input_path"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "stirling_get_info":
                r = await client().get("/api/v1/info/status")
                r.raise_for_status()
                return _text(r.json())

            case "stirling_list_endpoints":
                r = await client().get("/api/v1/info/endpoints")
                r.raise_for_status()
                return _text(r.json())

            case "stirling_compress_pdf":
                result = await _pdf_op(
                    "/api/v1/general/compress-pdf",
                    arguments["input_path"],
                    arguments["output_path"],
                    {"optimizeLevel": str(arguments.get("optimize_level", 3))},
                )
                return _text(result)

            case "stirling_merge_pdfs":
                files = [
                    ("fileInput", (Path(p).name, _read_file(p), "application/pdf"))
                    for p in arguments["input_paths"]
                ]
                r = await client().post("/api/v1/general/merge-pdfs", files=files)
                r.raise_for_status()
                Path(arguments["output_path"]).write_bytes(r.content)
                return _text({"output": arguments["output_path"], "bytes": len(r.content)})

            case "stirling_pdf_to_images":
                result = await _pdf_op(
                    "/api/v1/convert/pdf/img",
                    arguments["input_path"],
                    arguments["output_path"],
                    {"imageFormat": arguments.get("image_format", "png"), "singleOrMultiple": "multiple"},
                )
                return _text(result)

            case "stirling_rotate_pdf":
                result = await _pdf_op(
                    "/api/v1/general/rotate-pdf",
                    arguments["input_path"],
                    arguments["output_path"],
                    {"angle": str(arguments.get("angle", 90))},
                )
                return _text(result)

            case "stirling_remove_pages":
                result = await _pdf_op(
                    "/api/v1/general/remove-pages",
                    arguments["input_path"],
                    arguments["output_path"],
                    {"pageNumbers": arguments["page_numbers"]},
                )
                return _text(result)

            case "stirling_extract_text":
                files = {"fileInput": (Path(arguments["input_path"]).name, _read_file(arguments["input_path"]), "application/pdf")}
                r = await client().post("/api/v1/misc/extract-text-from-pdf", files=files)
                r.raise_for_status()
                try:
                    return _text(r.json())
                except Exception:
                    return [types.TextContent(type="text", text=r.text)]

            case _:
                return _text({"error": f"Unknown tool: {name}"})

    except httpx.HTTPStatusError as e:
        return _text({"error": f"HTTP {e.response.status_code}", "detail": e.response.text})
    except FileNotFoundError as e:
        return _text({"error": f"File not found: {e}"})
    except Exception as e:
        return _text({"error": str(e)})


async def main():
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
