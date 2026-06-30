#!/usr/bin/env python3
"""
ComfyUI MCP Server — manage generation queue, history, and models in ComfyUI.

Setup:
  pip install mcp httpx
  COMFYUI_URL=http://localhost:8188 python comfyui_server.py
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

COMFYUI_URL = os.environ.get("COMFYUI_URL", "http://localhost:8188").rstrip("/")

server = Server("comfyui")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(base_url=COMFYUI_URL, timeout=30)
    return _client


async def cu_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def cu_post(path: str, body: dict | None = None) -> Any:
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
            name="comfyui_get_queue",
            description="Get the current generation queue — pending and running jobs",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="comfyui_get_history",
            description="Get recently completed generation jobs with output filenames",
            inputSchema={
                "type": "object",
                "properties": {
                    "max_items": {"type": "integer", "default": 20},
                },
            },
        ),
        types.Tool(
            name="comfyui_get_history_item",
            description="Get details for a single completed generation by prompt ID",
            inputSchema={
                "type": "object",
                "properties": {
                    "prompt_id": {"type": "string", "description": "Prompt ID from comfyui_get_history"},
                },
                "required": ["prompt_id"],
            },
        ),
        types.Tool(
            name="comfyui_list_models",
            description="List available checkpoint models and LoRAs",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="comfyui_get_system_stats",
            description="Get system stats: GPU VRAM usage, RAM, Python version, device info",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="comfyui_interrupt",
            description="Cancel the currently running generation",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="comfyui_clear_queue",
            description="Clear all pending jobs from the generation queue",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="comfyui_get_embeddings",
            description="List available textual inversion embeddings",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="comfyui_get_extensions",
            description="List installed custom nodes and extensions",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "comfyui_get_queue":
                return _text(await cu_get("/queue"))

            case "comfyui_get_history":
                return _text(await cu_get("/history", params={"max_items": arguments.get("max_items", 20)}))

            case "comfyui_get_history_item":
                data = await cu_get(f"/history/{arguments['prompt_id']}")
                return _text(data)

            case "comfyui_list_models":
                checkpoints, loras = await asyncio.gather(
                    cu_get("/object_info/CheckpointLoaderSimple"),
                    cu_get("/object_info/LoraLoader"),
                )
                ckpt_list = checkpoints.get("CheckpointLoaderSimple", {}).get("input", {}).get("required", {}).get("ckpt_name", [None])[0] or []
                lora_list = loras.get("LoraLoader", {}).get("input", {}).get("required", {}).get("lora_name", [None])[0] or []
                return _text({"checkpoints": ckpt_list, "loras": lora_list})

            case "comfyui_get_system_stats":
                return _text(await cu_get("/system_stats"))

            case "comfyui_interrupt":
                return _text(await cu_post("/interrupt"))

            case "comfyui_clear_queue":
                return _text(await cu_post("/queue", {"clear": True}))

            case "comfyui_get_embeddings":
                return _text(await cu_get("/embeddings"))

            case "comfyui_get_extensions":
                return _text(await cu_get("/extensions"))

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
