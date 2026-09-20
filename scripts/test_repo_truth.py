#!/usr/bin/env python3
"""CLI behavior tests for the repository-native VoidTower source inventory."""

from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("repo_truth.py")


def write(root: Path, relative: str, content: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=root,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return result.stdout.strip()


def make_checkout(root: Path, *, object_format: str | None = None) -> None:
    write(root, "AGENTS.md", "# VoidTower Development Instructions\n")
    write(root, "README.md", "# VoidTower\n")
    write(root, "backend/Cargo.toml", '[package]\nname = "voidtower"\nversion = "0.9.1"\n')
    write(root, "frontend/package.json", '{"name":"voidtower-frontend","version":"0.9.2"}\n')
    write(root, "mobile/package.json", '{"name":"mobile","version":"1.0.1"}\n')
    write(
        root,
        "backend/src/action_registry.rs",
        """pub const ROUTES: &[RouteMetadata] = &[
    route_metadata!(Get, "/one"),
    operation_route_metadata!(Post, "/two"),
];

pub const ACTIONS: &[ActionMetadata] = &[
    action_metadata!("one"),
    durable_mutation!("two"),
];
""",
    )
    write(
        root,
        "backend/src/api/mcp.rs",
        """fn handle_tools_list() {
    let tools = [
        { "name": "alpha", "description": "a" },
        { "name": "beta", "description": "b" },
    ];
}

// tools/call
pub async fn invoke_tool(name: &str) {}
""",
    )
    write(root, "backend/migrations/0001_first.sql", "SELECT 1;\n")
    write(root, "backend/tests/golden_path.rs", "#[test]\nfn golden_path() {}\n")
    write(root, "app-vault/apps/alpha.yml", "name: alpha\n")
    write(root, "odysseus-mcp-servers/alpha_server.py", "# server\n")
    write(root, "docs/internal/handoffs/2026-09-01-new.md", "# New\n")
    write(root, "scripts/check-schema-migration-ownership.sh", "#!/bin/sh\nexit 0\n")
    write(root, "scripts/check-repository-hygiene.sh", "#!/bin/sh\nexit 0\n")

    init_args = ["init", "-b", "dev"]
    if object_format is not None:
        init_args.append(f"--object-format={object_format}")
    git(root, *init_args)
    git(root, "config", "user.name", "Repo Truth Test")
    git(root, "config", "user.email", "repo-truth@example.invalid")
    git(root, "add", ".")
    git(root, "commit", "-m", "fixture")


class RepoTruthCliTests(unittest.TestCase):
    def run_script(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), *args],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def test_ci_runs_tests_then_commit_report_then_hygiene_as_distinct_steps(self) -> None:
        workflow = (SCRIPT.parent.parent / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        lines = workflow.splitlines()
        job_start = lines.index("  repository-hygiene:")
        job_end = next(
            index
            for index in range(job_start + 1, len(lines))
            if lines[index].startswith("  ") and not lines[index].startswith("    ")
        )
        steps: list[tuple[str, str]] = []
        current_name = ""
        for line in lines[job_start:job_end]:
            stripped = line.strip()
            if stripped.startswith("- name: "):
                current_name = stripped.removeprefix("- name: ")
            elif stripped.startswith("run: "):
                steps.append((current_name, stripped.removeprefix("run: ")))

        commands = [command for _, command in steps]
        self.assertEqual(
            commands[:4],
            [
                "python3 scripts/test_repo_truth.py -v",
                "python3 -m unittest scripts.test_repository_prerequisites -v",
                (
                    "python3 scripts/repo_truth.py --repo . --json --check "
                    '--base "$BASE_COMMIT" --head "$HEAD_COMMIT"'
                ),
                "bash scripts/check-repository-hygiene.sh",
            ],
        )
        self.assertIn("BASE_COMMIT: ${{ github.event.pull_request.base.sha || github.event.before }}", workflow)
        self.assertIn("HEAD_COMMIT: ${{ github.event.pull_request.head.sha || github.sha }}", workflow)

    def test_json_check_reports_source_inventory_and_git_state(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            write(repo, "AGENTS.md", "# VoidTower Development Instructions\nmodified\n")
            write(repo, "README.md", "# VoidTower\nstaged\n")
            git(repo, "add", "README.md")
            write(repo, "untracked.txt", "new\n")

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["evidence_scope"], "source_inventory_only")
            self.assertFalse(report["runtime_support_claimed"])
            self.assertEqual(report["git"]["branch"], "dev")
            self.assertEqual(report["git"]["staged_paths"], ["README.md"])
            self.assertEqual(report["git"]["modified_paths"], ["AGENTS.md", "untracked.txt"])
            self.assertEqual(report["newest_handoff"], "docs/internal/handoffs/2026-09-01-new.md")
            self.assertEqual(report["route_metadata_count"], 2)
            self.assertEqual(report["structured_action_count"], 2)
            self.assertEqual(report["built_in_mcp_tool_count"], 2)
            self.assertEqual(report["standalone_mcp_server_count"], 1)
            self.assertEqual(report["app_vault_yaml_count"], 1)
            self.assertEqual(report["backend_integration_test_files"], ["backend/tests/golden_path.rs"])
            self.assertEqual(
                report["versions"],
                {"backend": "0.9.1", "frontend": "0.9.2", "mobile": "1.0.1"},
            )
            self.assertEqual(report["check"], {"errors": [], "status": "passed"})

    def test_check_fails_when_required_inventory_landmark_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            (repo / "backend/src/api/mcp.rs").unlink()

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 2)
            self.assertIn("cannot resolve", result.stderr)
            self.assertNotIn("Traceback", result.stderr)

    def test_check_rejects_a_non_voidtower_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = self.run_script("--repo", temporary, "--json", "--check")

            self.assertEqual(result.returncode, 2)
            self.assertIn("not a VoidTower checkout", result.stderr)

    def test_check_rejects_a_discovered_source_symlink_outside_the_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            repo = base / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            outside = base / "outside_server.py"
            outside.write_text("# outside\n", encoding="utf-8")
            server = repo / "odysseus-mcp-servers/alpha_server.py"
            server.unlink()
            server.symlink_to(outside)

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 2)
            self.assertIn("escapes repository", result.stderr)

    def test_absent_ignored_handoff_directory_reports_null(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            shutil.rmtree(repo / "docs/internal/handoffs")

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIsNone(json.loads(result.stdout)["newest_handoff"])

    def test_handoff_directory_symlink_outside_checkout_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            repo = base / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            shutil.rmtree(repo / "docs/internal/handoffs")
            outside = base / "handoffs"
            outside.mkdir()
            (repo / "docs/internal/handoffs").symlink_to(outside, target_is_directory=True)

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 2)
            self.assertIn("escapes repository", result.stderr)

    def test_changed_agent_paths_map_to_exact_verification_gates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            write(repo, "backend/src/agent/collector.rs", "pub fn collect() {}\n")
            git(repo, "add", "backend/src/agent/collector.rs")

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(
                report["changed_subsystems"],
                [
                    {
                        "evidence_limit": "integration-verified",
                        "focused_gates": [
                            "cd backend && cargo test agent:: --all-features",
                        ],
                        "full_gates": [
                            "cd backend && cargo clippy --all-targets --all-features -- -D warnings",
                            "cd backend && cargo test --all-targets --all-features",
                            "scripts/check-schema-migration-ownership.sh",
                            "git diff --check",
                        ],
                        "name": "agent",
                        "paths": ["backend/src/agent/collector.rs"],
                    }
                ],
            )

    def test_staged_cross_subsystem_rename_maps_source_and_destination_gates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            write(repo, "backend/src/agent/collector.rs", "pub fn collect() {}\n")
            git(repo, "add", "backend/src/agent/collector.rs")
            git(repo, "commit", "-m", "add collector")
            git(repo, "mv", "backend/src/agent/collector.rs", "frontend/collector.rs")

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["git"]["staged_paths"], ["frontend/collector.rs"])
            self.assertEqual(report["git"]["modified_paths"], [])
            self.assertEqual(
                [(entry["name"], entry["paths"]) for entry in report["changed_subsystems"]],
                [
                    ("agent", ["backend/src/agent/collector.rs"]),
                    ("frontend", ["frontend/collector.rs"]),
                ],
            )

    def test_worktree_only_rename_status_maps_source_and_destination_gates(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path.cwd()) as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            wrapper_dir = Path(temporary) / "bin"
            wrapper_dir.mkdir()
            wrapper = wrapper_dir / "git"
            wrapper.write_text(
                """#!/bin/sh
if [ "$1" = "status" ]; then
    printf ' R frontend/collector.rs\\0backend/src/agent/collector.rs\\0'
    exit 0
fi
exec "$REAL_GIT" "$@"
""",
                encoding="utf-8",
            )
            wrapper.chmod(0o755)
            environment = os.environ.copy()
            environment["REAL_GIT"] = shutil.which("git") or "git"
            environment["PATH"] = f"{wrapper_dir}:{environment['PATH']}"

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(repo), "--json", "--check"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=environment,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["git"]["staged_paths"], [])
            self.assertEqual(report["git"]["modified_paths"], ["frontend/collector.rs"])
            self.assertEqual(
                [(entry["name"], entry["paths"]) for entry in report["changed_subsystems"]],
                [
                    ("agent", ["backend/src/agent/collector.rs"]),
                    ("frontend", ["frontend/collector.rs"]),
                ],
            )

    def test_report_does_not_execute_repository_hygiene(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            write(
                repo,
                "scripts/check-repository-hygiene.sh",
                "#!/bin/sh\nprintf 'credential=R0_01_SECRET_SENTINEL\\n' >&2\nsleep 30\nexit 1\n",
            )

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(repo), "--json", "--check"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=3,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["check"], {"errors": [], "status": "passed"})
            self.assertNotIn("gates", report)
            self.assertNotIn("R0_01_SECRET_SENTINEL", result.stdout + result.stderr)

    def test_git_output_is_bounded_and_child_diagnostics_are_not_emitted(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path.cwd()) as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            wrapper_dir = Path(temporary) / "bin"
            wrapper_dir.mkdir()
            wrapper = wrapper_dir / "git"
            wrapper.write_text(
                """#!/bin/sh
if [ "$1" = "status" ]; then
    printf 'R0_01_SECRET_SENTINEL' >&2
    exec python3 -c 'import sys; sys.stdout.write("x" * (9 * 1024 * 1024))'
fi
exec "$REAL_GIT" "$@"
""",
                encoding="utf-8",
            )
            wrapper.chmod(0o755)
            environment = os.environ.copy()
            environment["REAL_GIT"] = shutil.which("git") or "git"
            environment["PATH"] = f"{wrapper_dir}:{environment['PATH']}"

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(repo), "--json", "--check"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=environment,
                timeout=5,
            )

            self.assertEqual(result.returncode, 2)
            self.assertLess(len(result.stdout + result.stderr), 1024)
            self.assertNotIn("R0_01_SECRET_SENTINEL", result.stdout + result.stderr)

    def test_clean_checkout_commit_range_maps_source_changes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            base = git(repo, "rev-parse", "HEAD")
            write(repo, "backend/src/agent/collector.rs", "pub fn collect() {}\n")
            git(repo, "add", "backend/src/agent/collector.rs")
            git(repo, "commit", "-m", "add collector")
            head = git(repo, "rev-parse", "HEAD")

            result = self.run_script(
                "--repo",
                str(repo),
                "--json",
                "--check",
                "--base",
                base,
                "--head",
                head,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["git"]["modified_paths"], [])
            self.assertEqual(report["git"]["staged_paths"], [])
            self.assertEqual(
                report["change_source"],
                {"base_commit": base, "head_commit": head, "mode": "commit_range"},
            )
            self.assertEqual(
                [(entry["name"], entry["paths"]) for entry in report["changed_subsystems"]],
                [("agent", ["backend/src/agent/collector.rs"])],
            )

    def test_sha256_commit_range_requires_full_width_literal_ids(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo, object_format="sha256")
            base = git(repo, "rev-parse", "HEAD")
            write(repo, "backend/src/agent/collector.rs", "pub fn collect() {}\n")
            git(repo, "add", "backend/src/agent/collector.rs")
            git(repo, "commit", "-m", "add collector")
            head = git(repo, "rev-parse", "HEAD")

            abbreviated = self.run_script(
                "--repo",
                str(repo),
                "--json",
                "--base",
                base,
                "--head",
                head[:40],
            )
            full_range = self.run_script(
                "--repo",
                str(repo),
                "--json",
                "--check",
                "--base",
                base,
                "--head",
                head,
            )
            new_branch = self.run_script(
                "--repo",
                str(repo),
                "--json",
                "--check",
                "--base",
                "0" * 64,
                "--head",
                head,
            )

            self.assertEqual(abbreviated.returncode, 2)
            self.assertIn("literal commit ID", abbreviated.stderr)
            self.assertEqual(full_range.returncode, 0, full_range.stderr)
            self.assertEqual(
                json.loads(full_range.stdout)["change_source"],
                {"base_commit": base, "head_commit": head, "mode": "commit_range"},
            )
            self.assertEqual(new_branch.returncode, 0, new_branch.stderr)
            self.assertEqual(
                json.loads(new_branch.stdout)["change_source"],
                {"base_commit": "0" * 64, "head_commit": head, "mode": "commit_range"},
            )

    def test_all_zero_new_branch_range_maps_every_path_in_head_tree(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            write(repo, "backend/src/agent/collector.rs", "pub fn collect() {}\n")
            git(repo, "add", "backend/src/agent/collector.rs")
            git(repo, "commit", "-m", "add collector")
            write(repo, "frontend/src/new_branch.ts", "export const newBranch = true;\n")
            git(repo, "add", "frontend/src/new_branch.ts")
            git(repo, "commit", "-m", "add frontend path")
            head = git(repo, "rev-parse", "HEAD")

            result = self.run_script(
                "--repo",
                str(repo),
                "--json",
                "--check",
                "--base",
                "0" * 40,
                "--head",
                head,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(
                [(entry["name"], entry["paths"]) for entry in report["changed_subsystems"]],
                [
                    ("agent", ["backend/src/agent/collector.rs"]),
                    (
                        "backend",
                        [
                            "backend/Cargo.toml",
                            "backend/src/action_registry.rs",
                            "backend/src/api/mcp.rs",
                            "backend/tests/golden_path.rs",
                        ],
                    ),
                    (
                        "frontend",
                        ["frontend/package.json", "frontend/src/new_branch.ts"],
                    ),
                    (
                        "governance",
                        [
                            "AGENTS.md",
                            "README.md",
                            "app-vault/apps/alpha.yml",
                            "docs/internal/handoffs/2026-09-01-new.md",
                            "scripts/check-repository-hygiene.sh",
                            "scripts/check-schema-migration-ownership.sh",
                        ],
                    ),
                    ("mobile", ["mobile/package.json"]),
                    ("schema", ["backend/migrations/0001_first.sql"]),
                    (
                        "standalone-mcp",
                        ["odysseus-mcp-servers/alpha_server.py"],
                    ),
                ],
            )

    def test_commit_range_rejects_nonliteral_revision_names(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repo = Path(temporary) / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            head = git(repo, "rev-parse", "HEAD")

            result = self.run_script(
                "--repo",
                str(repo),
                "--json",
                "--base",
                "HEAD~1",
                "--head",
                head,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("literal commit ID", result.stderr)

    def test_json_is_byte_identical_after_filesystem_order_is_shuffled(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            source = base / "source"
            source.mkdir()
            make_checkout(source)
            first = base / "first"
            second = base / "second"
            shutil.copytree(source, first)
            shutil.copytree(source, second)

            relative_paths = [
                "app-vault/apps/alpha.yml",
                "backend/migrations/0001_first.sql",
                "docs/internal/handoffs/2026-09-01-new.md",
                "odysseus-mcp-servers/alpha_server.py",
            ]
            contents = {path: (first / path).read_bytes() for path in relative_paths}
            for root, order in ((first, relative_paths), (second, reversed(relative_paths))):
                for path in relative_paths:
                    (root / path).unlink()
                for path in order:
                    target = root / path
                    target.write_bytes(contents[path])

            first_result = self.run_script("--repo", str(first), "--json", "--check")
            second_result = self.run_script("--repo", str(second), "--json", "--check")

            self.assertEqual(first_result.returncode, 0, first_result.stderr)
            self.assertEqual(second_result.returncode, 0, second_result.stderr)
            self.assertEqual(first_result.stdout, second_result.stdout)

    def test_report_does_not_read_or_emit_credential_file_contents(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            repo = base / "voidtower"
            repo.mkdir()
            make_checkout(repo)
            secret_value = "R0_01_SENTINEL_SECRET_MUST_NOT_APPEAR"
            outside = base / "credential-value"
            outside.write_text(secret_value, encoding="utf-8")
            (repo / ".env").symlink_to(outside)

            result = self.run_script("--repo", str(repo), "--json", "--check")

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn(secret_value, result.stdout)
            self.assertNotIn(secret_value, result.stderr)
            report = json.loads(result.stdout)
            self.assertIn(".env", report["git"]["modified_paths"])


if __name__ == "__main__":
    unittest.main()
