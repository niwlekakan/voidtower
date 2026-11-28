#!/usr/bin/env python3
"""Validate release archives and their gate reports before publication."""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
from typing import Any


DIGEST = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
ARCHIVE_PATTERN = "voidtower-*.tar.gz"
REPORT_PATTERN = "release-gate-*.json"
EXTERNAL_REPORT_PATTERN = "release-external-*.json"
ARCHIVE_IDENTITY = re.compile(r"^voidtower-(?P<version>[A-Za-z0-9][A-Za-z0-9._+-]*)-(?P<architecture>[A-Za-z0-9_]+)-unknown-linux-musl\.tar\.gz$")
SUPPORTED_ARCHITECTURES = ("x86_64", "aarch64")
MAX_COMMAND_ARGUMENTS = 64
MAX_METADATA_TEXT = 512
MAX_EVIDENCE_BYTES = 64 * 1024
MAX_REPORT_BYTES = 4 * 1024 * 1024
MAX_OBSERVED_BYTES = 16 * 1024
MAX_GATE_OUTPUT_BYTES = 16 * 1024 + 64
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
EVIDENCE_LIMITS = {"implemented", "unit-verified", "integration-verified", "runtime-verified", "release-qualified"}
EXTERNAL_REPORT_KEYS = {
    "artifact",
    "artifact_sha256",
    "candidate_commit",
    "command",
    "cwd",
    "evidence_file",
    "evidence_scope",
    "exit_code",
    "gate_id",
    "platform",
    "schema_version",
    "status",
    "workflow_job",
}
SHELL_INTERPRETERS = {"sh", "bash", "dash", "zsh", "fish", "csh", "ksh", "cmd", "cmd.exe", "powershell", "pwsh"}
TRUSTED_EXECUTABLES = {"python", "python3", "cargo", "npm", "git"}
TRUSTED_REPOSITORY_SCRIPTS = {"scripts/check-repository-hygiene.sh", "scripts/check-schema-migration-ownership.sh"}
REQUIRED_REPORT_KEYS = {
    "artifacts",
    "evidence_scope",
    "external_gates",
    "gates",
    "git",
    "manifest",
    "platform",
    "schema_version",
    "selected_subsystems",
    "skipped_gates",
    "status",
}
REPORT_KEYS = REQUIRED_REPORT_KEYS | {"artifact_error"}
REPORT_GIT_KEYS = {"ahead", "behind", "branch", "head", "impact_paths", "modified_paths", "path_semantics", "staged_paths"}
REPORT_PATH_SEMANTICS_KEYS = {"impact_paths", "modified_paths", "staged_paths"}
REPORT_PLATFORM_KEYS = {"machine", "system", "release"}
REPORT_GATE_KEYS = {"argv", "artifacts", "cwd", "description", "error", "evidence_limit", "exit_code", "id", "required", "status", "stderr", "stdout"}
REPORT_ARTIFACT_KEYS = {"path", "sha256"}
MANIFEST_KEYS = {"schema_version", "external_gates", "gates"}
MANIFEST_GATE_KEYS = {"argv", "artifacts", "cwd", "description", "evidence_limit", "id", "required", "subsystems", "timeout_seconds"}
EXTERNAL_DESCRIPTOR_KEYS = {"id", "reason", "required", "evidence_file", "workflow_job", "command", "cwd"}
_ACTIVE_VALIDATED_DESCRIPTORS: list[int] | None = None


class EvidenceError(ValueError):
    """A downloaded release evidence set is not safe or internally consistent."""


def reject_unknown_fields(entry: dict[str, Any], allowed: set[str], context: str) -> None:
    unknown_fields = set(entry) - allowed
    if unknown_fields:
        raise EvidenceError(f"{context} contains unknown fields: {', '.join(sorted(unknown_fields))}")


def validate_string(value: object, context: str, *, maximum: int = MAX_METADATA_TEXT, allow_empty: bool = False) -> None:
    if not isinstance(value, str) or (not allow_empty and not value) or len(value) > maximum or not value.isprintable():
        raise EvidenceError(f"{context} is invalid")


def validate_string_list(value: object, context: str, *, maximum: int = MAX_METADATA_TEXT) -> None:
    if not isinstance(value, list) or any(not isinstance(item, str) or not item or len(item) > maximum or not item.isprintable() for item in value):
        raise EvidenceError(f"{context} is invalid")


def validate_relative_path(value: object, context: str) -> None:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > 4096
        or not value.isprintable()
        or "\x00" in value
        or "\\" in value
        or re.match(r"^[A-Za-z]:", value)
        or Path(value).is_absolute()
        or ".." in Path(value).parts
    ):
        raise EvidenceError(f"{context} is invalid")


def archive_identity(name: str) -> tuple[str, str]:
    match = ARCHIVE_IDENTITY.fullmatch(name)
    if match is None:
        raise EvidenceError(f"release archive name is unsupported: {name}")
    return match["version"], match["architecture"]


def validate_executable_command(argv: object, context: str, repository_root: Path) -> tuple[list[str], list[str]]:
    if not isinstance(argv, list) or not argv or len(argv) > MAX_COMMAND_ARGUMENTS or any(not isinstance(argument, str) or not argument or len(argument) > 4096 or not argument.isprintable() for argument in argv):
        raise EvidenceError(f"release gate manifest has malformed executable command metadata: {context}")
    executable_path = Path(argv[0])
    if executable_path.is_absolute() or re.match(r"^[A-Za-z]:", argv[0]):
        raise EvidenceError(f"release gate executable must be repository-relative or a trusted tool name: {context}")
    executable_as_posix = executable_path.as_posix()
    has_path_separator = executable_as_posix != executable_path.name
    if has_path_separator and executable_as_posix not in TRUSTED_REPOSITORY_SCRIPTS:
        raise EvidenceError(f"release gate executable is not allowlisted: {context}")
    executable = executable_path.name
    if executable.lower() in SHELL_INTERPRETERS or (not has_path_separator and executable not in TRUSTED_EXECUTABLES):
        raise EvidenceError(f"release gate executable may not use a shell interpreter or non-allowlisted executable: {context}")
    report_argv = list(argv)
    if has_path_separator:
        script_path = repository_root / executable_as_posix
        try:
            resolved_script = script_path.resolve(strict=True)
            resolved_script.relative_to(repository_root)
        except (OSError, ValueError) as exc:
            raise EvidenceError(f"release gate executable must resolve inside the repository: {context}") from exc
        if resolved_script != script_path.absolute() or not resolved_script.is_file() or not os.access(resolved_script, os.X_OK):
            raise EvidenceError(f"release gate executable must be a regular executable without symlink traversal: {context}")
        # Reports use the manifest's repository-relative spelling.  The
        # resolved path above is only an execution-safety check; emitting it
        # would make validator metadata differ from the runner's report.
        report_argv[0] = executable_as_posix
    for argument in argv[1:]:
        if "\x00" in argument:
            raise EvidenceError(f"release gate command contains a NUL character: {context}")
        candidates = [argument]
        if "=" in argument:
            candidates.append(argument.split("=", 1)[1])
        for candidate in candidates:
            if not candidate:
                continue
            candidate_path = Path(candidate)
            if candidate_path.is_absolute() or re.match(r"^[A-Za-z]:", candidate) or candidate.startswith(("\\\\", "/")) or "\\" in candidate or ".." in candidate_path.parts:
                raise EvidenceError(f"release gate command contains a path outside the repository: {context}")
            if candidate_path.exists() or candidate_path.is_symlink() or "/" in candidate or candidate.startswith(".") or candidate.endswith((".py", ".sh", ".json", ".toml")):
                try:
                    (repository_root / candidate).resolve(strict=False).relative_to(repository_root)
                except ValueError as exc:
                    raise EvidenceError(f"release gate command contains a path outside the repository: {context}") from exc
    if executable in {"python", "python3"} and any(item in {"-c", "-m"} for item in argv[1:]):
        if "-m" not in argv[1:] or argv[1:3] != ["-m", "unittest"]:
            raise EvidenceError(f"release gate Python command may only invoke unittest: {context}")
    return list(argv), report_argv


def open_text_bounded(path: Path, limit: int) -> tuple[int, bytes]:
    if os.name != "posix" or not all(hasattr(os, flag) for flag in ("O_NOFOLLOW", "O_DIRECTORY", "O_NONBLOCK")):
        raise EvidenceError("safe evidence-file reading is unavailable on this platform")
    parent_descriptor = -1
    file_descriptor = -1
    try:
        parent_flags = os.O_RDONLY | os.O_DIRECTORY
        # The validator's root is a retained /proc/self/fd directory identity;
        # O_NOFOLLOW would reject that trusted descriptor path itself.
        if not (path.parent.parts[:4] == (os.sep, "proc", "self", "fd") and len(path.parent.parts) == 5 and path.parent.parts[4].isdigit()):
            parent_flags |= os.O_NOFOLLOW
        parent_descriptor = os.open(path.parent, parent_flags)
        file_descriptor = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent_descriptor)
        metadata = os.fstat(file_descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise EvidenceError(f"evidence file is not a regular file: {path}")
        if metadata.st_size > limit:
            raise EvidenceError(f"evidence file is oversized: {path}")
        content = os.read(file_descriptor, limit + 1)
        if len(content) > limit:
            raise EvidenceError(f"evidence file is oversized: {path}")
        retained_descriptor = file_descriptor
        file_descriptor = -1
        if _ACTIVE_VALIDATED_DESCRIPTORS is not None:
            _ACTIVE_VALIDATED_DESCRIPTORS.append(retained_descriptor)
        return retained_descriptor, content
    except EvidenceError:
        raise
    except (OSError, TypeError, NotImplementedError) as exc:
        raise EvidenceError(f"cannot read evidence file {path}: {type(exc).__name__}") from exc
    finally:
        if file_descriptor >= 0:
            os.close(file_descriptor)
        if parent_descriptor >= 0:
            os.close(parent_descriptor)


def read_text_bounded(path: Path, limit: int) -> str:
    descriptor, content = open_text_bounded(path, limit)
    try:
        return content.decode("utf-8")
    except UnicodeError as exc:
        raise EvidenceError(f"cannot read evidence file {path}: UnicodeDecodeError") from exc
    finally:
        if _ACTIVE_VALIDATED_DESCRIPTORS is not None and descriptor in _ACTIVE_VALIDATED_DESCRIPTORS:
            _ACTIVE_VALIDATED_DESCRIPTORS.remove(descriptor)
        os.close(descriptor)


def validate_external_descriptor(entry: object, context: str) -> dict[str, Any]:
    if not isinstance(entry, dict):
        raise EvidenceError(f"release gate external descriptor is malformed: {context}")
    reject_unknown_fields(entry, EXTERNAL_DESCRIPTOR_KEYS, f"release gate external descriptor {context}")
    gate_id = entry.get("id")
    reason = entry.get("reason")
    required = entry.get("required")
    evidence_file = entry.get("evidence_file")
    workflow_job = entry.get("workflow_job")
    command = entry.get("command")
    cwd = entry.get("cwd")
    if (
        not isinstance(gate_id, str)
        or not gate_id
        or len(gate_id) > 128
        or not gate_id.isprintable()
        or not isinstance(reason, str)
        or not reason
        or len(reason) > 512
        or not reason.isprintable()
        or not isinstance(required, bool)
        or not required
        or not isinstance(evidence_file, str)
        or not evidence_file
        or evidence_file != f"release-external-{gate_id}.json"
        or Path(evidence_file).parts != (evidence_file,)
        or "\\" in evidence_file
        or not isinstance(workflow_job, str)
        or not workflow_job
        or len(workflow_job) > 128
        or not workflow_job.isprintable()
        or not isinstance(command, list)
        or not command
        or len(command) > MAX_COMMAND_ARGUMENTS
        or any(not isinstance(argument, str) or not argument or len(argument) > 4096 or not argument.isprintable() for argument in command)
        or not isinstance(cwd, str)
        or not cwd
        or len(cwd) > 256
        or not cwd.isprintable()
        or "\\" in cwd
        or re.match(r"^[A-Za-z]:", cwd)
        or Path(cwd).is_absolute()
        or ".." in Path(cwd).parts
    ):
        raise EvidenceError(f"release gate external descriptor is malformed: {context}")
    return {
        "id": gate_id,
        "reason": reason,
        "required": required,
        "evidence_file": evidence_file,
        "workflow_job": workflow_job,
        "command": command,
        "cwd": cwd,
    }


def files_named(root: Path, pattern: str) -> list[Path]:
    for path in root.rglob("*"):
        if path.is_symlink():
            raise EvidenceError(f"symlinked evidence path is not allowed: {path}")
    matches: list[Path] = []
    for path in root.rglob(pattern):
        if path.is_file():
            matches.append(path)
    return sorted(matches, key=lambda path: path.relative_to(root).as_posix())


def _replace_with_sealed_snapshot(
    directory_descriptor: int,
    name: str,
    validated_descriptor: int,
    validated_bytes: bytes,
) -> None:
    """Replace a validated evidence file so pre-open writers retain only the old inode."""
    snapshot_descriptor = -1
    temporary_name = f".voidtower-sealed-{os.getpid()}-{name}"
    linkat = None
    try:
        source_metadata = os.fstat(validated_descriptor)
        path_metadata = os.stat(name, dir_fd=directory_descriptor, follow_symlinks=False)
        if not stat.S_ISREG(source_metadata.st_mode) or not stat.S_ISREG(path_metadata.st_mode):
            raise EvidenceError(f"evidence path is not a regular file: {name}")
        if (source_metadata.st_dev, source_metadata.st_ino) != (path_metadata.st_dev, path_metadata.st_ino):
            raise EvidenceError(f"evidence path changed before sealing: {name}")
        if len(validated_bytes) > MAX_REPORT_BYTES:
            raise EvidenceError(f"evidence file is oversized: {name}")
        snapshot_descriptor = os.open(
            ".",
            os.O_RDWR | os.O_TMPFILE,
            stat.S_IRUSR | stat.S_IWUSR,
            dir_fd=directory_descriptor,
        )
        offset = 0
        while offset < len(validated_bytes):
            chunk = validated_bytes[offset:offset + 1024 * 1024]
            written = 0
            while written < len(chunk):
                count = os.pwrite(snapshot_descriptor, chunk[written:], offset + written)
                if count <= 0:
                    raise EvidenceError(f"cannot snapshot evidence file: {name}")
                written += count
            offset += len(chunk)
        os.fsync(snapshot_descriptor)
        os.fchmod(snapshot_descriptor, stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        libc = ctypes.CDLL(None, use_errno=True)
        linkat = libc.linkat
        linkat.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
        linkat.restype = ctypes.c_int
        if linkat(snapshot_descriptor, b"", directory_descriptor, os.fsencode(temporary_name), 0x1000) != 0:
            error = ctypes.get_errno()
            raise EvidenceError(f"cannot retain sealed evidence file: {name}: {os.strerror(error)}")
        os.rename(temporary_name, name, src_dir_fd=directory_descriptor, dst_dir_fd=directory_descriptor)
        retained_descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory_descriptor)
        try:
            retained_metadata = os.fstat(retained_descriptor)
            if (
                (retained_metadata.st_dev, retained_metadata.st_ino) != (os.fstat(snapshot_descriptor).st_dev, os.fstat(snapshot_descriptor).st_ino)
                or retained_metadata.st_mode & 0o222
            ):
                raise EvidenceError(f"sealed evidence file identity changed: {name}")
        finally:
            os.close(retained_descriptor)
    except EvidenceError:
        raise
    except (OSError, TypeError) as exc:
        raise EvidenceError(f"cannot seal evidence file {name}: {type(exc).__name__}") from exc
    finally:
        if linkat is not None:
            try:
                os.unlink(temporary_name, dir_fd=directory_descriptor)
            except FileNotFoundError:
                pass
        if snapshot_descriptor >= 0:
            os.close(snapshot_descriptor)



def seal_evidence_tree(
    root: Path,
    root_descriptor: int,
    validated_files: dict[str, tuple[int, bytes]],
) -> None:
    """Make validated evidence pathnames immutable before downstream upload."""
    if not hasattr(os, "fchmod") or not hasattr(os, "O_TMPFILE"):
        raise EvidenceError("descriptor-bound evidence sealing is unavailable on this platform")

    def seal_directory(directory_descriptor: int, display_path: str) -> None:
        try:
            names = os.listdir(directory_descriptor)
            for name in names:
                metadata = os.stat(name, dir_fd=directory_descriptor, follow_symlinks=False)
                if stat.S_ISLNK(metadata.st_mode):
                    raise EvidenceError(f"symlinked evidence path is not allowed: {display_path}/{name}")
                if stat.S_ISDIR(metadata.st_mode):
                    child_descriptor = os.open(
                        name,
                        os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                        dir_fd=directory_descriptor,
                    )
                    try:
                        seal_directory(child_descriptor, f"{display_path}/{name}")
                    finally:
                        os.close(child_descriptor)
                    continue
                if not stat.S_ISREG(metadata.st_mode):
                    raise EvidenceError(f"evidence path is not a regular file: {display_path}/{name}")
                if name in validated_files:
                    validated_descriptor, validated_bytes = validated_files[name]
                    _replace_with_sealed_snapshot(directory_descriptor, name, validated_descriptor, validated_bytes)
                    continue
                descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory_descriptor)
                try:
                    opened = os.fstat(descriptor)
                    if (opened.st_dev, opened.st_ino) != (metadata.st_dev, metadata.st_ino) or not stat.S_ISREG(opened.st_mode):
                        raise EvidenceError(f"evidence path changed during sealing: {display_path}/{name}")
                    os.fchmod(descriptor, stat.S_IMODE(opened.st_mode) & ~0o222)
                finally:
                    os.close(descriptor)
            os.fchmod(directory_descriptor, stat.S_IMODE(os.fstat(directory_descriptor).st_mode) & ~0o222)
        except EvidenceError:
            raise
        except (OSError, TypeError) as exc:
            raise EvidenceError(f"cannot seal evidence tree: {type(exc).__name__}") from exc

    try:
        seal_directory(root_descriptor, str(root))
    except EvidenceError:
        raise
    except (OSError, TypeError) as exc:
        raise EvidenceError(f"cannot seal evidence tree: {type(exc).__name__}") from exc


def open_regular_archive(path: Path) -> int:
    if os.name != "posix" or not hasattr(os, "O_NOFOLLOW") or not hasattr(os, "O_DIRECTORY"):
        raise EvidenceError("safe release archive hashing is unavailable on this platform")
    parent_descriptor = -1
    file_descriptor = -1
    try:
        parent_flags = os.O_RDONLY | os.O_DIRECTORY
        if not (path.parent.parts[:4] == (os.sep, "proc", "self", "fd") and len(path.parent.parts) == 5 and path.parent.parts[4].isdigit()):
            parent_flags |= os.O_NOFOLLOW
        parent_descriptor = os.open(path.parent, parent_flags)
        file_descriptor = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=parent_descriptor)
        metadata = os.fstat(file_descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise EvidenceError(f"release archive is not a regular file: {path}")
        if metadata.st_size > MAX_ARCHIVE_BYTES:
            raise EvidenceError(f"release archive exceeds the bounded size: {path}")
        retained_descriptor = file_descriptor
        file_descriptor = -1
        return retained_descriptor
    except OSError as exc:
        raise EvidenceError(f"cannot open release archive {path}: {type(exc).__name__}") from exc
    finally:
        if file_descriptor >= 0:
            os.close(file_descriptor)
        if parent_descriptor >= 0:
            os.close(parent_descriptor)


def sha256_descriptor(file_descriptor: int, maximum_bytes: int = MAX_ARCHIVE_BYTES) -> str:
    digest = hashlib.sha256()
    offset = 0
    while True:
        chunk = os.pread(file_descriptor, 1024 * 1024, offset)
        if not chunk:
            return digest.hexdigest()
        if offset + len(chunk) > maximum_bytes:
            raise EvidenceError("release archive exceeds the bounded size")
        digest.update(chunk)
        offset += len(chunk)


def sha256(path: Path) -> str:
    file_descriptor = open_regular_archive(path)
    try:
        return sha256_descriptor(file_descriptor)
    except OSError as exc:
        raise EvidenceError(f"cannot hash release archive {path}: {type(exc).__name__}") from exc
    finally:
        os.close(file_descriptor)


def revalidate_archive_descriptors(archive_descriptors: dict[str, int], archive_digests: dict[str, str]) -> None:
    """Confirm retained archive contents did not change during validation."""
    for name, descriptor in sorted(archive_descriptors.items()):
        try:
            digest = sha256_descriptor(descriptor)
        except OSError as exc:
            raise EvidenceError(f"cannot revalidate release archive {name}: {type(exc).__name__}") from exc
        if digest != archive_digests[name]:
            raise EvidenceError(f"release archive changed during validation: {name}")


def revalidate_evidence_descriptors(
    validated_files: dict[str, tuple[int, bytes]],
    file_limits: dict[str, int],
    root_descriptor: int,
) -> None:
    """Confirm retained report/evidence bytes did not change during validation."""
    for name, (descriptor, expected_bytes) in sorted(validated_files.items()):
        limit = file_limits[name]
        try:
            actual_bytes = os.pread(descriptor, limit + 1, 0)
        except OSError as exc:
            raise EvidenceError(f"cannot revalidate release evidence {name}: {type(exc).__name__}") from exc
        if len(actual_bytes) > limit or actual_bytes != expected_bytes:
            raise EvidenceError(f"release evidence changed during validation: {name}")
        pathname_descriptor = -1
        try:
            pathname_descriptor = os.open(name, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=root_descriptor)
            pathname_metadata = os.fstat(pathname_descriptor)
            retained_metadata = os.fstat(descriptor)
            if (
                not stat.S_ISREG(pathname_metadata.st_mode)
                or (pathname_metadata.st_dev, pathname_metadata.st_ino)
                != (retained_metadata.st_dev, retained_metadata.st_ino)
            ):
                raise EvidenceError(f"release evidence pathname changed during validation: {name}")
        except OSError as exc:
            raise EvidenceError(f"cannot revalidate release evidence pathname {name}: {type(exc).__name__}") from exc
        finally:
            if pathname_descriptor >= 0:
                os.close(pathname_descriptor)


def publish_archive_links(
    archives: dict[str, int],
    archive_digests: dict[str, str],
    publication_root: Path,
    checkout: Path,
    checksums_output: Path,
    validated_files: dict[str, tuple[int, bytes]] | None = None,
    expected_checkout_identity: tuple[int, int] | None = None,
) -> dict[str, Path]:
    """Publish immutable archive snapshots through descriptor-bound links."""
    if (
        os.name != "posix"
        or not hasattr(os, "O_DIRECTORY")
        or not hasattr(os, "O_NOFOLLOW")
        or not hasattr(os, "O_TMPFILE")
        or not hasattr(os, "pwrite")
    ):
        raise EvidenceError("descriptor-bound release publication is unavailable on this platform")
    if checksums_output is None:
        raise EvidenceError("publication requires a checksum output to seal the pathname handoff")
    try:
        relative_root = publication_root.relative_to(checkout)
    except ValueError as exc:
        raise EvidenceError("publication root must be contained by the checkout") from exc
    if len(relative_root.parts) != 1 or any(part in {".", ".."} for part in relative_root.parts):
        raise EvidenceError("publication root must be a direct, symlink-free checkout child")
    checkout_descriptor = -1
    checkout_parent_descriptor = -1
    parent_descriptor = -1
    publication_descriptor = -1
    checksum_descriptor = -1
    checksum_identity: tuple[int, int, int] | None = None
    snapshot_descriptors: dict[str, int] = {}
    validated_snapshot_descriptors: dict[str, int] = {}
    try:
        directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
        checkout_parent_descriptor = os.open(checkout.parent, directory_flags)
        parent_mode = stat.S_IMODE(os.fstat(checkout_parent_descriptor).st_mode)
        os.fchmod(checkout_parent_descriptor, parent_mode & ~0o222)
        checkout_descriptor = os.open(checkout, directory_flags)
        checkout_metadata = os.fstat(checkout_descriptor)
        if expected_checkout_identity is not None and (checkout_metadata.st_dev, checkout_metadata.st_ino) != expected_checkout_identity:
            raise EvidenceError("checkout pathname identity changed before publication")
        named_checkout_metadata = os.stat(checkout.name, dir_fd=checkout_parent_descriptor, follow_symlinks=False)
        if (checkout_metadata.st_dev, checkout_metadata.st_ino) != (named_checkout_metadata.st_dev, named_checkout_metadata.st_ino):
            raise EvidenceError("checkout pathname identity changed before publication")
        named_checkout_metadata = os.stat(checkout.name, dir_fd=checkout_parent_descriptor, follow_symlinks=False)
        if (checkout_metadata.st_dev, checkout_metadata.st_ino) != (named_checkout_metadata.st_dev, named_checkout_metadata.st_ino):
            raise EvidenceError("checkout pathname identity changed while sealing its parent")
        parent_descriptor = os.dup(checkout_descriptor)
        libc = ctypes.CDLL(None, use_errno=True)
        linkat = libc.linkat
        linkat.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_int]
        linkat.restype = ctypes.c_int
        checksums_relative = checksums_output.relative_to(checkout)
        if len(checksums_relative.parts) != 1 or checksums_relative.parts[0] in {".", ".."}:
            raise EvidenceError("checksum output must be a direct checkout file")
        checksum_bytes = "".join(
            f"{archive_digests[name]}  {name}\n" for name in sorted(archive_digests)
        ).encode("ascii")
        checksum_descriptor = os.open(
            ".",
            os.O_RDWR | os.O_TMPFILE,
            stat.S_IRUSR | stat.S_IWUSR,
            dir_fd=checkout_descriptor,
        )
        written = 0
        while written < len(checksum_bytes):
            count = os.write(checksum_descriptor, checksum_bytes[written:])
            if count <= 0:
                raise EvidenceError("cannot write release checksum manifest")
            written += count
        os.fsync(checksum_descriptor)
        os.fchmod(checksum_descriptor, stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
        checksum_metadata = os.fstat(checksum_descriptor)
        checksum_identity = (checksum_metadata.st_dev, checksum_metadata.st_ino, checksum_metadata.st_size)
        if linkat(checksum_descriptor, b"", checkout_descriptor, os.fsencode(checksums_relative.parts[0]), 0x1000) != 0:
            error = ctypes.get_errno()
            raise EvidenceError(f"cannot retain release checksum identity: {os.strerror(error)}")
        os.close(checksum_descriptor)
        checksum_descriptor = -1
        for component in relative_root.parts[:-1]:
            next_descriptor = os.open(component, directory_flags, dir_fd=parent_descriptor)
            os.close(parent_descriptor)
            parent_descriptor = next_descriptor
        try:
            os.mkdir(relative_root.parts[-1], 0o700, dir_fd=parent_descriptor)
        except FileExistsError as exc:
            raise EvidenceError(f"publication root already exists: {publication_root}") from exc
        publication_descriptor = os.open(
            relative_root.parts[-1],
            directory_flags,
            dir_fd=parent_descriptor,
        )
        # Seal the checkout immediately after creating the direct publication
        # child. No later pathname can be renamed or replaced by a same-user
        # writer before downstream steps consume the handoff.
        checkout_mode = stat.S_IMODE(os.fstat(checkout_descriptor).st_mode)
        os.fchmod(checkout_descriptor, checkout_mode & ~0o222)
        named_publication_metadata = os.stat(
            relative_root.parts[0],
            dir_fd=checkout_descriptor,
            follow_symlinks=False,
        )
        publication_metadata = os.fstat(publication_descriptor)
        if (
            not stat.S_ISDIR(named_publication_metadata.st_mode)
            or (named_publication_metadata.st_dev, named_publication_metadata.st_ino)
            != (publication_metadata.st_dev, publication_metadata.st_ino)
        ):
            raise EvidenceError("publication root pathname identity changed while sealing checkout")
        for name, source_descriptor in sorted(archives.items()):
            snapshot_descriptor = os.open(
                ".",
                os.O_RDWR | os.O_TMPFILE,
                stat.S_IRUSR | stat.S_IWUSR,
                dir_fd=publication_descriptor,
            )
            snapshot_descriptors[name] = snapshot_descriptor
            offset = 0
            while True:
                if offset >= MAX_ARCHIVE_BYTES:
                    raise EvidenceError(f"release archive exceeds the bounded size: {name}")
                chunk = os.pread(source_descriptor, 1024 * 1024, offset)
                if not chunk:
                    break
                if offset + len(chunk) > MAX_ARCHIVE_BYTES:
                    raise EvidenceError(f"release archive exceeds the bounded size: {name}")
                written = 0
                while written < len(chunk):
                    count = os.pwrite(snapshot_descriptor, chunk[written:], offset + written)
                    if count <= 0:
                        raise EvidenceError(f"cannot snapshot release archive: {name}")
                    written += count
                offset += len(chunk)
            os.fsync(snapshot_descriptor)
            os.fchmod(snapshot_descriptor, stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
            if sha256_descriptor(snapshot_descriptor) != archive_digests[name]:
                raise EvidenceError(f"release archive changed while publishing: {name}")
        for name, snapshot_descriptor in sorted(snapshot_descriptors.items()):
            if linkat(snapshot_descriptor, b"", publication_descriptor, os.fsencode(name), 0x1000) != 0:
                error = ctypes.get_errno()
                raise EvidenceError(f"cannot retain release archive identity: {name}: {os.strerror(error)}")
        for name, (_, content) in sorted((validated_files or {}).items()):
            if Path(name).parts != (name,) or name in snapshot_descriptors:
                raise EvidenceError(f"validated evidence filename is unsafe: {name}")
            snapshot_descriptor = os.open(
                ".",
                os.O_RDWR | os.O_TMPFILE,
                stat.S_IRUSR | stat.S_IWUSR,
                dir_fd=publication_descriptor,
            )
            validated_snapshot_descriptors[name] = snapshot_descriptor
            offset = 0
            while offset < len(content):
                chunk = content[offset:offset + 1024 * 1024]
                written = 0
                while written < len(chunk):
                    count = os.pwrite(snapshot_descriptor, chunk[written:], offset + written)
                    if count <= 0:
                        raise EvidenceError(f"cannot snapshot release evidence: {name}")
                    written += count
                offset += len(chunk)
            os.fsync(snapshot_descriptor)
            os.fchmod(snapshot_descriptor, stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)
            if sha256_descriptor(snapshot_descriptor, maximum_bytes=MAX_REPORT_BYTES) != hashlib.sha256(content).hexdigest():
                raise EvidenceError(f"release evidence changed while publishing: {name}")
            if linkat(snapshot_descriptor, b"", publication_descriptor, os.fsencode(name), 0x1000) != 0:
                error = ctypes.get_errno()
                raise EvidenceError(f"cannot retain release evidence identity: {name}: {os.strerror(error)}")
        # Seal the directory before opening any destination pathname. A
        # concurrent writer can no longer unlink and replace a link between
        # creation and the descriptor/inode check below.
        os.fchmod(publication_descriptor, stat.S_IRUSR | stat.S_IXUSR)
        for name, snapshot_descriptor in sorted(snapshot_descriptors.items()):
            published_descriptor = -1
            try:
                published_descriptor = os.open(
                    name,
                    os.O_RDONLY | os.O_NOFOLLOW,
                    dir_fd=publication_descriptor,
                )
                published_metadata = os.fstat(published_descriptor)
                if not stat.S_ISREG(published_metadata.st_mode):
                    raise EvidenceError(f"published release archive is not a regular file: {name}")
                snapshot_metadata = os.fstat(snapshot_descriptor)
                if (published_metadata.st_dev, published_metadata.st_ino) != (snapshot_metadata.st_dev, snapshot_metadata.st_ino):
                    raise EvidenceError(f"published release archive identity changed during retention: {name}")
                if published_metadata.st_mode & 0o222:
                    raise EvidenceError(f"published release archive is writable: {name}")
                if sha256_descriptor(published_descriptor) != archive_digests[name]:
                    raise EvidenceError(f"published release archive changed during retention: {name}")
            finally:
                if published_descriptor >= 0:
                    os.close(published_descriptor)
        for name, snapshot_descriptor in sorted(validated_snapshot_descriptors.items()):
            published_descriptor = -1
            try:
                published_descriptor = os.open(
                    name,
                    os.O_RDONLY | os.O_NOFOLLOW,
                    dir_fd=publication_descriptor,
                )
                published_metadata = os.fstat(published_descriptor)
                snapshot_metadata = os.fstat(snapshot_descriptor)
                if not stat.S_ISREG(published_metadata.st_mode) or (published_metadata.st_dev, published_metadata.st_ino) != (snapshot_metadata.st_dev, snapshot_metadata.st_ino):
                    raise EvidenceError(f"published release evidence identity changed during retention: {name}")
                if published_metadata.st_mode & 0o222:
                    raise EvidenceError(f"published release evidence is writable: {name}")
                expected_digest = hashlib.sha256((validated_files or {})[name][1]).hexdigest()
                if sha256_descriptor(published_descriptor, maximum_bytes=MAX_REPORT_BYTES) != expected_digest:
                    raise EvidenceError(f"published release evidence changed during retention: {name}")
            finally:
                if published_descriptor >= 0:
                    os.close(published_descriptor)
        published_checksum_descriptor = os.open(
            checksums_relative.parts[0],
            os.O_RDONLY | os.O_NOFOLLOW,
            dir_fd=checkout_descriptor,
        )
        try:
            checksum_metadata = os.fstat(published_checksum_descriptor)
            if checksum_identity != (checksum_metadata.st_dev, checksum_metadata.st_ino, checksum_metadata.st_size):
                raise EvidenceError("release checksum manifest identity changed during publication")
        finally:
            os.close(published_checksum_descriptor)
        return {name: publication_root / name for name in archives}
    except OSError as exc:
        raise EvidenceError(f"cannot create descriptor-bound publication root: {type(exc).__name__}") from exc
    finally:
        for snapshot_descriptor in snapshot_descriptors.values():
            os.close(snapshot_descriptor)
        for snapshot_descriptor in validated_snapshot_descriptors.values():
            os.close(snapshot_descriptor)
        if publication_descriptor >= 0:
            os.close(publication_descriptor)
        if checkout_descriptor >= 0:
            os.close(checkout_descriptor)
        if checksum_descriptor >= 0:
            os.close(checksum_descriptor)
        if checkout_parent_descriptor >= 0:
            os.close(checkout_parent_descriptor)
        if parent_descriptor >= 0:
            os.close(parent_descriptor)


def _load_report_unmanaged(path: Path, expected_commit: str) -> tuple[dict[str, Any], dict[str, Any], int, bytes]:
    validated_descriptor = -1
    try:
        validated_descriptor, raw = open_text_bounded(path, MAX_REPORT_BYTES)
        payload = json.loads(raw.decode("utf-8"))
    except (EvidenceError, UnicodeError, json.JSONDecodeError) as exc:
        if isinstance(exc, EvidenceError):
            raise
        raise EvidenceError(f"cannot read release gate report {path}: {type(exc).__name__}") from exc
    if not isinstance(payload, dict):
        raise EvidenceError(f"release gate report is not an object: {path}")
    reject_unknown_fields(payload, REPORT_KEYS, f"release gate report {path}")
    if not REQUIRED_REPORT_KEYS.issubset(payload):
        raise EvidenceError(f"release gate report is structurally incomplete: {path}")
    if payload.get("evidence_scope") != "release_gate_execution" or payload.get("manifest") != "scripts/release-gates.json":
        raise EvidenceError(f"release gate report provenance is invalid: {path}")
    if "artifact_error" in payload:
        validate_string(payload["artifact_error"], f"release gate artifact error {path}")
        if payload.get("status") == "passed":
            raise EvidenceError(f"release gate artifact diagnostic cannot accompany a passed report: {path}")
    if payload.get("status") != "passed":
        raise EvidenceError(f"release gate is not passed: {path}")
    if type(payload.get("schema_version")) is not int or payload["schema_version"] != 1:
        raise EvidenceError(f"release gate schema is unsupported: {path}")
    git = payload.get("git")
    platform = payload.get("platform")
    if not isinstance(git, dict) or not isinstance(platform, dict):
        raise EvidenceError(f"release gate report candidate provenance is invalid: {path}")
    reject_unknown_fields(git, REPORT_GIT_KEYS, f"release gate report git metadata {path}")
    reject_unknown_fields(platform, REPORT_PLATFORM_KEYS, f"release gate report platform metadata {path}")
    for field in ("ahead", "behind"):
        value = git.get(field)
        if value is not None and (isinstance(value, bool) or not isinstance(value, int) or value < 0):
            raise EvidenceError(f"release gate report git metadata {path} is invalid")
    for field in ("branch", "head"):
        if field in git:
            validate_string(git[field], f"release gate report git {field} {path}")
    for field in ("impact_paths", "modified_paths", "staged_paths"):
        if field in git:
            validate_string_list(git[field], f"release gate report git {field} {path}", maximum=4096)
            for item in git[field]:
                validate_relative_path(item, f"release gate report git {field} {path}")
    path_semantics = git.get("path_semantics")
    if "path_semantics" in git:
        if not isinstance(path_semantics, dict):
            raise EvidenceError(f"release gate report Git path semantics are invalid: {path}")
        reject_unknown_fields(path_semantics, REPORT_PATH_SEMANTICS_KEYS, f"release gate report Git path semantics {path}")
        if set(path_semantics) != REPORT_PATH_SEMANTICS_KEYS:
            raise EvidenceError(f"release gate report Git path semantics are invalid: {path}")
        for value in path_semantics.values():
            validate_string(value, f"release gate report Git path semantics {path}")
    if not isinstance(git.get("head"), str) or not COMMIT.fullmatch(git["head"]):
        raise EvidenceError(f"release gate report candidate provenance is invalid: {path}")
    if git["head"] != expected_commit:
        raise EvidenceError(f"release gate report candidate commit does not match checked-out candidate: {path}")
    for field in ("staged_paths", "modified_paths"):
        paths = git.get(field)
        if not isinstance(paths, list) or any(not isinstance(item, str) or not item for item in paths):
            raise EvidenceError(f"release gate report candidate checkout state is invalid: {path}")
        if paths:
            raise EvidenceError(f"release gate report candidate checkout is dirty: {path}")
    for field in ("machine", "system", "release"):
        validate_string(platform.get(field), f"release gate report platform {field} {path}", maximum=256)
    external_gates = payload.get("external_gates")
    if not isinstance(external_gates, list):
        raise EvidenceError(f"release gate external classifications are invalid: {path}")
    for external in external_gates:
        validate_external_descriptor(external, str(path))
    gates = payload.get("gates")
    skipped = payload.get("skipped_gates")
    if not isinstance(gates, list) or not gates or not isinstance(skipped, list) or skipped:
        raise EvidenceError(f"release gate results are incomplete: {path}")
    for gate in gates:
        if not isinstance(gate, dict) or not isinstance(gate.get("id"), str) or not gate["id"] or gate.get("status") != "passed" or not isinstance(gate.get("required"), bool):
            raise EvidenceError(f"release gate contains a non-passed result: {path}")
        reject_unknown_fields(gate, REPORT_GATE_KEYS, f"release gate result {path}")
        argv = gate.get("argv")
        if not isinstance(argv, list) or not argv or len(argv) > MAX_COMMAND_ARGUMENTS or any(not isinstance(argument, str) or not argument or len(argument) > 4096 or not argument.isprintable() for argument in argv):
            raise EvidenceError(f"release gate execution evidence is incomplete: {path}")
        validate_string(gate["id"], f"release gate result id {path}", maximum=128)
        if not isinstance(gate.get("evidence_limit"), str) or gate["evidence_limit"] not in EVIDENCE_LIMITS:
            raise EvidenceError(f"release gate evidence-limit metadata is invalid: {path}")
        validate_string(gate.get("description"), f"release gate result description {path}", maximum=512)
        for field in ("stdout", "stderr"):
            validate_string(gate.get(field), f"release gate result {field} {path}", maximum=MAX_GATE_OUTPUT_BYTES, allow_empty=True)
        if "error" in gate:
            validate_string(gate["error"], f"release gate result error {path}", maximum=512)
        cwd = gate.get("cwd")
        if (
            not isinstance(cwd, str)
            or not cwd
            or len(cwd) > 256
            or not cwd.isprintable()
            or "\\" in cwd
            or Path(cwd).is_absolute()
            or ".." in Path(cwd).parts
            or type(gate.get("exit_code")) is not int
            or gate["exit_code"] != 0
        ):
            raise EvidenceError(f"release gate execution evidence is incomplete: {path}")
        gate_artifacts = gate.get("artifacts", [])
        if not isinstance(gate_artifacts, list):
            raise EvidenceError(f"release gate artifact records are invalid: {path}")
        for artifact in gate_artifacts:
            if not isinstance(artifact, dict):
                raise EvidenceError(f"release gate artifact records are invalid: {path}")
            reject_unknown_fields(artifact, REPORT_ARTIFACT_KEYS, f"release gate artifact record {path}")
            validate_relative_path(artifact.get("path"), f"release gate artifact record path {path}")
            if not isinstance(artifact.get("sha256"), str) or not DIGEST.fullmatch(artifact["sha256"]):
                raise EvidenceError(f"release gate artifact records are invalid: {path}")
    selected_subsystems = payload.get("selected_subsystems")
    if not isinstance(selected_subsystems, list) or any(not isinstance(item, str) or not item or len(item) > 128 or not item.isprintable() for item in selected_subsystems):
        raise EvidenceError(f"release gate selected subsystem metadata is invalid: {path}")
    records = payload.get("artifacts")
    if not isinstance(records, list):
        raise EvidenceError(f"release gate artifact records are invalid: {path}")
    if len(records) != 1:
        raise EvidenceError(f"release gate must bind exactly one packaged archive: {path}")
    record = records[0]
    if not isinstance(record, dict):
        raise EvidenceError(f"release gate must bind exactly one packaged archive: {path}")
    reject_unknown_fields(record, REPORT_ARTIFACT_KEYS, f"release gate artifact record {path}")
    if not str(record.get("path", "")).endswith(".tar.gz"):
        raise EvidenceError(f"release gate must bind exactly one packaged archive: {path}")
    artifact_path = record.get("path")
    digest = record.get("sha256")
    if not isinstance(artifact_path, str) or not artifact_path or "\x00" in artifact_path or "\\" in artifact_path or Path(artifact_path).is_absolute() or Path(artifact_path).parts != (Path(artifact_path).name,):
        raise EvidenceError(f"unsafe artifact path in release gate report: {path}")
    if not isinstance(digest, str) or not DIGEST.fullmatch(digest):
        raise EvidenceError(f"invalid artifact digest in release gate report: {path}")
    return payload, {"path": artifact_path, "sha256": digest}, validated_descriptor, raw


def load_report(path: Path, expected_commit: str) -> tuple[dict[str, Any], dict[str, Any], int, bytes]:
    global _ACTIVE_VALIDATED_DESCRIPTORS
    previous = _ACTIVE_VALIDATED_DESCRIPTORS
    pending: list[int] = []
    _ACTIVE_VALIDATED_DESCRIPTORS = pending
    try:
        result = _load_report_unmanaged(path, expected_commit)
        pending.remove(result[2])
        return result
    finally:
        for descriptor in pending:
            try:
                os.close(descriptor)
            except OSError:
                pass
        _ACTIVE_VALIDATED_DESCRIPTORS = previous


def _load_external_report_unmanaged(path: Path, descriptor: dict[str, Any], expected_commit: str) -> tuple[dict[str, str] | None, int, bytes]:
    validated_descriptor = -1
    try:
        validated_descriptor, raw = open_text_bounded(path, MAX_EVIDENCE_BYTES)
        payload = json.loads(raw.decode("utf-8"))
    except (EvidenceError, UnicodeError, json.JSONDecodeError) as exc:
        if isinstance(exc, EvidenceError):
            raise
        raise EvidenceError(f"cannot read external gate evidence {path}: {type(exc).__name__}") from exc
    if not isinstance(payload, dict):
        raise EvidenceError(f"external gate evidence is not an object: {path}")
    if not set(payload).issubset(EXTERNAL_REPORT_KEYS | {"observed_file", "observed_size", "observed_sha256"}):
        raise EvidenceError(f"external gate observed evidence metadata is invalid: {path}")
    if type(payload.get("schema_version")) is not int or payload.get("schema_version") != 1 or payload.get("evidence_scope") != "release_external_gate":
        raise EvidenceError(f"external gate evidence scope is invalid: {path}")
    if payload.get("gate_id") != descriptor["id"] or payload.get("workflow_job") != descriptor["workflow_job"] or payload.get("evidence_file") != descriptor["evidence_file"] or payload.get("command") != descriptor["command"] or payload.get("cwd") != descriptor["cwd"]:
        raise EvidenceError(f"external gate evidence does not match the manifest: {path}")
    candidate_commit = payload.get("candidate_commit")
    if not isinstance(candidate_commit, str) or candidate_commit != expected_commit or not COMMIT.fullmatch(candidate_commit):
        raise EvidenceError(f"external gate evidence candidate commit does not match checked-out candidate: {path}")
    if payload.get("status") != "passed" or type(payload.get("exit_code")) is not int or payload["exit_code"] != 0:
        raise EvidenceError(f"external gate evidence is not passed: {path}")
    command = payload.get("command")
    if not isinstance(command, list) or not command or len(command) > MAX_COMMAND_ARGUMENTS or any(not isinstance(argument, str) or not argument or len(argument) > 4096 or not argument.isprintable() for argument in command):
        raise EvidenceError(f"external gate execution evidence is incomplete: {path}")
    cwd = payload.get("cwd")
    if not isinstance(cwd, str) or not cwd or len(cwd) > 256 or not cwd.isprintable() or "\\" in cwd or re.match(r"^[A-Za-z]:", cwd) or Path(cwd).is_absolute() or ".." in Path(cwd).parts:
        raise EvidenceError(f"external gate execution evidence is incomplete: {path}")
    platform = payload.get("platform")
    if not isinstance(platform, dict) or any(not isinstance(platform.get(field), str) or not platform[field] or len(platform[field]) > 256 or not platform[field].isprintable() for field in ("machine", "system", "release")):
        raise EvidenceError(f"external gate platform evidence is incomplete: {path}")
    reject_unknown_fields(platform, REPORT_PLATFORM_KEYS, f"external gate platform metadata {path}")
    observed_file = payload.get("observed_file")
    observed_size = payload.get("observed_size")
    observed_digest = payload.get("observed_sha256")
    observed_content_present = "observed_content_b64" in payload
    if observed_file is None:
        if observed_size is not None or observed_digest is not None or observed_content_present:
            raise EvidenceError(f"release gate observed evidence metadata is invalid: {path}")
    elif (
        not isinstance(observed_file, str)
        or not observed_file
        or len(observed_file) > MAX_METADATA_TEXT
        or not observed_file.isprintable()
        or "\x00" in observed_file
        or Path(observed_file).is_absolute()
        or re.match(r"^[A-Za-z]:", observed_file)
        or ".." in Path(observed_file).parts
        or "\\" in observed_file
        or isinstance(observed_size, bool)
        or not isinstance(observed_size, int)
        or observed_size < 0
        or observed_size > MAX_OBSERVED_BYTES
        or not isinstance(observed_digest, str)
        or not DIGEST.fullmatch(observed_digest)
        or observed_content_present
    ):
        raise EvidenceError(f"release gate observed evidence metadata is invalid: {path}")
    artifact = payload.get("artifact")
    artifact_digest = payload.get("artifact_sha256")
    if descriptor["id"] == "golden-path-runtime":
        if (
            not isinstance(artifact, str)
            or Path(artifact).parts != (Path(artifact).name,)
            or not artifact.endswith(".tar.gz")
            or not isinstance(artifact_digest, str)
            or not DIGEST.fullmatch(artifact_digest)
        ):
            raise EvidenceError(f"external gate artifact binding is invalid: {path}")
        _, architecture = archive_identity(artifact)
        if architecture != "x86_64":
            raise EvidenceError(f"external gate artifact architecture is invalid: {path}")
        return {"path": artifact, "sha256": artifact_digest}, validated_descriptor, raw
    if artifact is not None or artifact_digest is not None:
        raise EvidenceError(f"external gate artifact metadata is unexpected: {path}")
    return None, validated_descriptor, raw


def load_external_report(path: Path, descriptor: dict[str, Any], expected_commit: str) -> tuple[dict[str, str] | None, int, bytes]:
    global _ACTIVE_VALIDATED_DESCRIPTORS
    previous = _ACTIVE_VALIDATED_DESCRIPTORS
    pending: list[int] = []
    _ACTIVE_VALIDATED_DESCRIPTORS = pending
    try:
        result = _load_external_report_unmanaged(path, descriptor, expected_commit)
        pending.remove(result[1])
        return result
    finally:
        for pending_descriptor in pending:
            try:
                os.close(pending_descriptor)
            except OSError:
                pass
        _ACTIVE_VALIDATED_DESCRIPTORS = previous


def load_manifest(path: Path) -> tuple[dict[str, dict[str, Any]], dict[str, dict[str, Any]]]:
    try:
        payload = json.loads(read_text_bounded(path, MAX_REPORT_BYTES))
    except (EvidenceError, UnicodeError, json.JSONDecodeError) as exc:
        if isinstance(exc, EvidenceError):
            raise
        raise EvidenceError(f"cannot read release gate manifest: {type(exc).__name__}") from exc
    if not isinstance(payload, dict) or type(payload.get("schema_version")) is not int or payload["schema_version"] != 1:
        raise EvidenceError("release gate manifest schema is unsupported")
    reject_unknown_fields(payload, MANIFEST_KEYS, "release gate manifest")
    gates = payload.get("gates")
    if not isinstance(gates, list) or not gates:
        raise EvidenceError("release gate manifest has no executable gates")
    required: dict[str, dict[str, Any]] = {}
    for gate in gates:
        if not isinstance(gate, dict) or not isinstance(gate.get("id"), str) or not gate["id"] or len(gate["id"]) > 128 or not gate["id"].isprintable() or gate["id"] in required or not isinstance(gate.get("required", True), bool):
            raise EvidenceError("release gate manifest has malformed or duplicate executable IDs")
        reject_unknown_fields(gate, MANIFEST_GATE_KEYS, f"release gate manifest entry {gate.get('id', '<unknown>')}")
        description = gate.get("description", gate["id"])
        validate_string(description, f"release gate manifest description {gate.get('id', '<unknown>')}", maximum=512)
        argv = gate.get("argv")
        cwd = gate.get("cwd", ".")
        if not isinstance(argv, list) or not argv or len(argv) > MAX_COMMAND_ARGUMENTS or any(not isinstance(argument, str) or not argument or len(argument) > 4096 or not argument.isprintable() for argument in argv):
            raise EvidenceError("release gate manifest has malformed executable command metadata")
        manifest_root = path.resolve().parent.parent
        _, report_argv = validate_executable_command(argv, gate["id"], manifest_root)
        if not isinstance(cwd, str) or not cwd or len(cwd) > 256 or not cwd.isprintable() or "\\" in cwd or re.match(r"^[A-Za-z]:", cwd) or Path(cwd).is_absolute() or ".." in Path(cwd).parts:
            raise EvidenceError("release gate manifest has malformed executable working-directory metadata")
        subsystems = gate.get("subsystems")
        if not isinstance(subsystems, list) or not subsystems or any(not isinstance(item, str) or not item or len(item) > 128 or not item.isprintable() for item in subsystems):
            raise EvidenceError("release gate manifest has malformed subsystem metadata")
        timeout = gate.get("timeout_seconds", 300)
        if isinstance(timeout, bool) or not isinstance(timeout, int) or timeout < 1 or timeout > 600:
            raise EvidenceError("release gate manifest has malformed timeout metadata")
        evidence_limit = gate.get("evidence_limit", "unit-verified")
        if not isinstance(evidence_limit, str) or evidence_limit not in EVIDENCE_LIMITS:
            raise EvidenceError("release gate manifest has malformed evidence-limit metadata")
        artifacts = gate.get("artifacts", [])
        validate_string_list(artifacts, f"release gate manifest artifacts for {gate['id']}", maximum=4096)
        for item in artifacts:
            validate_relative_path(item, f"release gate manifest artifact {gate['id']}")
        required[gate["id"]] = {
            "required": bool(gate.get("required", True)),
            "argv": argv,
            "report_argv": report_argv,
            "cwd": cwd,
            "subsystems": subsystems,
            "artifacts": artifacts,
            "evidence_limit": evidence_limit,
        }
    external_entries = payload.get("external_gates", [])
    if not isinstance(external_entries, list):
        raise EvidenceError("release gate manifest external classifications are invalid")
    external: dict[str, dict[str, Any]] = {}
    for entry in external_entries:
        descriptor = validate_external_descriptor(entry, "manifest")
        if descriptor["id"] in external:
            raise EvidenceError("release gate manifest has malformed or duplicate external IDs")
        external[descriptor["id"]] = descriptor
    if set(required).intersection(external):
        raise EvidenceError("release gate manifest executable and external gate IDs overlap")
    return required, external


def _validate_open_root(
    root: Path,
    root_descriptor: int,
    expected_reports: int,
    manifest: Path,
    expected_commit: str,
    publication_root: Path | None,
    checksums_output: Path | None,
) -> tuple[int, str]:
    archive_descriptors: dict[str, int] = {}
    archive_digests: dict[str, str] = {}
    validated_files: dict[str, tuple[int, bytes]] = {}
    validated_file_limits: dict[str, int] = {}
    try:
        if not root.is_dir() or expected_reports < 1:
            raise EvidenceError("evidence root and expected report count are invalid")
        if not COMMIT.fullmatch(expected_commit):
            raise EvidenceError("expected candidate commit must be a full lowercase Git SHA-1")
        checkout = Path.cwd().resolve(strict=True)
        checkout_metadata = checkout.stat()
        checkout_identity = (checkout_metadata.st_dev, checkout_metadata.st_ino)
        manifest_path = manifest.resolve(strict=True)
        if manifest_path != checkout / "scripts/release-gates.json":
            raise EvidenceError("release gate manifest must be the source-owned scripts/release-gates.json")
        expected_gates, expected_external = load_manifest(manifest_path)
        archives = files_named(root, ARCHIVE_PATTERN)
        archive_by_name: dict[str, Path] = {}

        seen_archive_names: set[str] = set()
        for archive in archives:
            if archive.name in seen_archive_names:
                raise EvidenceError(f"duplicate archive name in release artifacts: {archive.name}")
            seen_archive_names.add(archive.name)
        for archive in archives:
            if archive.parent != root:
                raise EvidenceError(f"release archive must be directly under evidence root: {archive}")
            archive_by_name[archive.name] = archive
        archive_identities = [archive_identity(name) for name in archive_by_name]
        if expected_reports == len(SUPPORTED_ARCHITECTURES) and {architecture for _, architecture in archive_identities} != set(SUPPORTED_ARCHITECTURES):
            raise EvidenceError("release archives do not match the supported archive matrix")
        if any(architecture not in SUPPORTED_ARCHITECTURES for _, architecture in archive_identities):
            raise EvidenceError("release archive architecture is unsupported")
        versions = {version for version, _ in archive_identities}
        if len(versions) > 1:
            raise EvidenceError("release archives must use the same release version")
        for name, archive in archive_by_name.items():
            archive_descriptors[name] = open_regular_archive(archive)
            archive_digests[name] = sha256_descriptor(archive_descriptors[name])

        external_reports = files_named(root, EXTERNAL_REPORT_PATTERN)
        expected_external_files = {descriptor["evidence_file"] for descriptor in expected_external.values()}
        actual_external_files = {path.name for path in external_reports}
        if actual_external_files != expected_external_files:
            raise EvidenceError("external gate evidence is missing or unexpected")
        external_artifacts: dict[str, dict[str, str]] = {}
        for external_path in external_reports:
            if external_path.parent != root:
                raise EvidenceError(f"external gate evidence must be directly under evidence root: {external_path}")
            descriptor = next(item for item in expected_external.values() if item["evidence_file"] == external_path.name)
            artifact, validated_descriptor, validated_bytes = load_external_report(external_path, descriptor, expected_commit)
            validated_files[external_path.name] = (validated_descriptor, validated_bytes)
            validated_file_limits[external_path.name] = MAX_EVIDENCE_BYTES
            if artifact is not None:
                if artifact["path"] not in archive_by_name:
                    raise EvidenceError(f"external gate artifact is missing: {artifact['path']}")
                actual = archive_digests[artifact["path"]]
                if actual != artifact["sha256"]:
                    raise EvidenceError(f"external gate artifact digest mismatch: {artifact['path']}")
                external_artifacts[descriptor["id"]] = artifact
        if "golden-path-runtime" in expected_external and "golden-path-runtime" not in external_artifacts:
            raise EvidenceError("golden-path-runtime evidence is not bound to a packaged archive")

        reports = files_named(root, REPORT_PATTERN)
        if len(reports) != expected_reports:
            raise EvidenceError(f"expected {expected_reports} release-gate reports, found {len(reports)}")
        matched: set[str] = set()
        for report_path in reports:
            if report_path.parent != root:
                raise EvidenceError(f"release gate report must be directly under evidence root: {report_path}")
            report_payload, record, validated_descriptor, validated_bytes = load_report(report_path, expected_commit)
            validated_files[report_path.name] = (validated_descriptor, validated_bytes)
            validated_file_limits[report_path.name] = MAX_REPORT_BYTES
            report_gates = report_payload["gates"]
            report_gate_ids = [gate["id"] for gate in report_gates]
            if len(report_gate_ids) != len(set(report_gate_ids)) or set(report_gate_ids) != set(expected_gates):
                raise EvidenceError(f"release gate report does not cover the executable manifest gate set: {report_path}")
            if any(gate["required"] != expected_gates[gate["id"]]["required"] for gate in report_gates):
                raise EvidenceError(f"release gate required metadata does not match the manifest: {report_path}")
            if any(gate["evidence_limit"] != expected_gates[gate["id"]]["evidence_limit"] for gate in report_gates):
                raise EvidenceError(f"release gate evidence-limit metadata does not match the manifest: {report_path}")
            if any(gate["argv"] != expected_gates[gate["id"]]["report_argv"] or gate["cwd"] != expected_gates[gate["id"]]["cwd"] for gate in report_gates):
                raise EvidenceError(f"release gate execution metadata does not match the manifest: {report_path}")
            expected_subsystems = sorted({subsystem for gate in expected_gates.values() for subsystem in gate["subsystems"]})
            if report_payload["selected_subsystems"] != expected_subsystems:
                raise EvidenceError(f"release gate selected subsystem metadata does not match the manifest: {report_path}")
            for gate in report_gates:
                report_artifacts = [artifact["path"] for artifact in gate.get("artifacts", [])]
                if report_artifacts != expected_gates[gate["id"]]["artifacts"]:
                    raise EvidenceError(f"release gate artifact metadata does not match the manifest: {report_path}")
            report_external = report_payload["external_gates"]
            report_external_map = {entry["id"]: validate_external_descriptor(entry, str(report_path)) for entry in report_external}
            if len(report_external_map) != len(report_external) or report_external_map != expected_external:
                raise EvidenceError(f"release gate external classifications do not match the manifest: {report_path}")
            name = record["path"]
            if name not in archive_by_name:
                raise EvidenceError(f"release archive is missing for report {report_path}: {name}")
            if name in matched:
                raise EvidenceError(f"multiple release-gate reports bind archive: {name}")
            actual = archive_digests[name]
            if actual != record["sha256"]:
                raise EvidenceError(f"release archive digest mismatch for {name}: {report_path}")
            matched.add(name)
        if matched != set(archive_by_name):
            missing_reports = sorted(set(archive_by_name) - matched)
            raise EvidenceError(f"every uploaded archive must have a matching release-gate report: {missing_reports}")
        revalidate_archive_descriptors(archive_descriptors, archive_digests)
        revalidate_evidence_descriptors(validated_files, validated_file_limits, root_descriptor)
        if publication_root is not None:
            if checksums_output is None:
                raise EvidenceError("publication requires a checksum output to seal the pathname handoff")
            if root.resolve(strict=True) != checkout:
                seal_evidence_tree(root, root_descriptor, validated_files)
            publish_archive_links(
                archive_descriptors,
                archive_digests,
                publication_root,
                checkout,
                checksums_output,
                validated_files,
                checkout_identity,
            )
        for descriptor in archive_descriptors.values():
            os.close(descriptor)
        archive_descriptors.clear()
        for descriptor, _ in validated_files.values():
            os.close(descriptor)
        validated_files.clear()
        return 0, f"validated {len(reports)} release-gate reports and {len(archives)} archive digests"
    except (EvidenceError, OSError) as exc:
        for descriptor in archive_descriptors.values():
            os.close(descriptor)
        for descriptor, _ in validated_files.values():
            os.close(descriptor)
        return 1, str(exc)


def validate(
    root: Path,
    expected_reports: int,
    manifest: Path,
    expected_commit: str,
    publication_root: Path | None = None,
    checksums_output: Path | None = None,
) -> tuple[int, str]:
    """Validate one evidence tree through a descriptor-bound root identity."""
    if os.name != "posix" or not hasattr(os, "O_DIRECTORY") or not hasattr(os, "O_NOFOLLOW"):
        return 1, "safe evidence-root validation is unavailable on this platform"
    try:
        if root.is_symlink():
            raise EvidenceError("evidence root must not be a symlink")
        resolved_root = root.resolve(strict=True)
        if not resolved_root.is_dir() or expected_reports < 1:
            raise EvidenceError("evidence root and expected report count are invalid")
        resolved_publication_root = None
        if publication_root is not None:
            checkout = Path.cwd().resolve(strict=True)
            resolved_publication_root = publication_root if publication_root.is_absolute() else checkout / publication_root
            relative_publication_root = resolved_publication_root.relative_to(checkout)
            if len(relative_publication_root.parts) != 1 or any(part in {".", ".."} for part in relative_publication_root.parts):
                raise EvidenceError("publication root must be a direct, symlink-free checkout child")
            if checksums_output is None:
                raise EvidenceError("publication requires a checksum output to seal the pathname handoff")
        if checksums_output is not None:
            checkout = Path.cwd().resolve(strict=True)
            resolved_checksums_output = checksums_output if checksums_output.is_absolute() else checkout / checksums_output
            relative_checksums_output = resolved_checksums_output.relative_to(checkout)
            if len(relative_checksums_output.parts) != 1 or relative_checksums_output.parts[0] in {".", ".."}:
                raise EvidenceError("checksum output must be a direct checkout file")
        else:
            resolved_checksums_output = None
        root_descriptor = os.open(resolved_root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    except (EvidenceError, OSError) as exc:
        return 1, str(exc)
    try:
        return _validate_open_root(
            Path(f"/proc/self/fd/{root_descriptor}"),
            root_descriptor,
            expected_reports,
            manifest,
            expected_commit,
            resolved_publication_root,
            resolved_checksums_output,
        )
    finally:
        os.close(root_descriptor)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="downloaded release-artifact directory")
    parser.add_argument("--manifest", type=Path, required=True, help="source-owned release-gate manifest")
    parser.add_argument("--expected-reports", type=int, required=True)
    parser.add_argument("--expected-commit", required=True, help="full lowercase Git commit checked out for this release")
    parser.add_argument("--publication-root", type=Path, help="checkout-contained directory receiving retained archive links")
    parser.add_argument("--checksums-output", type=Path, help="new checkout-root checksum manifest written before publication sealing")
    args = parser.parse_args()
    try:
        status, message = validate(
            args.root,
            args.expected_reports,
            args.manifest,
            args.expected_commit,
            args.publication_root,
            args.checksums_output,
        )
    except (ValueError, OSError) as exc:
        print(f"release evidence: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 2
    if status == 0:
        print(message)
        return 0
    print(f"release evidence: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
