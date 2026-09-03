#!/usr/bin/env python3
"""
VoidTower MCP Server — gives Odysseus AI tools to manage VoidTower infrastructure.

Setup:
  pip install mcp httpx
  VOIDTOWER_URL=http://localhost:8743 VOIDTOWER_TOKEN=<api-token> python voidtower_server.py

Register in Odysseus:
  Settings → MCP Servers → Add → Command: python /path/to/voidtower_server.py
  Env: VOIDTOWER_URL, VOIDTOWER_TOKEN
"""

import asyncio
import hashlib
import json
import os
import uuid
from typing import Any

import httpx
from mcp import types
from mcp.server import Server
from mcp.server.stdio import stdio_server

VOIDTOWER_URL = os.environ.get("VOIDTOWER_URL", "http://localhost:8743").rstrip("/")
VOIDTOWER_TOKEN = os.environ.get("VOIDTOWER_TOKEN", "")

_client: httpx.AsyncClient | None = None


def client() -> httpx.AsyncClient:
    global _client
    if _client is None:
        _client = httpx.AsyncClient(
            base_url=VOIDTOWER_URL,
            headers={"Authorization": f"Bearer {VOIDTOWER_TOKEN}"},
            timeout=30,
        )
    return _client


async def vt_get(path: str, params: dict | None = None) -> Any:
    r = await client().get(path, params=params)
    r.raise_for_status()
    return r.json()


VM_ACTIONS = {
    "start": "proxmox.guest.start",
    "stop": "proxmox.guest.stop",
    "reboot": "proxmox.guest.reboot",
    "shutdown": "proxmox.guest.shutdown",
}

DISABLED_LEGACY_MUTATIONS = frozenset(
    {
        "vt_acknowledge_alert",
        "vt_control_app",
        "vt_control_container",
        "vt_control_service",
        "vt_create_proxy",
        "vt_deploy_app",
        "vt_remove_app",
        "vt_resolve_alert",
        "vt_run_automation_job",
        "vt_run_backup",
        "vt_toggle_proxy",
        "vt_update_app_compose",
    }
)

READ_ONLY_TOOLS = frozenset(
    {
        "vt_get_app_compose",
        "vt_get_app_logs",
        "vt_get_app_status",
        "vt_get_audit_log",
        "vt_get_capabilities",
        "vt_get_container_logs",
        "vt_get_metrics",
        "vt_get_network_neighbors",
        "vt_get_service_logs",
        "vt_get_status_summary",
        "vt_get_storage",
        "vt_get_timeline",
        "vt_list_alerts",
        "vt_list_app_catalog",
        "vt_list_automations",
        "vt_list_backups",
        "vt_list_containers",
        "vt_list_deployed_apps",
        "vt_list_firewall_rules",
        "vt_list_proxies",
        "vt_list_secrets",
        "vt_list_services",
        "vt_list_status_checks",
        "vt_list_tags",
        "vt_list_users",
        "vt_list_vms",
        "vt_list_wireguard_peers",
        "vt_run_diagnostics",
    }
)
CANONICAL_MUTATION_TOOLS = frozenset({"vt_control_vm"})
ADVERTISED_TOOLS = READ_ONLY_TOOLS | CANONICAL_MUTATION_TOOLS

CANONICAL_ERROR_MESSAGES = {
    "ai_exposure_denied": "This action is not exposed to machine-capable ingress.",
    "capability_unavailable": "The requested capability is not currently available.",
    "forbidden": "The current credential does not permit this action.",
    "idempotency_conflict": "The idempotency key belongs to different intent.",
    "ingress_denied": "This action is not available from the current ingress.",
    "insufficient_scope": "This API token's scopes do not permit this action.",
    "invalid_idempotency_key": "A valid Idempotency-Key header is required.",
    "invalid_request": "The request body is invalid.",
    "operation_runtime_unavailable": "The durable operation runtime is unavailable.",
    "planning_rejected": "The operation could not be planned safely.",
    "policy_denied": "The operation was denied by policy.",
    "resource_kind_mismatch": "The action does not apply to this resource kind.",
    "resource_not_found": "The requested resource does not exist.",
    "stale_state": "Resource or provider state changed during planning.",
    "unauthorized": "Authentication is required.",
    "unknown_action": "The requested durable action does not exist.",
}


def _stable_error(payload: Any) -> dict[str, Any]:
    candidate = payload.get("error") if isinstance(payload, dict) else None
    code = candidate.get("code") if isinstance(candidate, dict) else None
    message = CANONICAL_ERROR_MESSAGES.get(code)
    if message is None:
        return {
            "error": {
                "code": "upstream_error",
                "message": "VoidTower returned an unexpected error.",
            }
        }
    error = {"code": code, "message": message}
    job_id = candidate.get("job_id")
    if isinstance(job_id, str):
        try:
            if str(uuid.UUID(job_id)) == job_id:
                error["job_id"] = job_id
        except ValueError:
            pass
    return {"error": error}


def _ambiguous_submission(idempotency_key: str) -> dict[str, Any]:
    return {
        "error": {
            "code": "ambiguous_submission",
            "message": (
                "The submission outcome is unknown; query canonical job state "
                "before any retry."
            ),
        },
        "idempotency_key": idempotency_key,
        "recovery_path": f"/api/jobs/by-idempotency/{idempotency_key}",
    }


def _selected(payload: Any, fields: tuple[str, ...]) -> dict[str, Any]:
    if not isinstance(payload, dict):
        return {}
    return {field: payload[field] for field in fields if field in payload}


def _stable_resource(resource: Any) -> dict[str, Any]:
    return _selected(resource, ("id", "kind", "display_name", "revision"))


def _stable_operation(operation: Any) -> dict[str, Any]:
    stable = _selected(
        operation,
        (
            "schema_version",
            "title",
            "risk",
            "preview",
            "external_fingerprint",
        ),
    )
    if not isinstance(operation, dict):
        return stable
    changes = operation.get("changes")
    if isinstance(changes, list):
        stable["changes"] = [
            _selected(change, ("label", "value"))
            for change in changes
            if isinstance(change, dict)
        ]
    steps = operation.get("steps")
    if isinstance(steps, list):
        stable["steps"] = [
            _selected(step, ("kind", "name", "retry_class", "recovery_class"))
            for step in steps
            if isinstance(step, dict)
        ]
    return stable


def _stable_plan(plan: dict[str, Any]) -> dict[str, Any]:
    stable = _selected(plan, ("action", "input_schema_id", "result_schema_id"))
    stable["resource"] = _stable_resource(plan.get("resource"))
    if isinstance(plan.get("operation"), dict):
        stable["operation"] = _stable_operation(plan["operation"])
    if isinstance(plan.get("policy"), dict):
        stable["policy"] = _selected(plan["policy"], ("outcome", "reason"))
    return stable


def _stable_job(job: dict[str, Any]) -> dict[str, Any]:
    stable = _selected(
        job,
        (
            "id",
            "action",
            "ingress",
            "state",
            "progress_current",
            "progress_total",
            "progress_message",
            "approval_id",
            "submitted_at",
            "started_at",
            "finished_at",
            "updated_at",
        ),
    )
    stable["resource"] = _stable_resource(job.get("resource"))
    if isinstance(job.get("actor"), dict):
        stable["actor"] = _selected(job["actor"], ("actor_type", "id", "source"))
    if isinstance(job.get("plan"), dict):
        stable["plan"] = _stable_operation(job["plan"])
    if job.get("result") is None and "result" in job:
        stable["result"] = None
    if isinstance(job.get("error"), dict):
        stable["error"] = _selected(
            job["error"], ("code", "message", "retryable", "job_id")
        )
    elif job.get("error") is None and "error" in job:
        stable["error"] = None
    return stable


def _canonical_uuid(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    try:
        canonical = str(uuid.UUID(value))
    except (ValueError, AttributeError):
        return None
    return canonical if value == canonical else None


def _canonical_resource_id(value: Any) -> str:
    if not isinstance(value, str):
        raise ValueError("resource_id must be a canonical UUID")
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError) as error:
        raise ValueError("resource_id must be a canonical UUID") from error
    canonical = str(parsed)
    if value != canonical:
        raise ValueError("resource_id must use canonical lowercase UUID form")
    return canonical


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _has_fields(value: Any, fields: tuple[str, ...]) -> bool:
    return isinstance(value, dict) and all(field in value for field in fields)


def _valid_resource(resource: Any, resource_id: str) -> bool:
    return (
        _has_fields(resource, ("id", "kind", "display_name", "revision"))
        and resource.get("id") == resource_id
        and _canonical_uuid(resource.get("id")) == resource_id
        and resource.get("kind") == "proxmox_guest"
        and isinstance(resource.get("display_name"), str)
        and _is_int(resource.get("revision"))
    )


def _valid_operation(operation: Any) -> bool:
    if not _has_fields(
        operation,
        (
            "schema_version",
            "title",
            "risk",
            "changes",
            "preview",
            "external_fingerprint",
            "steps",
        ),
    ):
        return False
    changes = operation.get("changes")
    steps = operation.get("steps")
    return (
        operation.get("schema_version") == 1
        and isinstance(operation.get("title"), str)
        and isinstance(operation.get("risk"), str)
        and isinstance(changes, list)
        and all(
            isinstance(change, dict)
            and isinstance(change.get("label"), str)
            and isinstance(change.get("value"), str)
            for change in changes
        )
        and (
            operation.get("preview") is None
            or isinstance(operation.get("preview"), str)
        )
        and isinstance(operation.get("external_fingerprint"), str)
        and isinstance(steps, list)
        and bool(steps)
        and all(
            isinstance(step, dict)
            and all(
                isinstance(step.get(field), str)
                for field in ("kind", "name", "retry_class", "recovery_class")
            )
            for step in steps
        )
    )


def _valid_plan(plan: Any, action: str, resource_id: str) -> bool:
    if not _has_fields(
        plan,
        (
            "action",
            "resource",
            "input_schema_id",
            "result_schema_id",
            "operation",
            "policy",
        ),
    ):
        return False
    policy = plan.get("policy")
    return (
        plan.get("action") == action
        and _valid_resource(plan.get("resource"), resource_id)
        and plan.get("input_schema_id") == f"{action}.input.v1"
        and plan.get("result_schema_id") == f"{action}.result.v1"
        and _valid_operation(plan.get("operation"))
        and _has_fields(policy, ("outcome", "reason"))
        and policy.get("outcome") in {"allow", "require_approval", "deny"}
        and (policy.get("reason") is None or isinstance(policy.get("reason"), str))
    )


def _valid_actor(actor: Any) -> bool:
    return (
        _has_fields(actor, ("actor_type", "id", "source"))
        and actor.get("actor_type")
        in {"human", "api_token", "automation", "plugin", "node", "ai", "system"}
        and (actor.get("id") is None or isinstance(actor.get("id"), str))
        and (actor.get("source") is None or isinstance(actor.get("source"), str))
    )


def _valid_job_error(error: Any) -> bool:
    if error is None:
        return True
    if not _has_fields(error, ("code", "message", "retryable", "job_id")):
        return False
    job_id = error.get("job_id")
    return (
        isinstance(error.get("code"), str)
        and isinstance(error.get("message"), str)
        and isinstance(error.get("retryable"), bool)
        and (job_id is None or _canonical_uuid(job_id) == job_id)
    )


def _valid_job(job: Any, action: str, resource_id: str, operation: dict) -> bool:
    if not _has_fields(
        job,
        (
            "id",
            "action",
            "resource",
            "actor",
            "ingress",
            "state",
            "progress_current",
            "progress_total",
            "progress_message",
            "plan",
            "approval_id",
            "result",
            "error",
            "submitted_at",
            "started_at",
            "finished_at",
            "updated_at",
        ),
    ):
        return False
    approval_id = job.get("approval_id")
    progress_message = job.get("progress_message")
    return (
        _canonical_uuid(job.get("id")) == job.get("id")
        and job.get("action") == action
        and _valid_resource(job.get("resource"), resource_id)
        and _valid_actor(job.get("actor"))
        and isinstance(job.get("ingress"), str)
        and job.get("state")
        in {
            "awaiting_approval",
            "queued",
            "running",
            "succeeded",
            "failed",
            "cancelled",
            "needs_attention",
            "rejected",
            "expired",
        }
        and _is_int(job.get("progress_current"))
        and _is_int(job.get("progress_total"))
        and (progress_message is None or isinstance(progress_message, str))
        and _valid_operation(job.get("plan"))
        and _stable_operation(job["plan"]) == _stable_operation(operation)
        and (approval_id is None or _canonical_uuid(approval_id) == approval_id)
        and _valid_job_error(job.get("error"))
        and _is_int(job.get("submitted_at"))
        and (job.get("started_at") is None or _is_int(job.get("started_at")))
        and (job.get("finished_at") is None or _is_int(job.get("finished_at")))
        and _is_int(job.get("updated_at"))
    )


def _idempotency_key(tool: str, request_id: str) -> str:
    operation = f"{tool}:{request_id}".encode("ascii")
    return f"mcp-v1-{hashlib.sha256(operation).hexdigest()}"


async def canonical_mutation(
    tool: str,
    request_id: str,
    resource_id: str,
    action: str,
    input_data: dict | None = None,
) -> dict[str, Any]:
    input_data = input_data or {}
    base = f"/api/resources/{resource_id}/actions/{action}"
    try:
        plan_response = await client().post(
            f"{base}/plan", json={"input": input_data}
        )
    except Exception:
        return {
            "error": {
                "code": "upstream_unavailable",
                "message": "VoidTower could not be reached before submission.",
            }
        }
    try:
        plan = plan_response.json()
    except Exception:
        return {
            "error": {
                "code": "upstream_error",
                "message": "VoidTower returned an invalid canonical plan response.",
            }
        }
    if plan_response.status_code != 200:
        return _stable_error(plan)
    canonical_plan = plan.get("plan") if isinstance(plan, dict) else None
    if not _valid_plan(canonical_plan, action, resource_id):
        return {
            "error": {
                "code": "upstream_error",
                "message": "VoidTower returned an invalid canonical plan response.",
            }
        }
    assert isinstance(canonical_plan, dict)
    key = _idempotency_key(tool, request_id)
    try:
        submit_response = await client().post(
            base,
            json={"input": input_data},
            headers={"Idempotency-Key": key},
        )
    except Exception:
        return _ambiguous_submission(key)
    try:
        submitted = submit_response.json()
    except Exception:
        return _ambiguous_submission(key)
    if submit_response.status_code != 202:
        if submit_response.status_code in {400, 401, 403, 404, 409, 422}:
            return _stable_error(submitted)
        return _ambiguous_submission(key)
    job = submitted.get("job") if isinstance(submitted, dict) else None
    raw_approval_id = job.get("approval_id") if isinstance(job, dict) else None
    approval_id = (
        _canonical_uuid(raw_approval_id) if raw_approval_id is not None else None
    )
    if (
        not _valid_job(job, action, resource_id, canonical_plan["operation"])
        or (raw_approval_id is not None and approval_id is None)
    ):
        return _ambiguous_submission(key)
    assert isinstance(job, dict)
    return {
        "plan": _stable_plan(canonical_plan),
        "job": _stable_job(job),
        "approval": approval_id,
        "idempotency_key": key,
    }


def _text(data: Any) -> list[types.TextContent]:
    return [types.TextContent(type="text", text=json.dumps(data, indent=2))]


# ─── Tool definitions ─────────────────────────────────────────────────────────

async def list_tools() -> list[types.Tool]:
    tools = [
        types.Tool(
            name="vt_get_metrics",
            description="Get current system metrics: CPU, RAM, disk, network, top processes",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_services",
            description="List all systemd services with their status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_control_service",
            description="Start, stop, restart, enable, or disable a systemd service",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Service name (e.g. nginx)"},
                    "action": {"type": "string", "enum": ["start", "stop", "restart", "enable", "disable"]},
                },
                "required": ["name", "action"],
            },
        ),
        types.Tool(
            name="vt_list_containers",
            description="List all Docker containers with state and ports",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_control_container",
            description="Start, stop, restart, or remove a Docker container",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Container ID or short ID"},
                    "action": {"type": "string", "enum": ["start", "stop", "restart", "remove"]},
                },
                "required": ["id", "action"],
            },
        ),
        types.Tool(
            name="vt_list_alerts",
            description="List active infrastructure alerts",
            inputSchema={
                "type": "object",
                "properties": {
                    "state": {"type": "string", "enum": ["active", "acknowledged", "resolved"], "default": "active"},
                },
            },
        ),
        types.Tool(
            name="vt_list_deployed_apps",
            description="List apps deployed from the App Vault",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_deploy_app",
            description="Deploy an app from the App Vault catalog",
            inputSchema={
                "type": "object",
                "properties": {
                    "app_id": {"type": "string", "description": "App ID from the catalog (e.g. 'nextcloud')"},
                    "project_name": {"type": "string", "description": "Optional Docker Compose project name"},
                    "env_overrides": {
                        "type": "object",
                        "description": "Optional env var overrides (key/value pairs, e.g. {\"ADMIN_PASSWORD\": \"secret\"})",
                        "additionalProperties": {"type": "string"},
                    },
                },
                "required": ["app_id"],
            },
        ),
        types.Tool(
            name="vt_create_proxy",
            description="Create an nginx reverse proxy rule",
            inputSchema={
                "type": "object",
                "properties": {
                    "domain": {"type": "string", "description": "Domain name (e.g. app.example.com)"},
                    "upstream": {"type": "string", "description": "Upstream URL (e.g. http://localhost:8080)"},
                    "ssl": {"type": "boolean", "default": False},
                    "allow_embed": {"type": "boolean", "default": True},
                },
                "required": ["domain", "upstream"],
            },
        ),
        types.Tool(
            name="vt_list_backups",
            description="List backup configurations and their last run status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_run_backup",
            description="Trigger an immediate backup for a configured backup job",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Backup config ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_get_audit_log",
            description="Fetch recent audit log entries",
            inputSchema={
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "default": 20, "maximum": 100},
                    "offset": {"type": "integer", "default": 0},
                },
            },
        ),
        types.Tool(
            name="vt_list_users",
            description="List all VoidTower users",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_run_diagnostics",
            description="Run the VoidTower diagnostics check suite",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_timeline",
            description="Get a filtered activity timeline of all actions in VoidTower",
            inputSchema={
                "type": "object",
                "properties": {
                    "category": {"type": "string", "description": "Filter by category: auth, containers, services, backups, networking, etc."},
                    "search": {"type": "string", "description": "Free-text search"},
                    "limit": {"type": "integer", "default": 30},
                },
            },
        ),
        types.Tool(
            name="vt_list_wireguard_peers",
            description="List WireGuard VPN peers and their connection status",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_automations",
            description="List all configured automation jobs with their schedule, last status, and last run time",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_run_automation_job",
            description="Trigger an automation job immediately and wait for its output",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Automation job ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_get_container_logs",
            description="Get recent log lines from a Docker container",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Container ID or short ID"},
                    "tail": {"type": "integer", "default": 100, "description": "Number of lines to return"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_acknowledge_alert",
            description="Acknowledge an active infrastructure alert",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Alert ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_resolve_alert",
            description="Mark an infrastructure alert as resolved",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Alert ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_get_capabilities",
            description="Detect which tools are installed and available on this system (Docker, nginx, WireGuard, GPU, restic, etc.)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_storage",
            description="List block storage devices, mount points, and disk health",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_network_neighbors",
            description="List devices on the local network (ARP/LAN scan)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_secrets",
            description="List secret names and descriptions (values are never returned)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_status_summary",
            description="Get a high-level health summary: active alerts, failing status checks, and recent failures",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_service_logs",
            description="Get recent log lines from a systemd service",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Service name (e.g. nginx)"},
                    "tail": {"type": "integer", "default": 100, "description": "Number of lines to return"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_list_proxies",
            description="List all nginx reverse proxy rules with their domain, upstream, SSL status, and enabled state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_toggle_proxy",
            description="Enable or disable a specific nginx reverse proxy rule",
            inputSchema={
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "Proxy rule ID"},
                },
                "required": ["id"],
            },
        ),
        types.Tool(
            name="vt_list_firewall_rules",
            description="List all active firewall rules (ufw/firewalld/iptables depending on what is installed)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_app_catalog",
            description="List all apps available in the App Vault catalog (installable one-click apps)",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_get_app_status",
            description="Get the current status and container health of a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name (as shown in deployed apps list)"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_get_app_logs",
            description="Get recent log lines from a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                    "tail": {"type": "integer", "default": 100, "description": "Number of lines to return"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_control_app",
            description="Start, stop, restart, or redeploy a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                    "action": {"type": "string", "enum": ["start", "stop", "restart", "redeploy"]},
                },
                "required": ["name", "action"],
            },
        ),
        types.Tool(
            name="vt_get_app_compose",
            description="Read the Docker Compose file for a deployed App Vault application",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_update_app_compose",
            description="Replace the Docker Compose file for a deployed App Vault application. Changes take effect on next redeploy.",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                    "content": {"type": "string", "description": "Full YAML content of the new compose file"},
                },
                "required": ["name", "content"],
            },
        ),
        types.Tool(
            name="vt_remove_app",
            description="Remove a deployed App Vault application (removes config only — data on disk is not deleted)",
            inputSchema={
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "App project name"},
                },
                "required": ["name"],
            },
        ),
        types.Tool(
            name="vt_list_vms",
            description="List all Proxmox virtual machines and LXC containers across all configured Proxmox hosts",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_control_vm",
            description="Start, stop, reboot, or shutdown a Proxmox VM or LXC container",
            inputSchema={
                "type": "object",
                "properties": {
                    "resource_id": {"type": "string", "format": "uuid", "description": "Canonical VoidTower resources.id for the Proxmox guest"},
                    "action": {"type": "string", "enum": ["start", "stop", "reboot", "shutdown"]},
                    "request_id": {"type": "string", "format": "uuid", "description": "Caller-generated operation UUID; reuse only when retrying the same intended operation"},
                },
                "required": ["resource_id", "action", "request_id"],
                "additionalProperties": False,
            },
        ),
        types.Tool(
            name="vt_list_status_checks",
            description="List all HTTP/TCP status checks and their current up/down state",
            inputSchema={"type": "object", "properties": {}},
        ),
        types.Tool(
            name="vt_list_tags",
            description="List all resource tags defined in VoidTower",
            inputSchema={"type": "object", "properties": {}},
        ),
    ]
    return [tool for tool in tools if tool.name in ADVERTISED_TOOLS]


# ─── Tool handlers ────────────────────────────────────────────────────────────

async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
    if name not in ADVERTISED_TOOLS:
        return _text({"error": {"code": "unknown_tool", "message": "Unknown tool"}})
    try:
        match name:
            case "vt_get_metrics":
                return _text(await vt_get("/api/metrics/current"))

            case "vt_list_services":
                return _text(await vt_get("/api/services"))

            case "vt_list_containers":
                return _text(await vt_get("/api/containers"))

            case "vt_list_alerts":
                state = arguments.get("state", "active")
                return _text(await vt_get("/api/alerts", params={"state": state}))

            case "vt_list_deployed_apps":
                return _text(await vt_get("/api/apps/deployed"))

            case "vt_list_backups":
                return _text(await vt_get("/api/backups"))

            case "vt_get_audit_log":
                limit = arguments.get("limit", 20)
                offset = arguments.get("offset", 0)
                return _text(await vt_get("/api/audit", params={"limit": limit, "offset": offset}))

            case "vt_list_users":
                return _text(await vt_get("/api/users"))

            case "vt_run_diagnostics":
                return _text(await vt_get("/api/diagnostics"))

            case "vt_get_timeline":
                params: dict = {"limit": arguments.get("limit", 30)}
                if cat := arguments.get("category"):
                    params["category"] = cat
                if s := arguments.get("search"):
                    params["search"] = s
                return _text(await vt_get("/api/timeline", params=params))

            case "vt_list_wireguard_peers":
                return _text(await vt_get("/api/wireguard"))

            case "vt_list_automations":
                return _text(await vt_get("/api/automation"))

            case "vt_get_container_logs":
                tail = arguments.get("tail", 100)
                data = await vt_get(f"/api/containers/{arguments['id']}/logs", params={"tail": tail})
                lines = data.get("lines", [])
                return [types.TextContent(type="text", text="\n".join(lines))]

            case "vt_get_capabilities":
                return _text(await vt_get("/api/capabilities"))

            case "vt_get_storage":
                devices, mounts = await asyncio.gather(
                    vt_get("/api/storage/devices"),
                    vt_get("/api/storage/mounts"),
                )
                return _text({"devices": devices, "mounts": mounts})

            case "vt_get_network_neighbors":
                return _text(await vt_get("/api/network/neighbors"))

            case "vt_list_secrets":
                return _text(await vt_get("/api/secrets"))

            case "vt_get_status_summary":
                alerts, checks = await asyncio.gather(
                    vt_get("/api/alerts", params={"state": "active"}),
                    vt_get("/api/status-checks"),
                )
                alert_list = alerts.get("alerts", alerts) if isinstance(alerts, dict) else alerts
                check_list = checks.get("checks", checks) if isinstance(checks, dict) else checks
                failing = [c for c in check_list if isinstance(c, dict) and c.get("last_status") == "down"]
                return _text({
                    "active_alerts": len(alert_list),
                    "alerts": alert_list,
                    "failing_checks": len(failing),
                    "failing": failing,
                })

            case "vt_get_service_logs":
                tail = arguments.get("tail", 100)
                data = await vt_get(f"/api/services/{arguments['name']}/logs", params={"tail": tail})
                lines = data.get("lines", []) if isinstance(data, dict) else data
                return [types.TextContent(type="text", text="\n".join(lines) if isinstance(lines, list) else str(lines))]

            case "vt_list_proxies":
                return _text(await vt_get("/api/proxy"))

            case "vt_list_firewall_rules":
                return _text(await vt_get("/api/firewall"))

            case "vt_list_app_catalog":
                return _text(await vt_get("/api/apps/catalog"))

            case "vt_get_app_status":
                return _text(await vt_get(f"/api/apps/{arguments['name']}/status"))

            case "vt_get_app_logs":
                tail = arguments.get("tail", 100)
                data = await vt_get(f"/api/apps/{arguments['name']}/logs", params={"tail": tail})
                lines = data.get("lines", []) if isinstance(data, dict) else data
                return [types.TextContent(type="text", text="\n".join(lines) if isinstance(lines, list) else str(lines))]

            case "vt_get_app_compose":
                return _text(await vt_get(f"/api/apps/{arguments['name']}/compose"))

            case "vt_list_vms":
                return _text(await vt_get("/api/vms/proxmox/vms"))

            case "vt_control_vm":
                try:
                    resource_id = _canonical_resource_id(arguments.get("resource_id"))
                    request_id = _canonical_uuid(arguments.get("request_id"))
                    if request_id is None:
                        raise ValueError("request_id must be a canonical UUID")
                    requested_action = arguments.get("action")
                    action = (
                        VM_ACTIONS.get(requested_action)
                        if isinstance(requested_action, str)
                        else None
                    )
                    if action is None:
                        raise ValueError("unsupported VM lifecycle action")
                except ValueError:
                    return _text(
                        {
                            "error": {
                                "code": "invalid_arguments",
                                "message": "The VM mutation arguments are invalid.",
                            }
                        }
                    )
                return _text(
                    await canonical_mutation(
                        "vt_control_vm",
                        request_id,
                        resource_id,
                        action,
                    )
                )

            case "vt_list_status_checks":
                return _text(await vt_get("/api/status-checks"))

            case "vt_list_tags":
                return _text(await vt_get("/api/tags"))

            case _:
                return _text({"error": {"code": "unknown_tool", "message": "Unknown tool"}})

    except httpx.HTTPStatusError as e:
        return _text({"error": f"HTTP {e.response.status_code}", "detail": e.response.text})
    except Exception as e:
        return _text({"error": str(e)})


# ─── Entry point ─────────────────────────────────────────────────────────────

async def _list_tools_handler(_context, _params):
    return types.ListToolsResult(tools=await list_tools())


async def _call_tool_handler(_context, params):
    content = await call_tool(params.name, params.arguments or {})
    return types.CallToolResult(content=content)


server = Server(
    "voidtower",
    on_list_tools=_list_tools_handler,
    on_call_tool=_call_tool_handler,
)


async def main():
    async with stdio_server() as (read_stream, write_stream):
        await server.run(read_stream, write_stream, server.create_initialization_options())


if __name__ == "__main__":
    asyncio.run(main())
