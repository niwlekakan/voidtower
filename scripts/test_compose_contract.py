#!/usr/bin/env python3
"""Effective Compose contracts for local readiness and App Vault coexistence."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from scripts.compose_preflight import conflicting_host_ports


ROOT = Path(__file__).resolve().parents[1]
PAPERLESS_MANIFEST = ROOT / "app-vault" / "apps" / "paperless.yml"
PAPERLESS_MCP_SERVER = ROOT / "odysseus-mcp-servers" / "paperless_server.py"


def effective_compose() -> dict:
    completed = subprocess.run(
        ["docker", "compose", "config", "--format", "json"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(completed.stdout)


def published_ports(service: dict) -> set[tuple[str | None, int, int]]:
    return {
        (port.get("host_ip"), int(port["published"]), int(port["target"]))
        for port in service["ports"]
    }


def effective_paperless_compose() -> dict:
    """Validate the App Vault manifest's nested Compose document effectively."""
    lines = PAPERLESS_MANIFEST.read_text(encoding="utf-8").splitlines(keepends=True)
    compose_start = next(index for index, line in enumerate(lines) if line.rstrip() == "compose:")
    compose_lines: list[str] = []
    for line in lines[compose_start + 1 :]:
        if line.strip() and not line.startswith("  "):
            break
        compose_lines.append(line[2:] if line.startswith("  ") else line)

    with tempfile.NamedTemporaryFile("w", suffix=".yml", encoding="utf-8") as compose_file:
        compose_file.writelines(compose_lines)
        compose_file.flush()
        completed = subprocess.run(
            ["docker", "compose", "-f", compose_file.name, "config", "--format", "json"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
    return json.loads(completed.stdout)


class ComposeReadinessContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.compose = effective_compose()
        cls.voidtower = cls.compose["services"]["voidtower"]

    def test_readiness_alias_is_loopback_and_uses_declared_api_port(self) -> None:
        ports = published_ports(self.voidtower)
        self.assertEqual(
            ports,
            {
                (None, 80, 80),
                (None, 443, 443),
                (None, 8745, 8745),
                ("127.0.0.1", 8000, 80),
                ("127.0.0.1", 8743, 80),
            },
        )
        self.assertIn(("127.0.0.1", 8000, 80), ports)

    def test_readiness_aliases_do_not_publish_the_backend_directly(self) -> None:
        ports = published_ports(self.voidtower)
        self.assertNotIn((None, 8000, 8743), ports)
        self.assertNotIn((None, 8743, 8743), ports)
        self.assertEqual(
            {(ip, published, target) for ip, published, target in ports if ip == "127.0.0.1"},
            {("127.0.0.1", 8000, 80), ("127.0.0.1", 8743, 80)},
        )

    def test_healthcheck_is_the_backend_health_boundary(self) -> None:
        self.assertEqual(
            self.voidtower["healthcheck"]["test"],
            ["CMD", "curl", "-fs", "http://127.0.0.1:8743/api/health"],
        )

    def test_public_ports_remain_unchanged(self) -> None:
        ports = published_ports(self.voidtower)
        self.assertEqual(
            {(None, 80, 80), (None, 443, 443), (None, 8745, 8745)}
            & ports,
            {(None, 80, 80), (None, 443, 443), (None, 8745, 8745)},
        )

    def test_paperless_retains_its_host_port_without_readiness_collision(self) -> None:
        paperless = effective_paperless_compose()["services"]["paperless"]
        self.assertEqual(published_ports(paperless), {(None, 8001, 8000)})
        self.assertNotIn((None, 8000, 80), published_ports(self.voidtower))

    def test_paperless_mcp_default_follows_direct_host_port(self) -> None:
        source = PAPERLESS_MCP_SERVER.read_text(encoding="utf-8")
        self.assertIn("PAPERLESS_URL=http://localhost:8001", source)
        self.assertIn('os.environ.get("PAPERLESS_URL", "http://localhost:8001")', source)

    def test_compose_preflight_ignores_loopback_verifier_alias(self) -> None:
        self.assertEqual(
            conflicting_host_ports(
                [
                    "voidtower\t127.0.0.1:8000->80/tcp, 127.0.0.1:8743->80/tcp",
                    "paperless\t0.0.0.0:8000->8000/tcp",
                ]
            ),
            ["paperless"],
        )


if __name__ == "__main__":
    unittest.main()
