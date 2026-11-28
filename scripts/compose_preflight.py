#!/usr/bin/env python3
"""Fail-closed preflight for the root Compose/Paperless port migration."""

from __future__ import annotations

import re
import subprocess
import sys
from collections.abc import Iterable


_LEGACY_PAPERLESS_PORT = re.compile(r":8000->8000/(?:tcp|udp)(?:,|$)")


def conflicting_host_ports(lines: Iterable[str]) -> list[str]:
    """Return container names still publishing host port 8000 to port 8000.

    The root VoidTower verifier publishes host 8000 to nginx port 80 and is
    intentionally not a conflict. A legacy Paperless container publishes
    host 8000 to its application port 8000 and must be recreated on host 8001
    before the root Compose stack starts.
    """
    conflicts: list[str] = []
    for line in lines:
        name, separator, ports = line.partition("\t")
        if separator and name and _LEGACY_PAPERLESS_PORT.search(ports):
            conflicts.append(name)
    return sorted(set(conflicts))


def main() -> int:
    """Check Docker's current published ports without changing containers."""
    try:
        result = subprocess.run(
            ["docker", "ps", "--format", "{{.Names}}\t{{.Ports}}"],
            check=False,
            capture_output=True,
            text=True,
        )
    except OSError as exc:
        print(f"Compose preflight could not execute Docker: {exc}", file=sys.stderr)
        return 2

    if result.returncode != 0:
        print("Compose preflight could not inspect running Docker containers.", file=sys.stderr)
        return 2

    conflicts = conflicting_host_ports(result.stdout.splitlines())
    if conflicts:
        names = ", ".join(conflicts)
        print(
            f"Compose preflight blocked: {names} still publish host port 8000 to container port 8000. "
            "Stop/recreate the legacy Paperless deployment from app-vault/apps/paperless.yml "
            "so it uses host port 8001, then rerun this check before `docker compose up -d`.",
            file=sys.stderr,
        )
        return 1

    print("Compose preflight passed: no legacy host-port-8000-to-container-8000 publication detected.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
