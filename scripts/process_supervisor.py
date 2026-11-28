#!/usr/bin/env python3
"""Run one argv command in a parent-death-aware process group.

This is an internal helper for repository-owned evidence runners. It never
interprets a command as shell text. The caller validates the argv before
starting this helper.
"""

from __future__ import annotations

import ctypes
import errno
import os
import platform
import resource
import signal
import subprocess
import sys
import time

START_FAILURE_MARKER = "[voidtower supervisor start failure]"
START_FAILURE_MARKER_ENV = "VOIDTOWER_SUPERVISOR_FAILURE_MARKER"
MAX_PROCESS_ENTRIES = 32_768
MAX_PROCESS_STAT_BYTES = 4 * 1024
MAX_CLEANUP_PASSES = 10
MAX_REAPED_CHILDREN = 4_096


class CleanupError(RuntimeError):
    """The supervisor could not prove that descendant cleanup completed."""


class ProcessSnapshotError(CleanupError):
    """The bounded process snapshot was incomplete or unavailable."""

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


def _read_process_start_time(pid: int) -> int | None:
    try:
        with open(f"/proc/{pid}/stat", "rb") as stat_file:
            stat_line = stat_file.read(MAX_PROCESS_STAT_BYTES)
    except FileNotFoundError:
        return None
    except OSError as exc:
        raise CleanupError("process identity could not be read") from exc
    closing_name = stat_line.rfind(b")")
    if closing_name < 0:
        raise ProcessSnapshotError("process identity data was malformed")
    fields = stat_line[closing_name + 2 :].split()
    if len(fields) <= 19:
        raise ProcessSnapshotError("process identity data was incomplete")
    try:
        return int(fields[19])
    except ValueError as exc:
        raise ProcessSnapshotError("process identity data was malformed") from exc


def process_start_time(pid: int) -> int:
    """Return a process identity token, failing closed when it is unavailable."""
    start_time = _read_process_start_time(pid)
    if start_time is None:
        raise CleanupError("process identity could not be established")
    return start_time


def _process_snapshot_once() -> dict[int, tuple[int, int]]:
    snapshot: dict[int, tuple[int, int]] = {}
    try:
        with os.scandir("/proc") as entries:
            for entry in entries:
                if not entry.name.isdigit():
                    continue
                if len(snapshot) >= MAX_PROCESS_ENTRIES:
                    raise ProcessSnapshotError("process snapshot exceeded its bounded size")
                try:
                    with open(os.path.join(entry.path, "stat"), "rb") as stat_file:
                        stat_line = stat_file.read(MAX_PROCESS_STAT_BYTES)
                    closing_name = stat_line.rfind(b")")
                    if closing_name < 0:
                        raise ProcessSnapshotError("process snapshot contained malformed identity data")
                    fields = stat_line[closing_name + 2 :].split()
                    if len(fields) <= 19:
                        raise ProcessSnapshotError("process snapshot contained incomplete identity data")
                    snapshot[int(entry.name)] = (int(fields[1]), int(fields[19]))
                except FileNotFoundError:
                    continue
                except (OSError, ValueError) as exc:
                    raise ProcessSnapshotError("process snapshot contained unreadable identity data") from exc
    except ProcessSnapshotError:
        raise
    except OSError as exc:
        raise ProcessSnapshotError("process snapshot could not be collected") from exc
    return snapshot


def _process_snapshot() -> dict[int, tuple[int, int]]:
    last_error: ProcessSnapshotError | None = None
    for _attempt in range(3):
        try:
            return _process_snapshot_once()
        except ProcessSnapshotError as exc:
            last_error = exc
            time.sleep(0.005)
    assert last_error is not None
    raise last_error


def _descendants(root_pid: int, snapshot: dict[int, tuple[int, int]]) -> dict[int, int]:
    children: dict[int, list[int]] = {}
    for pid, (parent, _start_time) in snapshot.items():
        children.setdefault(parent, []).append(pid)
    pending = [root_pid]
    descendants: dict[int, int] = {}
    while pending:
        parent = pending.pop()
        for pid in children.get(parent, ()):
            if pid in descendants:
                continue
            descendants[pid] = snapshot[pid][1]
            pending.append(pid)
    return descendants


def _syscall(number: int, *arguments: int) -> int:
    libc = ctypes.CDLL(None, use_errno=True)
    syscall = libc.syscall
    syscall.restype = ctypes.c_long
    result = syscall(number, *arguments)
    if result == -1:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))
    return int(result)


def _pidfd_syscall_number(name: str) -> int:
    if not sys.platform.startswith("linux"):
        raise CleanupError("atomic process identity signaling is Linux-only")
    if platform.machine() not in {"aarch64", "arm", "ppc64", "ppc64le", "riscv64", "s390x", "x86_64"}:
        raise CleanupError("atomic process identity signaling is unsupported on this architecture")
    return {"pidfd_send_signal": 424, "pidfd_open": 434}[name]


def _open_pidfd(pid: int) -> int:
    try:
        if hasattr(os, "pidfd_open"):
            return os.pidfd_open(pid, 0)
        return _syscall(_pidfd_syscall_number("pidfd_open"), pid, 0)
    except ProcessLookupError:
        raise
    except OSError as exc:
        if exc.errno in {errno.ESRCH, errno.ECHILD}:
            raise ProcessLookupError(pid) from exc
        raise CleanupError("atomic process identity binding failed") from exc


def _send_pidfd_signal(pidfd: int, signum: int) -> bool:
    try:
        pidfd_send_signal = getattr(os, "pidfd_send_signal", None)
        if pidfd_send_signal is not None:
            pidfd_send_signal(pidfd, signum, None, 0)
        else:
            _syscall(_pidfd_syscall_number("pidfd_send_signal"), pidfd, signum, 0, 0)
        return True
    except ProcessLookupError:
        return False
    except OSError as exc:
        if exc.errno in {errno.ESRCH, errno.ECHILD}:
            return False
        raise CleanupError("atomic process identity signaling failed") from exc


def signal_process(pid: int, start_time: int, signum: int) -> bool:
    """Signal a process only while its captured identity remains bound."""
    try:
        pidfd = _open_pidfd(pid)
    except ProcessLookupError:
        return False
    try:
        if _read_process_start_time(pid) != start_time:
            return False
        return _send_pidfd_signal(pidfd, signum)
    finally:
        os.close(pidfd)


def signal_owned_process(pid: int, signum: int) -> bool:
    """Signal a still-owned child through a pidfd without a reusable PID kill."""
    try:
        pidfd = _open_pidfd(pid)
    except ProcessLookupError:
        return False
    try:
        return _send_pidfd_signal(pidfd, signum)
    finally:
        os.close(pidfd)


def _kill_if_same_process(pid: int, start_time: int, snapshot: dict[int, tuple[int, int]]) -> None:
    current = snapshot.get(pid)
    if current is None or current[1] != start_time:
        return
    try:
        pidfd = _open_pidfd(pid)
    except ProcessLookupError:
        return
    try:
        # Bind to the process first, then re-read its identity. If the PID was
        # reused between the snapshot and pidfd_open, do not signal it.
        if _read_process_start_time(pid) != start_time:
            return
        _send_pidfd_signal(pidfd, signal.SIGKILL)
    finally:
        os.close(pidfd)


def _reap_adopted_children(candidate_pids: list[int]) -> None:
    if len(candidate_pids) > MAX_REAPED_CHILDREN:
        raise CleanupError("adopted child cleanup exceeded its bounded reap size")
    for pid in candidate_pids:
        try:
            os.waitpid(pid, os.WNOHANG)
        except ChildProcessError:
            continue
        except OSError as exc:
            if exc.errno == errno.ECHILD:
                continue
            raise CleanupError("adopted child state could not be reaped") from exc


def kill_descendants(
    root_pid: int,
    *,
    protected_pids: set[int] | None = None,
    reap_adopted: bool = True,
    root_start_time: int | None = None,
) -> None:
    protected_pids = protected_pids or set()
    if root_start_time is None:
        root_start_time = process_start_time(root_pid)
    try:
        root_pidfd = _open_pidfd(root_pid)
    except ProcessLookupError as exc:
        raise CleanupError("root process identity could not be bound") from exc
    quiet_passes = 0
    try:
        for _ in range(MAX_CLEANUP_PASSES):
            snapshot = _process_snapshot()
            root_identity = snapshot.get(root_pid)
            if root_identity is None or root_identity[1] != root_start_time:
                raise CleanupError("root process identity changed during descendant cleanup")
            descendants = _descendants(root_pid, snapshot)
            unprotected = {pid: start_time for pid, start_time in descendants.items() if pid not in protected_pids}
            if not unprotected:
                quiet_passes += 1
                if quiet_passes >= 2:
                    return
            else:
                quiet_passes = 0
                for pid, start_time in descendants.items():
                    if not _send_pidfd_signal(root_pidfd, 0):
                        raise CleanupError("root process identity changed before descendant signaling")
                    if process_start_time(root_pid) != root_start_time:
                        raise CleanupError("root process identity changed before descendant signaling")
                    _kill_if_same_process(pid, start_time, snapshot)
                if reap_adopted:
                    _reap_adopted_children(list(unprotected))
            time.sleep(0.01)
        raise CleanupError("descendant cleanup did not quiesce")
    finally:
        os.close(root_pidfd)


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
        try:
            kill_descendants(
                os.getpid(),
                protected_pids={child.pid} if child is not None else set(),
                root_start_time=process_start_time(os.getpid()),
            )
            if child is not None:
                child.wait(timeout=1)
        except CleanupError:
            os._exit(125)
        except subprocess.TimeoutExpired:
            os._exit(125)
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
    try:
        kill_descendants(
            os.getpid(),
            protected_pids={child.pid} if child is not None else set(),
            root_start_time=process_start_time(os.getpid()),
        )
    except CleanupError:
        print(START_FAILURE_MARKER, file=sys.stderr, flush=True)
        return 125
    return return_code if return_code >= 0 else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
