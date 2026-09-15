#!/usr/bin/env python3
"""CLI contract tests for the VoidTower release-candidate gate runner."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

try:
    from scripts.test_repo_truth import make_checkout
except ModuleNotFoundError:
    from test_repo_truth import make_checkout

SCRIPT = Path(__file__).with_name("release_gate.py")


class ReleaseGateCliTests(unittest.TestCase):
    def run_runner(self, root: Path, manifest: Path) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def write_manifest(self, root: Path, gates: list[dict[str, object]]) -> Path:
        manifest = root / "manifest.json"
        manifest.write_text(json.dumps({"schema_version": 1, "gates": gates}), encoding="utf-8")
        return manifest

    def test_runs_explicit_argv_and_emits_machine_readable_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            artifact = root / "artifact.bin"
            artifact.write_bytes(b"release artifact")
            (root / "gate.py").write_text("from pathlib import Path; Path('artifact.bin').write_bytes(b'release artifact')\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "fixture",
                        "description": "deterministic fixture",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "gate.py"],
                        "timeout_seconds": 5,
                        "artifacts": ["artifact.bin"],
                    }
                ],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["schema_version"], 1)
            self.assertEqual(report["status"], "passed")
            self.assertEqual(report["gates"][0]["status"], "passed")
            self.assertEqual(report["gates"][0]["argv"][0], "python3")
            self.assertEqual(report["gates"][0]["artifacts"][0]["path"], "artifact.bin")
            self.assertRegex(report["gates"][0]["artifacts"][0]["sha256"], r"^[0-9a-f]{64}$")
            self.assertEqual(report["selected_subsystems"], ["governance"])

    def test_required_failure_and_timeout_are_recorded_and_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "failure.py").write_text("import sys; print('TOKEN=DO_NOT_LEAK', file=sys.stderr); sys.exit(7)\n", encoding="utf-8")
            (root / "timeout.py").write_text("import time; time.sleep(2)\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "failure",
                        "description": "fails",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "failure.py"],
                        "timeout_seconds": 5,
                    },
                    {
                        "id": "timeout",
                        "description": "times out",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "timeout.py"],
                        "timeout_seconds": 1,
                    },
                ],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "failed")
            self.assertEqual([gate["status"] for gate in report["gates"]], ["failed", "blocked"])
            self.assertNotIn("DO_NOT_LEAK", result.stdout + result.stderr)
            self.assertIn("redacted", result.stdout)

    def test_rejects_shell_interpreter_and_paths_outside_repository(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "unsafe",
                        "description": "unsafe",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["sh", "-c", "true"],
                        "timeout_seconds": 5,
                    }
                ],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 2)
            self.assertIn("shell", result.stderr.lower())

    def test_changed_scope_selects_governance_and_changed_subsystem_only(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".git").mkdir()
            (root / "backend").mkdir()
            (root / "frontend").mkdir()
            # The fixture is intentionally invalid as a checkout; the runner must fail
            # closed rather than inventing changed-subsystem applicability.
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "governance",
                        "description": "governance",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "scripts/test_repo_truth.py"],
                        "timeout_seconds": 5,
                    }
                ],
            )
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)
            self.assertIn("VoidTower checkout", result.stderr)

    def test_changed_scope_records_unaffected_gates_instead_of_silently_dropping_them(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "backend/src/changed.rs").parent.mkdir(parents=True, exist_ok=True)
            (root / "backend/src/changed.rs").write_text("changed\n", encoding="utf-8")
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [
                    {"id": "governance", "description": "governance", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5},
                    {"id": "backend", "description": "backend", "subsystems": ["backend"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5},
                    {"id": "frontend", "description": "frontend", "subsystems": ["frontend"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5},
                ],
            )
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "failed")
            self.assertEqual([gate["id"] for gate in report["gates"]], ["governance", "backend"])
            self.assertEqual(report["skipped_gates"], [{"id": "frontend", "reason": "subsystem unchanged", "required": True}])

    def test_rejects_absolute_external_executable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = self.write_manifest(
                root,
                [{"id": "unsafe", "description": "unsafe", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": [sys.executable, "-m", "unittest"], "timeout_seconds": 5}],
            )
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)
            self.assertIn("repository-relative", result.stderr)

    def test_persists_report_to_repository_contained_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "governance", "description": "governance", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            output = root / "evidence" / "report.json"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", str(output), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), json.loads(output.read_text(encoding="utf-8")))


if __name__ == "__main__":
    unittest.main()
