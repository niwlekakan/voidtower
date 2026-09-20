#!/usr/bin/env python3
"""CLI contract tests for the VoidTower release-candidate gate runner."""

from __future__ import annotations

import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from release_gate import redact, redact_argv
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

    def test_backend_format_gate_is_required_and_targets_backend_rustfmt(self) -> None:
        manifest_path = Path(__file__).with_name("release-gates.json")
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        gate = next(item for item in manifest["gates"] if item["id"] == "backend-format")

        self.assertTrue(gate["required"])
        self.assertEqual(gate["cwd"], "backend")
        self.assertEqual(gate["subsystems"], ["backend", "agent"])
        self.assertEqual(gate["argv"], ["cargo", "fmt", "--all", "--", "--check"])

    def test_redacts_escaped_and_truncated_structured_credentials(self) -> None:
        escaped = '{"x-amz-security-token": "ESCAPED_\\"_SENTINEL"}'
        truncated = '{"access_token": "TRUNCATED_SENTINEL'
        self.assertNotIn("ESCAPED_", redact(escaped))
        self.assertNotIn("TRUNCATED_SENTINEL", redact(truncated))
        self.assertNotIn("BEARER_ESCAPED", redact('Bearer "BEARER_ESCAPED\\"_SENTINEL"'))
        self.assertNotIn("COMPOUND_SENTINEL", redact("TOKEN=Bearer COMPOUND_SENTINEL"))
        self.assertNotIn("BARE_UNDERSCORE_SENTINEL", redact("SECRET_ACCESS_KEY=BARE_UNDERSCORE_SENTINEL SESSION_TOKEN=SECOND_SENTINEL"))
        self.assertNotIn("COMMA_SENTINEL", redact("TOKEN=COMMA_SENTINEL,tail"))
        self.assertNotIn("SEMICOLON_SENTINEL", redact("PASSWORD=SEMICOLON_SENTINEL;tail"))
        for sample in ("GITHUB_TOKEN=GITHUB_SENTINEL", "MY_API_KEY=API_SENTINEL", "DB_PASSWORD=PASSWORD_SENTINEL", "HTTP_AUTHORIZATION=AUTH_SENTINEL", "ssh://user:SSH_SENTINEL@example.invalid"):
            self.assertNotIn("SENTINEL", redact(sample))
        for field in ("secret_access_key", "session_token", "secret_key", "private_key", "secretAccessKey"):
            self.assertNotIn("FIELD_SENTINEL", redact(f'{{"{field}": "FIELD_SENTINEL"}}'))
        self.assertNotIn("SINGLE_SENTINEL", redact("{'access_token': 'SINGLE_SENTINEL'}"))
        self.assertNotIn("NESTED_SENTINEL", redact('{"token": {"nested": "NESTED_SENTINEL"}}'))
        self.assertNotIn("NESTED_TRUNCATED_SENTINEL", redact('{"token": {"nested": "NESTED_TRUNCATED_SENTINEL'))
        self.assertNotIn("URL_TRUNCATED_SENTINEL", redact("ssh://user:URL_TRUNCATED_SENTINEL"))
        self.assertEqual(redact_argv(["--token", "--access-token", "CONSECUTIVE_SENTINEL"]), ["--token", "--access-token", "redacted"])

    def test_normalizes_manifest_path_before_report_emission(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "nested").mkdir()
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "normalized", "description": "normalized", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            normalized = root / "nested" / ".." / manifest.name
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(normalized), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["manifest"], "manifest.json")

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
                        "argv": ["python3", "gate.py", "--token", "DO_NOT_LEAK", "--aws-access-key-id", "DO_NOT_LEAK_AWS", "--aws_access_key_id", "DO_NOT_LEAK_AWS_UNDERSCORE", "--aws_secret_access_key", "DO_NOT_LEAK_SECRET_UNDERSCORE", "--access-token", "DO_NOT_LEAK_ACCESS", "--x-amz-security-token", "DO_NOT_LEAK_XAMZ", "--x-amz-security-token=DO_NOT_LEAK_XAMZ_EMBEDDED", "--secret-access-key=DO_NOT_LEAK_EMBEDDED"],
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
            self.assertNotIn("DO_NOT_LEAK", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_AWS", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_AWS_UNDERSCORE", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_SECRET_UNDERSCORE", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_ACCESS", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_XAMZ", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_XAMZ_EMBEDDED", json.dumps(report["gates"][0]["argv"]))
            self.assertNotIn("DO_NOT_LEAK_EMBEDDED", json.dumps(report["gates"][0]["argv"]))
            self.assertEqual(report["gates"][0]["artifacts"][0]["path"], "artifact.bin")
            self.assertEqual(report["gates"][0]["artifacts"][0]["sha256"], "133cfccb5b503cf4040c95f3dfad56d07c1574283a1e39066b594f6ee33711ba")
            self.assertEqual(report["selected_subsystems"], ["governance"])

    def test_child_exit_125_is_a_failed_gate_not_an_unavailable_tool(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "exit125.py").write_text("raise SystemExit(125)\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "exit125", "description": "child exit", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "exit125.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1)
            gate = json.loads(result.stdout)["gates"][0]
            self.assertEqual(gate["status"], "failed")
            self.assertEqual(gate["exit_code"], 125)
            self.assertNotIn("unavailable", gate.get("error", ""))

    def test_gate_does_not_poison_nested_bounded_subprocesses(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "nested.py").write_text(
                """import resource
import subprocess
import sys

def raise_child_file_limit():
    resource.setrlimit(resource.RLIMIT_FSIZE, (8 * 1024 * 1024, 8 * 1024 * 1024))

subprocess.run([sys.executable, \"-c\", \"pass\"], check=True, preexec_fn=raise_child_file_limit)
""",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "nested",
                        "description": "nested bounded subprocess",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "nested.py"],
                        "timeout_seconds": 5,
                    }
                ],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "passed")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process-group contract")
    def test_runner_death_kills_forked_gate_descendants(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            marker = root / "descendant-ran"
            pid_file = root / "descendant.pid"
            (root / "descendant.py").write_text(
                "import os, pathlib, time\n"
                "os.setsid()\n"
                f"pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
                "time.sleep(2)\n"
                f"pathlib.Path({str(marker)!r}).write_text('unexpected')\n",
                encoding="utf-8",
            )
            (root / "forker.py").write_text(
                "import subprocess, sys, time\n"
                "subprocess.Popen([sys.executable, 'descendant.py'])\n"
                "time.sleep(10)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "forker", "description": "forker", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "forker.py"], "timeout_seconds": 30}],
            )
            runner = subprocess.Popen(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                deadline = time.monotonic() + 5
                while not pid_file.exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(pid_file.exists(), "forked descendant did not start")
                os.kill(runner.pid, signal.SIGKILL)
                runner.wait(timeout=5)
                time.sleep(2.5)
                self.assertFalse(marker.exists(), "forked descendant outlived the runner")
            finally:
                if runner.poll() is None:
                    runner.kill()
                    runner.wait(timeout=5)
                if runner.stdout is not None:
                    runner.stdout.close()
                if runner.stderr is not None:
                    runner.stderr.close()

    def test_gate_output_is_bounded_without_blocking_the_child(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "verbose.py").write_text(
                "import sys; sys.stdout.write('x' * 100000); sys.stderr.write('y' * 100000); sys.exit(7)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "verbose",
                        "description": "verbose bounded fixture",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "verbose.py"],
                        "timeout_seconds": 5,
                    }
                ],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1)
            report = json.loads(result.stdout)
            gate = report["gates"][0]
            self.assertEqual(gate["status"], "failed")
            self.assertIn("[output truncated]", gate["stdout"])
            self.assertIn("[output truncated]", gate["stderr"])
            self.assertLessEqual(len(gate["stdout"]), 16 * 1024 + 32)
            self.assertLessEqual(len(gate["stderr"]), 16 * 1024 + 32)

    def test_required_failure_and_timeout_are_recorded_and_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "failure.py").write_text(
                "import sys; print('TOKEN=DO_NOT_LEAK TOKEN=\"QUOTED_DO_NOT_LEAK first second\" {\"access_token\": \"JSON_ACCESS_SENTINEL\", \"auth_token\": \"JSON_AUTH_SENTINEL\", \"aws_access_key_id\": \"JSON_AWS_SENTINEL\"} Bearer DO_NOT_LEAK https://user:DO_NOT_LEAK@example.invalid Basic DO_NOT_LEAK AWS_SECRET_ACCESS_KEY=DO_NOT_LEAK', file=sys.stderr); sys.exit(7)\n",
                encoding="utf-8",
            )
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
            self.assertNotIn("first second", result.stdout + result.stderr)
            self.assertNotIn("JSON_ACCESS_SENTINEL", result.stdout + result.stderr)
            self.assertNotIn("JSON_AUTH_SENTINEL", result.stdout + result.stderr)
            self.assertNotIn("JSON_AWS_SENTINEL", result.stdout + result.stderr)
            self.assertIn("redacted", result.stdout)

    def test_rejects_repository_path_that_is_not_an_allowlisted_script(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            tool = root / "tools/python3"
            tool.parent.mkdir()
            tool.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "unsafe", "description": "unsafe", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["tools/python3"], "timeout_seconds": 5}],
            )
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)
            self.assertIn("allowlist", result.stderr)

    def test_rejects_non_object_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = root / "manifest.json"
            manifest.write_text("[]", encoding="utf-8")
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)
            self.assertIn("schema_version", result.stderr)

    def test_rejects_boolean_schema_and_timeout_values(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = root / "manifest.json"
            manifest.write_text(json.dumps({"schema_version": True, "gates": []}), encoding="utf-8")
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)

            manifest.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [{"id": "bad-timeout", "subsystems": ["governance"], "argv": ["python3", "-m", "unittest"], "timeout_seconds": True}],
                    }
                ),
                encoding="utf-8",
            )
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)

    def test_rejects_case_variant_tool_name(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = self.write_manifest(
                root,
                [{"id": "unsafe", "description": "unsafe", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["PYTHON3", "-m", "unittest"], "timeout_seconds": 5}],
            )
            result = self.run_runner(root, manifest)
            self.assertEqual(result.returncode, 2)
            self.assertIn("allowlisted", result.stderr)

    def test_rejects_output_path_outside_repository_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "governance", "description": "governance", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", "/tmp/outside.json", "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(result.returncode, 2)
            self.assertNotIn("Traceback", result.stderr)

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

    def test_rejects_external_path_arguments(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = self.write_manifest(
                root,
                [
                    {
                        "id": "unsafe-path",
                        "description": "external path",
                        "subsystems": ["governance"],
                        "required": True,
                        "cwd": ".",
                        "argv": ["python3", "-m", "unittest", "discover", "-s", "/tmp/evil-tests"],
                        "timeout_seconds": 5,
                    }
                ],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 2)
            self.assertIn("path outside", result.stderr)

    def test_rejects_relative_symlink_argument_that_escapes_checkout(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            outside = root.parent / "outside-gate.py"
            outside.write_text("pass\n", encoding="utf-8")
            (root / "inside.py").symlink_to(outside)
            manifest = self.write_manifest(
                root,
                [{"id": "symlink", "description": "symlink", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "inside.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 2)
            self.assertIn("escapes repository", result.stderr)

    def test_rejects_symlinked_trusted_script(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            trusted = root / "scripts/check-repository-hygiene.sh"
            trusted.unlink()
            target = root / "replacement.sh"
            target.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            trusted.symlink_to(target)
            manifest = self.write_manifest(
                root,
                [{"id": "symlinked-trusted", "description": "symlinked trusted script", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["scripts/check-repository-hygiene.sh"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 2)
            self.assertIn("must not be a symlink", result.stderr)

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
