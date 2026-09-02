#!/usr/bin/env python3
"""Report deterministic source inventory for a VoidTower checkout.

This script reports source and Git evidence only. It never inspects runtime
services, databases, environment files, or credential values.
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import resource
import subprocess
import sys
import tempfile
import tomllib
from typing import Any, Iterable


class RepoTruthError(RuntimeError):
    """Raised when a source report cannot be derived safely."""


GIT_OUTPUT_LIMIT = 8 * 1024 * 1024
GIT_TIMEOUT_SECONDS = 10


SUBSYSTEM_GATES: dict[str, dict[str, Any]] = {
    "agent": {
        "evidence_limit": "integration-verified",
        "focused_gates": ["cd backend && cargo test agent:: --all-features"],
        "full_gates": [
            "cd backend && cargo clippy --all-targets --all-features -- -D warnings",
            "cd backend && cargo test --all-targets --all-features",
            "scripts/check-schema-migration-ownership.sh",
            "git diff --check",
        ],
    },
    "backend": {
        "evidence_limit": "unit-verified",
        "focused_gates": ["cd backend && cargo test --all-features"],
        "full_gates": [
            "cd backend && cargo clippy --all-targets --all-features -- -D warnings",
            "cd backend && cargo test --all-targets --all-features",
            "scripts/check-schema-migration-ownership.sh",
            "git diff --check",
        ],
    },
    "frontend": {
        "evidence_limit": "unit-verified",
        "focused_gates": ["cd frontend && npm test"],
        "full_gates": [
            "cd frontend && npm test",
            "cd frontend && npm run type-check",
            "cd frontend && npm run lint",
            "cd frontend && npm run build",
            "git diff --check",
        ],
    },
    "governance": {
        "evidence_limit": "unit-verified",
        "focused_gates": ["python scripts/test_repo_truth.py -v"],
        "full_gates": [
            "python scripts/repo_truth.py --repo . --json --check",
            "scripts/check-repository-hygiene.sh",
            "scripts/check-schema-migration-ownership.sh",
            "git diff --check",
        ],
    },
    "mobile": {
        "evidence_limit": "implemented",
        "focused_gates": ["cd mobile && npm test"],
        "full_gates": ["cd mobile && npm test", "git diff --check"],
    },
    "schema": {
        "evidence_limit": "integration-verified",
        "focused_gates": ["cd backend && cargo test db::tests --all-features"],
        "full_gates": [
            "scripts/check-schema-migration-ownership.sh",
            "cd backend && cargo test --all-targets --all-features",
            "git diff --check",
        ],
    },
    "standalone-mcp": {
        "evidence_limit": "integration-verified",
        "focused_gates": ["python -m unittest discover -s odysseus-mcp-servers/tests -v"],
        "full_gates": [
            "python -m unittest discover -s odysseus-mcp-servers/tests -v",
            "cd backend && cargo test --all-targets --all-features",
            "git diff --check",
        ],
    },
}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd(), help="VoidTower repository root")
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    parser.add_argument("--base", help="literal base commit ID for source-derived CI changes")
    parser.add_argument("--head", help="literal head commit ID for source-derived CI changes")
    parser.add_argument(
        "--check",
        action="store_true",
        help="fail when required source landmarks are missing or unreadable",
    )
    return parser.parse_args()


def run_git(
    repo: Path,
    *args: str,
    check: bool = True,
    input_text: str | None = None,
) -> subprocess.CompletedProcess[bytes]:
    def limit_output_file() -> None:
        resource.setrlimit(resource.RLIMIT_FSIZE, (GIT_OUTPUT_LIMIT, GIT_OUTPUT_LIMIT))

    try:
        with tempfile.TemporaryFile() as output:
            result = subprocess.run(
                ["git", *args],
                cwd=repo,
                check=False,
                input=None if input_text is None else input_text.encode("utf-8"),
                stdout=output,
                stderr=subprocess.DEVNULL,
                timeout=GIT_TIMEOUT_SECONDS,
                preexec_fn=limit_output_file,
            )
            size = output.tell()
            if size > GIT_OUTPUT_LIMIT:
                raise RepoTruthError("git output exceeded the bounded report limit")
            output.seek(0)
            stdout = output.read(GIT_OUTPUT_LIMIT + 1)
    except subprocess.TimeoutExpired as exc:
        raise RepoTruthError("git command exceeded the bounded execution timeout") from exc
    except OSError as exc:
        raise RepoTruthError(f"cannot execute git: {exc}") from exc
    result.stdout = stdout
    result.stderr = b""
    if check and result.returncode != 0:
        raise RepoTruthError(f"git command failed with exit code {result.returncode}")
    return result


def repository_root(path: Path) -> Path:
    try:
        root = path.expanduser().resolve(strict=True)
    except OSError as exc:
        raise RepoTruthError(f"repository path cannot be resolved: {exc}") from exc
    if not root.is_dir():
        raise RepoTruthError(f"repository path is not a directory: {root}")

    landmarks = (root / "backend" / "Cargo.toml", root / "frontend" / "package.json")
    if not (root / ".git").exists() or not all(item.exists() for item in landmarks):
        raise RepoTruthError(f"{root} is not a VoidTower checkout")
    return root


def contained(root: Path, path: Path, *, must_exist: bool = True) -> Path:
    try:
        resolved = path.resolve(strict=must_exist)
    except OSError as exc:
        raise RepoTruthError(f"cannot resolve {path}: {exc}") from exc
    try:
        resolved.relative_to(root)
    except ValueError as exc:
        raise RepoTruthError(f"source path escapes repository: {path} -> {resolved}") from exc
    return resolved


def relative(root: Path, path: Path) -> str:
    return contained(root, path).relative_to(root).as_posix()


def read_text(root: Path, relative_path: str) -> str:
    path = contained(root, root / relative_path)
    if not path.is_file():
        raise RepoTruthError(f"required source landmark is not a file: {relative_path}")
    try:
        return path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise RepoTruthError(f"cannot read {relative_path}: {exc}") from exc


def sorted_files(root: Path, relative_dir: str, patterns: Iterable[str]) -> list[Path]:
    directory = contained(root, root / relative_dir)
    if not directory.is_dir():
        raise RepoTruthError(f"required source landmark is not a directory: {relative_dir}")
    discovered: dict[str, Path] = {}
    for pattern in patterns:
        for candidate in directory.glob(pattern):
            safe = contained(root, candidate)
            if safe.is_file():
                discovered[safe.relative_to(root).as_posix()] = safe
    return [discovered[name] for name in sorted(discovered)]


def parse_status(repo: Path) -> tuple[list[str], list[str], list[str]]:
    payload = run_git(repo, "status", "--porcelain=v1", "-z", "--untracked-files=all").stdout
    records = payload.split(b"\0")
    staged: set[str] = set()
    modified: set[str] = set()
    impact: set[str] = set()
    index = 0
    while index < len(records):
        record = records[index]
        index += 1
        if not record:
            continue
        if len(record) < 3:
            raise RepoTruthError("git status returned an invalid porcelain record")
        x = chr(record[0])
        y = chr(record[1])
        current_path = Path(os.fsdecode(record[3:])).as_posix()
        impact.add(current_path)
        if x in {"R", "C"} or y in {"R", "C"}:
            if index >= len(records) or not records[index]:
                raise RepoTruthError("git status omitted a rename/copy source path")
            source_path = Path(os.fsdecode(records[index])).as_posix()
            index += 1
            impact.add(source_path)
        if x not in {" ", "?"}:
            staged.add(current_path)
        if y != " " or (x == "?" and y == "?"):
            modified.add(current_path)
    return sorted(modified), sorted(staged), sorted(impact)


def git_report(repo: Path) -> dict[str, Any]:
    branch = run_git(repo, "branch", "--show-current").stdout.decode("utf-8", errors="replace").strip()
    head = run_git(repo, "rev-parse", "HEAD").stdout.decode("ascii", errors="strict").strip()
    upstream = run_git(repo, "rev-parse", "--abbrev-ref", "@{upstream}", check=False)
    ahead: int | None = None
    behind: int | None = None
    if upstream.returncode == 0:
        counts = run_git(repo, "rev-list", "--left-right", "--count", "HEAD...@{upstream}")
        parts = counts.stdout.decode("ascii", errors="strict").split()
        if len(parts) != 2:
            raise RepoTruthError("cannot derive Git ahead/behind counts")
        ahead, behind = int(parts[0]), int(parts[1])
    modified, staged, impact = parse_status(repo)
    return {
        "ahead": ahead,
        "behind": behind,
        "branch": branch or "DETACHED",
        "head": head,
        "impact_paths": impact,
        "modified_paths": modified,
        "path_semantics": {
            "impact_paths": "all source and destination paths used for gate mapping",
            "modified_paths": "current worktree paths",
            "staged_paths": "current index paths",
        },
        "staged_paths": staged,
    }


def subsystem_for_path(path: str) -> str:
    if path.startswith("backend/migrations/"):
        return "schema"
    if path.startswith("backend/src/agent/"):
        return "agent"
    if path.startswith("backend/"):
        return "backend"
    if path.startswith("frontend/"):
        return "frontend"
    if path.startswith("mobile/"):
        return "mobile"
    if path.startswith("odysseus-mcp-servers/"):
        return "standalone-mcp"
    return "governance"


def changed_subsystems(paths: Iterable[str]) -> list[dict[str, Any]]:
    paths_by_name: dict[str, list[str]] = {}
    for path in sorted(set(paths)):
        paths_by_name.setdefault(subsystem_for_path(path), []).append(path)

    return [
        {"name": name, "paths": paths_by_name[name], **SUBSYSTEM_GATES[name]}
        for name in sorted(paths_by_name)
    ]


def validate_commit(repo: Path, value: str, label: str) -> str:
    object_format = (
        run_git(repo, "rev-parse", "--show-object-format")
        .stdout.decode("ascii", errors="strict")
        .strip()
    )
    commit_width = {"sha1": 40, "sha256": 64}.get(object_format)
    if commit_width is None:
        raise RepoTruthError(f"unsupported Git object format: {object_format}")
    if not re.fullmatch(rf"[0-9a-fA-F]{{{commit_width}}}", value):
        raise RepoTruthError(f"{label} must be a literal commit ID")
    normalized = value.lower()
    if set(normalized) == {"0"} and label == "base commit":
        return normalized
    if run_git(repo, "cat-file", "-e", f"{normalized}^{{commit}}", check=False).returncode != 0:
        raise RepoTruthError(f"{label} is not an available commit")
    return normalized


def parse_name_status(payload: bytes) -> list[str]:
    records = payload.split(b"\0")
    paths: set[str] = set()
    index = 0
    while index < len(records):
        status = records[index]
        index += 1
        if not status:
            continue
        kind = chr(status[0])
        path_count = 2 if kind in {"R", "C"} else 1
        if index + path_count > len(records):
            raise RepoTruthError("git diff returned an invalid name-status record")
        for raw_path in records[index : index + path_count]:
            if not raw_path:
                raise RepoTruthError("git diff returned an empty changed path")
            paths.add(Path(os.fsdecode(raw_path)).as_posix())
        index += path_count
    return sorted(paths)


def commit_range_paths(repo: Path, base: str, head: str) -> tuple[str, str, list[str]]:
    base_commit = validate_commit(repo, base, "base commit")
    head_commit = validate_commit(repo, head, "head commit")
    if set(base_commit) == {"0"}:
        empty_tree = run_git(
            repo, "hash-object", "-t", "tree", "--stdin", input_text=""
        ).stdout.decode("ascii", errors="strict").strip()
        payload = run_git(
            repo,
            "diff",
            "--name-status",
            "-z",
            "--find-renames",
            "--find-copies",
            empty_tree,
            head_commit,
            "--",
        ).stdout
    else:
        payload = run_git(
            repo,
            "diff",
            "--name-status",
            "-z",
            "--find-renames",
            "--find-copies",
            base_commit,
            head_commit,
            "--",
        ).stdout
    return base_commit, head_commit, parse_name_status(payload)


def newest_handoff(root: Path) -> str | None:
    directory = root / "docs/internal/handoffs"
    if not directory.exists() and not directory.is_symlink():
        return None
    dated: list[tuple[str, str]] = []
    for path in sorted_files(root, "docs/internal/handoffs", ("*.md",)):
        match = re.match(r"^(\d{4}-\d{2}-\d{2})-.*\.md$", path.name)
        if match:
            dated.append((match.group(1), path.relative_to(root).as_posix()))
    return max(dated)[1] if dated else None


def source_region(text: str, start_marker: str, end_markers: tuple[str, ...]) -> str:
    try:
        start = text.index(start_marker)
    except ValueError as exc:
        raise RepoTruthError(f"missing source marker: {start_marker}") from exc
    ends = [position for marker in end_markers if (position := text.find(marker, start + len(start_marker))) >= 0]
    return text[start : min(ends) if ends else len(text)]


def action_registry_counts(root: Path) -> tuple[int, int]:
    text = read_text(root, "backend/src/action_registry.rs")
    routes = source_region(text, "pub const ROUTES", ("pub const ACTIONS", "macro_rules! action_metadata"))
    actions = source_region(text, "pub const ACTIONS", ("\n];",))
    route_count = len(re.findall(r"(?m)^\s*(?:route_metadata|operation_route_metadata)!\s*\(", routes))
    action_count = len(re.findall(r"(?m)^\s*[A-Za-z_][A-Za-z0-9_]*!\s*\(", actions))
    if route_count == 0 or action_count == 0:
        raise RepoTruthError("route/action counts cannot be derived from action_registry.rs")
    return route_count, action_count


def built_in_mcp_tool_count(root: Path) -> int:
    text = read_text(root, "backend/src/api/mcp.rs")
    tools = source_region(
        text,
        "fn handle_tools_list",
        ("fn handle_tool_call", "pub async fn invoke_tool", "// tools/call"),
    )
    count = len(re.findall(r'"name"\s*:\s*"[A-Za-z0-9_.-]+"', tools))
    if count == 0:
        raise RepoTruthError("built-in MCP tool count cannot be derived from backend/src/api/mcp.rs")
    return count


def package_versions(root: Path) -> dict[str, str]:
    try:
        backend = tomllib.loads(read_text(root, "backend/Cargo.toml"))["package"]["version"]
        frontend = json.loads(read_text(root, "frontend/package.json"))["version"]
        mobile = json.loads(read_text(root, "mobile/package.json"))["version"]
    except (KeyError, TypeError, json.JSONDecodeError, tomllib.TOMLDecodeError) as exc:
        raise RepoTruthError(f"cannot derive package versions: {exc}") from exc
    versions = {"backend": str(backend), "frontend": str(frontend), "mobile": str(mobile)}
    if any(not value for value in versions.values()):
        raise RepoTruthError("one or more package versions are empty")
    return versions


def build_report(
    root: Path,
    *,
    base_commit: str | None = None,
    head_commit: str | None = None,
) -> dict[str, Any]:
    migrations = [relative(root, path) for path in sorted_files(root, "backend/migrations", ("*.sql",))]
    integration_tests = [relative(root, path) for path in sorted_files(root, "backend/tests", ("*.rs",))]
    app_vault = sorted_files(root, "app-vault/apps", ("*.yml", "*.yaml"))
    standalone_mcp = sorted_files(root, "odysseus-mcp-servers", ("*_server.py",))
    route_count, action_count = action_registry_counts(root)

    errors: list[str] = []
    if not migrations:
        errors.append("no backend migrations found")
    if not integration_tests:
        errors.append("no backend integration-test Rust files found")

    git = git_report(root)
    if base_commit is not None and head_commit is not None:
        base, head, impact_paths = commit_range_paths(root, base_commit, head_commit)
        change_source: dict[str, Any] = {
            "base_commit": base,
            "head_commit": head,
            "mode": "commit_range",
        }
    else:
        impact_paths = git["impact_paths"]
        change_source = {"mode": "worktree"}
    return {
        "app_vault_yaml_count": len(app_vault),
        "backend_integration_test_files": integration_tests,
        "built_in_mcp_tool_count": built_in_mcp_tool_count(root),
        "change_source": change_source,
        "changed_subsystems": changed_subsystems(impact_paths),
        "check": {"errors": errors, "status": "passed" if not errors else "failed"},
        "evidence_scope": "source_inventory_only",
        "git": git,
        "migrations": migrations,
        "newest_handoff": newest_handoff(root),
        "route_metadata_count": route_count,
        "runtime_support_claimed": False,
        "standalone_mcp_server_count": len(standalone_mcp),
        "structured_action_count": action_count,
        "versions": package_versions(root),
    }


def render_text(report: dict[str, Any]) -> str:
    git = report["git"]
    return "\n".join(
        [
            "VoidTower repository truth (source inventory only)",
            f"branch: {git['branch']}",
            f"head: {git['head']}",
            f"ahead/behind: {git['ahead']}/{git['behind']}",
            f"newest handoff: {report['newest_handoff']}",
            f"route metadata: {report['route_metadata_count']}",
            f"structured actions: {report['structured_action_count']}",
            f"built-in MCP tools: {report['built_in_mcp_tool_count']}",
            f"standalone MCP servers: {report['standalone_mcp_server_count']}",
            f"App Vault YAML files: {report['app_vault_yaml_count']}",
            f"check: {report['check']['status']}",
            "runtime support claimed: no",
        ]
    )


def main() -> int:
    args = parse_args()
    try:
        if (args.base is None) != (args.head is None):
            raise RepoTruthError("--base and --head must be provided together")
        root = repository_root(args.repo)
        report = build_report(
            root,
            base_commit=args.base,
            head_commit=args.head,
        )
    except RepoTruthError as exc:
        print(f"repo_truth: {exc}", file=sys.stderr)
        return 2

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(render_text(report))

    if args.check and report["check"]["status"] != "passed":
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
