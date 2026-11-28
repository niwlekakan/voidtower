#!/usr/bin/env python3
"""Extract one bounded, regular binary from a release tar archive safely."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import stat
import tarfile


MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_MEMBER_COUNT = 10_000
MAX_UNPACKED_BYTES = 2 * 1024 * 1024 * 1024
MAX_BINARY_BYTES = 256 * 1024 * 1024
MEMBER_NAME = "voidtower"


def relative_parts(value: str, field: str) -> tuple[str, ...]:
    path = Path(value)
    if (
        not value
        or path.is_absolute()
        or ".." in path.parts
        or "\\" in value
        or any(not part or part == "." for part in path.parts if part != ".")
    ):
        raise ValueError(f"{field} must be a clean relative path")
    return tuple(part for part in path.parts if part != ".")


def open_relative(path: str, flags: int, *, field: str) -> int:
    directory, filename = open_relative_parent(path, field=field)
    try:
        return os.open(filename, flags | os.O_NOFOLLOW, dir_fd=directory)
    finally:
        os.close(directory)


def open_relative_parent(path: str, *, field: str) -> tuple[int, str]:
    parts = relative_parts(path, field)
    if not parts:
        raise ValueError(f"{field} must name a file")
    if os.name != "posix" or not hasattr(os, "O_DIRECTORY") or not hasattr(os, "O_NOFOLLOW"):
        raise RuntimeError("safe release extraction requires POSIX no-follow file descriptors")
    directory_flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    directory = os.open(".", directory_flags)
    try:
        for part in parts[:-1]:
            next_directory = os.open(part, directory_flags, dir_fd=directory)
            os.close(directory)
            directory = next_directory
        return directory, parts[-1]
    except BaseException:
        os.close(directory)
        raise


def validate_member(member: tarfile.TarInfo) -> tuple[str, int]:
    path = Path(member.name)
    if path.is_absolute() or "\\" in member.name or ".." in path.parts:
        raise ValueError("archive contains an unsafe member path")
    normalized = tuple(part for part in path.parts if part not in ("", "."))
    if member.isdir():
        return "/".join(normalized), 0
    if not member.isreg():
        raise ValueError("archive contains a non-regular member")
    if member.size < 0:
        raise ValueError("archive contains a negative member size")
    return "/".join(normalized), member.size


def extract(archive: str, output: str) -> None:
    archive_fd = open_relative(archive, os.O_RDONLY, field="archive")
    try:
        archive_stat = os.fstat(archive_fd)
        if not stat.S_ISREG(archive_stat.st_mode) or archive_stat.st_size > MAX_ARCHIVE_BYTES:
            raise ValueError("archive is not a bounded regular file")
        with os.fdopen(archive_fd, "rb") as archive_stream:
            archive_fd = -1
            found: tarfile.TarInfo | None = None
            seen_members: set[str] = set()
            member_count = 0
            unpacked_bytes = 0
            with tarfile.open(fileobj=archive_stream, mode="r:gz") as package:
                for member in package:
                    member_count += 1
                    if member_count > MAX_MEMBER_COUNT:
                        raise ValueError("archive has too many members")
                    normalized, member_size = validate_member(member)
                    if normalized in seen_members:
                        raise ValueError("archive contains duplicate members")
                    seen_members.add(normalized)
                    unpacked_bytes += member_size
                    if unpacked_bytes > MAX_UNPACKED_BYTES:
                        raise ValueError("archive expands beyond the bounded size")
                    if normalized == MEMBER_NAME:
                        if found is not None or member_size > MAX_BINARY_BYTES:
                            raise ValueError("archive has a duplicate or oversized VoidTower binary")
                        found = member
                if found is None:
                    raise ValueError("archive does not contain the VoidTower binary")
                source = package.extractfile(found)
                if source is None:
                    raise ValueError("archive binary cannot be read")
                output_parent, output_name = open_relative_parent(output, field="output")
                output_fd = -1
                output_created = False
                extraction_succeeded = False
                try:
                    output_fd = os.open(
                        output_name,
                        (os.O_WRONLY | os.O_CREAT | os.O_EXCL) | os.O_NOFOLLOW,
                        dir_fd=output_parent,
                    )
                    output_created = True
                    os.fchmod(output_fd, 0o700)
                    written = 0
                    with os.fdopen(output_fd, "wb") as destination:
                        output_fd = -1
                        while True:
                            chunk = source.read(min(1024 * 1024, MAX_BINARY_BYTES - written + 1))
                            if not chunk:
                                break
                            written += len(chunk)
                            if written > MAX_BINARY_BYTES:
                                raise ValueError("extracted binary exceeds the bounded size")
                            destination.write(chunk)
                    if written != found.size:
                        raise ValueError("archive binary changed while extracting")
                    extraction_succeeded = True
                finally:
                    if output_fd != -1:
                        os.close(output_fd)
                    if output_created and not extraction_succeeded:
                        try:
                            os.unlink(output_name, dir_fd=output_parent)
                        except FileNotFoundError:
                            pass
                    os.close(output_parent)
    finally:
        if archive_fd != -1:
            os.close(archive_fd)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    try:
        extract(args.archive, args.output)
    except (OSError, RuntimeError, tarfile.TarError, ValueError) as exc:
        print(f"release archive extraction: {exc}", file=os.sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
