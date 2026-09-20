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
import signal
import subprocess
import sys
import threading
import uuid
from typing import Any

from repo_truth import RepoTruthError, build_report, repository_root
from process_supervisor import START_FAILURE_MARKER, START_FAILURE_MARKER_ENV, kill_descendants


SCHEMA_VERSION = 1
MAX_OUTPUT_BYTES = 16 * 1024
MAX_TIMEOUT_SECONDS = 600
SHELL_INTERPRETERS = {"sh", "bash", "dash", "zsh", "fish", "csh", "ksh", "cmd", "cmd.exe", "powershell", "pwsh"}
TRUSTED_EXECUTABLES = {"python", "python3", "cargo", "npm", "git"}
TRUSTED_REPOSITORY_SCRIPTS = {"scripts/check-repository-hygiene.sh", "scripts/check-schema-migration-ownership.sh"}
CREDENTIAL_NAME = r"(?:[A-Za-z0-9_-]*(?:secret|token|password|passwd|credential|authorization|auth|api[_-]?key|access[_-]?key|private[_-]?key)[A-Za-z0-9_-]*)"
REDACTION_PATTERNS = (
    re.compile(r"(?i)((?:bearer|basic)\s+)(\"(?:\\.|[^\"\\])*\"|\"[^\"]*$|'(?:\\.|[^'\\])*'|'[^']*$|[^\s]+)"),
    re.compile(rf"(?i)({CREDENTIAL_NAME}\s*[:=]\s*)(\"(?:\\.|[^\"\\])*\"|\"[^\"]*$|'(?:\\.|[^'\\])*'|'[^']*$|[^\s]+)"),
    re.compile(rf"(?is)((?:['\"]{CREDENTIAL_NAME}['\"]\s*:\s*))(?:\{{[\s\S]*\}}|\[[\s\S]*\]|\{{[\s\S]*$|\[[\s\S]*$|\"(?:\\.|[^\"\\])*\"|'(?:\\.|[^'\\])*'|['\"][\s\S]*$|[^\s,;}}]+)"),
    re.compile(rf"(?i)(--{CREDENTIAL_NAME}(?:=|\s+))(\"(?:\\.|[^\"\\])*\"|\"[^\"]*$|'(?:\\.|[^'\\])*'|'[^']*$|[^\s]+)"),
    re.compile(r"(?i)([?&](?:api[_-]?key|secret|password|passwd|token)=)(\"[^\"]*\"|'[^']*'|[^&\s]+)"),
    re.compile(r"(?i)([A-Za-z][A-Za-z0-9+.-]*://[^\s/@:]+:)[^\s/@]+(?:@|$)"),
    re.compile(r"(?i)([A-Za-z][A-Za-z0-9+.-]*://)[^\s/@]+@"),
    re.compile(r"(?i)((?:aws_access_key_id|aws_secret_access_key|aws_session_token|x-amz-security-token)\s*[:=]\s*)(\"[^\"]*\"|'[^']*'|[^\s]+)"),
)
CREDENTIAL_FLAG = re.compile(rf"(?i)^--{CREDENTIAL_NAME}$")


class GateError(ValueError):
    pass


def redact(text: str) -> str:
    result = text
    for pattern in REDACTION_PATTERNS:
        result = pattern.sub(lambda match: f"{match.group(1)}redacted", result)
    return result[:MAX_OUTPUT_BYTES] + ("\n[output truncated]" if len(result) > MAX_OUTPUT_BYTES else "")


def redact_argv(command: list[str]) -> list[str]:
    redacted: list[str] = []
    redact_next = False
    for item in command:
        if redact_next:
            if CREDENTIAL_FLAG.fullmatch(item):
                redacted.append(redact(item))
                redact_next = True
                continue
            redacted.append("redacted")
            redact_next = False
            continue
        redacted.append(redact(item))
        redact_next = bool(CREDENTIAL_FLAG.fullmatch(item))
    return redacted


def contained(root: Path, relative: str, *, must_exist: bool = False) -> Path:
    try:
        candidate = (root / relative).resolve(strict=must_exist)
    except (OSError, ValueError) as exc:
        raise GateError(f"path cannot be resolved safely: {relative}") from exc
    try:
        candidate.relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {relative}") from exc
    return candidate


def reject_external_argument(item: str) -> None:
    if "\x00" in item:
        raise GateError("gate argv contains a NUL character")
    candidates = [item]
    if "=" in item:
        candidates.append(item.split("=", 1)[1])
    for candidate in candidates:
        if not candidate:
            continue
        path = Path(candidate)
        if path.is_absolute() or re.match(r"^[A-Za-z]:[\\/]", candidate) or ".." in path.parts:
            raise GateError("gate argv contains a path outside the repository")


def validate_repository_argument(root: Path, item: str) -> None:
    def validate(value: str) -> None:
        candidate = root / value
        if not value or "://" in value or not (
            candidate.exists()
            or candidate.is_symlink()
            or value in {".", ".."}
            or "/" in value
            or value.startswith(".")
            or value.endswith((".py", ".sh", ".json", ".toml"))
        ):
            return
        contained(root, value, must_exist=False)

    if "=" in item:
        prefix, value = item.split("=", 1)
        validate(value)
    else:
        validate(item)


def parse_manifest(root: Path, path: Path) -> list[dict[str, Any]]:
    try:
        manifest_path = path.resolve(strict=True)
        manifest_path.relative_to(root)
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError, RecursionError, ValueError) as exc:
        raise GateError(f"cannot read gate manifest safely: {exc}") from exc
    schema_version = payload.get("schema_version") if isinstance(payload, dict) else None
    if not isinstance(payload, dict) or type(schema_version) is not int or schema_version != SCHEMA_VERSION or not isinstance(payload.get("gates"), list):
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
            executable_path = contained(root, executable_as_posix, must_exist=True)
        executable = executable_path.name
        executable_lower = executable.lower()
        trusted_script = executable_as_posix in TRUSTED_REPOSITORY_SCRIPTS
        if trusted_script and contained(root, executable_as_posix, must_exist=True) != (root / executable_as_posix).absolute():
            raise GateError(f"gate {gate_id} trusted executable must not be a symlink")
        if executable_lower in SHELL_INTERPRETERS or (has_path_separator and not trusted_script) or (not has_path_separator and executable not in TRUSTED_EXECUTABLES):
            raise GateError(f"gate {gate_id} may not use a shell interpreter or non-allowlisted executable")
        for argument in argv[1:]:
            reject_external_argument(argument)
            validate_repository_argument(root, argument)
        safe_argv = [executable_path.as_posix() if has_path_separator else argv[0]] + argv[1:]
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
        if isinstance(timeout, bool) or not isinstance(timeout, int) or timeout < 1 or timeout > MAX_TIMEOUT_SECONDS:
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
        gates.append({**entry, "argv": safe_argv, "required": bool(entry.get("required", True)), "cwd": cwd, "timeout_seconds": timeout, "artifacts": artifacts})
    if not gates:
        raise GateError("gate manifest declares no gates")
    return gates


def supervised_command(root: Path, command: list[str]) -> list[str]:
    supervisor = Path(__file__).with_name("process_supervisor.py").resolve(strict=True)
    return [sys.executable, supervisor.as_posix(), *command]


def set_parent_death_signal() -> None:
    if sys.platform.startswith("linux"):
        import ctypes

        libc = ctypes.CDLL(None, use_errno=True)
        if libc.prctl(1, signal.SIGTERM) != 0:  # PR_SET_PDEATHSIG
            raise OSError(ctypes.get_errno(), "prctl(PR_SET_PDEATHSIG) failed")
        if os.getppid() == 1:
            os.kill(os.getpid(), signal.SIGTERM)


def run_gate(root: Path, gate: dict[str, Any]) -> dict[str, Any]:
    command = list(gate["argv"])
    result: dict[str, Any] = {
        "argv": redact_argv(command),
        "cwd": gate["cwd"],
        "description": redact(str(gate.get("description", gate["id"]))),
        "id": gate["id"],
        "required": gate["required"],
        "status": "blocked",
    }
    process: subprocess.Popen[bytes] | None = None
    output: dict[str, tuple[bytes, bool]] = {}
    reader_state: dict[str, dict[str, Any]] = {
        "stdout": {"done": False, "error": None},
        "stderr": {"done": False, "error": None},
    }

    def collect_output(name: str, stream: Any) -> None:
        captured = bytearray()
        overflowed = False
        received = 0
        try:
            while True:
                chunk = stream.read(64 * 1024)
                if not chunk:
                    break
                if len(captured) < MAX_OUTPUT_BYTES:
                    captured.extend(chunk[: MAX_OUTPUT_BYTES - len(captured)])
                received += len(chunk)
                if received > MAX_OUTPUT_BYTES:
                    overflowed = True
        except Exception as exc:
            reader_state[name]["error"] = type(exc).__name__
        finally:
            output[name] = (bytes(captured), overflowed)
            reader_state[name]["done"] = True

    supervisor_failure_marker = f"{START_FAILURE_MARKER[:-1]} {uuid.uuid4().hex}]"
    try:
        process = subprocess.Popen(
            supervised_command(root, command),
            cwd=contained(root, gate["cwd"], must_exist=True),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            shell=False,
            start_new_session=True,
            preexec_fn=set_parent_death_signal,
            env={**os.environ, START_FAILURE_MARKER_ENV: supervisor_failure_marker},
        )
        readers = [
            threading.Thread(target=collect_output, args=("stdout", process.stdout), daemon=True),
            threading.Thread(target=collect_output, args=("stderr", process.stderr), daemon=True),
        ]
        for reader in readers:
            reader.start()
        timed_out = False
        try:
            return_code = process.wait(timeout=gate["timeout_seconds"])
        except subprocess.TimeoutExpired:
            timed_out = True
            result["error"] = "gate exceeded its bounded timeout"
            return_code = None
        finally:
            if timed_out:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    process.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    kill_descendants(process.pid)
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    process.wait()
            for reader in readers:
                reader.join(timeout=1)
            if any(reader.is_alive() for reader in readers):
                for stream in (process.stdout, process.stderr):
                    if stream is not None:
                        stream.close()
                for reader in readers:
                    reader.join(timeout=1)
        capture_incomplete = any(not state["done"] for state in reader_state.values())
        capture_failed = any(state["error"] is not None for state in reader_state.values())
        if capture_incomplete:
            result["error"] = "gate output capture did not complete"
        elif capture_failed and not result.get("error"):
            result["error"] = "gate output capture failed"
        if timed_out or capture_incomplete or capture_failed:
            result["status"] = "blocked"
        else:
            result["status"] = "passed" if return_code == 0 else "failed"
        if return_code is not None:
            result["exit_code"] = return_code
        supervisor_start_failed = False
        for name in ("stdout", "stderr"):
            captured, overflowed = output.get(name, (b"", False))
            text = captured.decode("utf-8", errors="replace")
            if supervisor_failure_marker in text:
                supervisor_start_failed = True
                text = text.replace(supervisor_failure_marker, "")
            if overflowed:
                text += "\n[output truncated]"
            result[name] = redact(text)
        if supervisor_start_failed:
            result["error"] = "gate executable or command was unavailable"
            result["status"] = "blocked"
    except (OSError, subprocess.SubprocessError) as exc:
        result["error"] = f"gate could not start: {type(exc).__name__}"
        result["status"] = "blocked"
    artifact_records: list[dict[str, str]] = []
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
        manifest_path = contained(root, args.manifest.as_posix() if args.manifest.is_absolute() else (root / args.manifest).as_posix(), must_exist=True)
        gates = parse_manifest(root, manifest_path)
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
            "manifest": manifest_path.relative_to(root).as_posix(),
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
        try:
            output_path = contained(root, args.output.as_posix(), must_exist=False)
            output_path.parent.mkdir(parents=True, exist_ok=True)
            output_path.write_text(output + "\n", encoding="utf-8")
        except (GateError, OSError) as exc:
            print(f"release_gate: cannot persist evidence safely: {exc}", file=sys.stderr)
            return 2
    print(output)
    return 1 if report["status"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
