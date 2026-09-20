#!/usr/bin/env python3
"""Run one argv command in a parent-death-aware process group.

This is an internal helper for repository-owned evidence runners. It never
interprets a command as shell text. The caller validates the argv before
starting this helper.
"""

from __future__ import annotations

import ctypes
import os
import resource
import signal
import subprocess
import sys
import time

START_FAILURE_MARKER = "[voidtower supervisor start failure]"
START_FAILURE_MARKER_ENV = "VOIDTOWER_SUPERVISOR_FAILURE_MARKER"

def _parent_death_signal() -> None:
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(1, signal.SIGTERM) != 0:  # PR_SET_PDEATHSIG
        raise OSError(ctypes.get_errno(), "prctl(PR_SET_PDEATHSIG) failed")
    # Close the small race where the parent exits between fork and prctl.
    if os.getppid() == 1:
        os.kill(os.getpid(), signal.SIGTERM)


def _install_subreaper() -> None:
    if sys.platform.startswith("linux"):
        libc = ctypes.CDLL(None, use_errno=True)
        if libc.prctl(36, 1) != 0:  # PR_SET_CHILD_SUBREAPER
            raise OSError(ctypes.get_errno(), "prctl(PR_SET_CHILD_SUBREAPER) failed")


def _descendants(root_pid: int) -> list[int]:
    parents: dict[int, int] = {}
    for entry in os.scandir("/proc"):
        if not entry.name.isdigit():
            continue
        try:
            with open(os.path.join(entry.path, "stat"), encoding="ascii") as stat_file:
                fields = stat_file.read().rsplit(") ", 1)[1].split()
            parents[int(entry.name)] = int(fields[1])
        except (OSError, IndexError, ValueError):
            continue
    pending = [root_pid]
    descendants: list[int] = []
    while pending:
        parent = pending.pop()
        children = [pid for pid, ppid in parents.items() if ppid == parent]
        descendants.extend(children)
        pending.extend(children)
    return descendants


def kill_descendants(root_pid: int) -> None:
    for _ in range(3):
        for pid in reversed(_descendants(root_pid)):
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        time.sleep(0.01)


def main(argv: list[str]) -> int:
    output_limit: int | None = None
    command_start = 1
    if len(argv) >= 3 and argv[1] == "--max-output-bytes":
        try:
            output_limit = int(argv[2])
        except ValueError:
            return 2
        if output_limit < 1:
            return 2
        command_start = 3
    command = argv[command_start:]
    if not command or any("\x00" in item for item in command):
        return 2

    try:
        _install_subreaper()
    except OSError:
        return 2

    child: subprocess.Popen[bytes] | None = None

    def terminate_group(_signum: int, _frame: object) -> None:
        # Process groups cover ordinary forks; the subreaper tree also covers
        # descendants that call setsid() and leave the original process group.
        kill_descendants(os.getpid())
        os._exit(128 + signal.SIGTERM)

    signal.signal(signal.SIGTERM, terminate_group)
    signal.signal(signal.SIGINT, terminate_group)

    def configure_child() -> None:
        _parent_death_signal()
        if output_limit is not None:
            resource.setrlimit(resource.RLIMIT_FSIZE, (output_limit + 1, output_limit + 1))

    try:
        input_data = sys.stdin.buffer.read()
        child = subprocess.Popen(
            command,
            stdin=subprocess.PIPE if input_data else subprocess.DEVNULL,
            stdout=None,
            stderr=None,
            shell=False,
            preexec_fn=configure_child,
        )
        if input_data:
            child.communicate(input_data)
        else:
            child.wait()
        return_code = child.returncode
    except (OSError, subprocess.SubprocessError):
        print(os.environ.get(START_FAILURE_MARKER_ENV, START_FAILURE_MARKER), file=sys.stderr, flush=True)
        return 125
    kill_descendants(os.getpid())
    return return_code if return_code >= 0 else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
