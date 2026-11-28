import ast
import importlib.util
import json
import os
import re
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any

import httpx
from mcp import ClientSession, StdioServerParameters, types
from mcp.client.stdio import stdio_client


SERVER_PATH = Path(__file__).resolve().parents[1] / "voidtower_server.py"
RESOURCE_ID = "11111111-1111-4111-8111-111111111111"
JOB_ID = "22222222-2222-4222-8222-222222222222"
REQUEST_ID = "33333333-3333-4333-8333-333333333333"
NEXT_REQUEST_ID = "44444444-4444-4444-8444-444444444444"
NEXT_JOB_ID = "55555555-5555-4555-8555-555555555555"


def load_server() -> Any:
    spec = importlib.util.spec_from_file_location("voidtower_server_under_test", SERVER_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class FakeResponse:
    def __init__(self, status_code, body):
        self.status_code = status_code
        self._body = body

    def json(self):
        if isinstance(self._body, Exception):
            raise self._body
        return self._body


class FakeClient:
    def __init__(self, responses):
        self.responses = iter(responses)
        self.posts = []

    async def post(self, path, *, json=None, headers=None):
        self.posts.append({"path": path, "json": json, "headers": headers or {}})
        response = next(self.responses)
        if isinstance(response, Exception):
            raise response
        return response


def canonical_resource():
    return {
        "id": RESOURCE_ID,
        "kind": "proxmox_guest",
        "display_name": "vm-101",
        "revision": 7,
    }


def canonical_operation(action="proxmox.guest.start"):
    return {
        "schema_version": 1,
        "title": f"Control guest: {action}",
        "risk": "mutate",
        "changes": [],
        "preview": None,
        "external_fingerprint": f"fixture:{action}",
        "steps": [
            {
                "kind": "http",
                "name": action,
                "retry_class": "never",
                "recovery_class": "reconcile",
            }
        ],
    }


def canonical_plan(action="proxmox.guest.start"):
    return {
        "action": action,
        "resource": canonical_resource(),
        "input_schema_id": f"{action}.input.v1",
        "result_schema_id": f"{action}.result.v1",
        "operation": canonical_operation(action),
        "policy": {"outcome": "allow", "reason": None},
    }


def canonical_job(action="proxmox.guest.start", job_id=JOB_ID):
    return {
        "id": job_id,
        "action": action,
        "resource": canonical_resource(),
        "actor": {"actor_type": "api_token", "id": "fixture-token", "source": "mcp"},
        "ingress": "mcp",
        "state": "queued",
        "progress_current": 0,
        "progress_total": 1,
        "progress_message": None,
        "plan": canonical_operation(action),
        "approval_id": None,
        "result": None,
        "error": None,
        "submitted_at": 1,
        "started_at": None,
        "finished_at": None,
        "updated_at": 1,
    }


class VoidTowerServerContractTests(unittest.IsolatedAsyncioTestCase):
    async def test_vm_control_plans_then_submits_canonical_action_with_deterministic_idempotency(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, {"job": canonical_job()}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(
            [call["path"] for call in fake.posts],
            [
                f"/api/resources/{RESOURCE_ID}/actions/proxmox.guest.start/plan",
                f"/api/resources/{RESOURCE_ID}/actions/proxmox.guest.start",
            ],
        )
        self.assertEqual(fake.posts[0]["json"], {"input": {}})
        self.assertEqual(fake.posts[1]["json"], {"input": {}})
        key = fake.posts[1]["headers"]["Idempotency-Key"]
        self.assertRegex(key, r"^mcp-v1-[0-9a-f]{64}$")
        self.assertLessEqual(len(key), 128)
        payload = json.loads(result[0].text)
        self.assertEqual(payload["plan"]["action"], "proxmox.guest.start")
        self.assertEqual(payload["job"]["id"], JOB_ID)
        self.assertEqual(payload["approval"], None)
        self.assertEqual(payload["idempotency_key"], key)

    async def test_request_id_scopes_idempotency_to_one_operation(self):
        server = load_server()
        def plan(action):
            return FakeResponse(200, {"plan": canonical_plan(action)})

        def job(action, job_id):
            return FakeResponse(202, {"job": canonical_job(action, job_id)})

        fake = FakeClient(
            [
                plan("proxmox.guest.start"),
                job("proxmox.guest.start", JOB_ID),
                plan("proxmox.guest.start"),
                job("proxmox.guest.start", JOB_ID),
                plan("proxmox.guest.stop"),
                FakeResponse(
                    409,
                    {
                        "error": {
                            "code": "idempotency_conflict",
                            "job_id": JOB_ID,
                        }
                    },
                ),
                plan("proxmox.guest.start"),
                job("proxmox.guest.start", NEXT_JOB_ID),
            ]
        )
        server._client = fake

        await server.call_tool(
            "vt_control_vm",
            {
                "resource_id": RESOURCE_ID,
                "action": "start",
                "request_id": REQUEST_ID,
            },
        )
        await server.call_tool(
            "vt_control_vm",
            {
                "action": "start",
                "resource_id": RESOURCE_ID,
                "request_id": REQUEST_ID,
            },
        )
        conflict = await server.call_tool(
            "vt_control_vm",
            {
                "resource_id": RESOURCE_ID,
                "action": "stop",
                "request_id": REQUEST_ID,
            },
        )
        await server.call_tool(
            "vt_control_vm",
            {
                "resource_id": RESOURCE_ID,
                "action": "start",
                "request_id": NEXT_REQUEST_ID,
            },
        )

        keys = [fake.posts[index]["headers"]["Idempotency-Key"] for index in (1, 3, 5, 7)]
        self.assertEqual(keys[0], keys[1])
        self.assertEqual(keys[0], keys[2])
        self.assertNotEqual(keys[0], keys[3])
        self.assertEqual(
            json.loads(conflict[0].text)["error"]["code"],
            "idempotency_conflict",
        )

    async def test_approved_vm_lifecycle_actions_use_exact_canonical_paths(self):
        server = load_server()
        for requested, canonical in {
            "start": "proxmox.guest.start",
            "stop": "proxmox.guest.stop",
            "reboot": "proxmox.guest.reboot",
            "shutdown": "proxmox.guest.shutdown",
        }.items():
            fake = FakeClient(
                [
                    FakeResponse(200, {"plan": canonical_plan(canonical)}),
                    FakeResponse(202, {"job": canonical_job(canonical)}),
                ]
            )
            server._client = fake

            await server.call_tool(
                "vt_control_vm",
                {"resource_id": RESOURCE_ID, "action": requested, "request_id": REQUEST_ID},
            )

            self.assertEqual(
                [call["path"] for call in fake.posts],
                [
                    f"/api/resources/{RESOURCE_ID}/actions/{canonical}/plan",
                    f"/api/resources/{RESOURCE_ID}/actions/{canonical}",
                ],
            )

    async def test_only_canonical_mutations_are_advertised_or_callable(self):
        server = load_server()
        legacy_mutations = {
            "vt_control_service",
            "vt_control_container",
            "vt_deploy_app",
            "vt_create_proxy",
            "vt_run_backup",
            "vt_run_automation_job",
            "vt_acknowledge_alert",
            "vt_resolve_alert",
            "vt_toggle_proxy",
            "vt_control_app",
            "vt_update_app_compose",
            "vt_remove_app",
        }

        tools = await server.list_tools()
        advertised = {tool.name for tool in tools}
        vm_tool = next(tool for tool in tools if tool.name == "vt_control_vm")
        self.assertEqual(
            set(vm_tool.model_dump(by_alias=True)["inputSchema"]["required"]),
            {"resource_id", "action", "request_id"},
        )
        explicitly_safe = getattr(server, "READ_ONLY_TOOLS", set()) | getattr(
            server, "CANONICAL_MUTATION_TOOLS", set()
        )
        self.assertEqual(advertised, explicitly_safe)
        self.assertEqual(server.DISABLED_LEGACY_MUTATIONS, legacy_mutations)
        self.assertTrue(legacy_mutations.isdisjoint(advertised))
        self.assertIn("vt_control_vm", advertised)

        fake = FakeClient([FakeResponse(500, {"error": "must not be called"})])
        server._client = fake
        for name in sorted(legacy_mutations):
            result = await server.call_tool(name, {})
            self.assertEqual(
                json.loads(result[0].text),
                {"error": {"code": "unknown_tool", "message": "Unknown tool"}},
            )
        self.assertEqual(fake.posts, [])

    def test_source_contains_no_legacy_direct_mutation_path(self):
        server = load_server()
        tree = ast.parse(SERVER_PATH.read_text(encoding="utf-8"))
        call_tool = next(
            node
            for node in tree.body
            if isinstance(node, ast.AsyncFunctionDef) and node.name == "call_tool"
        )
        case_names = {
            case.pattern.value.value
            for match in (node for node in ast.walk(call_tool) if isinstance(node, ast.Match))
            for case in match.cases
            if isinstance(case.pattern, ast.MatchValue)
            and isinstance(case.pattern.value, ast.Constant)
            and isinstance(case.pattern.value.value, str)
        }
        self.assertTrue(server.DISABLED_LEGACY_MUTATIONS.isdisjoint(case_names))

        violations = []
        for function in (
            node
            for node in tree.body
            if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
            and node.name != "canonical_mutation"
        ):
            for node in ast.walk(function):
                if not isinstance(node, ast.Call):
                    continue
                if isinstance(node.func, ast.Name) and node.func.id == "vt_post":
                    violations.append(f"{function.name}:vt_post")
                elif isinstance(node.func, ast.Attribute) and node.func.attr in {
                    "post",
                    "put",
                    "patch",
                    "delete",
                    "request",
                }:
                    violations.append(f"{function.name}:{node.func.attr}")

        self.assertEqual(violations, [])

    async def test_canonical_success_contract_drops_unknown_fields(self):
        server = load_server()
        plan = canonical_plan()
        plan["provider_body"] = "must-not-leak"
        plan["resource"]["provider_body"] = "must-not-leak"
        plan["operation"]["provider_body"] = "must-not-leak"
        plan["policy"]["debug"] = "must-not-leak"
        job = canonical_job()
        job["provider_body"] = "must-not-leak"
        job["resource"]["provider_body"] = "must-not-leak"
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": plan}),
                FakeResponse(202, {"job": job}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        payload = json.loads(result[0].text)
        self.assertEqual(
            payload["plan"]["operation"]["title"],
            "Control guest: proxmox.guest.start",
        )
        self.assertEqual(payload["job"]["id"], JOB_ID)
        self.assertNotIn("must-not-leak", result[0].text)
        self.assertNotIn("provider_body", result[0].text)
        self.assertNotIn("debug", result[0].text)

    async def test_canonical_error_contract_drops_raw_provider_fields(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(
                    403,
                    {
                        "error": {
                            "code": "insufficient_scope",
                            "message": "This API token's scopes do not permit this action.",
                            "job_id": "must-not-leak",
                            "provider_body": "Authorization: PVEAPIToken=must-not-leak",
                        },
                        "debug": "must-not-leak",
                    },
                )
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        payload = json.loads(result[0].text)
        self.assertEqual(
            payload,
            {
                "error": {
                    "code": "insufficient_scope",
                    "message": "This API token's scopes do not permit this action.",
                }
            },
        )
        self.assertNotIn("must-not-leak", result[0].text)

    async def test_idempotency_conflict_preserves_only_canonical_job_id(self):
        server = load_server()
        conflict_job_id = "22222222-2222-4222-8222-222222222222"
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(
                    409,
                    {
                        "error": {
                            "code": "idempotency_conflict",
                            "message": "raw backend message",
                            "job_id": conflict_job_id,
                            "provider_body": "must-not-leak",
                        }
                    },
                ),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "idempotency_conflict",
                    "message": "The idempotency key belongs to different intent.",
                    "job_id": conflict_job_id,
                }
            },
        )

    async def test_invalid_vm_arguments_use_stable_error_contract(self):
        server = load_server()
        server._client = FakeClient([])

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": "not-a-canonical-uuid", "action": "destroy"},
        )

        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "invalid_arguments",
                    "message": "The VM mutation arguments are invalid.",
                }
            },
        )
        self.assertEqual(server._client.posts, [])

    async def test_incomplete_plan_fails_closed_before_submission(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(
                    200,
                    {
                        "plan": {
                            "action": "proxmox.guest.start",
                            "resource": {"id": RESOURCE_ID, "kind": "proxmox_guest"},
                        }
                    },
                ),
                FakeResponse(202, {"job": {"id": JOB_ID}}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text)["error"]["code"],
            "upstream_error",
        )

    async def test_plan_schema_ids_must_match_action(self):
        server = load_server()
        plan = canonical_plan()
        plan["input_schema_id"] = "other.action.input.v1"
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": plan}),
                FakeResponse(202, {"job": canonical_job()}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text).get("error", {}).get("code"),
            "upstream_error",
        )

    async def test_plan_requires_an_executable_step(self):
        server = load_server()
        plan = canonical_plan()
        plan["operation"]["steps"] = []
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": plan}),
                FakeResponse(202, {"job": canonical_job()}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text).get("error", {}).get("code"),
            "upstream_error",
        )

    async def test_plan_requires_serialized_nullable_fields(self):
        server = load_server()
        plan = canonical_plan()
        del plan["operation"]["preview"]
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": plan}),
                FakeResponse(202, {"job": canonical_job()}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text).get("error", {}).get("code"),
            "upstream_error",
        )

    async def test_invalid_plan_response_fails_closed_before_submission(self):
        server = load_server()
        fake = FakeClient([FakeResponse(200, {"raw": "must-not-leak"})])
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "upstream_error",
                    "message": "VoidTower returned an invalid canonical plan response.",
                }
            },
        )
        self.assertNotIn("must-not-leak", result[0].text)

    async def test_non_json_plan_response_fails_closed_before_submission(self):
        server = load_server()
        fake = FakeClient([FakeResponse(200, ValueError("raw provider page"))])
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "upstream_error",
                    "message": "VoidTower returned an invalid canonical plan response.",
                }
            },
        )
        self.assertNotIn("provider page", result[0].text)

    async def test_unexpected_plan_decode_failure_is_redacted(self):
        server = load_server()
        fake = FakeClient([FakeResponse(200, RuntimeError("decoder state must not leak"))])
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        payload = json.loads(result[0].text)
        self.assertIsInstance(payload.get("error"), dict)
        self.assertEqual(payload.get("error", {}).get("code"), "upstream_error")
        self.assertNotIn("decoder state", result[0].text)

    async def test_plan_transport_failure_is_stable_and_never_submits(self):
        server = load_server()
        fake = FakeClient([httpx.ConnectError("backend address must not leak")])
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "upstream_unavailable",
                    "message": "VoidTower could not be reached before submission.",
                }
            },
        )
        self.assertNotIn("backend address", result[0].text)

    async def test_plan_invalid_url_is_redacted_and_never_submits(self):
        server = load_server()
        fake = FakeClient([httpx.InvalidURL("configured upstream must not leak")])
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "upstream_unavailable",
                    "message": "VoidTower could not be reached before submission.",
                }
            },
        )
        self.assertNotIn("configured upstream", result[0].text)

    async def test_unexpected_plan_client_failure_is_redacted(self):
        server = load_server()
        fake = FakeClient([RuntimeError("client internals must not leak")])
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 1)
        payload = json.loads(result[0].text)
        self.assertIsInstance(payload.get("error"), dict)
        self.assertEqual(
            payload.get("error", {}).get("code"),
            "upstream_unavailable",
        )
        self.assertNotIn("client internals", result[0].text)

    async def test_invalid_submit_identity_is_ambiguous(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(
                    202,
                    {
                        "job": {
                            "id": "must-not-leak",
                            "action": "proxmox.guest.start",
                            "resource": {
                                "id": RESOURCE_ID,
                                "kind": "proxmox_guest",
                            },
                            "state": "awaiting_approval",
                            "approval_id": "must-not-leak",
                        }
                    },
                ),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        key = fake.posts[1]["headers"]["Idempotency-Key"]
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "ambiguous_submission",
                    "message": (
                        "The submission outcome is unknown; query canonical job state "
                        "before any retry."
                    ),
                },
                "idempotency_key": key,
                "recovery_path": f"/api/jobs/by-idempotency/{key}",
            },
        )
        self.assertNotIn("must-not-leak", result[0].text)

    async def test_incomplete_successful_job_is_ambiguous(self):
        server = load_server()
        job = canonical_job()
        del job["actor"]
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, {"job": job}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(
            json.loads(result[0].text).get("error", {}).get("code"),
            "ambiguous_submission",
        )

    async def test_job_requires_serialized_nullable_fields(self):
        server = load_server()
        job = canonical_job()
        del job["started_at"]
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, {"job": job}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(
            json.loads(result[0].text).get("error", {}).get("code"),
            "ambiguous_submission",
        )

    async def test_submitted_job_plan_must_match_advisory_plan(self):
        server = load_server()
        job = canonical_job()
        job["plan"] = canonical_operation("proxmox.guest.stop")
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, {"job": job}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(
            json.loads(result[0].text).get("error", {}).get("code"),
            "ambiguous_submission",
        )

    async def test_invalid_submit_response_is_ambiguous(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, {"raw": "must-not-leak"}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        key = fake.posts[1]["headers"]["Idempotency-Key"]
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "ambiguous_submission",
                    "message": (
                        "The submission outcome is unknown; query canonical job state "
                        "before any retry."
                    ),
                },
                "idempotency_key": key,
                "recovery_path": f"/api/jobs/by-idempotency/{key}",
            },
        )
        self.assertNotIn("must-not-leak", result[0].text)

    async def test_non_json_submit_response_is_ambiguous(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, ValueError("raw provider page")),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        key = fake.posts[1]["headers"]["Idempotency-Key"]
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "ambiguous_submission",
                    "message": (
                        "The submission outcome is unknown; query canonical job state "
                        "before any retry."
                    ),
                },
                "idempotency_key": key,
                "recovery_path": f"/api/jobs/by-idempotency/{key}",
            },
        )
        self.assertNotIn("provider page", result[0].text)

    async def test_unexpected_submit_decode_failure_is_ambiguous_and_redacted(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, RuntimeError("decoder state must not leak")),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 2)
        payload = json.loads(result[0].text)
        self.assertIsInstance(payload.get("error"), dict)
        self.assertEqual(payload.get("error", {}).get("code"), "ambiguous_submission")
        self.assertNotIn("decoder state", result[0].text)

    async def test_submit_transport_failure_is_ambiguous_and_not_blindly_retried(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                httpx.ReadTimeout("submit response was not received"),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 2)
        key = fake.posts[1]["headers"]["Idempotency-Key"]
        self.assertEqual(
            json.loads(result[0].text),
            {
                "error": {
                    "code": "ambiguous_submission",
                    "message": (
                        "The submission outcome is unknown; query canonical job state "
                        "before any retry."
                    ),
                },
                "idempotency_key": key,
                "recovery_path": f"/api/jobs/by-idempotency/{key}",
            },
        )

    async def test_unexpected_submit_client_failure_is_redacted_and_ambiguous(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                RuntimeError("client internals must not leak"),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        self.assertEqual(len(fake.posts), 2)
        payload = json.loads(result[0].text)
        self.assertIsInstance(payload.get("error"), dict)
        self.assertEqual(payload.get("error", {}).get("code"), "ambiguous_submission")
        self.assertNotIn("client internals", result[0].text)

    async def test_submit_gateway_error_is_ambiguous_with_recovery_handle(self):
        server = load_server()
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(502, {"error": {"code": "gateway_failure", "detail": "must-not-leak"}}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        key = fake.posts[1]["headers"]["Idempotency-Key"]
        payload = json.loads(result[0].text)
        self.assertEqual(payload["error"]["code"], "ambiguous_submission")
        self.assertEqual(payload["idempotency_key"], key)
        self.assertEqual(payload["recovery_path"], f"/api/jobs/by-idempotency/{key}")
        self.assertNotIn("must-not-leak", result[0].text)

    async def test_needs_attention_job_preserves_only_stable_error_fields(self):
        server = load_server()
        job = canonical_job()
        job["state"] = "needs_attention"
        job["error"] = {
            "code": "provider_outcome_unknown",
            "message": "Outcome requires operator reconciliation.",
            "retryable": False,
            "job_id": JOB_ID,
            "provider_body": "must-not-leak",
        }
        fake = FakeClient(
            [
                FakeResponse(200, {"plan": canonical_plan()}),
                FakeResponse(202, {"job": job}),
            ]
        )
        server._client = fake

        result = await server.call_tool(
            "vt_control_vm",
            {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
        )

        payload = json.loads(result[0].text)
        self.assertEqual(
            payload["job"]["error"],
            {
                "code": "provider_outcome_unknown",
                "message": "Outcome requires operator reconciliation.",
                "retryable": False,
                "job_id": JOB_ID,
            },
        )
        self.assertNotIn("must-not-leak", result[0].text)

    async def test_mcp_stdio_calls_canonical_mutation_over_http(self):
        requests = []

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                size = int(self.headers.get("Content-Length", "0"))
                body = json.loads(self.rfile.read(size) or b"{}")
                requests.append(
                    {
                        "path": self.path,
                        "body": body,
                        "authorization": self.headers.get("Authorization"),
                        "idempotency": self.headers.get("Idempotency-Key"),
                    }
                )
                action = "proxmox.guest.start"
                if self.path.endswith("/plan"):
                    status = 200
                    payload = {"plan": canonical_plan(action)}
                else:
                    status = 202
                    payload = {"job": canonical_job(action)}
                encoded = json.dumps(payload).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(encoded)))
                self.end_headers()
                self.wfile.write(encoded)

            def log_message(self, format, *args):
                del format, args

        http_server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=http_server.serve_forever, daemon=True)
        thread.start()
        try:
            params = StdioServerParameters(
                command=sys.executable,
                args=[str(SERVER_PATH)],
                env={
                    **os.environ,
                    "VOIDTOWER_URL": f"http://127.0.0.1:{http_server.server_port}",
                    "VOIDTOWER_TOKEN": "[REDACTED]",
                },
            )
            async with stdio_client(params) as (read_stream, write_stream):
                async with ClientSession(read_stream, write_stream) as session:
                    await session.initialize()
                    result = await session.call_tool(
                        "vt_control_vm",
                        {"resource_id": RESOURCE_ID, "action": "start", "request_id": REQUEST_ID},
                    )
            content = result.content[0]
            self.assertIsInstance(content, types.TextContent)
            assert isinstance(content, types.TextContent)
            payload = json.loads(content.text)
        finally:
            http_server.shutdown()
            http_server.server_close()
            thread.join(timeout=2)

        base = f"/api/resources/{RESOURCE_ID}/actions/proxmox.guest.start"
        self.assertEqual([item["path"] for item in requests], [f"{base}/plan", base])
        self.assertTrue(
            all(item["authorization"] == "Bearer [REDACTED]" for item in requests)
        )
        self.assertIsNone(requests[0]["idempotency"])
        self.assertRegex(requests[1]["idempotency"], r"^mcp-v1-[0-9a-f]{64}$")
        self.assertEqual(payload["job"]["id"], JOB_ID)
        self.assertIsNone(payload["approval"])
        self.assertTrue(
            re.fullmatch(r"mcp-v1-[0-9a-f]{64}", payload["idempotency_key"])
        )


if __name__ == "__main__":
    unittest.main()
