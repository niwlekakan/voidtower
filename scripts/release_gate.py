#!/usr/bin/env python3
"""Run the declared, non-publishing gates for a VoidTower checkout.

The runner is deliberately an evidence collector, not a release publisher. It
never stages, commits, edits source, reads environment files, or claims runtime
support from a passing source-only gate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import resource
import signal
import subprocess
import sys
import tempfile
from typing import Any

from repo_truth import RepoTruthError, build_report, repository_root


SCHEMA_VERSION = 1
MAX_OUTPUT_BYTES = 16 * 1024
MAX_TIMEOUT_SECONDS = 600
SHELL_INTERPRETERS = {"sh", "bash", "dash", "zsh", "fish", "csh", "ksh", "cmd", "cmd.exe", "powershell", "pwsh"}
TRUSTED_EXECUTABLES = {"python", "python3", "cargo", "npm", "git"}
TRUSTED_REPOSITORY_SCRIPTS = {"scripts/check-repository-hygiene.sh", "scripts/check-schema-migration-ownership.sh"}
REDACTION_PATTERNS = (
    re.compile(r"(?i)(api[_-]?key|secret|password|passwd|token)\s*[:=]\s*[^\s,;]+"),
    re.compile(r'(?i)("(?:api[_-]?key|secret|password|passwd|token)"\s*:\s*)"[^"]*"'),
    re.compile(r"(?i)(--(?:api[_-]?key|secret|password|passwd|token))(?:=|\s+)[^\s]+"),
    re.compile(r"(?i)([?&](?:api[_-]?key|secret|password|passwd|token)=)[^&\s]+"),
    re.compile(r"(?i)bearer\s+[A-Za-z0-9._~+/=-]+"),
)


class GateError(ValueError):
    pass


def redact(text: str) -> str:
    result = text
    for pattern in REDACTION_PATTERNS:
        result = pattern.sub(lambda match: f"{match.group(1) if match.lastindex else match.group(0).split(':', 1)[0].split('=', 1)[0]}=redacted", result)
    return result[:MAX_OUTPUT_BYTES] + ("\n[output truncated]" if len(result) > MAX_OUTPUT_BYTES else "")


def contained(root: Path, relative: str, *, must_exist: bool = False) -> Path:
    candidate = (root / relative).resolve(strict=must_exist)
    try:
        candidate.relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {relative}") from exc
    return candidate


def parse_manifest(root: Path, path: Path) -> list[dict[str, Any]]:
    try:
        manifest_path = path.resolve(strict=True)
        manifest_path.relative_to(root)
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as exc:
        raise GateError(f"cannot read gate manifest safely: {exc}") from exc
    if payload.get("schema_version") != SCHEMA_VERSION or not isinstance(payload.get("gates"), list):
        raise GateError("gate manifest schema_version or gates is invalid")
    gates: list[dict[str, Any]] = []
    seen: set[str] = set()
    for entry in payload["gates"]:
        if not isinstance(entry, dict):
            raise GateError("each gate must be an object")
        gate_id = entry.get("id")
        argv = entry.get("argv")
        subsystems = entry.get("subsystems")
        if not isinstance(gate_id, str) or not gate_id or gate_id in seen:
            raise GateError("gate IDs must be unique, non-empty strings")
        if not isinstance(argv, list) or not argv or any(not isinstance(item, str) or not item for item in argv):
            raise GateError(f"gate {gate_id} argv must be a non-empty string array")
        executable_path = Path(argv[0])
        if executable_path.is_absolute():
            raise GateError(f"gate {gate_id} executable must be repository-relative or a trusted tool name")
        executable_as_posix = executable_path.as_posix()
        has_path_separator = executable_as_posix != executable_path.name
        if has_path_separator:
            contained(root, executable_as_posix, must_exist=True)
        executable = executable_path.name.lower()
        trusted_script = executable_as_posix in TRUSTED_REPOSITORY_SCRIPTS
        if executable in SHELL_INTERPRETERS or (executable not in TRUSTED_EXECUTABLES and not trusted_script):
            raise GateError(f"gate {gate_id} may not use a shell interpreter")
        if executable in {"python", "python3"} and any(item in {"-c", "-m"} for item in argv[1:]):
            if "-m" in argv[1:] and argv[1:3] == ["-m", "unittest"]:
                pass
            else:
                raise GateError(f"gate {gate_id} may not use inline interpreter code")
        if not isinstance(entry.get("required", True), bool):
            raise GateError(f"gate {gate_id} required must be boolean")
        if not isinstance(subsystems, list) or not subsystems or any(not isinstance(item, str) for item in subsystems):
            raise GateError(f"gate {gate_id} subsystems must be a non-empty string array")
        timeout = entry.get("timeout_seconds", 300)
        if not isinstance(timeout, int) or timeout < 1 or timeout > MAX_TIMEOUT_SECONDS:
            raise GateError(f"gate {gate_id} timeout_seconds is outside 1..{MAX_TIMEOUT_SECONDS}")
        cwd = entry.get("cwd", ".")
        if not isinstance(cwd, str):
            raise GateError(f"gate {gate_id} cwd must be a repository-relative string")
        contained(root, cwd, must_exist=True)
        artifacts = entry.get("artifacts", [])
        if not isinstance(artifacts, list) or any(not isinstance(item, str) for item in artifacts):
            raise GateError(f"gate {gate_id} artifacts must be a string array")
        for artifact in artifacts:
            contained(root, artifact, must_exist=True)
        seen.add(gate_id)
        gates.append({**entry, "required": bool(entry.get("required", True)), "cwd": cwd, "timeout_seconds": timeout, "artifacts": artifacts})
    if not gates:
        raise GateError("gate manifest declares no gates")
    return gates


def run_gate(root: Path, gate: dict[str, Any]) -> dict[str, Any]:
    command = list(gate["argv"])
    result: dict[str, Any] = {
        "argv": [redact(item) for item in command],
        "cwd": gate["cwd"],
        "description": gate.get("description", gate["id"]),
        "id": gate["id"],
        "required": gate["required"],
        "status": "blocked",
    }
    def limit_output() -> None:
        resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_OUTPUT_BYTES, MAX_OUTPUT_BYTES))
        os.setpgid(0, 0)

    with tempfile.TemporaryFile() as stdout_file, tempfile.TemporaryFile() as stderr_file:
        process: subprocess.Popen[bytes] | None = None
        try:
            process = subprocess.Popen(
                command,
                cwd=contained(root, gate["cwd"], must_exist=True),
                stdin=subprocess.DEVNULL,
                stdout=stdout_file,
                stderr=stderr_file,
                shell=False,
                preexec_fn=limit_output,
            )
            return_code = process.wait(timeout=gate["timeout_seconds"])
            result["exit_code"] = return_code
            stdout_file.seek(0)
            stderr_file.seek(0)
            result["stdout"] = redact(stdout_file.read(MAX_OUTPUT_BYTES + 1).decode("utf-8", errors="replace"))
            result["stderr"] = redact(stderr_file.read(MAX_OUTPUT_BYTES + 1).decode("utf-8", errors="replace"))
            result["status"] = "passed" if return_code == 0 else "failed"
        except subprocess.TimeoutExpired:
            if process is not None:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            result["error"] = "gate exceeded its bounded timeout"
            result["status"] = "blocked"
        except OSError as exc:
            result["error"] = f"gate could not start: {type(exc).__name__}"
            result["status"] = "blocked"
    artifact_records = []
    for artifact in gate["artifacts"]:
        try:
            path = contained(root, artifact, must_exist=True)
            digest = hashlib.sha256()
            with path.open("rb") as artifact_file:
                for chunk in iter(lambda: artifact_file.read(1024 * 1024), b""):
                    digest.update(chunk)
            artifact_records.append({"path": artifact, "sha256": digest.hexdigest()})
        except (GateError, OSError) as exc:
            result["error"] = f"artifact could not be hashed: {type(exc).__name__}"
            result["status"] = "blocked"
    result["artifacts"] = artifact_records
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--manifest", type=Path, default=Path("scripts/release-gates.json"))
    parser.add_argument("--scope", choices=("changed", "all"), default="changed")
    parser.add_argument("--output", type=Path, help="repository-contained path for the JSON evidence report")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    try:
        root = repository_root(args.repo)
        truth = build_report(root)
        if truth["check"]["status"] != "passed":
            raise GateError("repository truth check is not green")
        gates = parse_manifest(root, args.manifest if args.manifest.is_absolute() else root / args.manifest)
        changed = {entry["name"] for entry in truth["changed_subsystems"]}
        selected = [gate for gate in gates if args.scope == "all" or "governance" in gate["subsystems"] or changed.intersection(gate["subsystems"])]
        if not selected:
            raise GateError("no applicable gates selected")
        skipped = [
            {"id": gate["id"], "required": gate["required"], "reason": "subsystem unchanged"}
            for gate in gates
            if gate not in selected
        ]
        results = [run_gate(root, gate) for gate in selected]
        failed_required = any(gate["required"] and gate["status"] != "passed" for gate in results)
        skipped_required = any(gate["required"] for gate in skipped)
        report = {
            "evidence_scope": "release_gate_execution",
            "git": truth["git"],
            "gates": results,
            "manifest": str(args.manifest.relative_to(root).as_posix() if args.manifest.is_absolute() else args.manifest.as_posix()),
            "platform": {"machine": platform.machine(), "system": platform.system(), "release": platform.release()},
            "schema_version": SCHEMA_VERSION,
            "selected_subsystems": sorted({name for gate in selected for name in gate["subsystems"]}),
            "skipped_gates": skipped,
            "status": "failed" if failed_required or skipped_required else "passed",
        }
    except (RepoTruthError, GateError, OSError) as exc:
        print(f"release_gate: {exc}", file=sys.stderr)
        return 2
    output = json.dumps(report, indent=2, sort_keys=True)
    if args.output is not None:
        output_path = contained(root, args.output.as_posix(), must_exist=False)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(output + "\n", encoding="utf-8")
    print(output)
    return 1 if report["status"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
