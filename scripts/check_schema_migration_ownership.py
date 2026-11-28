#!/usr/bin/env python3
"""Fail-closed ownership checks for production SQLite schema changes."""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import subprocess
import sys


RUST_TOKEN_GAP = r"(?:\s|/\*.*?\*/|//[^\n]*)*"
DDL_CALL = re.compile(
    rf"sqlx{RUST_TOKEN_GAP}::{RUST_TOKEN_GAP}"
    rf"(?:query(?:_[a-z_]+)?|raw_sql){RUST_TOKEN_GAP}"
    rf"(?:!{RUST_TOKEN_GAP})?"
    rf"(?:::{RUST_TOKEN_GAP}<.*?>{RUST_TOKEN_GAP})?"
    rf"\({RUST_TOKEN_GAP}[^\"]*?(?:r#*)?[\"']\s*"
    r"(?:CREATE\s+(?:UNIQUE\s+)?INDEX|CREATE\s+TABLE|ALTER\s+TABLE|DROP\s+TABLE|DROP\s+INDEX)",
    re.IGNORECASE | re.DOTALL,
)
DDL_FILE_CALL = re.compile(
    rf"sqlx{RUST_TOKEN_GAP}::{RUST_TOKEN_GAP}query_file(?:_[a-z0-9_]+)?"
    rf"{RUST_TOKEN_GAP}!{RUST_TOKEN_GAP}\(",
    re.IGNORECASE | re.DOTALL,
)
MIGRATION_NAME = re.compile(r"^(\d{4})_[^/]+\.sql$")


def tracked(root: Path, relative: Path) -> bool:
    result = subprocess.run(
        ["git", "ls-files", "--error-unmatch", "--", relative.as_posix()],
        cwd=root,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def find_ddl_violations(root: Path) -> list[str]:
    source_root = root / "backend/src"
    if not source_root.is_dir():
        return ["backend/src: source directory is missing"]

    violations: list[str] = []
    current = root
    for component in Path("backend/src").parts:
        current /= component
        if current.is_symlink():
            violations.append(f"backend/src: symlinked path component {current.relative_to(root)}")
            return violations
    for path in sorted(source_root.rglob("*")):
        relative = path.relative_to(root)
        if path.is_symlink():
            violations.append(f"{relative}: symlinks are not allowed in backend/src")
            continue
        if not path.is_file() or path.suffix != ".rs":
            continue
        if relative.as_posix() == "backend/src/db/legacy.rs":
            continue
        try:
            resolved = path.resolve(strict=True)
            resolved.relative_to(root)
            content = path.read_text(encoding="utf-8")
        except ValueError:
            violations.append(f"{relative}: resolved source escapes repository root")
            continue
        except (OSError, UnicodeError) as exc:
            violations.append(f"{relative}: cannot inspect source safely ({exc})")
            continue
        matches = list(DDL_CALL.finditer(content))
        matches.extend(DDL_FILE_CALL.finditer(content))
        for match in sorted(matches, key=lambda item: item.start()):
            line = content.count("\n", 0, match.start()) + 1
            violations.append(f"{relative}:{line}")
    return violations


def find_migration_violations(root: Path) -> list[str]:
    migration_root = root / "backend/migrations"
    if not migration_root.is_dir():
        return ["backend/migrations: migration directory is missing"]

    migrations: list[tuple[int, Path]] = []
    violations: list[str] = []
    current = root
    for component in Path("backend/migrations").parts:
        current /= component
        if current.is_symlink():
            violations.append(
                f"backend/migrations: symlinked path component {current.relative_to(root)}"
            )
            return violations
    for path in sorted(migration_root.iterdir()):
        if path.is_symlink():
            violations.append(f"{path.relative_to(root)}: symlinks are not allowed in backend/migrations")
            continue
        if not path.is_file():
            continue
        match = MIGRATION_NAME.fullmatch(path.name)
        if match is None:
            if path.suffix == ".sql":
                return [f"{path.relative_to(root)}: migration filename must be NNNN_name.sql"]
            continue
        migrations.append((int(match.group(1)), path))

    if not migrations:
        return ["backend/migrations: no numbered SQL migrations found"]

    expected = list(range(1, len(migrations) + 1))
    actual = [number for number, _ in migrations]
    if actual != expected:
        violations.append(
            "backend/migrations: numbered migrations must be contiguous "
            f"starting at 0001 (found {', '.join(f'{number:04d}' for number in actual)})"
        )
        return violations

    for _, path in migrations:
        relative = path.relative_to(root)
        if not tracked(root, relative):
            violations.append(f"{relative}: migration must be tracked")
    return violations


def check(root: Path) -> list[str]:
    return find_ddl_violations(root) + find_migration_violations(root)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    root = args.repo.resolve()
    violations = check(root)
    if violations:
        print(
            "Schema migration ownership check failed; production DDL or migration policy violations:",
            file=sys.stderr,
        )
        for violation in violations:
            print(f"  {violation}", file=sys.stderr)
        return 1
    print("Schema migration ownership check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
