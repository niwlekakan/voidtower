#!/usr/bin/env python3
"""
Home Assistant MCP Server — control entities, automations, scenes, and services.

Setup:
  pip install mcp httpx
  HA_URL=http://localhost:8123 HA_TOKEN=<token> python homeassistant_server.py

Get a token: HA → Profile (bottom-left) → Long-Lived Access Tokens → Create Token
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

HA_URL = os.environ.get("HA_URL", "http://localhost:8123").rstrip("/")
HA_TOKEN = os.environ.get("HA_TOKEN", "")

server = Server("homeassistant")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=HA_URL,
            headers={"Authorization": f"Bearer {HA_TOKEN}", "Content-Type": "application/json"},
            timeout=30,
        )
    return _client


async def ha_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def ha_post(path: str, body: dict | None = None) -> Any:
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
            name="ha_get_status",
            description="Get Home Assistant version and API status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="ha_get_states",
            description="Get all entity states. Optionally filter by domain prefix (e.g. 'light', 'switch', 'sensor')",
            inputSchema={
                "type": "object",
                "properties": {
                    "domain": {"type": "string", "description": "Filter by domain prefix e.g. light, switch, sensor, climate"},
                },
            },
        ),
        types.Tool(
            name="ha_get_state",
            description="Get the current state and attributes of a single entity",
            inputSchema={
                "type": "object",
                "properties": {
                    "entity_id": {"type": "string", "description": "Entity ID e.g. light.living_room"},
                },
                "required": ["entity_id"],
            },
        ),
        types.Tool(
            name="ha_call_service",
            description="Call a Home Assistant service to control an entity or trigger an action",
            inputSchema={
                "type": "object",
                "properties": {
                    "domain": {"type": "string", "description": "Service domain e.g. light, switch, media_player, climate"},
                    "service": {"type": "string", "description": "Service name e.g. turn_on, turn_off, toggle, set_temperature"},
                    "entity_id": {"type": "string", "description": "Target entity ID (optional for global services)"},
                    "data": {"type": "object", "description": "Additional service data e.g. {brightness: 200, color_temp: 4000}"},
                },
                "required": ["domain", "service"],
            },
        ),
        types.Tool(
            name="ha_list_services",
            description="List all available services grouped by domain",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="ha_list_automations",
            description="List all automation entities with their state (on/off) and last triggered time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="ha_trigger_automation",
            description="Manually trigger an automation",
            inputSchema={
                "type": "object",
                "properties": {
                    "entity_id": {"type": "string", "description": "Automation entity ID e.g. automation.morning_lights"},
                },
                "required": ["entity_id"],
            },
        ),
        types.Tool(
            name="ha_list_scenes",
            description="List all scene entities",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="ha_activate_scene",
            description="Activate a scene",
            inputSchema={
                "type": "object",
                "properties": {
                    "entity_id": {"type": "string", "description": "Scene entity ID e.g. scene.movie_night"},
                },
                "required": ["entity_id"],
            },
        ),
        types.Tool(
            name="ha_get_history",
            description="Get state history for an entity over a time period",
            inputSchema={
                "type": "object",
                "properties": {
                    "entity_id": {"type": "string"},
                    "start_time": {"type": "string", "description": "ISO 8601 start time e.g. 2024-06-01T00:00:00Z"},
                    "end_time": {"type": "string", "description": "ISO 8601 end time (defaults to now)"},
                },
                "required": ["entity_id", "start_time"],
            },
        ),
        types.Tool(
            name="ha_get_logbook",
            description="Get the human-readable event logbook, optionally filtered to one entity",
            inputSchema={
                "type": "object",
                "properties": {
                    "start_time": {"type": "string", "description": "ISO 8601 start time"},
                    "entity_id": {"type": "string", "description": "Filter to a specific entity (optional)"},
                },
                "required": ["start_time"],
            },
        ),
        types.Tool(
            name="ha_fire_event",
            description="Fire a custom Home Assistant event",
            inputSchema={
                "type": "object",
                "properties": {
                    "event_type": {"type": "string", "description": "Event type name"},
                    "data": {"type": "object", "description": "Event data payload"},
                },
                "required": ["event_type"],
            },
        ),
    ]


@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "ha_get_status":
                return _text(await ha_get("/api/"))

            case "ha_get_states":
                states = await ha_get("/api/states")
                if domain := arguments.get("domain"):
                    states = [s for s in states if s.get("entity_id", "").startswith(f"{domain}.")]
                return _text(states)

            case "ha_get_state":
                return _text(await ha_get(f"/api/states/{arguments['entity_id']}"))

            case "ha_call_service":
                body: dict = arguments.get("data") or {}
                if eid := arguments.get("entity_id"):
                    body["entity_id"] = eid
                return _text(await ha_post(f"/api/services/{arguments['domain']}/{arguments['service']}", body))

            case "ha_list_services":
                return _text(await ha_get("/api/services"))

            case "ha_list_automations":
                states = await ha_get("/api/states")
                automations = [s for s in states if s.get("entity_id", "").startswith("automation.")]
                return _text(automations)

            case "ha_trigger_automation":
                return _text(await ha_post("/api/services/automation/trigger", {"entity_id": arguments["entity_id"]}))

            case "ha_list_scenes":
                states = await ha_get("/api/states")
                scenes = [s for s in states if s.get("entity_id", "").startswith("scene.")]
                return _text(scenes)

            case "ha_activate_scene":
                return _text(await ha_post("/api/services/scene/turn_on", {"entity_id": arguments["entity_id"]}))

            case "ha_get_history":
                params: dict = {"filter_entity_id": arguments["entity_id"]}
                if et := arguments.get("end_time"):
                    params["end_time"] = et
                return _text(await ha_get(f"/api/history/period/{arguments['start_time']}", params=params))

            case "ha_get_logbook":
                params = {}
                if eid := arguments.get("entity_id"):
                    params["entity_id"] = eid
                return _text(await ha_get(f"/api/logbook/{arguments['start_time']}", params=params))

            case "ha_fire_event":
                return _text(await ha_post(f"/api/events/{arguments['event_type']}", arguments.get("data") or {}))

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
