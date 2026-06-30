#!/usr/bin/env python3
"""
llama.cpp MCP Server — manage and query a llama.cpp HTTP inference server.

Setup:
  pip install mcp httpx
  LLAMACPP_URL=http://localhost:8080 python llama_cpp_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

LLAMACPP_URL = os.environ.get("LLAMACPP_URL", "http://localhost:8080").rstrip("/")

server = Server("llama-cpp")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=LLAMACPP_URL, timeout=120)
    return _client


async def lc_get(path: str) -> Any:
    r = await client().get(path)
    r.raise_for_status()
    return r.json()


async def lc_post(path: str, body: dict) -> Any:
    r = await client().post(path, json=body)
    r.raise_for_status()
    return r.json()


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="llamacpp_get_health",
            description="Check server health and inference slot availability",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="llamacpp_get_models",
            description="List loaded model information (name, context size)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="llamacpp_get_slots",
            description="Get inference slot status — shows concurrent request capacity and active slots",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="llamacpp_get_props",
            description="Get server properties: model path, context size, n_batch, rope settings, etc.",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="llamacpp_complete",
            description="Run a chat completion using the loaded model",
            inputSchema={
                "type": "object",
                "properties": {
                    "messages": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "role": {"type": "string", "enum": ["system", "user", "assistant"]},
                                "content": {"type": "string"},
                            },
                        },
                        "description": "Conversation messages",
                    },
                    "max_tokens": {"type": "integer", "default": 500},
                    "temperature": {"type": "number", "default": 0.7},
                },
                "required": ["messages"],
            },
        ),
        types.Tool(
            name="llamacpp_tokenize",
            description="Tokenize a string and return token count and token IDs",
            inputSchema={
                "type": "object",
                "properties": {
                    "content": {"type": "string", "description": "Text to tokenize"},
                },
                "required": ["content"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "llamacpp_get_health":
                return _text(await lc_get("/health"))

            case "llamacpp_get_models":
                return _text(await lc_get("/v1/models"))

            case "llamacpp_get_slots":
                return _text(await lc_get("/slots"))

            case "llamacpp_get_props":
                return _text(await lc_get("/props"))

            case "llamacpp_complete":
                return _text(await lc_post("/v1/chat/completions", {
                    "model": "local",
                    "messages": arguments["messages"],
                    "max_tokens": arguments.get("max_tokens", 500),
                    "temperature": arguments.get("temperature", 0.7),
                    "stream": False,
                }))

            case "llamacpp_tokenize":
                return _text(await lc_post("/tokenize", {"content": arguments["content"]}))

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
