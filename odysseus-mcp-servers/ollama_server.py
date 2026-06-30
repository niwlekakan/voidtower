#!/usr/bin/env python3
"""
Ollama MCP Server — manage and query a local Ollama LLM runtime.

Setup:
  pip install mcp httpx
  OLLAMA_URL=http://localhost:11434 python ollama_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

OLLAMA_URL = os.environ.get("OLLAMA_URL", "http://localhost:11434").rstrip("/")

server = Server("ollama")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=OLLAMA_URL, timeout=120)
    return _client


async def ol_get(path: str) -> Any:
    r = await client().get(path)
    r.raise_for_status()
    return r.json()


async def ol_post(path: str, body: dict) -> Any:
    r = await client().post(path, json=body)
    r.raise_for_status()
    return r.json()


async def ol_delete(path: str, body: dict) -> Any:
    r = await client().request("DELETE", path, json=body)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="ollama_list_models",
            description="List all pulled models with size and modified date",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="ollama_show_model",
            description="Show details for a model: parameters, template, modelfile, license",
            inputSchema={
                "type": "object",
                "properties": {
                    "model": {"type": "string", "description": "Model name (e.g. llama3.2)"},
                },
                "required": ["model"],
            },
        ),
        types.Tool(
            name="ollama_pull_model",
            description="Download a model from the Ollama registry",
            inputSchema={
                "type": "object",
                "properties": {
                    "model": {"type": "string", "description": "Model name to pull (e.g. llama3.2, mistral)"},
                },
                "required": ["model"],
            },
        ),
        types.Tool(
            name="ollama_delete_model",
            description="Delete a pulled model from local storage",
            inputSchema={
                "type": "object",
                "properties": {
                    "model": {"type": "string", "description": "Model name to delete"},
                },
                "required": ["model"],
            },
        ),
        types.Tool(
            name="ollama_copy_model",
            description="Copy a model to a new name (useful for creating aliases)",
            inputSchema={
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Source model name"},
                    "destination": {"type": "string", "description": "Destination model name"},
                },
                "required": ["source", "destination"],
            },
        ),
        types.Tool(
            name="ollama_generate",
            description="Run a single-turn completion with a model",
            inputSchema={
                "type": "object",
                "properties": {
                    "model": {"type": "string", "description": "Model name"},
                    "prompt": {"type": "string", "description": "Input prompt"},
                    "options": {
                        "type": "object",
                        "description": "Optional model parameters (temperature, top_p, num_predict, etc.)",
                    },
                },
                "required": ["model", "prompt"],
            },
        ),
        types.Tool(
            name="ollama_chat",
            description="Run a multi-turn chat completion with a model",
            inputSchema={
                "type": "object",
                "properties": {
                    "model": {"type": "string", "description": "Model name"},
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
                    "options": {"type": "object", "description": "Optional model parameters"},
                },
                "required": ["model", "messages"],
            },
        ),
        types.Tool(
            name="ollama_get_running",
            description="List models currently loaded in memory with their expiry time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="ollama_get_version",
            description="Get the Ollama server version",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "ollama_list_models":
                return _text(await ol_get("/api/tags"))

            case "ollama_show_model":
                return _text(await ol_post("/api/show", {"model": arguments["model"]}))

            case "ollama_pull_model":
                return _text(await ol_post("/api/pull", {"model": arguments["model"], "stream": False}))

            case "ollama_delete_model":
                return _text(await ol_delete("/api/delete", {"model": arguments["model"]}))

            case "ollama_copy_model":
                return _text(await ol_post("/api/copy", {
                    "source": arguments["source"],
                    "destination": arguments["destination"],
                }))

            case "ollama_generate":
                body: dict = {"model": arguments["model"], "prompt": arguments["prompt"], "stream": False}
                if opts := arguments.get("options"):
                    body["options"] = opts
                return _text(await ol_post("/api/generate", body))

            case "ollama_chat":
                body = {"model": arguments["model"], "messages": arguments["messages"], "stream": False}
                if opts := arguments.get("options"):
                    body["options"] = opts
                return _text(await ol_post("/api/chat", body))

            case "ollama_get_running":
                return _text(await ol_get("/api/ps"))

            case "ollama_get_version":
                return _text(await ol_get("/api/version"))

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
