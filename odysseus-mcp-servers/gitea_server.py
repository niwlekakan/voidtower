#!/usr/bin/env python3
"""
Gitea MCP Server — manage repositories, issues, PRs, and users on a Gitea instance.

Setup:
  pip install mcp httpx
  GITEA_URL=http://localhost:3000 GITEA_TOKEN=<token> python gitea_server.py

Get a token: Gitea → Settings → Applications → Generate Token
"""

import asyncio
import os
import json
from typing import Any
import httpx
from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp import types

GITEA_URL = os.environ.get("GITEA_URL", "http://localhost:3000").rstrip("/")
GITEA_TOKEN = os.environ.get("GITEA_TOKEN", "")

server = Server("gitea")
_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=f"{GITEA_URL}/api/v1",
            headers={"Authorization": f"token {GITEA_TOKEN}"},
            timeout=30,
        )
    return _client


async def gt_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


async def gt_post(path: str, body: dict | None = None) -> Any:
    r = await client().post(path, json=body or {})
    r.raise_for_status()
    return r.json()


async def gt_patch(path: str, body: dict | None = None) -> Any:
    r = await client().patch(path, json=body or {})
    r.raise_for_status()
    return r.json()


async def gt_delete(path: str) -> Any:
    r = await client().delete(path)
    r.raise_for_status()
    return {"status": r.status_code}


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

@server.list_tools()
async def list_tools() -> list[types.Tool]:
    return [
        types.Tool(
            name="gitea_get_server_info",
            description="Get Gitea server version and configuration",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="gitea_list_repos",
            description="List repositories — optionally scoped to a user or organization",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string", "description": "User or org name (omit for all accessible repos)"},
                    "limit": {"type": "integer", "default": 30},
                    "page": {"type": "integer", "default": 1},
                },
            },
        ),
        types.Tool(
            name="gitea_search_repos",
            description="Search repositories by keyword",
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
            name="gitea_get_repo",
            description="Get details for a specific repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string", "description": "Repository owner (user or org)"},
                    "repo": {"type": "string", "description": "Repository name"},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_create_repo",
            description="Create a new repository under the authenticated user or an organization",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Repository name"},
                    "description": {"type": "string"},
                    "private": {"type": "boolean", "default": True},
                    "auto_init": {"type": "boolean", "default": True, "description": "Initialize with README"},
                    "org": {"type": "string", "description": "Create under this organization instead of the authenticated user"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="gitea_delete_repo",
            description="Delete a repository permanently",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_list_issues",
            description="List issues for a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "state": {"type": "string", "enum": ["open", "closed", "all"], "default": "open"},
                    "type": {"type": "string", "enum": ["issues", "pulls"], "default": "issues"},
                    "limit": {"type": "integer", "default": 30},
                    "page": {"type": "integer", "default": 1},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_get_issue",
            description="Get a specific issue or pull request",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "index": {"type": "integer", "description": "Issue number"},
                },
                "required": ["owner", "repo", "index"],
            },
        ),
        types.Tool(
            name="gitea_create_issue",
            description="Create a new issue in a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "title": {"type": "string"},
                    "body": {"type": "string", "description": "Issue body (Markdown)"},
                    "labels": {"type": "array", "items": {"type": "integer"}, "description": "Label IDs to assign"},
                    "assignees": {"type": "array", "items": {"type": "string"}, "description": "Usernames to assign"},
                },
                "required": ["owner", "repo", "title"],
            },
        ),
        types.Tool(
            name="gitea_comment_on_issue",
            description="Add a comment to an issue or pull request",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "index": {"type": "integer", "description": "Issue number"},
                    "body": {"type": "string", "description": "Comment body (Markdown)"},
                },
                "required": ["owner", "repo", "index", "body"],
            },
        ),
        types.Tool(
            name="gitea_close_issue",
            description="Close an open issue",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "index": {"type": "integer"},
                },
                "required": ["owner", "repo", "index"],
            },
        ),
        types.Tool(
            name="gitea_list_prs",
            description="List pull requests for a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "state": {"type": "string", "enum": ["open", "closed", "all"], "default": "open"},
                    "limit": {"type": "integer", "default": 20},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_merge_pr",
            description="Merge a pull request",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "index": {"type": "integer", "description": "PR number"},
                    "merge_style": {
                        "type": "string",
                        "enum": ["merge", "rebase", "rebase-merge", "squash"],
                        "default": "merge",
                    },
                    "message": {"type": "string", "description": "Merge commit message"},
                },
                "required": ["owner", "repo", "index"],
            },
        ),
        types.Tool(
            name="gitea_list_branches",
            description="List branches for a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "limit": {"type": "integer", "default": 30},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_list_releases",
            description="List releases for a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "limit": {"type": "integer", "default": 10},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_list_users",
            description="List all users on the Gitea instance (admin required)",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 30},
                    "page": {"type": "integer", "default": 1},
                },
            },
        ),
        types.Tool(
            name="gitea_get_user",
            description="Get details for a specific Gitea user",
            inputSchema={
                "type": "object",
                "properties": {
                    "username": {"type": "string"},
                },
                "required": ["username"],
            },
        ),
        types.Tool(
            name="gitea_create_user",
            description="Create a new Gitea user (admin required)",
            inputSchema={
                "type": "object",
                "properties": {
                    "username": {"type": "string"},
                    "email": {"type": "string"},
                    "password": {"type": "string"},
                    "must_change_password": {"type": "boolean", "default": True},
                },
                "required": ["username", "email", "password"],
            },
        ),
        types.Tool(
            name="gitea_list_orgs",
            description="List all organizations on the Gitea instance",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="gitea_get_repo_topics",
            description="Get topics/tags for a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_list_webhooks",
            description="List webhooks configured on a repository",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                },
                "required": ["owner", "repo"],
            },
        ),
        types.Tool(
            name="gitea_get_commit_log",
            description="Get recent commits for a repository branch",
            inputSchema={
                "type": "object",
                "properties": {
                    "owner": {"type": "string"},
                    "repo": {"type": "string"},
                    "branch": {"type": "string", "description": "Branch name (omit for default branch)"},
                    "limit": {"type": "integer", "default": 20},
                },
                "required": ["owner", "repo"],
            },
        ),
    ]


# ─── Tool handlers ────────────────────────────────────────────────────────────

@server.call_tool()
async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    try:
        match name:
            case "gitea_get_server_info":
                return _text(await gt_get("/version"))

            case "gitea_list_repos":
                owner = arguments.get("owner")
                params = {"limit": arguments.get("limit", 30), "page": arguments.get("page", 1)}
                if owner:
                    data = await gt_get(f"/repos/search", params={"q": "", "owner": owner, **params})
                    return _text(data.get("data", data))
                else:
                    return _text(await gt_get("/repos/search", params=params))

            case "gitea_search_repos":
                data = await gt_get("/repos/search", params={
                    "q": arguments["query"],
                    "limit": arguments.get("limit", 20),
                })
                return _text(data.get("data", data))

            case "gitea_get_repo":
                return _text(await gt_get(f"/repos/{arguments['owner']}/{arguments['repo']}"))

            case "gitea_create_repo":
                body: dict = {
                    "name": arguments["name"],
                    "description": arguments.get("description", ""),
                    "private": arguments.get("private", True),
                    "auto_init": arguments.get("auto_init", True),
                }
                if org := arguments.get("org"):
                    return _text(await gt_post(f"/orgs/{org}/repos", body))
                return _text(await gt_post("/user/repos", body))

            case "gitea_delete_repo":
                return _text(await gt_delete(f"/repos/{arguments['owner']}/{arguments['repo']}"))

            case "gitea_list_issues":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/issues",
                    params={
                        "state": arguments.get("state", "open"),
                        "type": arguments.get("type", "issues"),
                        "limit": arguments.get("limit", 30),
                        "page": arguments.get("page", 1),
                    },
                ))

            case "gitea_get_issue":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/issues/{arguments['index']}"
                ))

            case "gitea_create_issue":
                body = {"title": arguments["title"]}
                if b := arguments.get("body"):
                    body["body"] = b
                if labels := arguments.get("labels"):
                    body["labels"] = labels
                if assignees := arguments.get("assignees"):
                    body["assignees"] = assignees
                return _text(await gt_post(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/issues", body
                ))

            case "gitea_comment_on_issue":
                return _text(await gt_post(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/issues/{arguments['index']}/comments",
                    {"body": arguments["body"]},
                ))

            case "gitea_close_issue":
                return _text(await gt_patch(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/issues/{arguments['index']}",
                    {"state": "closed"},
                ))

            case "gitea_list_prs":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/pulls",
                    params={"state": arguments.get("state", "open"), "limit": arguments.get("limit", 20)},
                ))

            case "gitea_merge_pr":
                body = {"Do": arguments.get("merge_style", "merge")}
                if msg := arguments.get("message"):
                    body["merge_message_field"] = msg
                return _text(await gt_post(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/pulls/{arguments['index']}/merge",
                    body,
                ))

            case "gitea_list_branches":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/branches",
                    params={"limit": arguments.get("limit", 30)},
                ))

            case "gitea_list_releases":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/releases",
                    params={"limit": arguments.get("limit", 10)},
                ))

            case "gitea_list_users":
                return _text(await gt_get("/admin/users", params={
                    "limit": arguments.get("limit", 30),
                    "page": arguments.get("page", 1),
                }))

            case "gitea_get_user":
                return _text(await gt_get(f"/users/{arguments['username']}"))

            case "gitea_create_user":
                return _text(await gt_post("/admin/users", {
                    "username": arguments["username"],
                    "email": arguments["email"],
                    "password": arguments["password"],
                    "must_change_password": arguments.get("must_change_password", True),
                    "source_id": 0,
                    "login_name": arguments["username"],
                }))

            case "gitea_list_orgs":
                return _text(await gt_get("/admin/orgs", params={"limit": arguments.get("limit", 30)}))

            case "gitea_get_repo_topics":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/topics"
                ))

            case "gitea_list_webhooks":
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/hooks"
                ))

            case "gitea_get_commit_log":
                params = {"limit": arguments.get("limit", 20)}
                if branch := arguments.get("branch"):
                    params["sha"] = branch
                return _text(await gt_get(
                    f"/repos/{arguments['owner']}/{arguments['repo']}/commits",
                    params=params,
                ))

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
