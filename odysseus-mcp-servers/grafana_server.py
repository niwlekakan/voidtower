#!/usr/bin/env python3
"""
Grafana MCP Server — manage dashboards, alerts, datasources, and annotations.

Setup:
  pip install mcp httpx
  GRAFANA_URL=http://localhost:3000 GRAFANA_API_KEY=<service-account-token> python grafana_server.py

Get a token: Grafana → Administration → Service accounts → Add service account → Add token
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

GRAFANA_URL = os.environ.get("GRAFANA_URL", "http://localhost:3000").rstrip("/")
GRAFANA_API_KEY = os.environ.get("GRAFANA_API_KEY", "")

server = Server("grafana")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=GRAFANA_URL,
            headers={"Authorization": f"Bearer {GRAFANA_API_KEY}"},
            timeout=30,
        )
    return _client


async def gf_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def gf_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    try:
        return r.json()
    except Exception:
        return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="grafana_get_health",
            description="Check Grafana server health and version",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="grafana_list_datasources",
            description="List all configured datasources (Prometheus, Loki, InfluxDB, PostgreSQL, etc.)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="grafana_get_datasource",
            description="Get configuration details for a specific datasource",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "integer", "description": "Datasource ID from grafana_list_datasources"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="grafana_list_dashboards",
            description="List all dashboards with their title, UID, folder, and tags",
            inputSchema={
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search term to filter dashboards"},
                    "tag": {"type": "string", "description": "Filter by tag"},
                },
            },
        ),
        types.Tool(
            name="grafana_get_dashboard",
            description="Get full dashboard JSON model by UID",
            inputSchema={
                "type": "object",
                "properties": {
                    "uid": {"type": "string", "description": "Dashboard UID from grafana_list_dashboards"},
                },
                "required": ["uid"],
            },
        ),
        types.Tool(
            name="grafana_list_alerts",
            description="List all currently firing or pending alert instances",
            inputSchema={
                "type": "object",
                "properties": {
                    "active": {"type": "boolean", "default": True},
                    "silenced": {"type": "boolean", "default": False},
                },
            },
        ),
        types.Tool(
            name="grafana_list_alert_rules",
            description="List all configured Grafana alert rules across all folders",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="grafana_silence_alert",
            description="Create a silence to suppress alert notifications for a time period",
            inputSchema={
                "type": "object",
                "properties": {
                    "matchers": {
                        "type": "array",
                        "description": "Label matchers, e.g. [{\"name\": \"alertname\", \"value\": \"HighCPU\", \"isRegex\": false}]",
                        "items": {"type": "object"},
                    },
                    "starts_at": {"type": "string", "description": "ISO 8601 start time (e.g. 2026-06-30T00:00:00Z)"},
                    "ends_at": {"type": "string", "description": "ISO 8601 end time"},
                    "comment": {"type": "string", "description": "Reason for the silence"},
                    "created_by": {"type": "string", "default": "voidtower-mcp"},
                },
                "required": ["matchers", "starts_at", "ends_at", "comment"],
            },
        ),
        types.Tool(
            name="grafana_list_orgs",
            description="List all Grafana organizations (requires admin token)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="grafana_list_users",
            description="List all Grafana users with their roles and last active time",
            inputSchema={
                "type": "object",
                "properties": {
                    "perpage": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="grafana_get_org_users",
            description="List users in the current organization with their role assignments",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="grafana_create_annotation",
            description="Create a time annotation on dashboards (useful for marking deployments, incidents, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Annotation text"},
                    "tags": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Tags for filtering (e.g. [\"deploy\", \"production\"])",
                    },
                    "time": {"type": "integer", "description": "Unix timestamp in milliseconds (omit for now)"},
                },
                "required": ["text"],
            },
        ),
        types.Tool(
            name="grafana_query_datasource",
            description="Execute a query against a datasource (Prometheus PromQL, SQL, etc.)",
            inputSchema={
                "type": "object",
                "properties": {
                    "datasource_id": {"type": "integer", "description": "Datasource ID"},
                    "expr": {"type": "string", "description": "Query expression (PromQL, SQL, LogQL, etc.)"},
                    "from": {"type": "string", "default": "now-1h", "description": "Start time (e.g. now-1h or ISO timestamp)"},
                    "to": {"type": "string", "default": "now"},
                    "ref_id": {"type": "string", "default": "A"},
                },
                "required": ["datasource_id", "expr"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "grafana_get_health":
                return _text(await gf_get("/api/health"))

            case "grafana_list_datasources":
                return _text(await gf_get("/api/datasources"))

            case "grafana_get_datasource":
                return _text(await gf_get(f"/api/datasources/{arguments['id']}"))

            case "grafana_list_dashboards":
                params: dict = {"type": "dash-db"}
                if q := arguments.get("query"):
                    params["query"] = q
                if tag := arguments.get("tag"):
                    params["tag"] = tag
                return _text(await gf_get("/api/search", params=params))

            case "grafana_get_dashboard":
                return _text(await gf_get(f"/api/dashboards/uid/{arguments['uid']}"))

            case "grafana_list_alerts":
                params = {
                    "active": str(arguments.get("active", True)).lower(),
                    "silenced": str(arguments.get("silenced", False)).lower(),
                }
                return _text(await gf_get("/api/alertmanager/grafana/api/v2/alerts", params=params))

            case "grafana_list_alert_rules":
                return _text(await gf_get("/api/ruler/grafana/api/v1/rules"))

            case "grafana_silence_alert":
                import datetime
                body: dict = {
                    "matchers": arguments["matchers"],
                    "startsAt": arguments["starts_at"],
                    "endsAt": arguments["ends_at"],
                    "comment": arguments["comment"],
                    "createdBy": arguments.get("created_by", "voidtower-mcp"),
                }
                return _text(await gf_post("/api/alertmanager/grafana/api/v2/silences", body))

            case "grafana_list_orgs":
                return _text(await gf_get("/api/orgs"))

            case "grafana_list_users":
                return _text(await gf_get("/api/users", params={"perpage": arguments.get("perpage", 30)}))

            case "grafana_get_org_users":
                return _text(await gf_get("/api/org/users"))

            case "grafana_create_annotation":
                import time as _time
                body = {
                    "text": arguments["text"],
                    "tags": arguments.get("tags", []),
                    "time": arguments.get("time", int(_time.time() * 1000)),
                }
                return _text(await gf_post("/api/annotations", body))

            case "grafana_query_datasource":
                body = {
                    "queries": [{
                        "datasourceId": arguments["datasource_id"],
                        "expr": arguments["expr"],
                        "refId": arguments.get("ref_id", "A"),
                    }],
                    "from": arguments.get("from", "now-1h"),
                    "to": arguments.get("to", "now"),
                }
                return _text(await gf_post("/api/ds/query", body))

            case _:
                return _text({"error": f"Unknown tool: {name}"})

    except httpx.HTTPStatusError as e:
        return _text({"error": f"HTTP {e.response.status_code}", "detail": e.response.text})
    except Exception as e:
        return _text({"error": str(e)})


# ─── Entry point ─────────────────────────────────────────────────────────────

async def main():
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
