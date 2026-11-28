#!/usr/bin/env python3
"""Run the declared, non-publishing gates for a VoidTower checkout.

The runner is deliberately an evidence collector, not a release publisher. It
never stages, commits, edits source, reads environment files, or claims runtime
support from a passing source-only gate.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import ctypes
import errno
import hashlib
import json
import os
import platform
from pathlib import Path
import re
import secrets
import signal
import selectors
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from typing import Any, Mapping

from repo_truth import RepoTruthError, build_report, repository_root
from process_supervisor import (
    CleanupError,
    ProcessSnapshotError,
    START_FAILURE_MARKER,
    START_FAILURE_MARKER_ENV,
    kill_descendants,
    process_start_time,
    signal_owned_process,
    signal_process,
)


SCHEMA_VERSION = 1
EVIDENCE_LIMITS = {"implemented", "unit-verified", "integration-verified", "runtime-verified", "release-qualified"}
MAX_OUTPUT_BYTES = 16 * 1024
MAX_REPORT_BYTES = 1024 * 1024
REPORT_MODE = 0o600
QUARANTINE_MODE = 0o700
MAX_QUARANTINE_DIRECTORIES = 8
MAX_TIMEOUT_SECONDS = 600
MAX_COMMAND_ARGUMENTS = 64
MAX_TOOL_SNAPSHOT_BYTES = 512 * 1024 * 1024
MAX_TOOL_SNAPSHOT_ENTRIES = 10_000
MAX_GIT_METADATA_BYTES = 64 * 1024
MAX_GIT_POINTER_BYTES = 4 * 1024
SHELL_INTERPRETERS = {"sh", "bash", "dash", "zsh", "fish", "csh", "ksh", "cmd", "cmd.exe", "powershell", "pwsh"}
TRUSTED_EXECUTABLES = {"python", "python3", "cargo", "npm", "git"}
TRUSTED_REPOSITORY_SCRIPTS = {"scripts/check-repository-hygiene.sh", "scripts/check-schema-migration-ownership.sh"}
MANIFEST_KEYS = {"schema_version", "external_gates", "gates"}
MANIFEST_GATE_KEYS = {"argv", "artifacts", "cwd", "description", "evidence_limit", "id", "required", "subsystems", "timeout_seconds"}
EXTERNAL_DESCRIPTOR_KEYS = {"id", "reason", "required", "evidence_file", "workflow_job", "command", "cwd"}
SAFE_TOOL_DIRECTORIES = (Path("/usr/local/cargo/bin"), Path("/usr/local/bin"), Path("/usr/bin"), Path("/usr/sbin"), Path("/sbin"))
TOOLCHAIN_MARKERS = {".cargo", ".hermes", "node", "nodejs", "hostedtoolcache", "rustup"}
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


SANDBOX_DEPTH_ENV = "VOIDTOWER_GATE_SANDBOX_DEPTH"
SANDBOX_CAPABILITY_PATH_ENV = "VOIDTOWER_GATE_SANDBOX_CAPABILITY_PATH"
SANDBOX_CAPABILITY_DIGEST_ENV = "VOIDTOWER_GATE_SANDBOX_CAPABILITY_DIGEST"
SANDBOX_CAPABILITY_DIR = Path("/run/voidtower-gate-capability")
SANDBOX_CAPABILITY_NAME = re.compile(r"^[0-9a-f]{32}$")


@dataclass(frozen=True)
class SelectedTool:
    """A tool pathname plus the identity captured during selection."""

    path: Path
    identity: tuple[int, int, int, int]
    digest: str


class SandboxedCommand(list[str]):
    """A bubblewrap argv plus directory descriptors held for its launch."""

    def __init__(self, argv: list[str], pass_fds: list[int]) -> None:
        super().__init__(argv)
        self.pass_fds = tuple(pass_fds)

    def close(self) -> None:
        for file_descriptor in self.pass_fds:
            try:
                os.close(file_descriptor)
            except OSError:
                pass
        self.pass_fds = ()


def selected_path(selection: Path | SelectedTool) -> Path:
    return selection.path if isinstance(selection, SelectedTool) else selection


def reject_unknown_fields(entry: dict[str, Any], allowed: set[str], context: str) -> None:
    unknown_fields = set(entry) - allowed
    if unknown_fields:
        raise GateError(f"{context} contains unknown fields: {', '.join(sorted(unknown_fields))}")


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
    raw_candidate = root / relative
    try:
        raw_relative = raw_candidate.relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {relative}") from exc
    current = root
    for part in raw_relative.parts:
        current /= part
        if current.is_symlink():
            try:
                current.resolve(strict=False).relative_to(root)
            except ValueError as exc:
                raise GateError(f"path escapes repository: {relative}") from exc
            raise GateError(f"path must not be a symlink: {relative}")
    try:
        candidate = raw_candidate.resolve(strict=must_exist)
    except (OSError, ValueError) as exc:
        raise GateError(f"path cannot be resolved safely: {relative}") from exc
    try:
        candidate.relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {relative}") from exc
    return candidate


def open_contained_directory(root: Path, relative: str) -> int:
    """Open a gate cwd by identity, rather than passing a re-resolved path."""
    if os.name != "posix" or not hasattr(os, "O_DIRECTORY") or not hasattr(os, "O_NOFOLLOW"):
        raise GateError("safe gate working-directory binding is unavailable on this platform")
    try:
        relative_path = (root / relative).relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {relative}") from exc
    parts = relative_path.parts
    if any(part in {"", ".", ".."} for part in parts):
        raise GateError(f"path is not a safe gate working directory: {relative}")
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    descriptor = os.open(root, flags)
    try:
        for part in parts:
            next_descriptor = os.open(part, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = next_descriptor
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def snapshot_path_metadata(root: Path, relative: str) -> tuple[object, ...]:
    """Capture candidate path metadata and bounded-safe content identity."""
    path = root / relative
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return ("missing", relative)
    identity = (relative, metadata.st_mode, metadata.st_size, metadata.st_mtime_ns, metadata.st_ino)
    if stat.S_ISLNK(metadata.st_mode):
        return ("symlink", *identity, os.readlink(path))
    if not stat.S_ISDIR(metadata.st_mode):
        if stat.S_ISREG(metadata.st_mode) and not any(part.lower().startswith((".env", "credential", "secret")) for part in path.parts):
            digest = hashlib.sha256()
            with path.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    digest.update(chunk)
            return ("file", *identity, digest.hexdigest())
        return ("entry", *identity)
    children = []
    for child in sorted(path.iterdir(), key=lambda item: item.name):
        child_relative = (Path(relative) / child.name).as_posix()
        children.append(snapshot_path_metadata(root, child_relative))
    return ("directory", *identity, tuple(children))


def hash_contained_file(root: Path, relative: str) -> str:
    """Hash one regular repository file through no-follow directory handles."""
    if not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_DIRECTORY") or os.name != "posix":
        raise GateError("safe artifact hashing is unavailable on this platform")
    raw_candidate = root / relative
    try:
        raw_relative = raw_candidate.relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {relative}") from exc
    parts = raw_relative.parts
    if not parts or any(part in {"", ".", ".."} for part in parts):
        raise GateError(f"path is not a safe repository file: {relative}")
    nofollow = getattr(os, "O_NOFOLLOW", 0)
    directory_flag = getattr(os, "O_DIRECTORY", 0)
    root_descriptor = -1
    directory_descriptor = -1
    file_descriptor = -1
    try:
        root_descriptor = os.open(root, os.O_RDONLY | directory_flag | nofollow)
        directory_descriptor = root_descriptor
        for component in parts[:-1]:
            try:
                next_descriptor = os.open(component, os.O_RDONLY | directory_flag | nofollow, dir_fd=directory_descriptor)
            except OSError as exc:
                try:
                    component_mode = os.stat(component, dir_fd=directory_descriptor, follow_symlinks=False).st_mode
                except OSError:
                    raise exc
                if stat.S_ISLNK(component_mode):
                    raise GateError(f"path must not be a symlink: {relative}") from exc
                raise
            if directory_descriptor != root_descriptor:
                os.close(directory_descriptor)
            directory_descriptor = next_descriptor
        file_descriptor = os.open(parts[-1], os.O_RDONLY | nofollow, dir_fd=directory_descriptor)
        if not stat.S_ISREG(os.fstat(file_descriptor).st_mode):
            raise GateError(f"artifact must be a regular file: {relative}")
        digest = hashlib.sha256()
        with os.fdopen(file_descriptor, "rb") as artifact_file:
            file_descriptor = -1
            for chunk in iter(lambda: artifact_file.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()
    except OSError as exc:
        if exc.errno == errno.ELOOP:
            raise GateError(f"path must not be a symlink: {relative}") from exc
        raise
    finally:
        if file_descriptor >= 0:
            os.close(file_descriptor)
        if directory_descriptor >= 0 and directory_descriptor != root_descriptor:
            os.close(directory_descriptor)
        if root_descriptor >= 0:
            os.close(root_descriptor)


def optional_artifact_digest(root: Path, relative: str) -> str | None:
    try:
        return hash_contained_file(root, relative)
    except FileNotFoundError:
        return None


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
        if path.is_absolute() or re.match(r"^[A-Za-z]:", candidate) or candidate.startswith(("\\\\", "/")) or "\\" in candidate or ".." in path.parts:
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


def parse_manifest(root: Path, path: Path) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    try:
        manifest_path = path.resolve(strict=True)
        manifest_path.relative_to(root)
        payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError, RecursionError, ValueError) as exc:
        raise GateError(f"cannot read gate manifest safely: {exc}") from exc
    schema_version = payload.get("schema_version") if isinstance(payload, dict) else None
    if not isinstance(payload, dict) or type(schema_version) is not int or schema_version != SCHEMA_VERSION or not isinstance(payload.get("gates"), list):
        raise GateError("gate manifest schema_version or gates is invalid")
    reject_unknown_fields(payload, MANIFEST_KEYS, "gate manifest")
    external_entries = payload.get("external_gates", [])
    if not isinstance(external_entries, list):
        raise GateError("gate manifest external_gates must be an array")
    external_gates: list[dict[str, Any]] = []
    external_ids: set[str] = set()
    for entry in external_entries:
        if (
            not isinstance(entry, dict)
            or not isinstance(entry.get("id"), str)
            or not entry["id"]
            or entry["id"] in external_ids
            or not isinstance(entry.get("reason"), str)
            or not entry["reason"]
            or len(entry["id"]) > 128
            or len(entry["reason"]) > 512
            or not entry["id"].isprintable()
            or not entry["reason"].isprintable()
            or entry.get("required") is not True
            or entry.get("evidence_file") != f"release-external-{entry.get('id')}.json"
            or Path(str(entry.get("evidence_file"))).parts != (str(entry.get("evidence_file")),)
            or "\\" in str(entry.get("evidence_file"))
            or not isinstance(entry.get("workflow_job"), str)
            or not entry["workflow_job"]
            or len(entry["workflow_job"]) > 128
            or not entry["workflow_job"].isprintable()
            or not isinstance(entry.get("command"), list)
            or not entry["command"]
            or len(entry["command"]) > 64
            or any(not isinstance(argument, str) or not argument or len(argument) > 4096 or not argument.isprintable() for argument in entry["command"])
            or not isinstance(entry.get("cwd"), str)
            or not entry["cwd"]
            or len(entry["cwd"]) > 256
            or not entry["cwd"].isprintable()
            or "\\" in entry["cwd"]
            or re.match(r"^[A-Za-z]:", entry["cwd"])
            or Path(entry["cwd"]).is_absolute()
            or ".." in Path(entry["cwd"]).parts
        ):
            raise GateError("external gate entries require unique bounded id and reason strings")
        reject_unknown_fields(entry, EXTERNAL_DESCRIPTOR_KEYS, "external gate entry")
        external_ids.add(entry["id"])
        external_gates.append(dict(entry))
    gates: list[dict[str, Any]] = []
    seen: set[str] = set()
    for entry in payload["gates"]:
        if not isinstance(entry, dict):
            raise GateError("each gate must be an object")
        gate_id = entry.get("id")
        argv = entry.get("argv")
        subsystems = entry.get("subsystems")
        reject_unknown_fields(entry, MANIFEST_GATE_KEYS, f"gate {gate_id or '<unknown>'}")
        if not isinstance(gate_id, str) or not gate_id or len(gate_id) > 128 or not gate_id.isprintable() or gate_id in seen:
            raise GateError("gate IDs must be unique, non-empty strings")
        if not isinstance(argv, list) or not argv or len(argv) > MAX_COMMAND_ARGUMENTS or any(not isinstance(item, str) or not item or len(item) > 4096 or not item.isprintable() for item in argv):
            raise GateError(f"gate {gate_id} argv must be a non-empty string array")
        executable_path = Path(argv[0])
        if executable_path.is_absolute():
            raise GateError(f"gate {gate_id} executable must be repository-relative or an approved trusted tool path")
        executable_as_posix = executable_path.as_posix()
        has_path_separator = executable_as_posix != executable_path.name
        if has_path_separator:
            executable_path = contained(root, executable_as_posix, must_exist=True)
        executable = executable_path.name
        executable_lower = executable.lower()
        trusted_script = executable_as_posix in TRUSTED_REPOSITORY_SCRIPTS
        if trusted_script:
            trusted_path = contained(root, executable_as_posix, must_exist=True)
            if trusted_path != (root / executable_as_posix).absolute() or not trusted_path.is_file() or not os.access(trusted_path, os.X_OK):
                raise GateError(f"gate {gate_id} trusted executable must be a regular executable and must not be a symlink")
        if executable_lower in SHELL_INTERPRETERS or (has_path_separator and not trusted_script) or (not has_path_separator and executable not in TRUSTED_EXECUTABLES):
            raise GateError(f"gate {gate_id} may not use a shell interpreter or non-allowlisted executable")
        for argument in argv[1:]:
            reject_external_argument(argument)
            validate_repository_argument(root, argument)
        safe_argv = [executable_as_posix if has_path_separator else argv[0]] + argv[1:]
        if executable in {"python", "python3"} and any(item in {"-c", "-m"} for item in argv[1:]):
            if "-m" in argv[1:] and argv[1:3] == ["-m", "unittest"]:
                pass
            else:
                raise GateError(f"gate {gate_id} may not use inline interpreter code")
        if not isinstance(entry.get("required", True), bool):
            raise GateError(f"gate {gate_id} required must be boolean")
        if not isinstance(subsystems, list) or not subsystems or any(not isinstance(item, str) or not item or len(item) > 128 or not item.isprintable() for item in subsystems):
            raise GateError(f"gate {gate_id} subsystems must be a non-empty string array")
        description = entry.get("description", gate_id)
        if not isinstance(description, str) or not description or len(description) > 512 or not description.isprintable():
            raise GateError(f"gate {gate_id} description must be a bounded printable string")
        timeout = entry.get("timeout_seconds", 300)
        if isinstance(timeout, bool) or not isinstance(timeout, int) or timeout < 1 or timeout > MAX_TIMEOUT_SECONDS:
            raise GateError(f"gate {gate_id} timeout_seconds is outside 1..{MAX_TIMEOUT_SECONDS}")
        evidence_limit = entry.get("evidence_limit", "unit-verified")
        if not isinstance(evidence_limit, str) or evidence_limit not in EVIDENCE_LIMITS:
            raise GateError(f"gate {gate_id} evidence_limit is invalid")
        cwd = entry.get("cwd", ".")
        if (
            not isinstance(cwd, str)
            or not cwd
            or len(cwd) > 256
            or not cwd.isprintable()
            or "\\" in cwd
            or re.match(r"^[A-Za-z]:", cwd)
            or Path(cwd).is_absolute()
            or ".." in Path(cwd).parts
        ):
            raise GateError(f"gate {gate_id} cwd must be a repository-relative string")
        contained(root, cwd, must_exist=True)
        artifacts = entry.get("artifacts", [])
        if not isinstance(artifacts, list) or any(not isinstance(item, str) or not item or len(item) > 4096 or not item.isprintable() or "\\" in item or re.match(r"^[A-Za-z]:", item) or Path(item).is_absolute() or ".." in Path(item).parts for item in artifacts):
            raise GateError(f"gate {gate_id} artifacts must be a string array")
        for artifact in artifacts:
            contained(root, artifact, must_exist=True)
        seen.add(gate_id)
        gates.append({**entry, "argv": safe_argv, "required": bool(entry.get("required", True)), "cwd": cwd, "timeout_seconds": timeout, "artifacts": artifacts, "evidence_limit": evidence_limit})
    if not gates:
        raise GateError("gate manifest declares no gates")
    if external_ids.intersection(seen):
        raise GateError("external gate IDs must not overlap executable gate IDs")
    return gates, external_gates


def supervised_command(root: Path, command: list[str]) -> list[str]:
    supervisor = Path(__file__).with_name("process_supervisor.py").resolve(strict=True)
    return [sys.executable, supervisor.as_posix(), *command]


def new_evidence_output(root: Path, requested: Path) -> Path:
    output_path = requested if requested.is_absolute() else root / requested
    output_path = output_path.absolute()
    try:
        relative = output_path.relative_to(root)
    except ValueError as exc:
        raise GateError(f"path escapes repository: {requested}") from exc
    if any(part == ".." for part in relative.parts):
        raise GateError(f"path escapes repository: {requested}")
    if not relative.parts or relative.parts[0] != "dev-data":
        raise GateError("evidence output must be under the repository dev-data directory")
    parent = root
    for part in relative.parts[:-1]:
        parent /= part
        if parent.is_symlink():
            raise GateError("evidence output parent must not be a symlink")
    if output_path.exists() or output_path.is_symlink():
        raise GateError("evidence output already exists")
    return output_path


def directory_identity(metadata: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        metadata.st_dev,
        metadata.st_ino,
        stat.S_IMODE(metadata.st_mode),
        metadata.st_uid,
        metadata.st_gid,
    )


def directory_identity_matches(
    metadata: os.stat_result,
    expected_identity: tuple[int, ...],
    *,
    include_security: bool = True,
) -> bool:
    if not stat.S_ISDIR(metadata.st_mode):
        return False
    if include_security and len(expected_identity) >= 5:
        return directory_identity(metadata) == expected_identity[:5]
    return (metadata.st_dev, metadata.st_ino) == expected_identity[:2]


def capture_evidence_ancestor_identities(root: Path, output_path: Path) -> dict[str, tuple[int, ...]]:
    """Capture existing evidence ancestors without following symlinks."""
    relative = output_path.relative_to(root)
    identities: dict[str, tuple[int, ...]] = {}
    missing = False
    current = root
    for component in relative.parts[:-1]:
        current /= component
        relative_component = current.relative_to(root).as_posix()
        if missing:
            continue
        try:
            metadata = os.lstat(current)
        except FileNotFoundError:
            missing = True
            continue
        if not stat.S_ISDIR(metadata.st_mode):
            raise GateError("evidence ancestor must be a directory")
        if metadata.st_uid != os.getuid() or metadata.st_gid != os.getgid() or stat.S_IMODE(metadata.st_mode) & 0o022:
            raise GateError("evidence ancestor owner and mode are unsafe")
        identities[relative_component] = directory_identity(metadata)
    return identities


def open_evidence_parent(
    root: Path,
    output_path: Path,
    *,
    expected_root_identity: tuple[int, int] | None = None,
    expected_ancestor_identities: dict[str, tuple[int, ...]] | None = None,
    enforce_ancestor_safety: bool = True,
) -> tuple[int, str]:
    """Open the evidence parent through retained no-follow directory handles."""
    relative = output_path.relative_to(root)
    parts = relative.parts
    if len(parts) < 2 or parts[0] != "dev-data":
        raise GateError("evidence output must be under the repository dev-data directory")
    nofollow = getattr(os, "O_NOFOLLOW", 0)
    directory_flag = getattr(os, "O_DIRECTORY", 0)
    root_descriptor = os.open(root, os.O_RDONLY | directory_flag | nofollow)
    directory_descriptor = root_descriptor
    try:
        root_metadata = os.fstat(root_descriptor)
        if expected_root_identity is not None and (root_metadata.st_dev, root_metadata.st_ino) != expected_root_identity:
            raise GateError("evidence root identity changed before opening parent")
        for index, component in enumerate(parts[:-1]):
            created = False
            created_identity: tuple[int, int] | None = None
            relative_component = Path(*parts[: index + 1]).as_posix()
            expected_identity = None if expected_ancestor_identities is None else expected_ancestor_identities.get(relative_component)
            try:
                os.mkdir(component, 0o700, dir_fd=directory_descriptor)
                created = True
            except FileExistsError:
                if expected_ancestor_identities is not None and expected_identity is None:
                    raise GateError("evidence ancestor appeared during open")
            if created:
                created_metadata = os.stat(component, dir_fd=directory_descriptor, follow_symlinks=False)
                if not stat.S_ISDIR(created_metadata.st_mode):
                    raise GateError("new evidence ancestor is not a directory")
                created_identity = (created_metadata.st_dev, created_metadata.st_ino)
                # A newly-created evidence directory is not durable until its
                # containing directory records the directory entry.  This
                # includes the repository root for the first dev-data entry
                # and every nested parent below it.
                os.fsync(directory_descriptor)
            next_descriptor = os.open(
                component,
                os.O_RDONLY | directory_flag | nofollow,
                dir_fd=directory_descriptor,
            )
            next_metadata = os.fstat(next_descriptor)
            if created_identity is not None and (next_metadata.st_dev, next_metadata.st_ino) != created_identity:
                os.close(next_descriptor)
                raise GateError("new evidence ancestor identity changed during open")
            if expected_ancestor_identities is not None:
                if expected_identity is not None and not directory_identity_matches(
                    next_metadata,
                    expected_identity,
                    include_security=enforce_ancestor_safety,
                ):
                    os.close(next_descriptor)
                    raise GateError("evidence ancestor identity changed during open")
                if expected_identity is None and not created:
                    os.close(next_descriptor)
                    raise GateError("evidence ancestor appeared during open")
            if enforce_ancestor_safety and (
                next_metadata.st_uid != os.getuid()
                or next_metadata.st_gid != os.getgid()
                or stat.S_IMODE(next_metadata.st_mode) & 0o022
            ):
                os.close(next_descriptor)
                raise GateError("evidence ancestor owner and mode are unsafe")
            if directory_descriptor != root_descriptor:
                os.close(directory_descriptor)
            directory_descriptor = next_descriptor
        # The returned descriptor is the only handle the caller needs.  The
        # retained repository-root descriptor must not survive a successful
        # open or repeated report generation will leak one descriptor per run.
        if directory_descriptor != root_descriptor:
            retained_root_descriptor = root_descriptor
            root_descriptor = -1
            os.close(retained_root_descriptor)
        return directory_descriptor, parts[-1]
    except BaseException:
        if directory_descriptor != -1:
            os.close(directory_descriptor)
        if directory_descriptor != root_descriptor and root_descriptor != -1:
            os.close(root_descriptor)
        raise


def write_evidence_descriptor(descriptor: int, output: str, *, truncate: bool = False) -> None:
    if truncate:
        os.ftruncate(descriptor, 0)
        os.lseek(descriptor, 0, os.SEEK_SET)
    remaining = (output + "\n").encode("utf-8")
    while remaining:
        written = os.write(descriptor, remaining)
        if written <= 0:
            raise OSError("evidence report write made no progress")
        remaining = remaining[written:]
    os.fsync(descriptor)


def serialize_report(report: Mapping[str, Any]) -> str:
    """Serialize a report while enforcing the producer-side size boundary."""
    output = json.dumps(report, indent=2, sort_keys=True)
    if len((output + "\n").encode("utf-8")) > MAX_REPORT_BYTES:
        raise GateError("evidence report exceeds the maximum supported size")
    return output


def rename_noreplace(source_parent: int, source: str, destination_parent: int, destination: str) -> None:
    """Atomically move one directory entry without replacing its destination."""
    if not sys.platform.startswith("linux"):
        raise OSError(errno.ENOTSUP, "identity-bound report cleanup requires Linux renameat2")
    libc = ctypes.CDLL(None, use_errno=True)
    renameat2 = getattr(libc, "renameat2", None)
    if renameat2 is not None:
        renameat2.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        renameat2.restype = ctypes.c_int
        result = renameat2(
            source_parent,
            source.encode("utf-8"),
            destination_parent,
            destination.encode("utf-8"),
            1,
        )
    else:
        machine = platform.machine()
        syscall_number = {"x86_64": 316, "aarch64": 276, "ppc64le": 357, "s390x": 347}.get(machine)
        if syscall_number is None:
            raise OSError(errno.ENOTSUP, "identity-bound report cleanup requires a supported renameat2 syscall")
        result = libc.syscall(
            ctypes.c_long(syscall_number),
            ctypes.c_int(source_parent),
            ctypes.c_char_p(source.encode("utf-8")),
            ctypes.c_int(destination_parent),
            ctypes.c_char_p(destination.encode("utf-8")),
            ctypes.c_uint(1),
        )
    if result != 0:
        error_number = ctypes.get_errno()
        if error_number == errno.EEXIST:
            raise FileExistsError(error_number, os.strerror(error_number), destination)
        if error_number == errno.ENOENT:
            raise FileNotFoundError(error_number, os.strerror(error_number), source)
        raise OSError(error_number, os.strerror(error_number), source)


def identity_cleanup_supported() -> bool:
    """Return whether the host can provide the report cleanup primitive."""
    if not sys.platform.startswith("linux"):
        return False
    libc = ctypes.CDLL(None, use_errno=True)
    renameat2 = getattr(libc, "renameat2", None)
    if renameat2 is not None:
        renameat2.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        renameat2.restype = ctypes.c_int
        ctypes.set_errno(0)
        renameat2(-1, b"probe", -1, b"destination", 1)
        return ctypes.get_errno() in {errno.EBADF}
    syscall_number = {"x86_64": 316, "aarch64": 276, "ppc64le": 357, "s390x": 347}.get(platform.machine())
    if syscall_number is None:
        return False
    ctypes.set_errno(0)
    libc.syscall(
        ctypes.c_long(syscall_number),
        ctypes.c_int(-1),
        ctypes.c_char_p(b"probe"),
        ctypes.c_int(-1),
        ctypes.c_char_p(b"destination"),
        ctypes.c_uint(1),
    )
    return ctypes.get_errno() in {errno.EBADF}


def report_identity(metadata: os.stat_result) -> tuple[int, ...]:
    # Include ctime so a reused device/inode pair cannot impersonate a report.
    return (
        metadata.st_dev,
        metadata.st_ino,
        stat.S_IMODE(metadata.st_mode),
        metadata.st_uid,
        metadata.st_gid,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def report_identity_matches(
    metadata: os.stat_result,
    expected_identity: tuple[int, ...],
    *,
    expected_mode: int | None = None,
) -> bool:
    if not stat.S_ISREG(metadata.st_mode) or (metadata.st_dev, metadata.st_ino) != expected_identity[:2]:
        return False
    identity_mode = expected_mode if expected_mode is not None else (expected_identity[2] if len(expected_identity) >= 3 else None)
    if identity_mode is not None and stat.S_IMODE(metadata.st_mode) != identity_mode:
        return False
    if len(expected_identity) >= 5 and (metadata.st_uid, metadata.st_gid) != expected_identity[3:5]:
        return False
    if len(expected_identity) >= 6 and metadata.st_size != expected_identity[5]:
        return False
    if len(expected_identity) >= 7 and metadata.st_mtime_ns != expected_identity[6]:
        return False
    if len(expected_identity) >= 8 and metadata.st_ctime_ns != expected_identity[7]:
        return False
    return True


def quarantine_identity_matches(
    metadata: os.stat_result,
    expected_identity: tuple[int, ...],
) -> bool:
    return (
        stat.S_ISDIR(metadata.st_mode)
        and (metadata.st_dev, metadata.st_ino, stat.S_IMODE(metadata.st_mode), metadata.st_uid, metadata.st_gid)
        == expected_identity
    )


def link_quarantine_entry_to_public_path(quarantine_descriptor: int, filename: str, parent_descriptor: int) -> bool:
    """Restore a quarantined replacement through an identity-bound file descriptor."""
    if not sys.platform.startswith("linux") or not hasattr(os, "O_PATH"):
        raise OSError(errno.ENOTSUP, "descriptor-bound quarantine restoration requires Linux O_PATH")
    entry_descriptor = os.open(
        filename,
        os.O_PATH | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0),
        dir_fd=quarantine_descriptor,
    )
    try:
        libc = ctypes.CDLL(None, use_errno=True)
        linkat = libc.linkat
        linkat.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
        linkat.restype = ctypes.c_int
        result = linkat(entry_descriptor, b"", parent_descriptor, filename.encode("utf-8"), 0x1000)
        if result == 0:
            return True
        error_number = ctypes.get_errno()
        if error_number == errno.EEXIST:
            return False
        raise OSError(error_number, os.strerror(error_number), filename)
    finally:
        os.close(entry_descriptor)


def public_output_path_matches(
    root: Path,
    output_path: Path,
    expected_root_identity: tuple[int, int] | None,
    expected_parent_identity: tuple[int, int] | None,
    expected_report_identity: tuple[int, ...],
    expected_ancestor_identities: dict[str, tuple[int, ...]] | None = None,
) -> bool:
    """Revalidate the public ancestor chain without following symlinks."""
    if expected_root_identity is None or expected_parent_identity is None:
        return False
    root_metadata = os.lstat(root)
    if (root_metadata.st_dev, root_metadata.st_ino) != expected_root_identity or not stat.S_ISDIR(root_metadata.st_mode):
        return False
    if expected_ancestor_identities is not None:
        for relative_component, expected_identity in expected_ancestor_identities.items():
            current = root
            metadata = None
            for component in relative_component.split("/"):
                current /= component
                metadata = os.lstat(current)
                if not stat.S_ISDIR(metadata.st_mode):
                    return False
            assert metadata is not None
            if not directory_identity_matches(metadata, expected_identity):
                return False
    parent_metadata = os.lstat(output_path.parent)
    report_metadata = os.lstat(output_path)
    return (
        (parent_metadata.st_dev, parent_metadata.st_ino) == expected_parent_identity
        and report_identity_matches(report_metadata, expected_report_identity)
    )


def unlink_report_if_identity(parent_descriptor: int, filename: str, expected_identity: tuple[int, ...]) -> bool:
    """Move the report inode out of the publishable pathname safely."""
    quarantine_prefix = f".{filename}.invalidating-"
    try:
        retained = sum(1 for entry in os.listdir(parent_descriptor) if entry.startswith(quarantine_prefix))
    except OSError:
        retained = MAX_QUARANTINE_DIRECTORIES
    if retained >= MAX_QUARANTINE_DIRECTORIES:
        raise OSError(errno.ENOSPC, "report quarantine retention bound reached")
    quarantine = f"{quarantine_prefix}{secrets.token_hex(16)}"
    quarantine_descriptor = -1
    source_descriptor = -1
    moved = False
    restored = False
    quarantine_identity: tuple[int, int, int, int, int] | None = None
    for _ in range(8):
        try:
            os.mkdir(quarantine, 0o700, dir_fd=parent_descriptor)
            break
        except FileExistsError:
            quarantine = f".{filename}.invalidating-{secrets.token_hex(16)}"
    else:
        raise OSError(errno.EEXIST, "could not allocate an identity-bound report quarantine directory")

    try:
        quarantine_descriptor = os.open(
            quarantine,
            os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0),
            dir_fd=parent_descriptor,
        )
        os.fchmod(quarantine_descriptor, QUARANTINE_MODE)
        quarantine_metadata = os.fstat(quarantine_descriptor)
        quarantine_identity = (
            quarantine_metadata.st_dev,
            quarantine_metadata.st_ino,
            stat.S_IMODE(quarantine_metadata.st_mode),
            quarantine_metadata.st_uid,
            quarantine_metadata.st_gid,
        )
        if (
            not quarantine_identity_matches(quarantine_metadata, quarantine_identity)
            or quarantine_identity[2] != QUARANTINE_MODE
            or quarantine_identity[3:] != (os.getuid(), os.getgid())
        ):
            raise GateError("report quarantine identity, ownership, or mode changed")
        path_metadata = os.stat(quarantine, dir_fd=parent_descriptor, follow_symlinks=False)
        if not quarantine_identity_matches(path_metadata, quarantine_identity):
            raise GateError("report quarantine pathname was replaced before use")
        source_descriptor = os.open(
            filename,
            os.O_RDONLY
            | getattr(os, "O_CLOEXEC", 0)
            | getattr(os, "O_NOFOLLOW", 0)
            | getattr(os, "O_NONBLOCK", 0),
            dir_fd=parent_descriptor,
        )
        source_metadata = os.fstat(source_descriptor)
        if not report_identity_matches(source_metadata, expected_identity):
            return False
        # Retain the original inode across the rename.  Besides binding the
        # later tombstone open to this file descriptor, this prevents inode
        # reuse from making a concurrent replacement appear identical.
        rename_noreplace(parent_descriptor, filename, quarantine_descriptor, filename)
        moved = True
        os.fsync(quarantine_descriptor)
        os.fsync(parent_descriptor)

        metadata = os.stat(filename, dir_fd=quarantine_descriptor, follow_symlinks=False)
        current = os.stat(filename, dir_fd=quarantine_descriptor, follow_symlinks=False)
        held_metadata = os.fstat(source_descriptor)
        held_identity = (held_metadata.st_dev, held_metadata.st_ino)
        # Some filesystems may update ctime when an inode is renamed.  Keep
        # the pre-rename checks for every stable field, then bind the moved
        # pathname and tombstone checks to the descriptor post-rename state.
        stable_expected_identity = expected_identity[:7] if len(expected_identity) >= 8 else expected_identity
        post_rename_identity = report_identity(held_metadata)
        matches = report_identity_matches(held_metadata, stable_expected_identity) and all(
            report_identity_matches(candidate, post_rename_identity)
            and (candidate.st_dev, candidate.st_ino) == held_identity
            for candidate in (metadata, current)
        )
        if quarantine_identity is None or not quarantine_identity_matches(os.fstat(quarantine_descriptor), quarantine_identity):
            raise GateError("report quarantine identity, ownership, or mode changed")
        if not matches:
            link_quarantine_entry_to_public_path(quarantine_descriptor, filename, parent_descriptor)
            restored = True
            os.fsync(quarantine_descriptor)
            os.fsync(parent_descriptor)
            return False
        # The matching report is now private and no longer publishable.  Do
        # not unlink the validated name: POSIX has no identity-bound unlink by
        # descriptor, so a concurrent replacement could otherwise be deleted.
        # Instead, clear the expected inode through its descriptor and retain
        # only a bounded, empty tombstone directory for safe cleanup.
        tombstone_descriptor = os.open(
            filename,
            os.O_WRONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0),
            dir_fd=quarantine_descriptor,
        )
        try:
            tombstone_metadata = os.fstat(tombstone_descriptor)
            if (
                not report_identity_matches(tombstone_metadata, post_rename_identity)
                or (tombstone_metadata.st_dev, tombstone_metadata.st_ino) != held_identity
            ):
                raise GateError("report quarantine entry was replaced before tombstoning")
            os.ftruncate(tombstone_descriptor, 0)
            os.fsync(tombstone_descriptor)
        finally:
            os.close(tombstone_descriptor)
        os.fsync(quarantine_descriptor)
        return True
    finally:
        if source_descriptor >= 0:
            os.close(source_descriptor)
        if quarantine_descriptor >= 0:
            os.close(quarantine_descriptor)
        # There is no Linux unlinkat operation that removes a directory by
        # retained descriptor.  Never reopen/rmdir this pathname after the
        # identity checks: retaining an empty private quarantine directory is
        # safer than unlinking a replacement selected by a concurrent actor.


def invalidate_unbound_report(
    root: Path,
    output_path: Path,
    report_descriptor: int,
    *,
    expected_root_identity: tuple[int, int] | None,
    expected_ancestor_identities: dict[str, tuple[int, ...]] | None,
) -> None:
    """Invalidate only the retained inode when its identity is unavailable.

    The descriptor remains bound to the report created by this invocation even
    when its identity could not be captured.  Mutating the public pathname in
    this state would be unsafe: a concurrent replacement could be moved or
    removed as if it were the original report.  Truncating through the
    retained descriptor invalidates the original inode and leaves any
    replacement pathname untouched.
    """
    try:
        os.ftruncate(report_descriptor, 0)
        os.fsync(report_descriptor)
    finally:
        # The caller owns report_descriptor and closes it after this helper;
        # root/output_path are retained in the signature for the shared
        # recovery seam and future provenance diagnostics.
        del root, output_path, expected_root_identity, expected_ancestor_identities


def invalidate_persisted_report(
    root: Path,
    output_path: Path,
    expected_identity: tuple[int, ...],
    *,
    expected_root_identity: tuple[int, int] | None = None,
    expected_ancestor_identities: dict[str, tuple[int, ...]] | None = None,
) -> None:
    parent_descriptor, filename = open_evidence_parent(
        root,
        output_path,
        expected_root_identity=expected_root_identity,
        expected_ancestor_identities=expected_ancestor_identities,
        enforce_ancestor_safety=False,
    )
    try:
        unlink_report_if_identity(parent_descriptor, filename, expected_identity)
    finally:
        os.close(parent_descriptor)


def rewrite_persisted_report(
    root: Path,
    output_path: Path,
    output: str,
    expected_identity: tuple[int, ...],
    *,
    expected_root_identity: tuple[int, int] | None = None,
    expected_ancestor_identities: dict[str, tuple[int, ...]] | None = None,
) -> tuple[int, ...]:
    if len(expected_identity) < 8:
        raise GateError("interrupted-report recovery requires a complete persisted identity")
    parent_descriptor = -1
    descriptor = -1
    filename = ""
    verified_original = False
    cleanup_identity = expected_identity
    try:
        parent_descriptor, filename = open_evidence_parent(
            root,
            output_path,
            expected_root_identity=expected_root_identity,
            expected_ancestor_identities=expected_ancestor_identities,
            enforce_ancestor_safety=False,
        )
        descriptor = os.open(
            filename,
            os.O_WRONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0),
            dir_fd=parent_descriptor,
        )
        metadata = os.fstat(descriptor)
        if not report_identity_matches(metadata, expected_identity):
            raise GateError("evidence output was replaced before interrupted-report recovery")
        verified_original = True
        write_evidence_descriptor(descriptor, output, truncate=True)
        updated_identity = report_identity(os.fstat(descriptor))
        cleanup_identity = updated_identity
        current = os.stat(filename, dir_fd=parent_descriptor, follow_symlinks=False)
        if not report_identity_matches(current, updated_identity):
            raise GateError("evidence output was replaced during interrupted-report recovery")
        os.fsync(parent_descriptor)
        current = os.stat(filename, dir_fd=parent_descriptor, follow_symlinks=False)
        if not report_identity_matches(current, updated_identity):
            raise GateError("evidence output was replaced after interrupted-report recovery")
        return updated_identity
    except BaseException:
        # Invalidate only the inode verified before writing; preserve replacements.
        if descriptor >= 0 and verified_original:
            try:
                cleanup_identity = report_identity(os.fstat(descriptor))
            except OSError:
                pass
        if parent_descriptor >= 0 and filename:
            try:
                unlink_report_if_identity(parent_descriptor, filename, cleanup_identity)
            except (GateError, OSError) as cleanup_error:
                raise OSError("failed to invalidate interrupted evidence report safely") from cleanup_error
        raise
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        if parent_descriptor >= 0:
            os.close(parent_descriptor)


def set_parent_death_signal() -> None:
    if sys.platform.startswith("linux"):
        import ctypes

        libc = ctypes.CDLL(None, use_errno=True)
        if libc.prctl(1, signal.SIGTERM) != 0:  # PR_SET_PDEATHSIG
            raise OSError(ctypes.get_errno(), "prctl(PR_SET_PDEATHSIG) failed")
        if os.getppid() == 1:
            os.kill(os.getpid(), signal.SIGTERM)


def gate_child_preexec() -> None:
    """Deterministic umask, then parent-death signalling, for every gate child.

    Gate results must not depend on the invoking shell's umask. Debian-family
    user accounts and GitHub runners commonly run with 002, which makes code that
    creates a directory with the default mode produce a group-writable directory.
    The installer then correctly refuses to restore into it, so the same test
    passes on one machine and fails on another. Pinning 022 makes the sandbox
    environment identical everywhere.
    """
    os.umask(0o022)
    set_parent_death_signal()


def secure_path(path: Path, root: Path) -> bool:
    """Accept only executable paths outside the checkout with secure parents."""
    lexical_path = Path(os.path.abspath(path))
    lexical_root = Path(os.path.abspath(root))
    try:
        lexical_path.relative_to(lexical_root)
        return False
    except ValueError:
        pass
    try:
        resolved = lexical_path.resolve(strict=True)
        resolved.relative_to(lexical_root.resolve(strict=True))
        return False
    except (FileNotFoundError, ValueError):
        pass
    current = lexical_path
    while True:
        try:
            metadata = current.stat()
        except OSError:
            return False
        if metadata.st_uid not in {0, os.getuid()} or metadata.st_mode & 0o022:
            return False
        if current.parent == current:
            return True
        current = current.parent


def trusted_tool_directory(root: Path, name: str, candidate: Path) -> Path | None:
    """Resolve an ambient tool only when its directory is a trusted toolchain."""
    try:
        lexical_candidate = Path(os.path.abspath(candidate))
        resolved = lexical_candidate.resolve(strict=True)
    except OSError:
        return None
    lexical_directory = lexical_candidate.parent
    directory = resolved.parent
    if (
        not lexical_candidate.is_file()
        or not os.access(lexical_candidate, os.X_OK)
        or not secure_path(lexical_candidate, root)
        or lexical_directory.is_symlink()
        or not secure_path(lexical_directory, root)
        or not secure_path(directory, root)
    ):
        return None
    if lexical_directory in SAFE_TOOL_DIRECTORIES:
        return lexical_directory
    if lexical_directory.name != "bin" or not any(marker in lexical_directory.parts for marker in TOOLCHAIN_MARKERS):
        return None
    return lexical_directory


def path_tool(name: str, root: Path, inherited_path: str) -> Path | None:
    search_path = inherited_path.split(os.pathsep)
    search_path.extend(directory.as_posix() for directory in SAFE_TOOL_DIRECTORIES)
    for entry in dict.fromkeys(search_path):
        if not entry or not Path(entry).is_absolute():
            continue
        candidate = Path(entry) / name
        if candidate.exists():
            directory = trusted_tool_directory(root, name, candidate)
            if directory is not None:
                return directory
    return None


def selected_tool_executables(root: Path) -> dict[str, SelectedTool]:
    """Resolve and capture selected tool identities before constructing a gate PATH."""
    inherited_path = os.environ.get("PATH", "")
    selected: dict[str, SelectedTool] = {}
    for name in (
        "cargo",
        "cargo-clippy",
        "cargo-deny",
        "cargo-fmt",
        "git",
        "node",
        "npm",
        "rustc",
        "rustfmt",
        "clippy-driver",
    ):
        directory = path_tool(name, root, inherited_path)
        if directory is not None:
            executable = (directory / name).resolve(strict=True)
            source_flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
            try:
                source_fd = os.open(executable, source_flags)
                with os.fdopen(source_fd, "rb", closefd=True) as source:
                    identity = _regular_file_identity(os.fstat(source.fileno()), name)
                    digest, _ = _stream_digest(source)
            except OSError as exc:
                raise GateError(f"selected tool could not be captured: {name}") from exc
            selected[name] = SelectedTool(executable, identity, digest)
    return selected


def trusted_fixed_directories(root: Path, selected: Mapping[str, Path | SelectedTool] | None = None) -> list[Path]:
    """Return secure fixed PATH roots; private links win over their tools."""
    trusted: list[Path] = []
    for directory in SAFE_TOOL_DIRECTORIES:
        if not directory.is_dir() or directory.is_symlink() or not secure_path(directory, root):
            continue
        trusted.append(directory)
    return trusted


def trusted_tooling(root: Path, selected: Mapping[str, Path | SelectedTool] | None = None) -> tuple[list[Path], Path | None, Path | None]:
    """Return selected toolchain and compatible fixed PATH entries."""
    selected = selected if selected is not None else selected_tool_executables(root)
    # Selected inherited tools are rebound through the private gate bin below.
    # Never expose their ambient directory: it may contain an unselected
    # sibling (for example bash) that could shadow a trusted script's shebang.
    path_entries = trusted_fixed_directories(root, selected)
    cargo_home: Path | None = None
    rustup_home: Path | None = None
    cargo_selection = selected.get("cargo")
    cargo_executable = selected_path(cargo_selection) if cargo_selection is not None else None
    cargo_directory = cargo_executable.parent if cargo_executable is not None else None
    if cargo_directory is not None and cargo_directory.parent.name == ".cargo":
        cargo_home = cargo_directory.parent
        for candidate in (cargo_home.parent / ".rustup", cargo_home.parent / "rustup"):
            if candidate.is_dir() and not candidate.is_symlink() and secure_path(candidate, root):
                rustup_home = candidate
                break
    return list(dict.fromkeys(path_entries)), cargo_home, rustup_home


def build_truth(root: Path) -> dict[str, Any]:
    """Run repository truth with the same private tool environment as gates."""
    if has_authenticated_sandbox_capability():
        return build_report(root)
    private_home = Path(tempfile.mkdtemp(prefix="voidtower-truth-home-"))
    original_path = os.environ.get("PATH")
    try:
        os.environ["PATH"] = gate_environment(root, "truth-preflight", private_home)["PATH"]
        return build_report(root)
    finally:
        if original_path is None:
            os.environ.pop("PATH", None)
        else:
            os.environ["PATH"] = original_path
        cleanup_private_home(private_home)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def sha256_nofollow_file(path: Path) -> str:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise GateError("selected tool support tree contains a non-regular file")
        with os.fdopen(descriptor, "rb", closefd=True) as stream:
            digest, _ = _stream_digest(stream)
        return digest
    except BaseException:
        try:
            os.close(descriptor)
        except OSError:
            pass
        raise


def _stream_digest(stream: Any) -> tuple[str, int]:
    digest = hashlib.sha256()
    total = 0
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        total += len(chunk)
        if total > MAX_TOOL_SNAPSHOT_BYTES:
            raise GateError("selected tool snapshot exceeds the bounded size")
        digest.update(chunk)
    return digest.hexdigest(), total


def _regular_file_identity(metadata: os.stat_result, name: str) -> tuple[int, int, int, int]:
    if not stat.S_ISREG(metadata.st_mode):
        raise GateError(f"selected tool is not a regular file: {name}")
    return (metadata.st_dev, metadata.st_ino, metadata.st_size, metadata.st_mtime_ns)


def copy_verified_tool(
    executable: Path,
    bound: Path,
    name: str,
    expected_identity: tuple[int, int, int, int] | None = None,
    expected_digest: str | None = None,
) -> tuple[tuple[int, int, int, int], str]:
    """Copy a selected executable through a retained no-follow descriptor.

    The source descriptor stays open while the bytes are copied and is read a
    second time afterwards. This rejects source replacement or mutation during
    snapshot construction instead of authenticating a raced pathname.
    """
    source_flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        source_fd = os.open(executable, source_flags)
    except OSError as exc:
        raise GateError(f"selected tool could not be opened: {name}") from exc
    try:
        with os.fdopen(source_fd, "rb", closefd=True) as source:
            source_identity = _regular_file_identity(os.fstat(source.fileno()), name)
            source_digest, _ = _stream_digest(source)
            if expected_identity is not None and source_identity != expected_identity:
                raise GateError(f"selected tool changed before binding: {name}")
            if expected_digest is not None and source_digest != expected_digest:
                raise GateError(f"selected tool changed before binding: {name}")
            os.lseek(source.fileno(), 0, os.SEEK_SET)
            destination_flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
            destination_fd = os.open(bound, destination_flags, 0o500)
            try:
                with os.fdopen(destination_fd, "wb", closefd=True) as destination:
                    shutil.copyfileobj(source, destination, length=1024 * 1024)
                    destination.flush()
                    os.fsync(destination.fileno())
            except BaseException:
                try:
                    bound.unlink()
                except OSError:
                    pass
                raise
            bound_digest = sha256_file(bound)
            os.lseek(source.fileno(), 0, os.SEEK_SET)
            post_digest, _ = _stream_digest(source)
            post_identity = _regular_file_identity(os.fstat(source.fileno()), name)
    except GateError:
        raise
    except OSError as exc:
        raise GateError(f"selected tool could not be bound: {name}") from exc
    if post_identity != source_identity or post_digest != source_digest or bound_digest != source_digest:
        try:
            bound.unlink()
        except OSError:
            pass
        raise GateError(f"selected tool changed while being bound: {name}")
    return source_identity, bound_digest


def directory_digest(directory: Path) -> str:
    """Hash a bounded, regular-file-only directory tree deterministically."""
    digest = hashlib.sha256()
    entries = 0
    total = 0
    for current, directories, files in os.walk(directory, topdown=True, followlinks=False):
        directories.sort()
        files.sort()
        for item in directories + files:
            path = Path(current) / item
            metadata = path.lstat()
            entries += 1
            if entries > MAX_TOOL_SNAPSHOT_ENTRIES:
                raise GateError("selected tool support tree is not bounded")
            relative = path.relative_to(directory).as_posix().encode("utf-8")
            digest.update(relative + b"\0")
            if stat.S_ISLNK(metadata.st_mode):
                link_target = os.readlink(path)
                try:
                    (path.parent / link_target).resolve(strict=True).relative_to(directory.resolve(strict=True))
                except (OSError, ValueError) as exc:
                    raise GateError("selected tool support tree contains an unsafe link") from exc
                digest.update(b"l\0" + link_target.encode("utf-8") + b"\0")
                continue
            if stat.S_ISDIR(metadata.st_mode):
                digest.update(b"d\0")
                continue
            if not stat.S_ISREG(metadata.st_mode):
                raise GateError("selected tool support tree contains an unsafe entry")
            file_digest = sha256_nofollow_file(path)
            total += metadata.st_size
            if total > MAX_TOOL_SNAPSHOT_BYTES:
                raise GateError("selected tool support tree exceeds the bounded size")
            digest.update(b"f\0" + str(metadata.st_size).encode("ascii") + b"\0" + file_digest.encode("ascii"))
    return digest.hexdigest()


def copy_verified_directory(source: Path, destination: Path) -> None:
    before = directory_digest(source)
    shutil.copytree(source, destination, symlinks=True)
    try:
        if directory_digest(source) != before or directory_digest(destination) != before:
            raise GateError("selected tool support tree changed while being bound")
        for current, directories, files in os.walk(destination, topdown=True):
            for item in directories:
                path = Path(current) / item
                if not path.is_symlink():
                    path.chmod(0o555)
            for item in files:
                path = Path(current) / item
                if not path.is_symlink():
                    path.chmod(0o555)
        destination.chmod(0o555)
    except BaseException:
        shutil.rmtree(destination, ignore_errors=True)
        raise


def write_tool_identity_launcher(
    tool_bin: Path,
    bound_bin: Path,
    name: str,
    executable: Path,
    selection: SelectedTool | None = None,
) -> None:
    """Bind an independent selected-tool snapshot without PATH fallback."""
    if name == "npm":
        package_source = executable.parent.parent
        package_destination = bound_bin.parent / "npm"
        try:
            copy_verified_directory(package_source, package_destination)
            bound = package_destination / "bin" / executable.name
            bound.parent.chmod(0o755)
            bound.unlink()
        except (GateError, OSError) as exc:
            raise GateError("selected npm package could not be bound") from exc
    else:
        bound = bound_bin / name
    try:
        identity, bound_digest = copy_verified_tool(
            executable,
            bound,
            name,
            selection.identity if selection is not None else None,
            selection.digest if selection is not None else None,
        )
    except OSError as exc:
        raise GateError(f"selected tool could not be bound: {name}") from exc
    if name == "npm":
        for current, directories, files in os.walk(bound_bin.parent / "npm", topdown=True):
            for item in directories:
                path = Path(current) / item
                if not path.is_symlink():
                    path.chmod(0o555)
            for item in files:
                path = Path(current) / item
                if not path.is_symlink():
                    path.chmod(0o555)
        (bound_bin.parent / "npm").chmod(0o555)
    bound_identity = (bound.stat().st_dev, bound.stat().st_ino, bound.stat().st_size, bound.stat().st_mtime_ns)
    launcher = tool_bin / name
    launcher.write_text(
        "#!" + sys.executable + "\n"
        "import os\n"
        "import hashlib\n"
        "import sys\n"
        f"target = {executable.as_posix()!r}\n"
        f"identity = {identity!r}\n"
        f"bound = {bound.as_posix()!r}\n"
        f"bound_identity = {bound_identity!r}\n"
        f"bound_digest = {bound_digest!r}\n"
        "def digest(path):\n"
        "    value = hashlib.sha256()\n"
        "    with open(path, 'rb') as stream:\n"
        "        for chunk in iter(lambda: stream.read(1024 * 1024), b''):\n"
        "            value.update(chunk)\n"
        "    return value.hexdigest()\n"
        "try:\n"
        "    current = os.stat(target)\n"
        "    observed = (current.st_dev, current.st_ino, current.st_size, current.st_mtime_ns)\n"
        "    bound_current = os.stat(bound)\n"
        "    bound_observed = (bound_current.st_dev, bound_current.st_ino, bound_current.st_size, bound_current.st_mtime_ns)\n"
        "    if observed != identity or os.path.realpath(target) != target or bound_observed != bound_identity or digest(bound) != bound_digest:\n"
        "        raise OSError(\"selected tool identity changed\")\n"
        "    os.execv(bound, [bound, *sys.argv[1:]])\n"
        "except OSError:\n"
        "    print(\"selected tool unavailable\", file=sys.stderr)\n"
        "    raise SystemExit(126)\n",
        encoding="utf-8",
    )
    launcher.chmod(0o700)


def _read_bounded_metadata(path: Path, limit: int, description: str) -> str:
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = -1
    try:
        descriptor = os.open(path, flags)
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise GateError(f"{description} is not a regular file")
        with os.fdopen(descriptor, "rb", closefd=True) as stream:
            descriptor = -1
            content = stream.read(limit + 1)
        if len(content) > limit:
            raise GateError(f"{description} exceeds the bounded size")
        return content.decode("utf-8")
    except (OSError, UnicodeError) as exc:
        raise GateError(f"{description} could not be read") from exc
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _metadata_identity(path: Path) -> tuple[int, ...]:
    try:
        metadata = os.stat(path, follow_symlinks=False)
    except OSError as exc:
        raise GateError("Git metadata identity could not be captured") from exc
    return (metadata.st_dev, metadata.st_ino, metadata.st_mode, metadata.st_size, metadata.st_mtime_ns, metadata.st_ctime_ns)


def _trusted_git_executable(root: Path) -> Path:
    for directory in SAFE_TOOL_DIRECTORIES:
        candidate = directory / "git"
        if trusted_tool_directory(root, "git", candidate) is not None:
            return candidate.resolve(strict=True)
    raise GateError("linked worktree association requires a trusted Git executable")


def _git_metadata_environment(git_executable: Path) -> dict[str, str]:
    """Run association diagnostics without inheriting caller Git configuration."""
    return {
        "PATH": git_executable.parent.as_posix(),
        "HOME": "/",
        "LANG": "C",
        "LC_ALL": "C",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_SYSTEM": os.devnull,
        "GIT_TERMINAL_PROMPT": "0",
    }


def _set_parent_death_signal() -> None:
    """Make the association supervisor fail closed if this runner dies."""
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(1, signal.SIGTERM) != 0:  # PR_SET_PDEATHSIG
        os._exit(127)
    if os.getppid() == 1:
        os._exit(127)


def _signal_supervisor(process: subprocess.Popen[bytes], start_time: int | None, signum: int) -> bool:
    """Signal a supervisor through a pidfd, never through a reusable PID."""
    if process.poll() is not None:
        return True
    if start_time is None:
        return signal_owned_process(process.pid, signum)
    return signal_process(process.pid, start_time, signum)


def _bounded_git_worktree_listing(root: Path, git_executable: Path) -> bytes:
    process: subprocess.Popen[bytes] | None = None
    supervisor_start_time: int | None = None
    terminate_descendants = False
    cleanup_error: CleanupError | None = None

    def attempt_descendant_cleanup() -> None:
        nonlocal cleanup_error
        assert process is not None
        # Retry a transient process-snapshot failure before terminating the
        # supervisor. The first failure remains part of the blocked outcome,
        # even when the bounded retry proves cleanup on the second pass.
        for _attempt in range(2):
            try:
                kill_descendants(process.pid, reap_adopted=False, root_start_time=supervisor_start_time)
                return
            except CleanupError as exc:
                cleanup_error = cleanup_error or exc
                if not isinstance(exc, ProcessSnapshotError):
                    return

    try:
        supervisor_command = supervised_command(
            root,
            [git_executable.as_posix(), "-C", root.as_posix(), "--no-optional-locks", "worktree", "list", "--porcelain"],
        )
        process = subprocess.Popen(
            supervisor_command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            env=_git_metadata_environment(git_executable),
            close_fds=True,
            start_new_session=True,
            preexec_fn=_set_parent_death_signal if sys.platform.startswith("linux") else None,
        )
        supervisor_start_time = process_start_time(process.pid)
        assert process.stdout is not None
        os.set_blocking(process.stdout.fileno(), False)
        selector = selectors.DefaultSelector()
        selector.register(process.stdout, selectors.EVENT_READ)
        output = bytearray()
        deadline = time.monotonic() + 5
        try:
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise GateError("linked worktree association timed out")
                events = selector.select(min(0.1, remaining))
                if not events:
                    continue
                chunk = os.read(process.stdout.fileno(), 16 * 1024)
                if not chunk:
                    break
                output.extend(chunk)
                if len(output) > MAX_GIT_METADATA_BYTES:
                    raise GateError("linked worktree listing exceeds the bounded size")
        finally:
            selector.close()
            process.stdout.close()
        try:
            return_code = process.wait(timeout=max(0.1, deadline - time.monotonic()))
        except subprocess.TimeoutExpired as exc:
            raise GateError("linked worktree association timed out") from exc
        if return_code != 0:
            raise GateError("linked worktree association could not be verified")
        return bytes(output)
    except (OSError, subprocess.SubprocessError) as exc:
        terminate_descendants = True
        raise GateError("linked worktree association could not be verified") from exc
    except BaseException:
        terminate_descendants = True
        raise
    finally:
        if process is not None and terminate_descendants:
            if supervisor_start_time is None:
                cleanup_error = CleanupError("linked worktree supervisor identity was not established")
            else:
                attempt_descendant_cleanup()
            try:
                if _signal_supervisor(process, supervisor_start_time, signal.SIGTERM) and process.poll() is None:
                    try:
                        process.wait(timeout=1)
                    except subprocess.TimeoutExpired:
                        cleanup_error = cleanup_error or CleanupError("linked worktree supervisor did not terminate after SIGTERM")
                if process.poll() is None:
                    if not _signal_supervisor(process, supervisor_start_time, signal.SIGKILL):
                        cleanup_error = cleanup_error or CleanupError("linked worktree supervisor identity could not be revalidated")
                    else:
                        try:
                            process.wait(timeout=1)
                        except subprocess.TimeoutExpired:
                            cleanup_error = cleanup_error or CleanupError("linked worktree supervisor did not terminate after SIGKILL")
            except CleanupError as exc:
                cleanup_error = exc
            finally:
                if process.stdout is not None:
                    process.stdout.close()
            if cleanup_error is not None:
                raise GateError("linked worktree association cleanup could not be verified") from cleanup_error


def _close_metadata_anchors(anchors: Mapping[Path, int]) -> None:
    for descriptor in anchors.values():
        try:
            os.close(descriptor)
        except OSError:
            pass


def _open_metadata_anchors(
    mounts: list[tuple[Path, Path]],
    identities: Mapping[Path, tuple[int, ...]],
) -> dict[Path, int]:
    anchors: dict[Path, int] = {}
    base_flags = getattr(os, "O_PATH", os.O_RDONLY) | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    try:
        for source, _destination in mounts:
            expected = identities[source]
            flags = base_flags | (getattr(os, "O_DIRECTORY", 0) if stat.S_ISDIR(expected[2]) else 0)
            try:
                descriptor = os.open(source, flags)
            except OSError as exc:
                raise GateError("Git metadata identity anchor could not be opened") from exc
            anchors[source] = descriptor
            if _metadata_identity_from_descriptor(descriptor) != expected:
                raise GateError("Git metadata changed while its identity anchor was captured")
        return anchors
    except BaseException:
        _close_metadata_anchors(anchors)
        raise

def _validated_git_metadata_mounts(
    root: Path,
    git_executable: Path | None = None,
) -> tuple[list[tuple[Path, Path]], dict[Path, tuple[int, ...]], dict[Path, int]]:
    """Return read-only Git metadata mounts for normal and linked worktrees."""
    git_metadata = root / ".git"
    if git_metadata.is_symlink():
        raise GateError("repository metadata must not be a symlink")
    if not git_metadata.exists():
        return [], {}, {}
    git_metadata_identity = _metadata_identity(git_metadata)
    if git_metadata.is_dir():
        mounts = [(git_metadata, git_metadata)]
        identities = {git_metadata: git_metadata_identity}
        return mounts, identities, _open_metadata_anchors(mounts, identities)
    if not git_metadata.is_file():
        raise GateError("repository metadata must be a regular file or directory")

    try:
        pointer = _read_bounded_metadata(git_metadata, MAX_GIT_POINTER_BYTES, "linked worktree metadata")
    except GateError:
        raise
    prefix = "gitdir: "
    if not pointer.startswith(prefix) or "\n" not in pointer:
        raise GateError("linked worktree metadata has an invalid gitdir pointer")
    target_text = pointer[len(prefix) : pointer.index("\n")].strip()
    if not target_text or "\x00" in target_text:
        raise GateError("linked worktree metadata has an invalid gitdir pointer")
    target = Path(target_text)
    if not target.is_absolute():
        target = git_metadata.parent / target

    def resolve_metadata_directory(candidate: Path, description: str) -> Path:
        lexical = Path(os.path.abspath(candidate))
        current = Path(lexical.anchor)
        for part in lexical.parts[1:]:
            current /= part
            if current.is_symlink():
                raise GateError(f"linked worktree {description} must not traverse a symlink")
        try:
            resolved = lexical.resolve(strict=True)
        except (OSError, RuntimeError, ValueError) as exc:
            raise GateError(f"linked worktree {description} is unavailable") from exc
        if resolved != lexical or not resolved.is_dir():
            raise GateError(f"linked worktree {description} is not a regular directory")
        return resolved

    target = resolve_metadata_directory(target, "gitdir")
    target_identity = _metadata_identity(target)

    common_dir = target
    commondir = target / "commondir"
    if commondir.is_symlink() or commondir.exists():
        if commondir.is_symlink() or not commondir.is_file():
            raise GateError("linked worktree commondir is unsafe")
        try:
            common_text = _read_bounded_metadata(commondir, MAX_GIT_POINTER_BYTES, "linked worktree commondir").strip()
        except GateError:
            raise
        if not common_text or "\x00" in common_text:
            raise GateError("linked worktree commondir is invalid")
        common_dir = Path(common_text)
        if not common_dir.is_absolute():
            common_dir = target / common_dir
        common_dir = resolve_metadata_directory(common_dir, "common Git directory")
    else:
        common_dir = resolve_metadata_directory(common_dir, "common Git directory")
    common_dir_identity = _metadata_identity(common_dir)
    if common_dir.name != ".git" or target.parent != common_dir / "worktrees":
        raise GateError("linked worktree Git metadata has a non-canonical topology")
    for masked_root in (Path("/run"), Path("/tmp"), Path("/var/tmp"), Path("/dev/shm")):
        try:
            common_dir.relative_to(masked_root)
        except ValueError:
            continue
        raise GateError("linked worktree Git metadata is under a masked host runtime path")
    back_pointer = target / "gitdir"
    if back_pointer.is_symlink() or not back_pointer.is_file():
        raise GateError("linked worktree gitdir is missing its checkout back-pointer")
    try:
        back_text = _read_bounded_metadata(back_pointer, MAX_GIT_POINTER_BYTES, "linked worktree checkout back-pointer").strip()
    except GateError:
        raise
    if not back_text or "\x00" in back_text:
        raise GateError("linked worktree checkout back-pointer is invalid")
    back_target = Path(back_text)
    if not back_target.is_absolute():
        back_target = target / back_target
    if Path(os.path.abspath(back_target)) != Path(os.path.abspath(git_metadata)):
        raise GateError("linked worktree checkout back-pointer does not match the checkout")
    git_executable = git_executable or _trusted_git_executable(root)
    try:
        worktree_listing = _bounded_git_worktree_listing(root, git_executable).decode("utf-8")
    except UnicodeError as exc:
        raise GateError("linked worktree association output is not valid UTF-8") from exc
    checkout_path = Path(os.path.abspath(root))
    listed_paths = {
        Path(line.removeprefix("worktree ").strip()).absolute()
        for line in worktree_listing.splitlines()
        if line.startswith("worktree ")
    }
    if checkout_path not in listed_paths:
        raise GateError("linked worktree is not registered by the common Git directory")
    try:
        target.relative_to(common_dir)
    except ValueError as exc:
        raise GateError("linked worktree gitdir escapes its common Git directory") from exc
    mounts = [(git_metadata, git_metadata), (target, target), (common_dir, common_dir)]
    identities = {
        git_metadata: git_metadata_identity,
        target: target_identity,
        common_dir: common_dir_identity,
    }
    return mounts, identities, _open_metadata_anchors(mounts, identities)


def git_metadata_mounts(root: Path, git_executable: Path | None = None) -> list[tuple[Path, Path]]:
    # Return mounts only; validation anchors are private to this call.
    mounts, _identities, anchors = _validated_git_metadata_mounts(root, git_executable)
    try:
        return mounts
    finally:
        _close_metadata_anchors(anchors)


def _open_anchored_metadata(
    root: Path,
    mounts: list[tuple[Path, Path]],
    identities: dict[Path, tuple[int, ...]],
    anchors: Mapping[Path, int],
) -> list[int]:
    flags = getattr(os, "O_PATH", os.O_RDONLY) | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    root_identity = _metadata_identity(root)
    root_descriptor = os.open(root, flags | getattr(os, "O_DIRECTORY", 0))
    parent_descriptor = -1
    external_anchor_descriptors: list[int] = []
    descriptors: list[int] = []
    try:
        if _metadata_identity_from_descriptor(root_descriptor) != root_identity:
            raise GateError("checkout directory changed during Git metadata validation")
        parent_descriptor = os.open("..", flags | getattr(os, "O_DIRECTORY", 0), dir_fd=root_descriptor)
        root_absolute = Path(os.path.abspath(root))
        parent_absolute = root_absolute.parent
        for source, _destination in mounts:
            expected_identity = identities.get(source)
            anchor_descriptor = anchors.get(source)
            if expected_identity is None or anchor_descriptor is None:
                raise GateError("Git metadata identity anchor is unavailable")
            if _metadata_identity_from_descriptor(anchor_descriptor) != expected_identity:
                raise GateError("Git metadata identity anchor changed before its read-only mount was opened")
            if source == root_absolute / ".git":
                anchor = root_descriptor
                relative = Path(".git")
            else:
                try:
                    relative = Path(os.path.abspath(source)).relative_to(parent_absolute)
                    anchor = parent_descriptor
                except ValueError:
                    source_absolute = Path(os.path.abspath(source))
                    if not source_absolute.is_absolute() or len(source_absolute.parts) < 2:
                        raise GateError("Git metadata mount has an invalid absolute path")
                    anchor = os.open("/", flags | getattr(os, "O_DIRECTORY", 0))
                    external_anchor_descriptors.append(anchor)
                    relative = Path(*source_absolute.parts[1:])
            descriptor = anchor
            owned: list[int] = []
            try:
                for component in relative.parts:
                    next_descriptor = os.open(component, flags, dir_fd=descriptor)
                    if descriptor != anchor:
                        os.close(descriptor)
                    descriptor = next_descriptor
                    owned.append(descriptor)
                if _metadata_identity_from_descriptor(descriptor) != expected_identity:
                    raise GateError("Git metadata changed before its read-only mount was opened")
                descriptors.append(descriptor)
                owned.clear()
            finally:
                for opened in owned:
                    try:
                        os.close(opened)
                    except OSError:
                        pass
        return descriptors
    except BaseException:
        for descriptor in descriptors:
            try:
                os.close(descriptor)
            except OSError:
                pass
        raise
    finally:
        for descriptor in external_anchor_descriptors:
            try:
                os.close(descriptor)
            except OSError:
                pass
        if parent_descriptor >= 0:
            os.close(parent_descriptor)
        os.close(root_descriptor)


def _metadata_identity_from_descriptor(descriptor: int) -> tuple[int, ...]:
    metadata = os.fstat(descriptor)
    return (metadata.st_dev, metadata.st_ino, metadata.st_mode, metadata.st_size, metadata.st_mtime_ns, metadata.st_ctime_ns)


def _close_descriptor(descriptor: int) -> None:
    """Close a best-effort cleanup descriptor without masking the primary error."""
    try:
        os.close(descriptor)
    except OSError:
        pass


def _open_trusted_directory_descriptor(
    path: Path,
    root: Path,
    expected_identity: tuple[int, ...],
    label: str,
) -> int:
    """Open a trusted directory component-by-component without following links."""
    if not path.is_absolute() or not secure_path(path, root):
        raise GateError(f"trusted {label} is not a safe directory")
    flags = getattr(os, "O_PATH", os.O_RDONLY) | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_DIRECTORY", 0)
    descriptor = -1
    opened_descriptors: list[int] = []
    try:
        descriptor = os.open("/", flags)
        opened_descriptors.append(descriptor)
        for component in path.parts[1:]:
            child = os.open(component, flags, dir_fd=descriptor)
            opened_descriptors.append(child)
            previous = descriptor
            descriptor = child
            os.close(previous)
            opened_descriptors.remove(previous)
            metadata = os.fstat(descriptor)
            if metadata.st_uid not in {0, os.getuid()} or metadata.st_mode & 0o022 or not stat.S_ISDIR(metadata.st_mode):
                raise GateError(f"trusted {label} is not a safe directory")
        if _metadata_identity_from_descriptor(descriptor) != expected_identity:
            raise GateError(f"trusted {label} changed before binding")
        return descriptor
    except (OSError, GateError) as exc:
        for opened_descriptor in opened_descriptors:
            _close_descriptor(opened_descriptor)
        if isinstance(exc, GateError):
            raise
        raise GateError(f"trusted {label} could not be opened") from exc


def _open_checkout_descriptor(root: Path, expected_identity: tuple[int, int, int, int, int]) -> int:
    flags = getattr(os, "O_PATH", os.O_RDONLY) | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_DIRECTORY", 0)
    try:
        descriptor = os.open(root, flags)
    except OSError as exc:
        raise GateError("checkout directory could not be opened for identity-bound mounting") from exc
    try:
        if _metadata_identity_from_descriptor(descriptor) != expected_identity:
            raise GateError("checkout directory changed before its read-only namespace bind")
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def _copy_advisory_cache_from_descriptor(source_descriptor: int, destination: Path) -> None:
    entry_count = 0
    byte_count = 0
    directory_flags = (
        os.O_RDONLY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_DIRECTORY", 0)
    )
    file_flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(os, "O_NOFOLLOW", 0)
    destination_file_flags = (
        os.O_WRONLY
        | os.O_CREAT
        | os.O_EXCL
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
    )

    def identity(metadata: os.stat_result) -> tuple[int, int, int, int, int, int]:
        return (
            metadata.st_dev,
            metadata.st_ino,
            metadata.st_mode,
            metadata.st_size,
            metadata.st_mtime_ns,
            metadata.st_ctime_ns,
        )

    def require_trusted_owner(metadata: os.stat_result) -> None:
        if metadata.st_uid not in {0, os.getuid()}:
            raise GateError("trusted Cargo advisory cache contains an untrusted owner")
        if metadata.st_mode & 0o022:
            raise GateError("trusted Cargo advisory cache contains a group/world-writable entry")

    def copy_directory(source_fd: int, target: Path) -> None:
        nonlocal entry_count, byte_count
        before_directory = os.fstat(source_fd)
        if not stat.S_ISDIR(before_directory.st_mode):
            raise GateError("trusted Cargo advisory cache contains a non-directory")
        with os.scandir(source_fd) as entries:
            for entry in entries:
                name = entry.name
                if name in {"", ".", ".."} or "/" in name or "\x00" in name:
                    raise GateError("trusted Cargo advisory cache contains an unsafe path")
                entry_count += 1
                if entry_count > MAX_TOOL_SNAPSHOT_ENTRIES:
                    raise GateError("trusted Cargo advisory cache exceeds the entry limit")
                metadata = os.stat(name, dir_fd=source_fd, follow_symlinks=False)
                require_trusted_owner(metadata)
                if stat.S_ISDIR(metadata.st_mode):
                    child_fd = os.open(name, directory_flags, dir_fd=source_fd)
                    child_target = target / name
                    try:
                        if identity(os.fstat(child_fd)) != identity(metadata):
                            raise GateError("trusted Cargo advisory cache changed before copying")
                        child_target.mkdir(mode=0o700)
                        copy_directory(child_fd, child_target)
                        if identity(os.fstat(child_fd)) != identity(metadata):
                            raise GateError("trusted Cargo advisory cache changed while copying")
                    finally:
                        _close_descriptor(child_fd)
                elif stat.S_ISREG(metadata.st_mode):
                    source_file_fd = os.open(name, file_flags, dir_fd=source_fd)
                    target_fd = -1
                    try:
                        before_file = os.fstat(source_file_fd)
                        if identity(before_file) != identity(metadata):
                            raise GateError("trusted Cargo advisory cache changed before copying")
                        if before_file.st_size < 0 or byte_count + before_file.st_size > MAX_TOOL_SNAPSHOT_BYTES:
                            raise GateError("trusted Cargo advisory cache exceeds the size limit")
                        target_fd = os.open(target / name, destination_file_flags, 0o600)
                        file_bytes = 0
                        while True:
                            chunk = os.read(source_file_fd, 1024 * 1024)
                            if not chunk:
                                break
                            file_bytes += len(chunk)
                            byte_count += len(chunk)
                            if byte_count > MAX_TOOL_SNAPSHOT_BYTES or file_bytes > before_file.st_size:
                                raise GateError("trusted Cargo advisory cache changed while copying")
                            view = memoryview(chunk)
                            while view:
                                written = os.write(target_fd, view)
                                if written <= 0:
                                    raise OSError("short write while copying advisory cache")
                                view = view[written:]
                        if file_bytes != before_file.st_size or identity(os.fstat(source_file_fd)) != identity(before_file):
                            raise GateError("trusted Cargo advisory cache changed while copying")
                        os.fsync(target_fd)
                    finally:
                        if target_fd >= 0:
                            _close_descriptor(target_fd)
                        _close_descriptor(source_file_fd)
                else:
                    raise GateError("trusted Cargo advisory cache contains a symlink or special file")
        if identity(os.fstat(source_fd)) != identity(before_directory):
            raise GateError("trusted Cargo advisory cache changed while copying")

    try:
        destination.mkdir(mode=0o700)
        copy_directory(source_descriptor, destination)
        destination.chmod(0o700)
    except BaseException as exc:
        shutil.rmtree(destination, ignore_errors=True)
        if isinstance(exc, (GateError, KeyboardInterrupt, SystemExit)):
            raise
        raise GateError("trusted Cargo advisory cache could not be copied safely") from exc


def _open_cargo_cache_descriptors(
    root: Path,
    private_home: Path,
    selected: Mapping[str, Path | SelectedTool],
) -> list[tuple[Path, int]]:
    _path_entries, cargo_home_source, _rustup_home = trusted_tooling(root, selected)
    if cargo_home_source is None:
        return []
    try:
        cargo_home_metadata = cargo_home_source.stat(follow_symlinks=False)
    except OSError as exc:
        raise GateError("trusted Cargo home is not a safe directory") from exc
    cargo_home_identity = (
        cargo_home_metadata.st_dev,
        cargo_home_metadata.st_ino,
        cargo_home_metadata.st_mode,
        cargo_home_metadata.st_size,
        cargo_home_metadata.st_mtime_ns,
        cargo_home_metadata.st_ctime_ns,
    )
    if (
        not stat.S_ISDIR(cargo_home_metadata.st_mode)
        or cargo_home_metadata.st_uid not in {0, os.getuid()}
        or cargo_home_metadata.st_mode & 0o022
    ):
        raise GateError("trusted Cargo home is not a safe directory")
    private_cargo_home = private_home / "cargo-home"
    try:
        private_cargo_home.mkdir(mode=0o700, exist_ok=True)
        private_cargo_metadata = private_cargo_home.stat(follow_symlinks=False)
    except OSError as exc:
        raise GateError("private Cargo home is not a safe directory") from exc
    if (
        not stat.S_ISDIR(private_cargo_metadata.st_mode)
        or private_cargo_metadata.st_uid not in {0, os.getuid()}
        or private_cargo_metadata.st_mode & 0o022
    ):
        raise GateError("private Cargo home is not a safe directory")
    cache_descriptors: list[tuple[Path, int]] = []
    readonly_flags = (
        getattr(os, "O_PATH", os.O_RDONLY)
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_DIRECTORY", 0)
    )
    copy_flags = (
        os.O_RDONLY
        | getattr(os, "O_CLOEXEC", 0)
        | getattr(os, "O_NOFOLLOW", 0)
        | getattr(os, "O_DIRECTORY", 0)
    )
    cargo_home_descriptor = _open_trusted_directory_descriptor(cargo_home_source, root, cargo_home_identity, "Cargo home")
    try:
        for cache_name in ("registry", "git", "advisory-dbs"):
            try:
                expected = os.stat(cache_name, dir_fd=cargo_home_descriptor, follow_symlinks=False)
            except FileNotFoundError:
                continue
            if (
                not stat.S_ISDIR(expected.st_mode)
                or expected.st_uid not in {0, os.getuid()}
                or expected.st_mode & 0o022
            ):
                raise GateError(f"trusted Cargo {cache_name} cache is not a safe directory")
            destination = private_cargo_home / cache_name
            open_flags = readonly_flags if cache_name != "advisory-dbs" else copy_flags
            descriptor = os.open(cache_name, open_flags, dir_fd=cargo_home_descriptor)
            try:
                expected_identity = (
                    expected.st_dev,
                    expected.st_ino,
                    expected.st_mode,
                    expected.st_size,
                    expected.st_mtime_ns,
                    expected.st_ctime_ns,
                )
                if _metadata_identity_from_descriptor(descriptor) != expected_identity:
                    raise GateError(f"trusted Cargo {cache_name} cache changed before binding")
                if cache_name == "advisory-dbs":
                    _copy_advisory_cache_from_descriptor(descriptor, destination)
                    _close_descriptor(descriptor)
                    continue
                destination.mkdir(parents=True, exist_ok=True)
                cache_descriptors.append((destination, descriptor))
            except BaseException:
                _close_descriptor(descriptor)
                raise
        return cache_descriptors
    except BaseException:
        for _destination, descriptor in cache_descriptors:
            _close_descriptor(descriptor)
        raise
    finally:
        _close_descriptor(cargo_home_descriptor)

def sandboxed_command(
    root: Path,
    private_home: Path,
    command: list[str],
    environment: Mapping[str, str] | None = None,
    selected: Mapping[str, Path | SelectedTool] | None = None,
) -> SandboxedCommand:
    """Run a gate with immutable selected-tool mounts inside a Linux namespace."""
    if not sys.platform.startswith("linux"):
        raise GateError("read-only gate sandbox is only supported on Linux")
    bubblewrap = None
    for directory in SAFE_TOOL_DIRECTORIES:
        candidate = directory / "bwrap"
        if candidate.is_file() and os.access(candidate, os.X_OK) and secure_path(candidate, private_home):
            bubblewrap = candidate.as_posix()
            break
    if bubblewrap is None:
        raise GateError("read-only gate sandbox requires bubblewrap")
    capability_mount: tuple[Path, Path] | None = None
    if environment is not None:
        capability_path = Path(environment.get(SANDBOX_CAPABILITY_PATH_ENV, ""))
        capability_digest = environment.get(SANDBOX_CAPABILITY_DIGEST_ENV, "")
        if (
            capability_path.parent != SANDBOX_CAPABILITY_DIR
            or not SANDBOX_CAPABILITY_NAME.fullmatch(capability_path.name)
            or not re.fullmatch(r"[0-9a-f]{64}", capability_digest)
        ):
            raise GateError("gate sandbox capability is malformed")
        capability_source = private_home / "capability"
        try:
            if sha256_nofollow_file(capability_source) != capability_digest:
                raise GateError("gate sandbox capability changed before binding")
        except OSError as exc:
            raise GateError("gate sandbox capability is unavailable") from exc
        capability_mount = (capability_source, capability_path)
    checkout_identity = _metadata_identity(root)
    git_selection = selected.get("git") if selected is not None else None
    git_executable = private_home / "bound" / "git" if git_selection is not None else None
    mounts = [
        ("--ro-bind", private_home / "bin"),
        ("--ro-bind", private_home / "bound"),
        ("--ro-bind", private_home / "support"),
        ("--bind", private_home / "home"),
        ("--bind", private_home / "tmp"),
    ]
    cargo_home = private_home / "cargo-home"
    if cargo_home.exists():
        mounts.append(("--bind", cargo_home))
    cargo_cache_descriptors: list[tuple[Path, int]] = []
    private_npm = private_home / "npm"
    if private_npm.exists():
        mounts.insert(3, ("--ro-bind", private_npm))
    result = [
        bubblewrap,
        "--die-with-parent",
        "--new-session",
        "--unshare-pid",
        "--unshare-net",
        "--ro-bind",
        "/",
        "/",
        # A private /dev is required: with only a read-only bind of the host root,
        # /dev/null is not writable inside the namespace and every gate script that
        # redirects to it fails with "Permission denied". A private /proc keeps the
        # namespace's own process view.
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/home",
        "--tmpfs",
        "/run",
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/var/tmp",
    ]
    namespace_directories: set[str] = set()

    def add_namespace_parent_directories(path: Path) -> None:
        for masked_root in (Path("/tmp"), Path("/var/tmp"), Path("/dev/shm")):
            try:
                relative = path.relative_to(masked_root)
            except ValueError:
                continue
            current = masked_root
            for part in relative.parts[:-1]:
                current /= part
                if current.as_posix() not in namespace_directories:
                    result.extend(("--dir", current.as_posix()))
                    namespace_directories.add(current.as_posix())
            break

    add_namespace_parent_directories(root)
    add_namespace_parent_directories(private_home)
    try:
        root.relative_to(Path("/home"))
    except ValueError:
        pass
    else:
        parent = Path("/")
        for part in root.parts[1:]:
            parent /= part
            if parent.as_posix() not in namespace_directories:
                result.extend(("--dir", parent.as_posix()))
                namespace_directories.add(parent.as_posix())

    def add_mount_parent_directories(path: Path) -> None:
        parent = path.parent
        ancestors: list[Path] = []
        while parent != Path("/"):
            ancestors.append(parent)
            parent = parent.parent
        for directory in reversed(ancestors):
            if directory.as_posix() not in namespace_directories:
                result.extend(("--dir", directory.as_posix()))
                namespace_directories.add(directory.as_posix())

    for _option, mount_path in mounts:
        add_mount_parent_directories(mount_path)
    result.extend(("--dev", "/dev", "--proc", "/proc", "--tmpfs", "/dev/shm"))
    private_roots: list[Path] = []
    runtime = Path(sys.executable)
    try:
        runtime.absolute().relative_to(Path("/home"))
    except ValueError:
        pass
    else:
        private_roots.append(runtime.absolute().parent.parent)
    try:
        resolved_runtime = runtime.resolve(strict=True)
        resolved_runtime.relative_to(Path("/home"))
    except (FileNotFoundError, ValueError):
        pass
    else:
        # A launcher may be a symlink into a virtualenv under the hidden home.
        # Rebind that resolved runtime tree read-only so the launcher remains
        # executable without exposing the host's writable home.
        private_roots.append(resolved_runtime.parent.parent)
        if ".hermes-runtime" in resolved_runtime.parts:
            runtime_index = resolved_runtime.parts.index(".hermes-runtime")
            private_roots.append(Path(*resolved_runtime.parts[: runtime_index + 1]))
    link_path = runtime.absolute()
    for _ in range(8):
        if not link_path.is_symlink():
            break
        try:
            link_target = Path(os.readlink(link_path))
            if not link_target.is_absolute():
                link_target = link_path.parent / link_target
            link_target = link_target.absolute()
            link_target.relative_to(Path("/home"))
        except (OSError, ValueError):
            break
        link_root = link_target.parent.parent
        if link_root.is_symlink():
            try:
                link_root = link_root.resolve(strict=True)
            except OSError:
                break
        private_roots.append(link_root)
        link_path = link_target
    if len(command) > 1 and Path(command[1]).is_absolute():
        supervisor = Path(command[1]).resolve(strict=True)
        try:
            supervisor.relative_to(Path("/home"))
        except ValueError:
            pass
        else:
            private_roots.append(supervisor.parent)
    if environment is not None and environment.get("RUSTUP_HOME"):
        rustup_home = Path(environment["RUSTUP_HOME"]).resolve(strict=True)
        try:
            rustup_home.relative_to(Path("/home"))
        except ValueError:
            pass
        else:
            private_roots.append(rustup_home)
    for private_root in dict.fromkeys(private_roots):
        relative_parts = private_root.parts[1:]
        parent = Path("/")
        for part in relative_parts[:-1]:
            parent /= part
            result.extend(("--dir", parent.as_posix()))
        result.extend(("--dir", private_root.as_posix()))
        result.extend(("--ro-bind", private_root.as_posix(), private_root.as_posix()))
    for selection in dict.fromkeys(selected.values() if selected is not None else ()):
        bound_path = selected_path(selection)
        try:
            bound_path.relative_to(Path("/home"))
        except ValueError:
            continue
        parent = Path("/")
        for part in bound_path.parts[1:-1]:
            parent /= part
            result.extend(("--dir", parent.as_posix()))
        result.extend(("--ro-bind", bound_path.as_posix(), bound_path.as_posix()))
    metadata_descriptors: list[int] = []
    checkout_descriptor = -1
    try:
        cargo_cache_descriptors = _open_cargo_cache_descriptors(root, private_home, selected or {})
        for option, path in mounts:
            add_namespace_parent_directories(path)
            result.extend((option, path.as_posix(), path.as_posix()))
        metadata_mounts, metadata_identities, metadata_anchors = _validated_git_metadata_mounts(root, git_executable)
        try:
            metadata_descriptors = _open_anchored_metadata(root, metadata_mounts, metadata_identities, metadata_anchors)
        finally:
            _close_metadata_anchors(metadata_anchors)
        checkout_descriptor = _open_checkout_descriptor(root, checkout_identity)
        metadata_descriptors.append(checkout_descriptor)
        add_namespace_parent_directories(root)
        result.extend(("--bind-fd", str(checkout_descriptor), root.as_posix()))
        for (_source, destination), file_descriptor in zip(metadata_mounts, metadata_descriptors):
            add_namespace_parent_directories(destination)
            result.extend(("--ro-bind-fd", str(file_descriptor), destination.as_posix()))
        if capability_mount is not None:
            result.extend(("--dir", SANDBOX_CAPABILITY_DIR.as_posix()))
            result.extend(("--ro-bind", capability_mount[0].as_posix(), capability_mount[1].as_posix()))
        for destination, descriptor in cargo_cache_descriptors:
            add_namespace_parent_directories(destination)
            result.extend(("--ro-bind-fd", str(descriptor), destination.as_posix()))
        result.extend(("--", *command))
        return SandboxedCommand(result, [*metadata_descriptors, *(descriptor for _destination, descriptor in cargo_cache_descriptors)])
    except BaseException:
        for file_descriptor in metadata_descriptors:
            try:
                os.close(file_descriptor)
            except OSError:
                pass
        for _destination, file_descriptor in cargo_cache_descriptors:
            try:
                os.close(file_descriptor)
            except OSError:
                pass
        raise


def write_fixed_tool_overlay(root: Path, support_bin: Path, fixed_directories: list[Path], selected: Mapping[str, Path | SelectedTool]) -> None:
    """Expose trusted fixed tools without retaining selected-tool fallbacks."""
    seen: set[str] = set(selected)
    for directory in fixed_directories:
        try:
            entries = sorted(directory.iterdir(), key=lambda entry: entry.name)
        except OSError:
            continue
        for entry in entries:
            if entry.name in seen or not entry.name.isprintable() or "/" in entry.name:
                continue
            try:
                if not entry.is_file() or not os.access(entry, os.X_OK) or not secure_path(entry, root):
                    continue
                os.symlink(entry, support_bin / entry.name)
            except OSError:
                continue
            seen.add(entry.name)


def cleanup_private_home(private_home: Path) -> None:
    """Restore cleanup permissions after the sealed gate environment exits."""
    try:
        for current, directories, files in os.walk(private_home, topdown=True, followlinks=False):
            current_path = Path(current)
            current_path.chmod(0o700)
            directories[:] = [name for name in directories if not (current_path / name).is_symlink()]
            for name in files:
                path = current_path / name
                if not path.is_symlink():
                    path.chmod(0o600)
    except OSError:
        pass
    shutil.rmtree(private_home, ignore_errors=True)


def gate_environment(
    root: Path,
    marker_value: str,
    private_home: Path,
    selected: Mapping[str, Path | SelectedTool] | None = None,
) -> dict[str, str]:
    """Return the minimal non-secret environment exposed to a repository gate."""

    selected = selected if selected is not None else selected_tool_executables(root)
    path_entries, cargo_home_source, rustup_home = trusted_tooling(root, selected)
    tool_bin = private_home / "bin"
    bound_bin = private_home / "bound"
    support_bin = private_home / "support"
    tool_bin.mkdir()
    bound_bin.mkdir()
    support_bin.mkdir()
    for name, selection in selected.items():
        executable = selected_path(selection)
        write_tool_identity_launcher(tool_bin, bound_bin, name, executable, selection if isinstance(selection, SelectedTool) else None)
    write_fixed_tool_overlay(root, support_bin, path_entries, selected)
    home_dir = private_home / "home"
    home_dir.mkdir()
    home_dir.chmod(0o700)
    private_tmp = private_home / "tmp"
    private_tmp.mkdir()
    private_tmp.chmod(0o700)
    cargo_home = private_home / "cargo-home"
    cargo_home.mkdir()
    cargo_home.chmod(0o700)
    capability = secrets.token_bytes(32)
    capability_path = SANDBOX_CAPABILITY_DIR / secrets.token_hex(16)
    capability_file = private_home / "capability"
    capability_file.write_bytes(capability)
    capability_file.chmod(0o444)
    private_home.chmod(0o555)
    environment = {
        "HOME": home_dir.as_posix(),
        "LANG": "C",
        "LC_ALL": "C",
        "PATH": os.pathsep.join((tool_bin.as_posix(), support_bin.as_posix())),
        "TMPDIR": private_tmp.as_posix(),
        "PYTHONDONTWRITEBYTECODE": "1",
        "CARGO_HOME": cargo_home.as_posix(),
        "CARGO_NET_OFFLINE": "true",
    }
    if rustup_home is not None:
        environment["RUSTUP_HOME"] = rustup_home.as_posix()
    environment["PYTHONPATH"] = str(root)
    environment[START_FAILURE_MARKER_ENV] = marker_value
    environment[SANDBOX_CAPABILITY_PATH_ENV] = capability_path.as_posix()
    environment[SANDBOX_CAPABILITY_DIGEST_ENV] = hashlib.sha256(capability).hexdigest()
    return environment


def has_authenticated_sandbox_capability() -> bool:
    """Return true only inside the outer runner's private mounted capability."""
    capability_path = Path(os.environ.get(SANDBOX_CAPABILITY_PATH_ENV, ""))
    expected_digest = os.environ.get(SANDBOX_CAPABILITY_DIGEST_ENV, "")
    if (
        capability_path.parent != SANDBOX_CAPABILITY_DIR
        or not SANDBOX_CAPABILITY_NAME.fullmatch(capability_path.name)
        or not re.fullmatch(r"[0-9a-f]{64}", expected_digest)
    ):
        return False
    try:
        observed_digest = sha256_nofollow_file(capability_path)
    except (GateError, OSError):
        return False
    return secrets.compare_digest(observed_digest, expected_digest)


shutdown_requested = False


def request_shutdown(_signum: int, _frame: Any) -> None:
    global shutdown_requested
    shutdown_requested = True


def shutdown_pending() -> bool:
    if shutdown_requested:
        return True
    if hasattr(signal, "sigpending"):
        return any(signum in signal.sigpending() for signum in (signal.SIGINT, signal.SIGTERM))
    return False


def run_gate(root: Path, gate: dict[str, Any]) -> dict[str, Any]:
    command = list(gate["argv"])
    execution_command = command
    if command[0] in {"python", "python3"}:
        # Keep the manifest's canonical argv in evidence, but do not resolve a
        # Python gate through PATH. This preserves a virtualenv launched via a
        # symlink without allowing that directory to shadow other tools.
        execution_command = [sys.executable, *command[1:]]
    result: dict[str, Any] = {
        "argv": redact_argv(command),
        "cwd": gate["cwd"],
        "description": redact(str(gate.get("description", gate["id"]))),
        "evidence_limit": gate["evidence_limit"],
        "id": gate["id"],
        "required": gate["required"],
        "status": "blocked",
    }
    artifact_baselines = {artifact: optional_artifact_digest(root, artifact) for artifact in gate["artifacts"]}
    process: subprocess.Popen[bytes] | None = None
    supervisor_start_time: int | None = None
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

    def terminate_supervisor() -> bool:
        """Terminate the supervisor and wait before inspecting gate state."""
        if process is None:
            return True
        owned_process = process
        cleanup_ok = True
        cleanup_error_seen = False

        def attempt_descendant_cleanup() -> None:
            nonlocal cleanup_ok, cleanup_error_seen
            if cleanup_error_seen:
                return
            # A transient /proc snapshot failure must not turn into a fast
            # supervisor exit that leaves a forked descendant holding the
            # report pipes. Retry once before escalating the owned supervisor;
            # the first failure still keeps the gate blocked.
            for _attempt in range(2):
                try:
                    kill_descendants(owned_process.pid, reap_adopted=False, root_start_time=supervisor_start_time)
                    return
                except CleanupError as exc:
                    cleanup_ok = False
                    cleanup_error_seen = True
                    if not isinstance(exc, ProcessSnapshotError):
                        return

        if supervisor_start_time is not None:
            attempt_descendant_cleanup()
        try:
            if not _signal_supervisor(process, supervisor_start_time, signal.SIGTERM):
                cleanup_ok = False
        except CleanupError:
            cleanup_ok = False
        if process.poll() is None:
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                if supervisor_start_time is not None:
                    attempt_descendant_cleanup()
                try:
                    if not _signal_supervisor(process, supervisor_start_time, signal.SIGKILL):
                        cleanup_ok = False
                except CleanupError:
                    cleanup_ok = False
                if process.poll() is None:
                    try:
                        process.wait(timeout=1)
                    except subprocess.TimeoutExpired:
                        cleanup_ok = False
            except ProcessLookupError:
                pass
        return cleanup_ok and process.returncode != 125

    supervisor_failure_marker = f"{START_FAILURE_MARKER[:-1]} {uuid.uuid4().hex}]"
    private_home: Path | None = None
    cwd_descriptor = -1
    interrupted = False
    try:
        private_home = Path(tempfile.mkdtemp(prefix="voidtower-gate-home-"))
        contained(root, gate["cwd"], must_exist=True)
        cwd_descriptor = open_contained_directory(root, gate["cwd"])
        nested = has_authenticated_sandbox_capability()
        selected = None if nested else selected_tool_executables(root)
        environment = dict(os.environ) if nested else gate_environment(root, supervisor_failure_marker, private_home, selected)
        command = supervised_command(root, execution_command)
        if nested:
            command_line = command
        else:
            command_line = sandboxed_command(root, private_home, command, environment, selected)
        sandbox_pass_fds = getattr(command_line, "pass_fds", ())
        try:
            process = subprocess.Popen(
                command_line,
                cwd=f"/proc/self/fd/{cwd_descriptor}",
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                shell=False,
                start_new_session=True,
                preexec_fn=gate_child_preexec,
                env=environment,
                pass_fds=(cwd_descriptor, *sandbox_pass_fds),
            )
            supervisor_start_time = process_start_time(process.pid)
        finally:
            close_command = getattr(command_line, "close", None)
            if close_command is not None:
                close_command()
        os.close(cwd_descriptor)
        cwd_descriptor = -1
        readers = [
            threading.Thread(target=collect_output, args=("stdout", process.stdout), daemon=True),
            threading.Thread(target=collect_output, args=("stderr", process.stderr), daemon=True),
        ]
        for reader in readers:
            reader.start()
        timed_out = False
        return_code: int | None = None
        try:
            deadline = time.monotonic() + gate["timeout_seconds"]
            while True:
                if shutdown_requested:
                    interrupted = True
                    result["error"] = "runner interrupted during gate execution"
                    break
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    timed_out = True
                    result["error"] = "gate exceeded its bounded timeout"
                    break
                try:
                    return_code = process.wait(timeout=min(0.2, remaining))
                    break
                except subprocess.TimeoutExpired:
                    continue
        except subprocess.TimeoutExpired:
            timed_out = True
            result["error"] = "gate exceeded its bounded timeout"
            return_code = None
        finally:
            if timed_out or interrupted:
                if not terminate_supervisor():
                    result["error"] = "gate process cleanup could not be verified"
            for reader in readers:
                reader.join(timeout=1)
            if any(reader.is_alive() for reader in readers):
                for stream in (process.stdout, process.stderr):
                    if stream is not None:
                        stream.close()
                for reader in readers:
                    reader.join(timeout=1)
            for stream in (process.stdout, process.stderr):
                if stream is not None:
                    stream.close()
        capture_incomplete = any(not state["done"] for state in reader_state.values())
        capture_failed = any(state["error"] is not None for state in reader_state.values())
        if capture_incomplete:
            result["error"] = "gate output capture did not complete"
        elif capture_failed and not result.get("error"):
            result["error"] = "gate output capture failed"
        if interrupted:
            result["status"] = "blocked"
        elif timed_out or capture_incomplete or capture_failed:
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
    except (CleanupError, GateError, OSError, subprocess.SubprocessError) as exc:
        if process is not None:
            if not terminate_supervisor():
                result["error"] = "gate process cleanup could not be verified"
            for stream in (process.stdout, process.stderr):
                if stream is not None:
                    stream.close()
        if "error" not in result:
            result["error"] = f"gate could not start: {type(exc).__name__}"
        result["status"] = "blocked"
    finally:
        if cwd_descriptor >= 0:
            os.close(cwd_descriptor)
    if private_home is not None:
        cleanup_private_home(private_home)
    artifact_records: list[dict[str, str]] = []
    artifact_integrity_failed = False
    for artifact in gate["artifacts"]:
        try:
            digest = hash_contained_file(root, artifact)
            if artifact_baselines[artifact] is not None and digest != artifact_baselines[artifact]:
                if "error" not in result:
                    result["error"] = f"gate changed declared artifact: {artifact}"
                result["status"] = "failed"
                artifact_integrity_failed = True
                continue
            artifact_records.append({"path": artifact, "sha256": digest})
        except GateError as exc:
            if "error" not in result:
                result["error"] = f"artifact could not be hashed safely: {type(exc).__name__}"
            result["status"] = "failed"
            artifact_integrity_failed = True
        except OSError as exc:
            if "error" not in result:
                result["error"] = f"artifact could not be hashed: {type(exc).__name__}"
            result["status"] = "failed"
            artifact_integrity_failed = True
    result["artifacts"] = artifact_records
    if artifact_integrity_failed:
        result["_artifact_integrity_failure"] = True
    return result


def blocked_gate_result(gate: dict[str, Any], error: str) -> dict[str, Any]:
    """Describe a gate that was intentionally not started."""
    return {
        "argv": redact_argv(list(gate["argv"])),
        "artifacts": [],
        "cwd": gate["cwd"],
        "description": redact(str(gate.get("description", gate["id"]))),
        "evidence_limit": gate["evidence_limit"],
        "error": error,
        "id": gate["id"],
        "required": gate["required"],
        "status": "blocked",
        "stderr": "",
        "stdout": "",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--manifest", type=Path, default=Path("scripts/release-gates.json"))
    parser.add_argument("--scope", choices=("changed", "all"), default="changed")
    parser.add_argument("--output", type=Path, help="repository-contained path for the JSON evidence report")
    parser.add_argument("--artifact", action="append", default=[], help="repository-contained artifact to hash into the evidence report")
    parser.add_argument("--require-clean", action="store_true", help="fail before running gates when the checkout has staged or modified paths")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    if args.output is not None and not identity_cleanup_supported():
        print("release_gate: report persistence requires identity-bound Linux cleanup support", file=sys.stderr)
        return 2
    signal.signal(signal.SIGINT, request_shutdown)
    signal.signal(signal.SIGTERM, request_shutdown)
    try:
        root = repository_root(args.repo)
        truth = build_truth(root)
        if truth["check"]["status"] != "passed":
            raise GateError("repository truth check is not green")
        dirty_candidate = bool(args.require_clean and (truth["git"]["staged_paths"] or truth["git"]["modified_paths"]))
        if dirty_candidate and args.output is None:
            print("release_gate: clean checkout required; staged or modified paths are present", file=sys.stderr)
            return 1
        manifest_path = contained(root, args.manifest.as_posix() if args.manifest.is_absolute() else (root / args.manifest).as_posix(), must_exist=True)
        gates, external_gates = parse_manifest(root, manifest_path)
        changed = {entry["name"] for entry in truth["changed_subsystems"]}
        selected = [gate for gate in gates if args.scope == "all" or "governance" in gate["subsystems"] or changed.intersection(gate["subsystems"])]
        if not selected:
            raise GateError("no applicable gates selected")
        explicit_artifact_baselines = {artifact: optional_artifact_digest(root, artifact) for artifact in args.artifact}
        initial_dirty_paths = set(truth["git"]["staged_paths"]) | set(truth["git"]["modified_paths"])
        declared_artifact_paths = {artifact for gate in selected for artifact in gate["artifacts"]}
        initial_dirty_metadata = {
            path: snapshot_path_metadata(root, path)
            for path in initial_dirty_paths
            if path not in declared_artifact_paths
        }
        skipped = [
            {"id": gate["id"], "required": gate["required"], "reason": "subsystem unchanged"}
            for gate in gates
            if gate not in selected
        ]
        results: list[dict[str, Any]] = []
        executed_gate = False
        if dirty_candidate:
            print("release_gate: clean checkout required; staged or modified paths are present", file=sys.stderr)
            results = [blocked_gate_result(gate, "clean checkout required before gate execution") for gate in selected]
        else:
            for index, gate in enumerate(selected):
                if shutdown_requested:
                    results.extend(
                        blocked_gate_result(pending, "runner interrupted before gate execution")
                        for pending in selected[index:]
                    )
                    skipped.extend({"id": pending["id"], "required": pending["required"], "reason": "runner interrupted before execution"} for pending in selected[index:])
                    break
                executed_gate = True
                results.append(run_gate(root, gate))
                if shutdown_requested:
                    skipped.extend({"id": pending["id"], "required": pending["required"], "reason": "runner interrupted before execution"} for pending in selected[index + 1 :])
                    break
        # If shutdown arrived during the initial truth snapshot, no gate had
        # an opportunity to mutate the checkout. Reusing that completed
        # snapshot avoids a second expensive inventory pass while still
        # retaining the exact pre-execution state in the interrupted report.
        after_truth = truth if not executed_gate else build_truth(root)
        if after_truth["check"]["status"] != "passed":
            raise GateError("repository truth changed during gate execution")
        interrupted_after_execution = shutdown_requested
        after_dirty_paths = set(after_truth["git"]["staged_paths"]) | set(after_truth["git"]["modified_paths"])
        gate_created_dirty_paths = after_dirty_paths - initial_dirty_paths
        gate_modified_existing_paths = {
            path
            for path in initial_dirty_metadata
            if path not in after_dirty_paths or snapshot_path_metadata(root, path) != initial_dirty_metadata[path]
        }
        failed_required = any(gate["required"] and gate["status"] != "passed" for gate in results)
        artifact_integrity_failure = any(gate.pop("_artifact_integrity_failure", False) for gate in results)
        skipped_required = any(gate["required"] for gate in skipped)
        # Read the signal flag at the status decision itself, after the
        # post-gate truth snapshot has completed.
        report = {
            "evidence_scope": "release_gate_execution",
            "external_gates": external_gates,
            "git": after_truth["git"],
            "gates": results,
            "manifest": manifest_path.relative_to(root).as_posix(),
            "platform": {"machine": platform.machine(), "system": platform.system(), "release": platform.release()},
            "schema_version": SCHEMA_VERSION,
            "selected_subsystems": sorted({name for gate in selected for name in gate["subsystems"]}),
            "skipped_gates": skipped,
            "status": "failed" if dirty_candidate or shutdown_requested or interrupted_after_execution or failed_required or artifact_integrity_failure or skipped_required or gate_created_dirty_paths or gate_modified_existing_paths else "passed",
        }
        artifact_records: list[dict[str, str]] = []
        for artifact in args.artifact:
            try:
                digest = hash_contained_file(root, artifact)
                if explicit_artifact_baselines[artifact] is not None and digest != explicit_artifact_baselines[artifact]:
                    report["artifact_error"] = f"artifact changed during gate execution: {artifact}"
                    report["status"] = "failed"
                    continue
                artifact_records.append({"path": artifact, "sha256": digest})
            except GateError as exc:
                report["artifact_error"] = f"artifact could not be hashed safely: {type(exc).__name__}"
                report["status"] = "failed"
            except OSError as exc:
                report["artifact_error"] = f"artifact could not be hashed: {type(exc).__name__}"
                report["status"] = "failed"
        report["artifacts"] = artifact_records
        # A signal can arrive while the last artifact is being hashed.  Do
        # not let the status computed before that phase escape as a pass.
        if shutdown_requested:
            report["status"] = "failed"
    except (RepoTruthError, GateError, OSError) as exc:
        print(f"release_gate: {exc}", file=sys.stderr)
        return 2
    try:
        output = serialize_report(report)
    except GateError as exc:
        print(f"release_gate: cannot emit evidence report: {exc}", file=sys.stderr)
        return 2
    persisted_output: Path | None = None
    persisted_identity: tuple[int, ...] | None = None
    persisted_root_identity: tuple[int, int] | None = None
    persisted_parent_identity: tuple[int, int] | None = None
    persisted_ancestor_identities: dict[str, tuple[int, ...]] | None = None
    persisted_descriptor = -1
    if args.output is not None:
        output_path: Path | None = None
        try:
            output_path = new_evidence_output(root, args.output)
            root_metadata = os.stat(root, follow_symlinks=False)
            persisted_root_identity = (root_metadata.st_dev, root_metadata.st_ino)
            expected_ancestor_identities = capture_evidence_ancestor_identities(root, output_path)
            parent_descriptor, filename = open_evidence_parent(
                root,
                output_path,
                expected_root_identity=persisted_root_identity,
                expected_ancestor_identities=expected_ancestor_identities,
            )
            try:
                parent_metadata = os.fstat(parent_descriptor)
                persisted_parent_identity = (parent_metadata.st_dev, parent_metadata.st_ino)
                persisted_ancestor_identities = capture_evidence_ancestor_identities(root, output_path)
                flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
                try:
                    persisted_descriptor = os.open(filename, flags, REPORT_MODE, dir_fd=parent_descriptor)
                except FileExistsError as exc:
                    raise GateError("evidence output already exists") from exc
                persisted_metadata = os.fstat(persisted_descriptor)
                persisted_identity = (
                    persisted_metadata.st_dev,
                    persisted_metadata.st_ino,
                    stat.S_IMODE(persisted_metadata.st_mode),
                )
                if not stat.S_ISREG(persisted_metadata.st_mode):
                    raise GateError("evidence output is not a regular file")
                os.fchmod(persisted_descriptor, REPORT_MODE)
                persisted_metadata = os.fstat(persisted_descriptor)
                if (
                    not report_identity_matches(persisted_metadata, persisted_identity, expected_mode=REPORT_MODE)
                    or persisted_metadata.st_uid != os.getuid()
                    or persisted_metadata.st_gid != os.getgid()
                ):
                    raise GateError("evidence output identity, ownership, or mode is unsafe")
                persisted_identity = (
                    persisted_metadata.st_dev,
                    persisted_metadata.st_ino,
                    REPORT_MODE,
                )
                write_evidence_descriptor(persisted_descriptor, output)
                persisted_identity = report_identity(os.fstat(persisted_descriptor))
                if not report_identity_matches(os.fstat(persisted_descriptor), persisted_identity):
                    raise GateError("evidence output identity, ownership, or mode changed during persistence")
                if not public_output_path_matches(
                    root,
                    output_path,
                    persisted_root_identity,
                    persisted_parent_identity,
                    persisted_identity,
                    persisted_ancestor_identities,
                ):
                    raise GateError("evidence output pathname changed during persistence")
                os.fsync(parent_descriptor)
                if not public_output_path_matches(
                    root,
                    output_path,
                    persisted_root_identity,
                    persisted_parent_identity,
                    persisted_identity,
                    persisted_ancestor_identities,
                ):
                    raise GateError("evidence output pathname changed after persistence")
            finally:
                os.close(parent_descriptor)
            persisted_output = output_path
            os.close(persisted_descriptor)
            persisted_descriptor = -1
            # Re-check after persistence as well.  If the signal arrived
            # during the write, replace our just-created report with an
            # explicit failure before returning to the caller.
            if shutdown_requested:
                report["status"] = "failed"
                output = serialize_report(report)
                if persisted_identity is None:
                    raise GateError("missing persisted report identity")
                persisted_identity = rewrite_persisted_report(
                    root,
                    persisted_output,
                    output,
                    persisted_identity,
                    expected_root_identity=persisted_root_identity,
                    expected_ancestor_identities=persisted_ancestor_identities,
                )
        except (GateError, OSError) as exc:
            if output_path is not None:
                try:
                    if persisted_identity is not None:
                        invalidate_persisted_report(
                            root,
                            output_path,
                            persisted_identity,
                            expected_root_identity=persisted_root_identity,
                            expected_ancestor_identities=persisted_ancestor_identities,
                        )
                    elif persisted_descriptor != -1:
                        invalidate_unbound_report(
                            root,
                            output_path,
                            persisted_descriptor,
                            expected_root_identity=persisted_root_identity,
                            expected_ancestor_identities=persisted_ancestor_identities,
                        )
                except (GateError, OSError) as invalidation_error:
                    print(
                        f"release_gate: cannot invalidate unsafe evidence output: {type(invalidation_error).__name__}",
                        file=sys.stderr,
                    )
            if persisted_descriptor != -1:
                os.close(persisted_descriptor)
            print(f"release_gate: cannot persist evidence safely: {exc}", file=sys.stderr)
            return 2
    # Keep stdout and the process result bound to the post-persistence state.
    # `persisted_output` documents that the failed rewrite above is only ever
    # applied to the file created by this invocation.
    finalization_mask = None
    final_status = 1 if report["status"] == "failed" else 0
    if hasattr(signal, "pthread_sigmask"):
        finalization_mask = signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGINT, signal.SIGTERM})
    try:
        if shutdown_pending():
            report["status"] = "failed"
            output = serialize_report(report)
            if persisted_output is not None:
                try:
                    if persisted_identity is None:
                        raise GateError("missing persisted report identity")
                    persisted_identity = rewrite_persisted_report(
                        root,
                        persisted_output,
                        output,
                        persisted_identity,
                        expected_root_identity=persisted_root_identity,
                        expected_ancestor_identities=persisted_ancestor_identities,
                    )
                except (GateError, OSError) as recovery_error:
                    print(
                        f"release_gate: interrupted report recovery failed closed: {type(recovery_error).__name__}",
                        file=sys.stderr,
                    )
                    return 2
            final_status = 1
        # The final close is the report commit point. Signals stay blocked while
        # the pending set is checked and the descriptor is closed. A signal that
        # becomes pending during that close window is reconciled through a
        # no-follow reopen before stdout or the process result are committed.
        if shutdown_pending():
            report["status"] = "failed"
            output = serialize_report(report)
            if persisted_output is not None:
                try:
                    if persisted_identity is None:
                        raise GateError("missing persisted report identity")
                    persisted_identity = rewrite_persisted_report(
                        root,
                        persisted_output,
                        output,
                        persisted_identity,
                        expected_root_identity=persisted_root_identity,
                        expected_ancestor_identities=persisted_ancestor_identities,
                    )
                except (GateError, OSError) as recovery_error:
                    print(
                        f"release_gate: interrupted report recovery failed closed: {type(recovery_error).__name__}",
                        file=sys.stderr,
                    )
                    return 2
            final_status = 1
        if persisted_descriptor != -1:
            os.close(persisted_descriptor)
            persisted_descriptor = -1
        if shutdown_pending():
            report["status"] = "failed"
            output = serialize_report(report)
            if persisted_output is not None:
                try:
                    if persisted_identity is None:
                        raise GateError("missing persisted report identity")
                    persisted_identity = rewrite_persisted_report(
                        root,
                        persisted_output,
                        output,
                        persisted_identity,
                        expected_root_identity=persisted_root_identity,
                        expected_ancestor_identities=persisted_ancestor_identities,
                    )
                except (GateError, OSError) as recovery_error:
                    print(
                        f"release_gate: interrupted report recovery failed closed: {type(recovery_error).__name__}",
                        file=sys.stderr,
                    )
                    return 2
            final_status = 1
        print(output)
    finally:
        if finalization_mask is not None:
            signal.pthread_sigmask(signal.SIG_SETMASK, finalization_mask)
    return final_status


if __name__ == "__main__":
    raise SystemExit(main())
