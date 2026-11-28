#!/usr/bin/env python3
"""Write bounded evidence for an external release gate after its CI job passes."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import subprocess
import sys

from process_supervisor import _install_subreaper, kill_descendants


COMMIT = re.compile(r"^[0-9a-f]{40}$")
MAX_TEXT = 512
MAX_COMMAND_ARGUMENTS = 64
MAX_EVIDENCE_BYTES = 64 * 1024
MAX_OBSERVED_BYTES = 16 * 1024


def bounded(value: str, field: str) -> str:
    if not value or len(value) > MAX_TEXT or not value.isprintable():
        raise ValueError(f"{field} is empty or unbounded")
    return value


def safe_relative(value: str, field: str) -> str:
    bounded(value, field)
    path = Path(value)
    if path.is_absolute() or re.match(r"^[A-Za-z]:", value) or ".." in path.parts or "\\" in value:
        raise ValueError(f"{field} must stay within the checkout")
    return value


def open_relative_file(path: Path, flags: int) -> int:
    """Open a checkout-relative file through no-follow directory handles."""
    if os.name != "posix" or not hasattr(os, "O_DIRECTORY") or not hasattr(os, "O_NOFOLLOW"):
        raise OSError("safe external evidence paths require POSIX no-follow descriptors")
    parts = tuple(part for part in path.parts if part != ".")
    if not parts:
        raise ValueError("file path is empty")
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    directory = os.open(".", directory_flags)
    try:
        for part in parts[:-1]:
            try:
                next_directory = os.open(part, directory_flags, dir_fd=directory)
            except OSError as exc:
                raise ValueError("path component is not a directory or is symlinked") from exc
            os.close(directory)
            directory = next_directory
        try:
            return os.open(parts[-1], flags | os.O_NOFOLLOW, dir_fd=directory)
        except OSError as exc:
            raise ValueError("file is missing, symlinked, or not a regular file") from exc
    finally:
        os.close(directory)


def open_relative_directory(path: str) -> int:
    """Open a checkout-relative working directory and retain its identity."""
    if os.name != "posix" or not hasattr(os, "O_DIRECTORY") or not hasattr(os, "O_NOFOLLOW"):
        raise OSError("safe external evidence directories require POSIX no-follow descriptors")
    parts = tuple(part for part in Path(path).parts if part != ".")
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    directory = os.open(".", directory_flags)
    try:
        for part in parts:
            try:
                next_directory = os.open(part, directory_flags, dir_fd=directory)
            except OSError as exc:
                raise ValueError("working directory component is not a directory or is symlinked") from exc
            os.close(directory)
            directory = next_directory
        return directory
    except BaseException:
        os.close(directory)
        raise


def set_parent_death_signal() -> None:
    if sys.platform.startswith("linux"):
        import ctypes
        import signal

        libc = ctypes.CDLL(None, use_errno=True)
        if libc.prctl(1, signal.SIGTERM) != 0:
            raise OSError(ctypes.get_errno(), "prctl(PR_SET_PDEATHSIG) failed")
        if os.getppid() == 1:
            os.kill(os.getpid(), signal.SIGTERM)


def stop_command(process: subprocess.Popen[bytes], parent_pid: int) -> None:
    import signal

    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    kill_descendants(parent_pid)
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        kill_descendants(parent_pid)
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait()


def terminate_children(_signum: int, _frame: object) -> None:
    kill_descendants(os.getpid())
    os._exit(128 + _signum)


def read_observed_file(path: Path) -> bytes:
    descriptor = open_relative_file(path, os.O_RDONLY)
    try:
        file_stat = os.fstat(descriptor)
        if not stat.S_ISREG(file_stat.st_mode) or file_stat.st_size > MAX_OBSERVED_BYTES:
            raise ValueError("observed file is not a regular bounded file")
        with os.fdopen(descriptor, "rb") as stream:
            descriptor = -1
            observed_bytes = stream.read(MAX_OBSERVED_BYTES + 1)
        if len(observed_bytes) > MAX_OBSERVED_BYTES or len(observed_bytes) != file_stat.st_size:
            raise ValueError("observed file changed or is oversized")
        return observed_bytes
    finally:
        if descriptor != -1:
            os.close(descriptor)


def sha256_file(path: Path) -> str:
    descriptor = open_relative_file(path, os.O_RDONLY)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise ValueError("artifact is not a regular file")
        digest = hashlib.sha256()
        with os.fdopen(descriptor, "rb") as stream:
            descriptor = -1
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
        return digest.hexdigest()
    finally:
        if descriptor != -1:
            os.close(descriptor)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--gate-id", required=True)
    parser.add_argument("--workflow-job", required=True)
    parser.add_argument("--candidate-commit", required=True)
    parser.add_argument("--cwd", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--observed-file", type=Path, help="successful action marker produced by a preceding CI step")
    parser.add_argument("--artifact", type=Path, help="packaged artifact exercised by the external gate")
    parser.add_argument("--timeout-seconds", type=int, default=600)
    parser.add_argument("command", nargs="*")
    args = parser.parse_args()
    try:
        if sys.platform.startswith("linux"):
            _install_subreaper()
            import signal

            signal.signal(signal.SIGTERM, terminate_children)
            signal.signal(signal.SIGINT, terminate_children)
        gate_id = bounded(args.gate_id, "gate id")
        workflow_job = bounded(args.workflow_job, "workflow job")
        if not COMMIT.fullmatch(args.candidate_commit):
            raise ValueError("candidate commit must be a full lowercase Git SHA-1")
        cwd = safe_relative(args.cwd, "working directory")
        if args.timeout_seconds < 1 or args.timeout_seconds > 1800:
            raise ValueError("timeout is outside 1..1800 seconds")
        if len(args.command) > MAX_COMMAND_ARGUMENTS or any(not item or len(item) > 4096 or not item.isprintable() for item in args.command):
            raise ValueError("command is empty or unbounded")
        if args.observed_file is None and not args.command:
            raise ValueError("provide a command or observed-file")
        output = args.output
        expected_name = f"release-external-{gate_id}.json"
        if output.is_absolute() or output.name != expected_name or output.parent != Path("."):
            raise ValueError("external evidence output must be the manifest-declared root-level filename")
        if output.exists() or output.is_symlink():
            raise ValueError("external evidence output already exists")

        artifact = None
        artifact_sha256_before = None
        if args.artifact is not None:
            artifact = safe_relative(args.artifact.as_posix(), "artifact")
            artifact_sha256_before = sha256_file(args.artifact)

        observed_file = None
        if args.observed_file is not None:
            observed_file = safe_relative(args.observed_file.as_posix(), "observed file")
            observed_bytes = read_observed_file(args.observed_file)
            exit_code = 0
        else:
            cwd_descriptor = open_relative_directory(cwd)
            if not Path("/proc/self/fd").is_dir():
                os.close(cwd_descriptor)
                raise OSError("stable external gate working directories require /proc/self/fd")
            try:
                process = subprocess.Popen(
                    args.command,
                    cwd=f"/proc/self/fd/{cwd_descriptor}",
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    shell=False,
                    start_new_session=True,
                    preexec_fn=set_parent_death_signal,
                    pass_fds=(cwd_descriptor,),
                )
            except BaseException:
                os.close(cwd_descriptor)
                raise
            try:
                try:
                    exit_code = process.wait(timeout=args.timeout_seconds)
                except subprocess.TimeoutExpired as exc:
                    stop_command(process, os.getpid())
                    raise RuntimeError("external gate command exceeded its bounded timeout") from exc
                kill_descendants(os.getpid())
                if exit_code != 0:
                    raise RuntimeError(f"external gate command failed with exit code {exit_code}")
            finally:
                os.close(cwd_descriptor)

        artifact_sha256 = None
        if args.artifact is not None:
            artifact_sha256 = sha256_file(args.artifact)
            if artifact_sha256 != artifact_sha256_before:
                raise RuntimeError("artifact changed while the external gate ran")

        payload = {
            "candidate_commit": args.candidate_commit,
            "command": list(args.command) if args.command else ["observed-ci-step"],
            "cwd": cwd,
            "evidence_file": output.name,
            "evidence_scope": "release_external_gate",
            "exit_code": exit_code,
            "gate_id": gate_id,
            "platform": {
                "machine": bounded(platform.machine(), "platform machine"),
                "release": bounded(platform.release(), "platform release"),
                "system": bounded(platform.system(), "platform system"),
            },
            "schema_version": 1,
            "status": "passed",
            "workflow_job": workflow_job,
        }
        if observed_file is not None:
            payload["observed_file"] = observed_file
            payload["observed_size"] = len(observed_bytes)
            payload["observed_sha256"] = hashlib.sha256(observed_bytes).hexdigest()
        if artifact is not None:
            payload["artifact"] = artifact
            payload["artifact_sha256"] = artifact_sha256
        encoded = (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode("utf-8")
        if len(encoded) > MAX_EVIDENCE_BYTES:
            raise ValueError("external evidence record is oversized")
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(output, flags, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(encoded)
    except (OSError, ValueError, subprocess.SubprocessError, RuntimeError) as exc:
        print(f"external gate evidence: {exc}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
