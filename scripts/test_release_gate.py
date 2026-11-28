#!/usr/bin/env python3
"""CLI contract tests for the VoidTower release-candidate gate runner."""

from __future__ import annotations

import json
import ctypes
import errno
import os
from pathlib import Path
import signal
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
import unittest
import uuid
from unittest import mock

sys.path.insert(0, str(Path(__file__).parent))
import release_gate
import process_supervisor
from release_gate import GateError, redact, redact_argv
from test_repo_truth import make_checkout

SCRIPT = Path(__file__).with_name("release_gate.py")


class ReleaseGateCliTests(unittest.TestCase):
    def setUp(self) -> None:
        original_umask = os.umask(0o077)
        self.addCleanup(os.umask, original_umask)
        self.original_path = os.environ.get("PATH", "")
        previous_path = os.environ.get("PATH")

        def restore_path() -> None:
            if previous_path is None:
                os.environ.pop("PATH", None)
            else:
                os.environ["PATH"] = previous_path

        self.addCleanup(restore_path)
        self._host_tools_temp = tempfile.TemporaryDirectory(
            prefix="release-gate-host-tools-", dir=Path.home()
        )
        self.addCleanup(self._host_tools_temp.cleanup)
        host_root = Path(self._host_tools_temp.name)
        hermes_bin = host_root / ".hermes" / "bin"
        hermes_bin.mkdir(parents=True, mode=0o700)
        (host_root / ".hermes").chmod(0o700)
        hermes_bin.chmod(0o700)
        for name in ("git", "bwrap", "bash", "python3"):
            executable = shutil.which(name, path=os.defpath)
            if executable is not None:
                (hermes_bin / name).symlink_to(executable)
        os.environ["PATH"] = str(hermes_bin)

    def test_default_test_path_excludes_ambient_build_toolchains(self) -> None:
        expected_path = Path(self._host_tools_temp.name) / ".hermes" / "bin"
        self.assertEqual(os.environ["PATH"].split(os.pathsep), [str(expected_path)])
        self.assertIsNotNone(shutil.which("git"))
        self.assertIsNotNone(shutil.which("python3"))
        self.assertIsNone(shutil.which("cargo"))
        self.assertIsNone(shutil.which("node"))
        self.assertIsNone(shutil.which("npm"))

    def run_runner(
        self,
        root: Path,
        manifest: Path,
        *,
        environment: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=environment,
        )

    def test_require_clean_rejects_dirty_checkout_before_gate_execution(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "README.md").write_text("# dirty\n", encoding="utf-8")
            (root / "marker.py").write_text(
                "from pathlib import Path; Path('gate-ran').write_text('unexpected\\n')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{
                    "id": "must-not-run",
                    "description": "must not run",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "marker.py"],
                    "timeout_seconds": 5,
                }],
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--scope",
                    "all",
                    "--require-clean",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("clean checkout required", result.stderr)
            self.assertFalse((root / "gate-ran").exists())

    def test_require_clean_persists_a_failed_report_without_running_gates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "README.md").write_text("# dirty\n", encoding="utf-8")
            (root / "marker.py").write_text(
                "from pathlib import Path; Path('gate-ran').write_text('unexpected\\n')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{
                    "id": "must-not-run",
                    "description": "must not run",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "marker.py"],
                    "timeout_seconds": 5,
                }],
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--scope",
                    "all",
                    "--require-clean",
                    "--output",
                    "dev-data/dirty-report.json",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("clean checkout required", result.stderr)
            self.assertFalse((root / "gate-ran").exists())
            report = json.loads((root / "dev-data/dirty-report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual([gate["id"] for gate in report["gates"]], ["must-not-run"])
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertEqual(report["gates"][0]["error"], "clean checkout required before gate execution")
            self.assertIn("README.md", report["git"]["modified_paths"])

    def test_require_clean_optional_only_gates_still_fail_the_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "README.md").write_text("# dirty\n", encoding="utf-8")
            (root / "marker.py").write_text(
                "from pathlib import Path; Path('gate-ran').write_text('unexpected\\n')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{
                    "id": "optional-must-not-run",
                    "description": "optional gate must not run",
                    "subsystems": ["governance"],
                    "required": False,
                    "cwd": ".",
                    "argv": ["python3", "marker.py"],
                    "timeout_seconds": 5,
                }],
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--scope",
                    "all",
                    "--require-clean",
                    "--output",
                    "dev-data/optional-dirty-report.json",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertFalse((root / "gate-ran").exists())
            report = json.loads((root / "dev-data/optional-dirty-report.json").read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertEqual(report["gates"][0]["required"], False)

    def write_manifest(self, root: Path, gates: list[dict[str, object]], *, external_gates: list[dict[str, str]] | None = None) -> Path:
        manifest = root / "manifest.json"
        payload: dict[str, object] = {"schema_version": 1, "gates": gates}
        if external_gates is not None:
            payload["external_gates"] = external_gates
        manifest.write_text(json.dumps(payload), encoding="utf-8")
        return manifest

    def test_evidence_output_parent_does_not_leak_root_descriptor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            before = len(list(Path("/proc/self/fd").iterdir()))
            parent_descriptor, filename = release_gate.open_evidence_parent(root, root / "dev-data" / "report.json")
            try:
                self.assertEqual(filename, "report.json")
            finally:
                os.close(parent_descriptor)
            after = len(list(Path("/proc/self/fd").iterdir()))
            self.assertLessEqual(after, before)

    def test_evidence_writer_flushes_report_bytes_before_returning(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "report.json"
            descriptor = os.open(report, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
            try:
                with mock.patch.object(release_gate.os, "fsync", wraps=os.fsync) as flush:
                    release_gate.write_evidence_descriptor(descriptor, '{"status":"failed"}')
                flush.assert_called_once_with(descriptor)
                os.lseek(descriptor, 0, os.SEEK_SET)
                self.assertEqual(os.read(descriptor, 1024), b'{"status":"failed"}\n')
            finally:
                os.close(descriptor)

    def test_new_evidence_parent_flushes_repository_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            calls: list[str] = []
            real_fsync = os.fsync

            def observe_fsync(descriptor: int) -> None:
                calls.append(os.readlink(f"/proc/self/fd/{descriptor}"))
                real_fsync(descriptor)

            with mock.patch.object(release_gate.os, "fsync", side_effect=observe_fsync):
                descriptor, _filename = release_gate.open_evidence_parent(root, root / "dev-data" / "report.json")
                os.close(descriptor)

            self.assertIn(str(root), calls)

    def test_nested_evidence_parent_flushes_each_new_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            calls: list[str] = []
            real_fsync = os.fsync

            def observe_fsync(descriptor: int) -> None:
                calls.append(os.readlink(f"/proc/self/fd/{descriptor}"))
                real_fsync(descriptor)

            with mock.patch.object(release_gate.os, "fsync", side_effect=observe_fsync):
                descriptor, _filename = release_gate.open_evidence_parent(root, root / "dev-data" / "nested" / "report.json")
                os.close(descriptor)

            self.assertIn(str(root), calls)
            self.assertIn(str(root / "dev-data"), calls)

    def test_report_producer_rejects_oversized_output(self) -> None:
        with mock.patch.object(release_gate, "MAX_REPORT_BYTES", 32):
            with self.assertRaisesRegex(release_gate.GateError, "maximum supported size"):
                release_gate.serialize_report({"diagnostic": "x" * 64})

    def test_initial_report_fchmod_failure_invalidates_new_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "fchmod-failure", "description": "fchmod failure", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            report = root / "dev-data" / "fchmod-failure.json"
            calls = 0
            real_fchmod = os.fchmod

            def fail_report_fchmod(descriptor: int, mode: int) -> None:
                nonlocal calls
                calls += 1
                if calls == 1:
                    raise OSError("mode update failed")
                real_fchmod(descriptor, mode)

            with mock.patch.object(release_gate.os, "fchmod", side_effect=fail_report_fchmod), mock.patch.object(
                sys,
                "argv",
                [str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", "dev-data/fchmod-failure.json", "--json"],
            ):
                self.assertEqual(release_gate.main(), 2)
            self.assertFalse(report.exists())

    def test_initial_report_fstat_failure_invalidates_new_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "fstat-failure", "description": "fstat failure", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            report = root / "dev-data" / "fstat-failure.json"
            real_fstat = os.fstat
            failed = False

            def fail_report_fstat(descriptor: int) -> os.stat_result:
                nonlocal failed
                try:
                    target = os.readlink(f"/proc/self/fd/{descriptor}")
                except OSError:
                    target = ""
                if target == str(report) and not failed:
                    failed = True
                    report.unlink()
                    report.write_text("replacement\n", encoding="utf-8")
                    raise OSError("report identity unavailable")
                return real_fstat(descriptor)

            with mock.patch.object(release_gate.os, "fstat", side_effect=fail_report_fstat), mock.patch.object(
                sys,
                "argv",
                [str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", "dev-data/fstat-failure.json", "--json"],
            ):
                self.assertEqual(release_gate.main(), 2)

            self.assertEqual(report.read_text(encoding="utf-8"), "replacement\n")

    def test_unbound_report_cleanup_preserves_path_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report = evidence_dir / "bounded.json"
            report.write_text("original\n", encoding="utf-8")
            descriptor = os.open(report, os.O_WRONLY | os.O_NOFOLLOW)
            try:
                report.unlink()
                report.write_text("replacement\n", encoding="utf-8")
                release_gate.invalidate_unbound_report(
                    root,
                    report,
                    descriptor,
                    expected_root_identity=(root.stat().st_dev, root.stat().st_ino),
                    expected_ancestor_identities=release_gate.capture_evidence_ancestor_identities(root, report),
                )
                self.assertEqual(report.read_text(encoding="utf-8"), "replacement\n")
            finally:
                os.close(descriptor)

    def test_identity_cleanup_rejects_denied_renameat2_probe(self) -> None:
        class DeniedRename:
            def __init__(self) -> None:
                self.renameat2 = self

            def __call__(self, *_args: object) -> int:
                ctypes.set_errno(errno.EPERM)
                return -1

        with mock.patch.object(release_gate.ctypes, "CDLL", return_value=DeniedRename()):
            self.assertFalse(release_gate.identity_cleanup_supported())

    def test_report_identity_includes_expected_mode_when_bound(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "report.json"
            report.write_text("report\n", encoding="utf-8")
            report.chmod(0o600)
            metadata = report.stat()
            identity = (metadata.st_dev, metadata.st_ino, 0o600)
            self.assertTrue(release_gate.report_identity_matches(metadata, identity))
            report.chmod(0o644)
            self.assertFalse(release_gate.report_identity_matches(report.stat(), identity))

    def test_interrupted_report_cleanup_failure_is_not_suppressed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "dev-data").mkdir(mode=0o700)
            report = root / "dev-data" / "report.json"
            report.write_text("original\n", encoding="utf-8")
            identity = release_gate.report_identity(report.stat())

            with mock.patch.object(release_gate, "write_evidence_descriptor", side_effect=OSError("write failed")), mock.patch.object(
                release_gate,
                "unlink_report_if_identity",
                side_effect=OSError("cleanup failed"),
            ):
                with self.assertRaisesRegex(OSError, "failed to invalidate"):
                    release_gate.rewrite_persisted_report(root, report, "recovered\n", identity)

    def test_report_invalidation_cleans_when_ancestor_mode_changes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report = evidence_dir / "unsafe-parent.json"
            report.write_text("original\n", encoding="utf-8")
            report_metadata = report.stat()
            root_metadata = root.stat()
            ancestors = release_gate.capture_evidence_ancestor_identities(root, report)
            evidence_dir.chmod(0o777)

            release_gate.invalidate_persisted_report(
                root,
                report,
                (report_metadata.st_dev, report_metadata.st_ino),
                expected_root_identity=(root_metadata.st_dev, root_metadata.st_ino),
                expected_ancestor_identities=ancestors,
            )

            self.assertFalse(report.exists())

    def test_interrupted_report_rewrite_flushes_parent_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "dev-data").mkdir(mode=0o700)
            report = root / "dev-data" / "report.json"
            report.write_text('{"status":"passed"}\n', encoding="utf-8")
            identity = release_gate.report_identity(report.stat())

            calls: list[str] = []
            real_fsync = os.fsync

            def observe_fsync(descriptor: int) -> None:
                calls.append(os.readlink(f"/proc/self/fd/{descriptor}"))
                real_fsync(descriptor)

            with mock.patch.object(release_gate.os, "fsync", side_effect=observe_fsync):
                release_gate.rewrite_persisted_report(root, report, '{"status":"failed"}', identity)

            self.assertEqual(json.loads(report.read_text(encoding="utf-8"))["status"], "failed")
            self.assertEqual(calls, [str(report), str(root / "dev-data")])

    def test_interrupted_report_rewrite_removes_stale_report_on_flush_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "dev-data").mkdir(mode=0o700)
            report = root / "dev-data" / "report.json"
            report.write_text('{"status":"passed"}\n', encoding="utf-8")
            identity = release_gate.report_identity(report.stat())
            real_fsync = os.fsync

            def fail_parent_fsync(descriptor: int) -> None:
                if os.readlink(f"/proc/self/fd/{descriptor}") == str(root / "dev-data"):
                    raise OSError("directory flush failed")
                real_fsync(descriptor)

            with mock.patch.object(release_gate.os, "fsync", side_effect=fail_parent_fsync):
                with self.assertRaises(OSError):
                    release_gate.rewrite_persisted_report(root, report, '{"status":"failed"}', identity)

            self.assertFalse(report.exists())

    def test_initial_report_persistence_removes_stale_file_when_parent_flush_fails(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{
                    "id": "initial-persistence",
                    "description": "initial persistence",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "pass.py"],
                    "timeout_seconds": 5,
                }],
            )
            report = root / "dev-data" / "initial-persistence.json"
            real_fsync = os.fsync

            def fail_parent_fsync(descriptor: int) -> None:
                if os.readlink(f"/proc/self/fd/{descriptor}") == str(root / "dev-data"):
                    raise OSError("initial report directory flush failed")
                real_fsync(descriptor)

            with mock.patch.object(release_gate.os, "fsync", side_effect=fail_parent_fsync), mock.patch.object(
                sys,
                "argv",
                [
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--output",
                    "dev-data/initial-persistence.json",
                    "--json",
                ],
            ):
                self.assertEqual(release_gate.main(), 2)

            self.assertFalse(report.exists())

    def test_output_rejects_hosts_without_identity_cleanup_support(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{
                    "id": "unsupported-cleanup",
                    "description": "unsupported cleanup",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "pass.py"],
                    "timeout_seconds": 5,
                }],
            )
            report = root / "dev-data" / "unsupported-cleanup.json"

            with mock.patch.object(release_gate, "identity_cleanup_supported", return_value=False), mock.patch.object(
                sys,
                "argv",
                [
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--output",
                    "dev-data/unsupported-cleanup.json",
                    "--json",
                ],
            ):
                self.assertEqual(release_gate.main(), 2)

            self.assertFalse(report.exists())

    def test_backend_format_gate_is_required_and_targets_backend_rustfmt(self) -> None:
        manifest_path = Path(__file__).with_name("release-gates.json")
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        gate = next(item for item in manifest["gates"] if item["id"] == "backend-format")

        self.assertTrue(gate["required"])
        self.assertEqual(gate["cwd"], "backend")
        self.assertEqual(gate["subsystems"], ["backend", "agent"])
        self.assertEqual(gate["argv"], ["cargo", "fmt", "--all", "--", "--check"])

    def test_rejects_unknown_nested_manifest_fields(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "fixture", "description": "fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5, "unexpected": "metadata"}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 2)
            self.assertIn("unknown fields", result.stderr)

    def test_rejects_malformed_manifest_nested_values(self) -> None:
        for field, value in (("description", {"metadata": "object"}), ("artifacts", [{"metadata": "object"}]), ("artifacts", ["C:artifact"]), ("timeout_seconds", 0), ("evidence_limit", "not-a-class")):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                make_checkout(root)
                (root / "pass.py").write_text("pass\n", encoding="utf-8")
                gate = {"id": "fixture", "description": "fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}
                gate[field] = value
                manifest = self.write_manifest(root, [gate])

                result = self.run_runner(root, manifest)

                self.assertEqual(result.returncode, 2)
                self.assertIn("gate fixture", result.stderr)

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
            self.assertRegex(report["git"]["head"], r"^[0-9a-f]{40}$")
            self.assertTrue(report["platform"]["machine"])
            self.assertTrue(report["platform"]["system"])
            self.assertTrue(report["platform"]["release"])
            self.assertEqual(report["gates"][0]["status"], "passed")
            self.assertEqual(report["gates"][0]["cwd"], ".")
            self.assertEqual(report["gates"][0]["exit_code"], 0)
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

    def test_declared_artifact_mutation_persists_failed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            artifact = root / "artifact.bin"
            artifact.write_bytes(b"before")
            (root / "mutate.py").write_text(
                "from pathlib import Path; Path('artifact.bin').write_bytes(b'after')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{
                    "id": "mutating-artifact",
                    "description": "mutating artifact fixture",
                    "subsystems": ["governance"],
                    "required": False,
                    "cwd": ".",
                    "argv": ["python3", "mutate.py"],
                    "timeout_seconds": 5,
                    "artifacts": ["artifact.bin"],
                }],
            )
            report_path = root / "dev-data" / "artifact-mutation.json"

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--output",
                    "dev-data/artifact-mutation.json",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"][0]["status"], "failed")
            self.assertIn("changed declared artifact", report["gates"][0]["error"])

    def test_optional_multi_artifact_integrity_failure_cannot_report_green(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "first.bin").write_bytes(b"before")
            (root / "second.bin").write_bytes(b"before")
            (root / "mutate.py").write_text(
                "from pathlib import Path\n"
                "Path('first.bin').write_bytes(b'after')\n"
                "Path('second.bin').unlink()\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{
                    "id": "multi-artifact",
                    "description": "multi-artifact integrity fixture",
                    "subsystems": ["governance"],
                    "required": False,
                    "cwd": ".",
                    "argv": ["python3", "mutate.py"],
                    "timeout_seconds": 5,
                    "artifacts": ["first.bin", "second.bin"],
                }],
            )
            report_path = root / "dev-data" / "multi-artifact.json"

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--output",
                    "dev-data/multi-artifact.json",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"][0]["status"], "failed")
            self.assertIn("changed declared artifact", report["gates"][0]["error"])

    def test_declared_artifact_symlink_replacement_persists_failed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            artifact = root / "artifact.bin"
            artifact.write_bytes(b"before")
            (root / "replacement.bin").write_bytes(b"replacement")
            (root / "replace.py").write_text(
                "from pathlib import Path\n"
                "Path('artifact.bin').unlink()\n"
                "Path('artifact.bin').symlink_to('replacement.bin')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{
                    "id": "symlink-artifact",
                    "description": "symlink artifact fixture",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "replace.py"],
                    "timeout_seconds": 5,
                    "artifacts": ["artifact.bin"],
                }],
            )
            report_path = root / "dev-data" / "symlink-artifact.json"

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", "dev-data/symlink-artifact.json", "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"][0]["status"], "failed")
            self.assertIn("hashed safely", report["gates"][0]["error"])

    def test_gate_environment_is_allowlisted_and_keeps_required_tool_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "environment.py").write_text(
                "import os\n"
                "import sys\n"
                "sensitive = ('VT_RELEASE_SECRET', 'AWS_ACCESS_KEY_ID', 'PYTHONPATH', 'NPM_CONFIG_USERCONFIG')\n"
                "required = ('PATH', 'HOME', 'PYTHONDONTWRITEBYTECODE')\n"
                "raise SystemExit(0 if all(os.environ.get(name) not in (None, 'sentinel-value') for name in required) and os.environ.get('HOME') != 'sentinel-home' and os.environ.get('PYTHONDONTWRITEBYTECODE') == '1' and os.path.isfile(sys.executable) and all(os.environ.get(name) != 'sentinel-value' for name in sensitive) else 1)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "environment", "description": "environment isolation", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "environment.py"], "timeout_seconds": 5}],
            )

            environment = os.environ.copy()
            environment["VT_RELEASE_SECRET"] = "sentinel-value"
            environment["AWS_ACCESS_KEY_ID"] = "sentinel-value"
            environment["PYTHONPATH"] = "sentinel-value"
            environment["NPM_CONFIG_USERCONFIG"] = "sentinel-value"
            environment["HOME"] = "sentinel-home"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
                check=False,
                env=environment,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")
            self.assertNotIn("sentinel-value", result.stdout + result.stderr)

    def test_python_gate_uses_launcher_without_allowing_sibling_tool_shadowing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            launcher_bin = root / "launcher-venv" / "bin"
            launcher_bin.mkdir(parents=True)
            launcher = launcher_bin / "python"
            launcher.symlink_to(Path(sys.executable))
            fake_python3 = launcher_bin / "python3"
            fake_python3.write_text(
                "#!/bin/sh\n"
                "printf 'unexpected sibling python3 execution' >&2\n"
                "exit 77\n",
                encoding="utf-8",
            )
            fake_python3.chmod(0o755)
            (root / "environment.py").write_text(
                "import os\n"
                "import sys\n"
                f"expected = {launcher_bin.as_posix()!r}\n"
                "actual_path = os.environ.get('PATH', '').split(os.pathsep)\n"
                "raise SystemExit(0 if expected not in actual_path and os.path.dirname(sys.executable) == expected else 1)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "launcher-environment", "description": "launcher interpreter environment", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "environment.py"], "timeout_seconds": 5}],
            )

            result = subprocess.run(
                [str(launcher), str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")

    def test_python_gate_with_copied_launcher_does_not_expose_launcher_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            launcher_bin = root / "copied-venv" / "bin"
            launcher_bin.mkdir(parents=True)
            launcher = launcher_bin / "python3"
            base_launcher = Path("/usr/bin/python3").resolve()
            if not base_launcher.is_file():
                self.skipTest("system Python launcher unavailable for copy-based fixture")
            shutil.copy2(base_launcher, launcher)
            launcher.chmod(0o755)
            (root / "environment.py").write_text(
                "import os\n"
                "import sys\n"
                f"expected = {launcher_bin.as_posix()!r}\n"
                "raise SystemExit(0 if os.path.dirname(sys.executable) == expected and expected not in os.environ.get('PATH', '').split(os.pathsep) else 1)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "copied-launcher", "description": "copied launcher", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "environment.py"], "timeout_seconds": 5}],
            )

            result = subprocess.run(
                [str(launcher), str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")

    def test_gate_environment_rejects_ambient_cargo_and_home_overrides(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_bin = root / "fake-bin"
            fake_bin.mkdir()
            fake_cargo = fake_bin / "cargo"
            fake_cargo.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            fake_cargo.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()
            with mock.patch.dict(
                os.environ,
                {"PATH": str(fake_bin), "CARGO_HOME": str(root / "attacker-cargo"), "RUSTUP_HOME": str(root / "attacker-rustup")},
                clear=False,
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)

            self.assertNotIn(str(fake_bin), environment["PATH"].split(os.pathsep))
            self.assertNotEqual(environment.get("CARGO_HOME"), str(root / "attacker-cargo"))
            self.assertNotIn("RUSTUP_HOME", environment)

    def test_gate_environment_uses_private_writable_cargo_home(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            selected = root / ".cargo" / "bin" / "cargo"
            selected.parent.mkdir(parents=True)
            selected.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected.chmod(0o755)
            source_cargo_home = root / ".cargo"
            source_rustup_home = Path(temporary) / "rustup"
            source_rustup_home.mkdir()
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.object(release_gate, "selected_tool_executables", return_value={"cargo": selected}), mock.patch.object(
                release_gate,
                "trusted_tooling",
                return_value=([], source_cargo_home, source_rustup_home),
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)

            cargo_home = Path(environment["CARGO_HOME"])
            self.assertEqual(cargo_home, private_home / "cargo-home")
            self.assertTrue(cargo_home.is_dir())
            self.assertTrue(os.access(cargo_home, os.W_OK))
            self.assertEqual(Path(environment["RUSTUP_HOME"]), source_rustup_home)

    def test_gate_environment_always_uses_private_offline_cargo_home(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.object(release_gate, "selected_tool_executables", return_value={"cargo": Path(sys.executable).resolve()}), mock.patch.object(
                release_gate,
                "trusted_tooling",
                return_value=([], None, None),
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)

            self.assertEqual(Path(environment["CARGO_HOME"]), private_home / "cargo-home")
            self.assertTrue(Path(environment["CARGO_HOME"]).is_dir())
            self.assertEqual(environment["CARGO_NET_OFFLINE"], "true")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_cargo_gate_uses_offline_private_cache_from_trusted_home(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            cargo_home = Path(temporary) / ".cargo"
            cargo_bin = cargo_home / "bin"
            cargo_bin.mkdir(parents=True)
            cargo_home.chmod(0o755)
            cargo_bin.chmod(0o755)
            (cargo_home / "registry" / "fixture").mkdir(parents=True)
            (cargo_home / "git" / "fixture").mkdir(parents=True)
            (cargo_home / "advisory-dbs" / "fixture").mkdir(parents=True)
            (cargo_home / "registry").chmod(0o755)
            (cargo_home / "git").chmod(0o755)
            advisory_cache = cargo_home / "advisory-dbs"
            advisory_cache.chmod(0o755)
            (advisory_cache / "fixture").chmod(0o755)
            cargo = cargo_bin / "cargo"
            cargo.write_text(
                "#!/bin/sh\n"
                "test \"$CARGO_NET_OFFLINE\" = true || exit 11\n"
                "test -d \"$CARGO_HOME/registry/fixture\" || exit 12\n"
                "test -d \"$CARGO_HOME/git/fixture\" || exit 13\n"
                "test -d \"$CARGO_HOME/advisory-dbs/fixture\" || exit 14\n"
                '/usr/bin/touch "$CARGO_HOME/advisory-dbs/fixture/write-probe" 2>/dev/null || exit 15\n'
                'if /usr/bin/touch "$CARGO_HOME/registry/fixture/write-probe" 2>/dev/null; then exit 16; fi\n'
                'if /usr/bin/touch "$CARGO_HOME/git/fixture/write-probe" 2>/dev/null; then exit 17; fi\n'
                "printf 'offline-cache-ok\\n'\n",
                encoding="utf-8",
            )
            cargo.chmod(0o755)
            manifest = self.write_manifest(
                root,
                [{"id": "cargo-cache", "description": "offline cargo cache", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["cargo", "probe"], "timeout_seconds": 5}],
            )

            environment = os.environ.copy()
            environment["PATH"] = os.pathsep.join(part for part in (str(cargo_bin), environment.get("PATH", "")) if part)
            result = self.run_runner(root, manifest, environment=environment)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "passed")
            self.assertEqual(report["gates"][0]["stdout"], "offline-cache-ok\n")
            self.assertFalse((advisory_cache / "fixture" / "write-probe").exists())

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_supply_chain_manifest_runs_cargo_deny_offline(self) -> None:
        manifest_path = Path(__file__).resolve().parents[1] / "scripts" / "release-gates.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        supply_chain = next(gate for gate in manifest["gates"] if gate["id"] == "supply-chain")
        self.assertEqual(supply_chain["argv"], ["cargo", "deny", "--offline", "check"])

    def test_cargo_gate_rejects_symlinked_trusted_cache(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            cargo_home = Path(temporary) / ".cargo"
            cargo_bin = cargo_home / "bin"
            cargo_bin.mkdir(parents=True)
            cargo_home.chmod(0o755)
            cargo_bin.chmod(0o755)
            (cargo_home / "registry-target").mkdir()
            (cargo_home / "registry").symlink_to(cargo_home / "registry-target", target_is_directory=True)
            cargo = cargo_bin / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)
            manifest = self.write_manifest(
                root,
                [{"id": "cargo-cache", "description": "offline cargo cache", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["cargo", "probe"], "timeout_seconds": 5}],
            )

            environment = os.environ.copy()
            environment["PATH"] = os.pathsep.join(part for part in (str(cargo_bin), environment.get("PATH", "")) if part)
            result = self.run_runner(root, manifest, environment=environment)

            self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertEqual(report["gates"][0]["error"], "gate could not start: GateError")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_advisory_cache_copy_bounds_entries_and_cleans_partial_copy(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            source = Path(temporary) / "advisory-dbs"
            source.mkdir(mode=0o755)
            for name in ("first", "second"):
                entry = source / name
                entry.write_text("advisory data", encoding="utf-8")
                entry.chmod(0o644)
            destination = Path(temporary) / "private-copy"
            source_fd = os.open(
                source,
                os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0),
            )
            try:
                with mock.patch.object(release_gate, "MAX_TOOL_SNAPSHOT_ENTRIES", 1):
                    with mock.patch.object(release_gate.os, "listdir") as listdir_mock:
                        with self.assertRaises(GateError):
                            release_gate._copy_advisory_cache_from_descriptor(source_fd, destination)
                        listdir_mock.assert_not_called()
                self.assertFalse(destination.exists())
            finally:
                os.close(source_fd)

    def test_cargo_gate_rejects_writable_nested_advisory_cache_entry(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            cargo_home = Path(temporary) / ".cargo"
            cargo_bin = cargo_home / "bin"
            cargo_bin.mkdir(parents=True)
            cargo_home.chmod(0o755)
            cargo_bin.chmod(0o755)
            advisory_cache = cargo_home / "advisory-dbs"
            fixture = advisory_cache / "fixture"
            fixture.mkdir(parents=True)
            advisory_cache.chmod(0o755)
            fixture.chmod(0o777)
            cargo = cargo_bin / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)
            manifest = self.write_manifest(
                root,
                [{"id": "cargo-cache", "description": "offline cargo cache", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["cargo", "probe"], "timeout_seconds": 5}],
            )
            environment = os.environ.copy()
            environment["PATH"] = os.pathsep.join(part for part in (str(cargo_bin), environment.get("PATH", "")) if part)
            result = self.run_runner(root, manifest, environment=environment)
            self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertEqual(report["gates"][0]["error"], "gate could not start: GateError")
            fixture.chmod(0o755)
            metadata_file = fixture / "nested-advisory.toml"
            metadata_file.write_text('fixture = true\n', encoding="utf-8")
            metadata_file.chmod(0o666)
            result = self.run_runner(root, manifest, environment=environment)
            self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertEqual(report["gates"][0]["error"], "gate could not start: GateError")

    def test_cargo_gate_advisory_cache_without_registry_or_git(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            cargo_home = Path(temporary) / ".cargo"
            cargo_bin = cargo_home / "bin"
            cargo_bin.mkdir(parents=True)
            cargo_home.chmod(0o755)
            cargo_bin.chmod(0o755)
            advisory_cache = cargo_home / "advisory-dbs"
            fixture = advisory_cache / "fixture"
            fixture.mkdir(parents=True)
            advisory_cache.chmod(0o755)
            fixture.chmod(0o755)
            cargo = cargo_bin / "cargo"
            cargo.write_text('#!/bin/sh\n[ -d "$CARGO_HOME/advisory-dbs/fixture" ] || exit 14\n/usr/bin/touch "$CARGO_HOME/advisory-dbs/fixture/write-probe" 2>/dev/null || exit 15\nexit 0\n', encoding="utf-8")
            cargo.chmod(0o755)
            manifest = self.write_manifest(
                root,
                [{"id": "cargo-cache", "description": "offline advisory cache only", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["cargo", "probe"], "timeout_seconds": 5}],
            )
            environment = os.environ.copy()
            environment["PATH"] = os.pathsep.join(part for part in (str(cargo_bin), environment.get("PATH", "")) if part)
            result = self.run_runner(root, manifest, environment=environment)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "passed")
            self.assertFalse((fixture / "write-probe").exists())

    def test_cargo_gate_rejects_writable_trusted_cache(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            cargo_home = Path(temporary) / ".cargo"
            cargo_bin = cargo_home / "bin"
            cargo_bin.mkdir(parents=True)
            cargo_home.chmod(0o755)
            cargo_bin.chmod(0o755)
            registry = cargo_home / "registry"
            registry.mkdir()
            registry.chmod(0o777)
            cargo = cargo_bin / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)
            manifest = self.write_manifest(
                root,
                [{"id": "cargo-cache", "description": "offline cargo cache", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["cargo", "probe"], "timeout_seconds": 5}],
            )

            environment = os.environ.copy()
            environment["PATH"] = os.pathsep.join(part for part in (str(cargo_bin), environment.get("PATH", "")) if part)
            result = self.run_runner(root, manifest, environment=environment)

            self.assertEqual(result.returncode, 1, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertEqual(report["gates"][0]["error"], "gate could not start: GateError")

    def test_repository_truth_uses_sanitized_git_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_bin = root / "fake-bin"
            fake_bin.mkdir()
            fake_git = fake_bin / "git"
            fake_git.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            fake_git.chmod(0o755)
            observed: list[str] = []

            def capture_path(_root: Path) -> dict[str, object]:
                observed.append(os.environ["PATH"])
                return {"check": {"status": "passed"}}

            with mock.patch.dict(os.environ, {"PATH": str(fake_bin)}, clear=False), mock.patch.object(
                release_gate, "build_report", side_effect=capture_path
            ):
                release_gate.build_truth(root)

            self.assertEqual(len(observed), 1)
            self.assertNotIn(str(fake_bin), observed[0].split(os.pathsep))

    def test_gate_environment_rejects_repository_symlink_tool_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            external_bin = Path(temporary) / "external" / ".cargo" / "bin"
            external_bin.mkdir(parents=True)
            cargo = external_bin / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)
            (root / ".cargo").symlink_to(external_bin.parent.parent, target_is_directory=True)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.dict(os.environ, {"PATH": str(root / ".cargo" / "bin")}, clear=False):
                environment = release_gate.gate_environment(root, "marker", private_home)

            self.assertNotIn(str(root / ".cargo" / "bin"), environment["PATH"].split(os.pathsep))
            if "CARGO_HOME" in environment:
                self.assertEqual(Path(environment["CARGO_HOME"]), private_home / "cargo-home")

    def test_gate_environment_rejects_cross_tool_shadowing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            first = Path(temporary) / "first"
            second = Path(temporary) / "second"
            first.mkdir()
            second.mkdir()
            for directory, name in ((first, "cargo"), (first, "git"), (second, "git")):
                tool = directory / name
                tool.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                tool.chmod(0o755)

            with mock.patch.object(release_gate, "SAFE_TOOL_DIRECTORIES", (second,)), mock.patch.dict(
                os.environ, {"PATH": os.pathsep.join((str(first), str(second)))}, clear=False
            ):
                private_home = root / "private-home"
                private_home.mkdir()
                environment = release_gate.gate_environment(root, "marker", private_home)

            self.assertNotIn(str(first), environment["PATH"].split(os.pathsep))
            self.assertNotIn(str(second), environment["PATH"].split(os.pathsep))
            self.assertIn(str(private_home / "support"), environment["PATH"].split(os.pathsep))

    def test_gate_environment_binds_toolchain_before_fixed_directories(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".hermes" / "bin"
            fixed = Path(temporary) / "fixed"
            toolchain.mkdir(parents=True)
            fixed.mkdir()
            selected_cargo = toolchain / "cargo"
            selected_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            for name in ("cargo-deny", "cargo-clippy", "cargo-fmt"):
                executable = toolchain / name
                executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                executable.chmod(0o755)
            fixed_bash = fixed / "bash"
            fixed_bash.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            fixed_bash.chmod(0o755)
            shadow_cargo = fixed / "cargo"
            shadow_cargo.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            shadow_cargo.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.object(release_gate, "SAFE_TOOL_DIRECTORIES", (fixed,)), mock.patch.dict(
                os.environ, {"PATH": os.pathsep.join((str(toolchain), str(fixed)))}, clear=False
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)

            path_entries = environment["PATH"].split(os.pathsep)
            self.assertEqual(path_entries[0], str(private_home / "bin"))
            self.assertNotIn(str(toolchain), path_entries)
            self.assertNotIn(str(fixed), path_entries)
            self.assertIn(str(private_home / "support"), path_entries)
            self.assertTrue((private_home / "support" / "bash").is_symlink())
            self.assertFalse((private_home / "support" / "cargo").exists())
            self.assertTrue((private_home / "bin" / "cargo").is_file())
            self.assertTrue((private_home / "bin" / "cargo-deny").is_file())
            self.assertTrue((private_home / "bin" / "cargo-clippy").is_file())
            self.assertTrue((private_home / "bin" / "cargo-fmt").is_file())
            self.assertTrue(os.access(private_home / "bin" / "cargo", os.X_OK))
            self.assertTrue(os.access(private_home / "bin" / "cargo-deny", os.X_OK))
            self.assertTrue(os.access(private_home / "bin" / "cargo-clippy", os.X_OK))
            self.assertTrue(os.access(private_home / "bin" / "cargo-fmt", os.X_OK))
            self.assertIn(selected_cargo.as_posix(), (private_home / "bin" / "cargo").read_text(encoding="utf-8"))
            bound_cargo = private_home / "bound" / "cargo"
            self.assertNotEqual(selected_cargo.stat().st_ino, bound_cargo.stat().st_ino)
            self.assertEqual(selected_cargo.read_bytes(), bound_cargo.read_bytes())
            self.assertEqual(bound_cargo.stat().st_mode & stat.S_IWUSR, 0)
            self.assertEqual(selected_cargo.read_text(encoding="utf-8"), "#!/bin/sh\nexit 0\n")
            self.assertEqual(private_home.stat().st_mode & stat.S_IWUSR, 0)
            self.assertEqual(private_home.stat().st_mode & stat.S_IWGRP, 0)
            self.assertEqual(private_home.stat().st_mode & stat.S_IWOTH, 0)

    def test_selected_tools_capture_rust_proxies_required_by_cargo_gates(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".cargo" / "bin"
            toolchain.mkdir(parents=True)
            names = (
                "cargo",
                "cargo-clippy",
                "cargo-deny",
                "cargo-fmt",
                "git",
                "node",
                "npm",
                "rustc",
                "rustfmt",
                "clippy-driver",
            )
            for name in names:
                executable = toolchain / name
                executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
                executable.chmod(0o755)

            with mock.patch.object(release_gate, "path_tool", side_effect=lambda name, _root, _path: toolchain):
                selected = release_gate.selected_tool_executables(root)

            self.assertEqual(set(selected), set(names))

    def test_sandboxed_command_rebinds_git_metadata_read_only(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            (root / ".git").mkdir()
            private_home = root / "private-home"
            for name in ("bin", "bound", "support", "home", "tmp", "cargo-home"):
                (private_home / name).mkdir(parents=True)

            command = release_gate.sandboxed_command(root, private_home, ["python3", "pass.py"])
            mount = [str(item) for item in command]
            git_source = str(root / ".git")
            git_mount = mount.index(git_source)
            self.assertEqual(mount[git_mount - 2], "--ro-bind-fd")
            self.assertEqual(mount[git_mount], git_source)
            checkout_mount = mount.index("--bind-fd")
            self.assertIn(int(mount[checkout_mount + 1]), command.pass_fds)
            self.assertEqual(mount[checkout_mount + 2], str(root))
            self.assertIn("--unshare-net", command)
            self.assertIn("--tmpfs", command)
            self.assertIn("/home", command)
            self.assertIn("/run", command)
            command.close()

    def test_sandboxed_command_supports_linked_worktree_git_metadata(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]) as temporary:
            source = Path(temporary) / "source"
            linked = Path(temporary) / "linked"
            make_checkout(source)
            subprocess.run(
                ["git", "-C", str(source), "worktree", "add", "--detach", str(linked), "HEAD"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                private_home = linked / "private-home"
                private_home.mkdir(mode=0o700)
                selected = release_gate.selected_tool_executables(linked)
                environment = release_gate.gate_environment(linked, "linked-worktree-git-metadata", private_home, selected)
                command = release_gate.sandboxed_command(
                    linked, private_home, ["git", "diff", "--check"], environment=environment, selected=selected
                )
                result = subprocess.run(
                    command,
                    cwd=linked,
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    env=environment,
                    pass_fds=command.pass_fds,
                )
                command.close()

                self.assertEqual(result.returncode, 0, result.stderr)
                pointer_before = (linked / ".git").read_text(encoding="utf-8")
                tamper_command = release_gate.sandboxed_command(
                    linked,
                    private_home,
                    ["python3", "-c", "from pathlib import Path; Path('.git').write_text('tampered\\n')"],
                    environment=environment,
                    selected=selected,
                )
                tamper = subprocess.run(
                    tamper_command,
                    cwd=linked,
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    env=environment,
                    pass_fds=tamper_command.pass_fds,
                )
                tamper_command.close()
                self.assertNotEqual(tamper.returncode, 0, tamper.stderr)
                self.assertEqual((linked / ".git").read_text(encoding="utf-8"), pointer_before)
            finally:
                release_gate.cleanup_private_home(private_home)
                subprocess.run(
                    ["git", "-C", str(source), "worktree", "remove", "--force", str(linked)],
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

    def test_linked_worktree_outside_source_parent_is_associated_safely(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]) as source_temporary, tempfile.TemporaryDirectory(
            dir=Path.home()
        ) as linked_temporary:
            source = Path(source_temporary) / "source"
            linked = Path(linked_temporary) / "linked"
            make_checkout(source)
            subprocess.run(
                ["git", "-C", str(source), "worktree", "add", "--detach", str(linked), "HEAD"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                mounts = release_gate.git_metadata_mounts(linked)
                self.assertEqual([mount[0] for mount in mounts], [linked / ".git", source / ".git" / "worktrees" / "linked", source / ".git"])
                private_home = linked / "private-home"
                private_home.mkdir(mode=0o700)
                selected = release_gate.selected_tool_executables(linked)
                environment = release_gate.gate_environment(linked, "linked-worktree-outside-parent", private_home, selected)
                command = release_gate.sandboxed_command(
                    linked, private_home, ["git", "diff", "--check"], environment=environment, selected=selected
                )
                try:
                    result = subprocess.run(
                        command,
                        cwd=linked,
                        check=False,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        text=True,
                        env=environment,
                        pass_fds=command.pass_fds,
                    )
                finally:
                    command.close()
                self.assertEqual(result.returncode, 0, result.stderr)
            finally:
                release_gate.cleanup_private_home(private_home)
                subprocess.run(
                    ["git", "-C", str(source), "worktree", "remove", "--force", str(linked)],
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

    def test_linked_worktree_metadata_rejects_unassociated_common_directory(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]) as temporary:
            root = Path(temporary) / "linked"
            root.mkdir()
            external = Path(temporary).parent / f"external-git-{uuid.uuid4().hex}"
            common = external / ".git"
            target = common / "worktrees" / "linked"
            target.mkdir(parents=True)
            try:
                (root / ".git").write_text(f"gitdir: {target}\n", encoding="utf-8")
                (target / "commondir").write_text("../..\n", encoding="utf-8")
                (target / "gitdir").write_text(f"{root / '.git'}\n", encoding="utf-8")

                with self.assertRaises(release_gate.GateError):
                    release_gate.git_metadata_mounts(root)
            finally:
                shutil.rmtree(external, ignore_errors=True)

    def test_linked_worktree_metadata_ignores_inherited_git_configuration(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]) as temporary:
            source = Path(temporary) / "source"
            linked = Path(temporary) / "linked"
            make_checkout(source)
            subprocess.run(
                ["git", "-C", str(source), "worktree", "add", "--detach", str(linked), "HEAD"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            fake_bin = Path(temporary) / "fake-bin"
            fake_bin.mkdir()
            fake_git = fake_bin / "git"
            fake_git.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            fake_git.chmod(0o755)
            try:
                with mock.patch.dict(
                    os.environ,
                    {
                        "PATH": fake_bin.as_posix(),
                        "GIT_DIR": (Path(temporary) / "attacker-git").as_posix(),
                        "GIT_WORK_TREE": (Path(temporary) / "attacker-worktree").as_posix(),
                        "GIT_CONFIG_GLOBAL": (Path(temporary) / "attacker-config").as_posix(),
                        "GIT_CONFIG_COUNT": "1",
                        "GIT_CONFIG_KEY_0": "core.worktree",
                        "GIT_CONFIG_VALUE_0": (Path(temporary) / "attacker-worktree").as_posix(),
                    },
                    clear=False,
                ):
                    mounts = release_gate.git_metadata_mounts(linked)
                self.assertEqual(len(mounts), 3)
            finally:
                subprocess.run(
                    ["git", "-C", str(source), "worktree", "remove", "--force", str(linked)],
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

    def test_linked_worktree_metadata_pointer_is_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            root.mkdir(exist_ok=True)
            (root / ".git").write_text(
                "gitdir: " + ("x" * (release_gate.MAX_GIT_POINTER_BYTES + 1)) + "\n",
                encoding="utf-8",
            )

            with self.assertRaisesRegex(release_gate.GateError, "bounded size"):
                release_gate.git_metadata_mounts(root)

    def test_linked_worktree_metadata_git_listing_is_bounded(self) -> None:
        with tempfile.TemporaryDirectory(dir=Path(__file__).resolve().parents[1]) as temporary:
            source = Path(temporary) / "source"
            linked = Path(temporary) / "linked"
            make_checkout(source)
            subprocess.run(
                ["git", "-C", str(source), "worktree", "add", "--detach", str(linked), "HEAD"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            fake_git = Path(temporary) / "git-overflow"
            fake_git.write_text("#!/bin/sh\nprintf '%70000s' x\n", encoding="utf-8")
            fake_git.chmod(0o755)
            try:
                with self.assertRaisesRegex(release_gate.GateError, "bounded size"):
                    release_gate.git_metadata_mounts(linked, fake_git)
            finally:
                subprocess.run(
                    ["git", "-C", str(source), "worktree", "remove", "--force", str(linked)],
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_linked_worktree_metadata_timeout_cleans_forked_git_descendant(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pid_file = root / "descendant.pid"
            marker = root / "descendant-marker"
            fake_git = root / "git-timeout"
            fake_git.write_text(
                f"#!{sys.executable}\n"
                "import os, pathlib, time\n"
                f"pid_file = pathlib.Path({str(pid_file)!r})\n"
                f"marker = pathlib.Path({str(marker)!r})\n"
                "child = os.fork()\n"
                "if child == 0:\n"
                "    os.setsid()\n"
                "    pid_file.write_text(str(os.getpid()))\n"
                "    time.sleep(6)\n"
                "    marker.write_text('leaked')\n"
                "else:\n"
                "    time.sleep(30)\n",
                encoding="utf-8",
            )
            fake_git.chmod(0o755)

            with self.assertRaisesRegex(release_gate.GateError, "timed out"):
                release_gate._bounded_git_worktree_listing(root, fake_git)
            time.sleep(7)
            self.assertFalse(marker.exists())

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_linked_worktree_timeout_escalates_when_supervisor_ignores_sigterm(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pid_file = root / "supervisor.pid"
            marker = root / "supervisor-marker"
            fake_git = root / "git-ignores-term"
            fake_git.write_text(
                f"#!{sys.executable}\n"
                "import pathlib, signal, time\n"
                f"pid_file = pathlib.Path({str(pid_file)!r})\n"
                f"marker = pathlib.Path({str(marker)!r})\n"
                "signal.signal(signal.SIGTERM, signal.SIG_IGN)\n"
                "pid_file.write_text(str(__import__('os').getpid()))\n"
                "time.sleep(6)\n"
                "marker.write_text('leaked')\n",
                encoding="utf-8",
            )
            fake_git.chmod(0o755)

            try:
                with self.assertRaisesRegex(release_gate.GateError, "timed out"):
                    release_gate._bounded_git_worktree_listing(root, fake_git)
                time.sleep(7)
                self.assertFalse(marker.exists())
            finally:
                if pid_file.exists():
                    try:
                        os.kill(int(pid_file.read_text(encoding="utf-8")), signal.SIGKILL)
                    except (OSError, ValueError):
                        pass

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_linked_worktree_cleanup_escalates_after_supervisor_wait_timeout(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_git = root / "git-stuck-supervisor"
            fake_git.write_text(f"#!{sys.executable}\nimport time\ntime.sleep(30)\n", encoding="utf-8")
            fake_git.chmod(0o755)
            signals: list[int] = []
            processes: list[tuple[subprocess.Popen[bytes], int]] = []

            def signal_without_term(process: subprocess.Popen[bytes], start_time: int | None, signum: int) -> bool:
                self.assertIsNotNone(start_time)
                assert start_time is not None
                processes.append((process, start_time))
                signals.append(signum)
                if signum == signal.SIGKILL:
                    return process_supervisor.signal_process(process.pid, start_time, signum)
                return True

            try:
                with mock.patch.object(release_gate, "kill_descendants"), mock.patch.object(
                    release_gate, "_signal_supervisor", side_effect=signal_without_term
                ):
                    with self.assertRaisesRegex(release_gate.GateError, "cleanup could not be verified"):
                        release_gate._bounded_git_worktree_listing(root, fake_git)
                self.assertEqual(signals, [signal.SIGTERM, signal.SIGKILL])
            finally:
                if processes and processes[-1][0].poll() is None:
                    process, start_time = processes[-1]
                    process_supervisor.signal_process(process.pid, start_time, signal.SIGKILL)
                    process.wait(timeout=5)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_linked_worktree_cleanup_retries_snapshot_failure_before_termination(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_git = root / "git-retry-snapshot"
            fake_git.write_text(f"#!{sys.executable}\nimport time\ntime.sleep(30)\n", encoding="utf-8")
            fake_git.chmod(0o755)
            cleanup_calls = 0

            def fail_once_then_cleanup(root_pid: int, *, reap_adopted: bool, root_start_time: int) -> None:
                nonlocal cleanup_calls
                cleanup_calls += 1
                if cleanup_calls == 1:
                    raise process_supervisor.ProcessSnapshotError("transient snapshot failure")
                process_supervisor.kill_descendants(root_pid, reap_adopted=reap_adopted, root_start_time=root_start_time)

            with mock.patch.object(release_gate, "kill_descendants", side_effect=fail_once_then_cleanup):
                with self.assertRaisesRegex(release_gate.GateError, "cleanup could not be verified"):
                    release_gate._bounded_git_worktree_listing(root, fake_git)
            self.assertEqual(cleanup_calls, 2)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_linked_worktree_cleanup_does_not_signal_without_root_identity(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_git = root / "git-identity-unavailable"
            fake_git.write_text(f"#!{sys.executable}\n", encoding="utf-8")
            fake_git.chmod(0o755)

            with mock.patch.object(release_gate, "process_start_time", side_effect=process_supervisor.CleanupError("unavailable")), mock.patch.object(
                release_gate, "kill_descendants"
            ) as kill_descendants:
                with self.assertRaisesRegex(release_gate.GateError, "cleanup could not be verified"):
                    release_gate._bounded_git_worktree_listing(root, fake_git)

            kill_descendants.assert_not_called()

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_linked_worktree_cleanup_does_not_retry_non_snapshot_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            fake_git = root / "git-no-retry"
            fake_git.write_text(f"#!{sys.executable}\nimport time\ntime.sleep(30)\n", encoding="utf-8")
            fake_git.chmod(0o755)

            with mock.patch.object(
                release_gate, "kill_descendants", side_effect=process_supervisor.CleanupError("permanent cleanup failure")
            ) as kill_descendants:
                with self.assertRaisesRegex(release_gate.GateError, "cleanup could not be verified"):
                    release_gate._bounded_git_worktree_listing(root, fake_git)

            kill_descendants.assert_called_once()

    def test_gate_start_identity_failure_stops_owned_supervisor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "identity-marker"
            (root / "sleep.py").write_text(
                "import pathlib, time\n"
                "time.sleep(5)\n"
                f"pathlib.Path({str(marker)!r}).write_text('leaked')\n",
                encoding="utf-8",
            )
            gate = {
                "id": "identity-failure",
                "description": "identity failure",
                "subsystems": ["governance"],
                "required": True,
                "cwd": ".",
                "argv": ["python3", "sleep.py"],
                "timeout_seconds": 5,
                "artifacts": [],
                "evidence_limit": "unit-verified",
            }

            with mock.patch.object(release_gate, "has_authenticated_sandbox_capability", return_value=True), mock.patch.object(
                release_gate, "process_start_time", side_effect=process_supervisor.CleanupError("unavailable")
            ):
                result = release_gate.run_gate(root, gate)

            self.assertEqual(result["status"], "blocked")
            time.sleep(0.5)
            self.assertFalse(marker.exists())

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux supervisor cleanup contract")
    def test_gate_cleanup_failure_still_escalates_supervisor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "sleep.py").write_text("import time\ntime.sleep(30)\n", encoding="utf-8")
            gate = {
                "id": "cleanup-failure",
                "description": "cleanup failure",
                "subsystems": ["governance"],
                "required": True,
                "cwd": ".",
                "argv": ["python3", "sleep.py"],
                "timeout_seconds": 1,
                "artifacts": [],
                "evidence_limit": "unit-verified",
            }
            signals: list[int] = []
            processes: list[tuple[subprocess.Popen[bytes], int]] = []

            def signal_without_term(process: subprocess.Popen[bytes], start_time: int | None, signum: int) -> bool:
                self.assertIsNotNone(start_time)
                assert start_time is not None
                processes.append((process, start_time))
                signals.append(signum)
                if signum == signal.SIGKILL:
                    return process_supervisor.signal_process(process.pid, start_time, signum)
                return True

            try:
                with mock.patch.object(release_gate, "has_authenticated_sandbox_capability", return_value=True), mock.patch.object(
                    release_gate, "kill_descendants", side_effect=process_supervisor.CleanupError("snapshot unavailable")
                ) as kill_descendants, mock.patch.object(release_gate, "_signal_supervisor", side_effect=signal_without_term):
                    result = release_gate.run_gate(root, gate)
                    self.assertEqual(result["status"], "blocked")
                    self.assertIn("cleanup", result["error"])
                    self.assertEqual(signals, [signal.SIGTERM, signal.SIGKILL])
                    kill_descendants.assert_called_once()
            finally:
                if processes and processes[-1][0].poll() is None:
                    process, start_time = processes[-1]
                    process_supervisor.signal_process(process.pid, start_time, signal.SIGKILL)
                    process.wait(timeout=5)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux supervisor cleanup contract")
    def test_gate_cleanup_failure_retries_descendant_cleanup_before_escalation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pid_file = root / "descendant.pid"
            ready = root / "descendant.ready"
            marker = root / "descendant-marker"
            (root / "forker.py").write_text(
                "import os, pathlib, time\n"
                "child = os.fork()\n"
                "if child == 0:\n"
                "    os.setsid()\n"
                f"    pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
                f"    pathlib.Path({str(ready)!r}).write_text('ready')\n"
                "    time.sleep(2)\n"
                f"    pathlib.Path({str(marker)!r}).write_text('leaked')\n"
                "else:\n"
                "    time.sleep(30)\n",
                encoding="utf-8",
            )
            gate = {
                "id": "cleanup-retry",
                "description": "cleanup retry",
                "subsystems": ["governance"],
                "required": True,
                "cwd": ".",
                "argv": ["python3", "forker.py"],
                "timeout_seconds": 1,
                "artifacts": [],
                "evidence_limit": "unit-verified",
            }
            cleanup_calls = 0
            original_cleanup = process_supervisor.kill_descendants

            def fail_once_then_cleanup(root_pid: int, *, reap_adopted: bool, root_start_time: int) -> None:
                nonlocal cleanup_calls
                cleanup_calls += 1
                if cleanup_calls == 1:
                    raise process_supervisor.ProcessSnapshotError("transient snapshot failure")
                original_cleanup(root_pid, reap_adopted=reap_adopted, root_start_time=root_start_time)

            with mock.patch.object(release_gate, "has_authenticated_sandbox_capability", return_value=True), mock.patch.object(
                release_gate, "kill_descendants", side_effect=fail_once_then_cleanup
            ):
                result = release_gate.run_gate(root, gate)

            self.assertEqual(result["status"], "blocked")
            self.assertEqual(cleanup_calls, 2)
            deadline = time.monotonic() + 3
            while not ready.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertTrue(ready.exists(), "forked descendant did not start")
            time.sleep(2.5)
            self.assertFalse(marker.exists())
            if pid_file.exists():
                self.assertFalse(process_supervisor._read_process_start_time(int(pid_file.read_text(encoding="utf-8"))))

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_fails_closed_when_snapshot_is_unavailable(self) -> None:
        with mock.patch.object(process_supervisor, "_process_snapshot", side_effect=process_supervisor.ProcessSnapshotError("unavailable")):
            with self.assertRaises(process_supervisor.CleanupError):
                process_supervisor.kill_descendants(os.getpid())

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_fails_closed_when_root_identity_changes(self) -> None:
        root_pid = os.getpid()
        with mock.patch.object(process_supervisor, "_process_snapshot", return_value={root_pid: (1, 456)}):
            with self.assertRaisesRegex(process_supervisor.CleanupError, "root process identity"):
                process_supervisor.kill_descendants(root_pid, root_start_time=123)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_fails_closed_when_root_disappears(self) -> None:
        root_pid = os.getpid()
        with mock.patch.object(process_supervisor, "_process_snapshot", return_value={}):
            with self.assertRaisesRegex(process_supervisor.CleanupError, "root process identity"):
                process_supervisor.kill_descendants(root_pid, root_start_time=123)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_fails_closed_on_malformed_process_identity(self) -> None:
        class ProcEntry:
            name = "123"
            path = "/proc/123"

        with mock.patch.object(process_supervisor.os, "scandir") as scandir, mock.patch(
            "builtins.open", mock.mock_open(read_data=b"malformed")
        ):
            scandir.return_value.__enter__.return_value = [ProcEntry()]
            with self.assertRaises(process_supervisor.ProcessSnapshotError):
                process_supervisor.kill_descendants(os.getpid())

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_bounds_adopted_child_reaping(self) -> None:
        with mock.patch.object(process_supervisor, "MAX_REAPED_CHILDREN", 2), mock.patch.object(
            process_supervisor.os, "waitpid", return_value=(0, 0)
        ) as waitpid:
            with self.assertRaisesRegex(process_supervisor.CleanupError, "bounded reap size"):
                process_supervisor._reap_adopted_children([101, 102, 103])
        waitpid.assert_not_called()

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_preserves_protected_child_wait_status(self) -> None:
        child = subprocess.Popen([sys.executable, "-c", "raise SystemExit(42)"])
        try:
            deadline = time.monotonic() + 5
            while child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertIsNotNone(child.poll(), "protected child did not exit")
            process_supervisor.kill_descendants(os.getpid(), protected_pids={child.pid})
            self.assertEqual(child.wait(timeout=1), 42)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=1)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux process cleanup contract")
    def test_process_cleanup_does_not_signal_reused_pid(self) -> None:
        pid = os.getpid()
        snapshot = {pid: (1, 123)}
        with mock.patch.object(process_supervisor, "_open_pidfd", return_value=os.open("/dev/null", os.O_RDONLY)) as open_pidfd, mock.patch.object(
            process_supervisor, "_read_process_start_time", return_value=456
        ) as read_start, mock.patch.object(process_supervisor, "_send_pidfd_signal") as send_signal:
            process_supervisor._kill_if_same_process(pid, 123, snapshot)
        open_pidfd.assert_called_once_with(pid)
        read_start.assert_called_once_with(pid)
        send_signal.assert_not_called()

    def test_git_metadata_mount_rejects_replaced_path_after_validation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / ".git").mkdir()
            mounts, identities, anchors = release_gate._validated_git_metadata_mounts(root)
            try:
                shutil.rmtree(root / ".git")
                (root / ".git").mkdir()
                with self.assertRaisesRegex(release_gate.GateError, "changed before"):
                    release_gate._open_anchored_metadata(root, mounts, identities, anchors)
            finally:
                release_gate._close_metadata_anchors(anchors)

    def test_gate_cannot_write_repository_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "tamper_git.py").write_text(
                "from pathlib import Path\n"
                "Path('.git/voidtower-tamper').write_text('unexpected')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "git-tamper", "description": "git metadata tamper", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "tamper_git.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "failed")
            self.assertFalse((root / ".git" / "voidtower-tamper").exists())

    def test_cleanup_private_home_removes_read_only_nested_snapshots(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            private_home = Path(temporary) / "private-home"
            nested = private_home / "npm" / "lib"
            nested.mkdir(parents=True)
            (nested / "cli.js").write_text("cli\n", encoding="utf-8")
            (nested / "cli.js").chmod(0o555)
            nested.chmod(0o555)
            (private_home / "npm").chmod(0o555)
            private_home.chmod(0o555)

            release_gate.cleanup_private_home(private_home)

            self.assertFalse(private_home.exists())

    def test_gate_environment_fails_closed_when_selected_tool_disappears(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".hermes" / "bin"
            fixed = Path(temporary) / "fixed"
            toolchain.mkdir(parents=True)
            fixed.mkdir()
            selected_cargo = toolchain / "cargo"
            selected_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            fallback_cargo = fixed / "cargo"
            fallback_cargo.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            fallback_cargo.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.object(release_gate, "SAFE_TOOL_DIRECTORIES", (fixed,)), mock.patch.dict(
                os.environ, {"PATH": os.pathsep.join((str(toolchain), str(fixed)))}, clear=False
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)
                selected_cargo.unlink()
                result = subprocess.run(
                    ["cargo"],
                    env=environment,
                    cwd=root,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    text=True,
                )

            self.assertEqual(result.returncode, 126)
            self.assertIn("selected tool unavailable", result.stderr)

            selected_cargo.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            replaced = subprocess.run(
                ["/bin/sh", "-c", "cargo"],
                env=environment,
                cwd=root,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
                text=True,
            )

            self.assertEqual(replaced.returncode, 126)
            self.assertIn("selected tool unavailable", replaced.stderr)

    def test_gate_environment_fails_closed_when_private_launcher_disappears(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".hermes" / "bin"
            fixed = Path(temporary) / "fixed"
            toolchain.mkdir(parents=True)
            fixed.mkdir()
            selected_cargo = toolchain / "cargo"
            selected_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            fallback_cargo = fixed / "cargo"
            fallback_cargo.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            fallback_cargo.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.object(release_gate, "SAFE_TOOL_DIRECTORIES", (fixed,)), mock.patch.dict(
                os.environ, {"PATH": os.pathsep.join((str(toolchain), str(fixed)))}, clear=False
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)
                private_home.chmod(0o755)
                (private_home / "bin").chmod(0o700)
                (private_home / "bin" / "cargo").unlink()
                result = subprocess.run(
                    ["/bin/sh", "-c", "cargo"],
                    env=environment,
                    cwd=root,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    text=True,
                )

            self.assertEqual(result.returncode, 127)
            self.assertIn("not found", result.stderr)

    def test_gate_environment_fails_closed_when_bound_tool_bytes_change(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".hermes" / "bin"
            toolchain.mkdir(parents=True)
            selected_cargo = toolchain / "cargo"
            selected_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.dict(os.environ, {"PATH": str(toolchain)}, clear=False):
                environment = release_gate.gate_environment(root, "marker", private_home)
                bound_cargo = private_home / "bound" / "cargo"
                private_home.chmod(0o755)
                bound_cargo.chmod(0o755)
                bound_cargo.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
                os.utime(bound_cargo, ns=(bound_cargo.stat().st_atime_ns, selected_cargo.stat().st_mtime_ns))
                result = subprocess.run(
                    ["/bin/sh", "-c", "cargo"],
                    env=environment,
                    cwd=root,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    check=False,
                    text=True,
                )

            self.assertEqual(result.returncode, 126)
            self.assertIn("selected tool unavailable", result.stderr)

    def test_gate_environment_rejects_tool_replaced_after_selection(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            selected = root / "toolchain" / "cargo"
            selected.parent.mkdir()
            selected.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()
            stale = release_gate.SelectedTool(selected, (0, 0, 1, 1), "0" * 64)

            with self.assertRaisesRegex(release_gate.GateError, "selected tool changed before binding"):
                release_gate.gate_environment(root, "marker", private_home, {"cargo": stale})

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_gate_process_cannot_replace_private_selected_tool_mount(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            toolchain = Path(temporary) / ".cargo" / "bin"
            toolchain.mkdir(parents=True)
            selected_cargo = toolchain / "cargo"
            selected_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            (root / "probe.py").write_text(
                "from pathlib import Path\n"
                "import os\n"
                "import subprocess\n"
                "tool = Path(os.environ['PATH'].split(os.pathsep)[0]) / 'cargo'\n"
                "try:\n"
                "    tool.unlink()\n"
                "    tool.write_text('#!/bin/sh\\nexit 77\\n')\n"
                "    tool.chmod(0o755)\n"
                "    replaced = True\n"
                "except OSError:\n"
                "    replaced = False\n"
                "result = subprocess.run(['cargo'], check=False)\n"
                "print(f'{replaced}:{result.returncode}')\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "sandbox", "description": "immutable tool mount", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "probe.py"], "timeout_seconds": 5}],
            )
            with mock.patch.dict(os.environ, {"PATH": str(toolchain)}, clear=False):
                result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertEqual(report["gates"][0]["stdout"], "False:0\n")
            self.assertEqual(selected_cargo.read_text(encoding="utf-8"), "#!/bin/sh\nexit 0\n")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_gate_process_cannot_see_host_runtime_sockets(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            (root / "probe.py").write_text(
                "from pathlib import Path\n"
                "for path in ('/run/docker.sock', '/var/run/docker.sock'):\n"
                "    if Path(path).exists():\n"
                "        raise SystemExit(path)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "runtime-sockets", "description": "runtime sockets", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "probe.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_forged_nested_marker_cannot_bypass_gate_namespace(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            socket_root = Path(temporary) / "host-socket"
            socket_root.mkdir()
            socket_path = socket_root / "sentinel.sock"
            sentinel = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sentinel.bind(socket_path.as_posix())
            try:
                (root / "probe.py").write_text(
                    "from pathlib import Path\n"
                    f"raise SystemExit(1 if Path({socket_path.as_posix()!r}).exists() else 0)\n",
                    encoding="utf-8",
                )
                manifest = self.write_manifest(
                    root,
                    [{"id": "forged-marker", "description": "forged nested marker", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "probe.py"], "timeout_seconds": 5}],
                )
                environment = os.environ.copy()
                environment[release_gate.SANDBOX_DEPTH_ENV] = "1"

                result = self.run_runner(root, manifest, environment=environment)

                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")
            finally:
                sentinel.close()

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_recursive_runner_reuses_authenticated_containing_namespace(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            socket_path = Path("/tmp") / f"voidtower-recursive-sentinel-{uuid.uuid4().hex}.sock"
            sentinel = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sentinel.bind(socket_path.as_posix())
            try:
                (root / "release_gate.py").write_text(SCRIPT.read_text(encoding="utf-8"), encoding="utf-8")
                (root / "repo_truth.py").write_text((SCRIPT.parent / "repo_truth.py").read_text(encoding="utf-8"), encoding="utf-8")
                (root / "process_supervisor.py").write_text((SCRIPT.parent / "process_supervisor.py").read_text(encoding="utf-8"), encoding="utf-8")
                (root / "pass.py").write_text(
                    "from pathlib import Path\n"
                    f"raise SystemExit(1 if Path({socket_path.as_posix()!r}).exists() else 0)\n",
                    encoding="utf-8",
                )
                (root / "nested.py").write_text(
                    "import subprocess, sys\n"
                    "result = subprocess.run([sys.executable, 'release_gate.py', '--repo', '.', '--manifest', 'nested-manifest.json', '--scope', 'all', '--json'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)\n"
                    "print(result.stderr, end='')\n"
                    "raise SystemExit(result.returncode)\n",
                    encoding="utf-8",
                )
                nested_manifest = self.write_manifest(
                    root,
                    [{"id": "nested-probe", "description": "nested namespace probe", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
                )
                nested_manifest.rename(root / "nested-manifest.json")
                manifest = self.write_manifest(
                    root,
                    [{"id": "recursive", "description": "recursive runner", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "nested.py"], "timeout_seconds": 20}],
                )

                result = self.run_runner(root, manifest)

                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")
            finally:
                sentinel.close()
                socket_path.unlink(missing_ok=True)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_gate_process_cannot_see_host_temporary_sockets(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            socket_paths: list[Path] = []
            sockets: list[socket.socket] = []
            try:
                for parent in (Path("/tmp"), Path("/var/tmp"), Path("/dev/shm")):
                    socket_path = parent / f"voidtower-gate-sentinel-{uuid.uuid4().hex}.sock"
                    listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    listener.bind(socket_path.as_posix())
                    socket_paths.append(socket_path)
                    sockets.append(listener)
                (root / "probe.py").write_text(
                    "from pathlib import Path\n"
                    f"paths = {[path.as_posix() for path in socket_paths]!r}\n"
                    "raise SystemExit(1 if any(Path(path).exists() for path in paths) else 0)\n",
                    encoding="utf-8",
                )
                manifest = self.write_manifest(
                    root,
                    [{"id": "temporary-sockets", "description": "temporary sockets", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "probe.py"], "timeout_seconds": 5}],
                )

                result = self.run_runner(root, manifest)

                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")
            finally:
                for listener in sockets:
                    listener.close()
                for socket_path in socket_paths:
                    socket_path.unlink(missing_ok=True)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_gate_process_cannot_read_host_process_environments(self) -> None:
        if shutil.which("bwrap") is None:
            self.skipTest("bubblewrap unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            make_checkout(root)
            sentinel = f"voidtower-proc-sentinel-{uuid.uuid4().hex}"
            (root / "probe.py").write_text(
                "from pathlib import Path\n"
                f"sentinel = {sentinel!r}.encode()\n"
                "for environ in Path('/proc').glob('*/environ'):\n"
                "    try:\n"
                "        if sentinel in environ.read_bytes():\n"
                "            raise SystemExit(1)\n"
                "    except (OSError, PermissionError):\n"
                "        continue\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "procfs", "description": "private procfs", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "probe.py"], "timeout_seconds": 5}],
            )
            environment = os.environ.copy()
            environment["VOIDTOWER_PROC_SENTINEL"] = sentinel

            result = self.run_runner(root, manifest, environment=environment)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertEqual(json.loads(result.stdout)["gates"][0]["status"], "passed")

    def test_npm_support_library_is_copied_and_bound(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".hermes" / "node" / "bin"
            library = toolchain.parent / "lib"
            toolchain.mkdir(parents=True)
            library.mkdir()
            npm = toolchain / "npm"
            npm.write_text("#!/usr/bin/env node\n", encoding="utf-8")
            npm.chmod(0o755)
            (library / "package.js").write_text("export const bound = true;\n", encoding="utf-8")
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.dict(os.environ, {"PATH": str(toolchain)}, clear=False):
                environment = release_gate.gate_environment(root, "marker", private_home)

            copied_library = private_home / "npm" / "lib"
            self.assertEqual(environment["PATH"].split(os.pathsep)[0], str(private_home / "bin"))
            self.assertTrue(copied_library.is_dir())
            self.assertFalse((private_home / "npm").is_symlink())
            self.assertEqual((copied_library / "package.js").read_text(encoding="utf-8"), "export const bound = true;\n")
            self.assertEqual((copied_library / "package.js").stat().st_mode & stat.S_IWUSR, 0)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux bubblewrap gate sandbox")
    def test_real_npm_gate_uses_the_snapshotted_package_layout(self) -> None:
        if (
            not self.original_path
            or shutil.which("npm", path=self.original_path) is None
            or shutil.which("bwrap", path=self.original_path) is None
        ):
            self.skipTest("npm or bubblewrap unavailable on the original worker PATH")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            manifest = self.write_manifest(
                root,
                [{"id": "npm-version", "description": "npm snapshot", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["npm", "--version"], "timeout_seconds": 10}],
            )

            environment = os.environ.copy()
            environment["PATH"] = self.original_path
            result = self.run_runner(root, manifest, environment=environment)

            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            report = json.loads(result.stdout)
            self.assertRegex(report["gates"][0]["stdout"], r"^\d+\.\d+\.\d+\n$")

    def test_gate_environment_does_not_expose_inherited_toolchain_siblings(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            toolchain = Path(temporary) / ".cargo" / "bin"
            toolchain.mkdir(parents=True)
            selected_cargo = toolchain / "cargo"
            selected_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            selected_cargo.chmod(0o755)
            shadowed_bash = toolchain / "bash"
            shadowed_bash.write_text("#!/bin/sh\nexit 77\n", encoding="utf-8")
            shadowed_bash.chmod(0o755)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.dict(os.environ, {"PATH": str(toolchain)}, clear=False):
                environment = release_gate.gate_environment(root, "marker", private_home)

            path_entries = environment["PATH"].split(os.pathsep)
            self.assertNotIn(str(toolchain), path_entries)
            self.assertTrue((private_home / "bin" / "cargo").is_file())
            self.assertFalse((private_home / "bin" / "bash").exists())

    def test_gate_environment_rejects_insecure_fixed_tool_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout"
            root.mkdir()
            fixed = Path(temporary) / "fixed"
            fixed.mkdir()
            cargo = fixed / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            cargo.chmod(0o755)
            fixed.chmod(0o777)
            private_home = root / "private-home"
            private_home.mkdir()

            with mock.patch.object(release_gate, "SAFE_TOOL_DIRECTORIES", (fixed,)), mock.patch.dict(
                os.environ, {"PATH": ""}, clear=False
            ):
                environment = release_gate.gate_environment(root, "marker", private_home)

            self.assertNotIn(str(fixed), environment["PATH"].split(os.pathsep))
            if "CARGO_HOME" in environment:
                self.assertEqual(Path(environment["CARGO_HOME"]), private_home / "cargo-home")

    def test_interrupted_run_persists_failed_partial_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "sleep.py").write_text("import time\ntime.sleep(30)\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "interruptible", "description": "interruptible fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "sleep.py"], "timeout_seconds": 30}],
            )
            report_path = root / "dev-data" / "interrupted.json"
            process = subprocess.Popen(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", "dev-data/interrupted.json", "--json"],
                cwd=root,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            time.sleep(0.5)
            process.send_signal(signal.SIGTERM)
            stdout, stderr = process.communicate(timeout=5)

            self.assertEqual(process.returncode, 1, stderr)
            self.assertTrue(report_path.is_file(), stdout)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertIn("interrupted", report["gates"][0]["error"])

    def test_interrupted_preflight_reuses_completed_truth_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "preflight-interrupt", "description": "preflight interrupt", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            report_path = root / "dev-data" / "preflight-interrupt.json"
            truth = {
                "check": {"status": "passed"},
                "changed_subsystems": [],
                "git": {
                    "ahead": None,
                    "behind": None,
                    "branch": "dev",
                    "impact_paths": [],
                    "modified_paths": [],
                    "path_semantics": {},
                    "staged_paths": [],
                },
            }
            build_truth_calls = 0

            def interrupt_after_preflight(_root: Path) -> dict[str, object]:
                nonlocal build_truth_calls
                build_truth_calls += 1
                release_gate.shutdown_requested = True
                return truth

            release_gate.shutdown_requested = False
            try:
                with mock.patch.object(release_gate, "build_truth", side_effect=interrupt_after_preflight), mock.patch.object(
                    sys,
                    "argv",
                    [
                        str(SCRIPT),
                        "--repo",
                        str(root),
                        "--manifest",
                        str(manifest),
                        "--output",
                        "dev-data/preflight-interrupt.json",
                        "--json",
                    ],
                ):
                    result = release_gate.main()
            finally:
                release_gate.shutdown_requested = False

            self.assertEqual(result, 1)
            self.assertEqual(build_truth_calls, 1)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertEqual(report["gates"][0]["status"], "blocked")
            self.assertIn("before gate execution", report["gates"][0]["error"])
            self.assertEqual(report["skipped_gates"][0]["id"], "preflight-interrupt")

    @unittest.skipUnless(sys.platform.startswith("linux"), "Linux supervisor cleanup contract")
    def test_interrupted_gate_waits_for_forked_descendants_before_reporting(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            pid_file = root / "descendant.pid"
            marker = root / "descendant-marker"
            (root / "descendant.py").write_text(
                "import os, pathlib, time\n"
                f"pathlib.Path({str(pid_file)!r}).write_text(str(os.getpid()))\n"
                "os.setsid()\n"
                "time.sleep(10)\n"
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
                [{"id": "forker", "description": "interrupt cleanup", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "forker.py"], "timeout_seconds": 30}],
            )
            runner = subprocess.Popen(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--json"],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                deadline = time.monotonic() + 15
                while not pid_file.exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(pid_file.exists(), "forked descendant did not start")
                runner.send_signal(signal.SIGTERM)
                stdout, stderr = runner.communicate(timeout=5)
                self.assertEqual(runner.returncode, 1, stderr)
                self.assertEqual(json.loads(stdout)["status"], "failed")
                time.sleep(1.0)
                self.assertFalse(marker.exists(), "interrupted gate descendant outlived the runner")
            finally:
                if runner.poll() is None:
                    runner.kill()
                    runner.wait(timeout=5)
                if runner.stdout is not None:
                    runner.stdout.close()
                if runner.stderr is not None:
                    runner.stderr.close()

    def test_gate_termination_after_final_gate_still_fails_the_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "signal_runner.py").write_text(
                "import os, signal\n"
                "os.kill(os.getpid(), signal.SIGTERM)\n",
                encoding="utf-8",
            )
            manifest = self.write_manifest(
                root,
                [{"id": "interrupt-after-final", "description": "interrupt after final gate", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "signal_runner.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "failed")

    def test_interrupt_during_report_close_window_rewrites_failed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "close-window", "description": "close window", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            report_path = root / "dev-data/close-window.json"
            calls = 0

            def pending_after_close_check() -> bool:
                nonlocal calls
                calls += 1
                if calls == 2:
                    release_gate.shutdown_requested = True
                    return False
                return release_gate.shutdown_requested

            release_gate.shutdown_requested = False
            with mock.patch.object(release_gate, "shutdown_pending", side_effect=pending_after_close_check), mock.patch.object(
                sys,
                "argv",
                [
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--output",
                    "dev-data/close-window.json",
                    "--json",
                ],
            ):
                result = release_gate.main()

            self.assertEqual(result, 1)
            self.assertEqual(json.loads(report_path.read_text(encoding="utf-8"))["status"], "failed")
            release_gate.shutdown_requested = False

    def test_interrupted_report_recovery_rejects_replaced_nonregular_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report_path = evidence_dir / "replaced.json"
            report_path.write_text("original\n", encoding="utf-8")
            original_identity = report_path.stat()
            report_path.unlink()
            os.mkfifo(report_path)

            with self.assertRaises(OSError):
                release_gate.rewrite_persisted_report(
                    root,
                    report_path,
                    "recovered\n",
                    release_gate.report_identity(original_identity),
                )
            self.assertTrue(report_path.is_fifo())

    def test_interrupted_report_recovery_preserves_replaced_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            target = root / "replacement-target.txt"
            target.write_text("replacement\n", encoding="utf-8")
            report_path = evidence_dir / "replaced.json"
            report_path.write_text("original\n", encoding="utf-8")
            original_identity = report_path.stat()
            report_path.unlink()
            report_path.symlink_to(target)

            with self.assertRaises(OSError):
                release_gate.rewrite_persisted_report(
                    root,
                    report_path,
                    "recovered\n",
                    release_gate.report_identity(original_identity),
                )

            self.assertTrue(report_path.is_symlink())
            self.assertEqual(report_path.resolve(), target)

    def test_interrupted_report_recovery_preserves_replaced_regular_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report_path = evidence_dir / "replaced.json"
            report_path.write_text("original\n", encoding="utf-8")
            original_identity = report_path.stat()
            report_path.unlink()
            report_path.write_text("replacement\n", encoding="utf-8")

            with self.assertRaises(GateError):
                release_gate.rewrite_persisted_report(
                    root,
                    report_path,
                    "recovered\n",
                    release_gate.report_identity(original_identity),
                )

            self.assertEqual(report_path.read_text(encoding="utf-8"), "replacement\n")

    def test_report_invalidation_preserves_quarantine_replacement(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report_path = evidence_dir / "replaced.json"
            report_path.write_text("original\n", encoding="utf-8")
            original_identity = report_path.stat()
            parent_descriptor, filename = release_gate.open_evidence_parent(root, report_path)
            real_rename = release_gate.rename_noreplace
            real_stat = release_gate.os.stat
            quarantine_name = ""
            quarantine_parent = -1
            replaced = False

            def replace_quarantine(parent: int, source: str, destination_parent: int, destination: str) -> None:
                nonlocal quarantine_name, quarantine_parent
                real_rename(parent, source, destination_parent, destination)
                if source == filename and destination_parent != parent:
                    quarantine_name = destination
                    quarantine_parent = destination_parent

            def replace_after_identity_check(path: str, *args: object, **kwargs: object) -> os.stat_result:
                nonlocal replaced
                metadata = real_stat(path, *args, **kwargs)
                if path == filename and kwargs.get("dir_fd") == quarantine_parent and not replaced:
                    replaced = True
                    os.unlink(path, dir_fd=quarantine_parent)
                    replacement = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600, dir_fd=quarantine_parent)
                    try:
                        os.write(replacement, b"replacement\n")
                    finally:
                        os.close(replacement)
                return metadata

            try:
                with mock.patch.object(release_gate, "rename_noreplace", side_effect=replace_quarantine), mock.patch.object(
                    release_gate.os, "stat", side_effect=replace_after_identity_check
                ):
                    self.assertFalse(
                        release_gate.unlink_report_if_identity(
                            parent_descriptor,
                            filename,
                            (original_identity.st_dev, original_identity.st_ino),
                        )
                    )
            finally:
                os.close(parent_descriptor)

            self.assertEqual(report_path.read_text(encoding="utf-8"), "replacement\n")

    def test_report_invalidation_accepts_rename_ctime_change(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report_path = evidence_dir / "rename-ctime.json"
            report_path.write_text("original\n", encoding="utf-8")
            report_path.chmod(0o600)
            original_identity = release_gate.report_identity(report_path.stat())
            parent_descriptor, filename = release_gate.open_evidence_parent(root, report_path)
            real_rename = release_gate.rename_noreplace
            real_stat = release_gate.os.stat
            real_fstat = release_gate.os.fstat
            renamed = False

            class PostRenameCtime:
                def __init__(self, metadata: os.stat_result) -> None:
                    self.metadata = metadata

                def __getattr__(self, name: str) -> object:
                    return getattr(self.metadata, name)

                @property
                def st_ctime_ns(self) -> int:
                    return self.metadata.st_ctime_ns + 1

            def rename_with_ctime_change(parent: int, source: str, destination_parent: int, destination: str) -> None:
                nonlocal renamed
                real_rename(parent, source, destination_parent, destination)
                renamed = True

            def stat_with_ctime_change(path: str, *args: object, **kwargs: object) -> os.stat_result:
                metadata = real_stat(path, *args, **kwargs)
                if renamed and stat.S_ISREG(metadata.st_mode):
                    return PostRenameCtime(metadata)  # type: ignore[return-value]
                return metadata

            def fstat_with_ctime_change(descriptor: int) -> os.stat_result:
                metadata = real_fstat(descriptor)
                if renamed and stat.S_ISREG(metadata.st_mode):
                    return PostRenameCtime(metadata)  # type: ignore[return-value]
                return metadata

            try:
                with mock.patch.object(release_gate, "rename_noreplace", side_effect=rename_with_ctime_change), mock.patch.object(
                    release_gate.os, "stat", side_effect=stat_with_ctime_change
                ), mock.patch.object(release_gate.os, "fstat", side_effect=fstat_with_ctime_change):
                    self.assertTrue(release_gate.unlink_report_if_identity(parent_descriptor, filename, original_identity))
            finally:
                os.close(parent_descriptor)

            self.assertTrue(renamed)
            self.assertFalse(report_path.exists())

    def test_report_invalidation_opens_tombstone_nonblocking(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report_path = evidence_dir / "nonblocking.json"
            report_path.write_text("original\n", encoding="utf-8")
            original_identity = report_path.stat()
            parent_descriptor, filename = release_gate.open_evidence_parent(root, report_path)
            observed_flags: list[int] = []
            real_open = release_gate.os.open

            def observe_open(path: str, flags: int, mode: int = 0, *, dir_fd: int | None = None) -> int:
                if path == filename and flags & os.O_WRONLY:
                    observed_flags.append(flags)
                if dir_fd is None:
                    return real_open(path, flags, mode)
                return real_open(path, flags, mode, dir_fd=dir_fd)

            try:
                with mock.patch.object(release_gate.os, "open", side_effect=observe_open):
                    self.assertTrue(
                        release_gate.unlink_report_if_identity(
                            parent_descriptor,
                            filename,
                            (original_identity.st_dev, original_identity.st_ino),
                        )
                    )
            finally:
                os.close(parent_descriptor)

            self.assertEqual(len(observed_flags), 1)
            self.assertTrue(observed_flags[0] & getattr(os, "O_NONBLOCK", 0))

    def test_open_evidence_parent_rejects_replaced_existing_ancestor(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            root_identity = root.stat()
            parent_identity = evidence_dir.stat()
            replacement = root / "replacement-parent"
            evidence_dir.rename(replacement)
            evidence_dir.mkdir(mode=0o700)

            with self.assertRaisesRegex(GateError, "ancestor identity"):
                release_gate.open_evidence_parent(
                    root,
                    evidence_dir / "report.json",
                    expected_root_identity=(root_identity.st_dev, root_identity.st_ino),
                    expected_ancestor_identities={
                        "dev-data": (parent_identity.st_dev, parent_identity.st_ino),
                    },
                )

    def test_open_evidence_parent_rejects_shared_writable_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            evidence_dir.chmod(0o777)

            with self.assertRaisesRegex(GateError, "owner and mode"):
                release_gate.open_evidence_parent(root, evidence_dir / "report.json")

    def test_public_output_path_matches_rejects_intermediate_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            evidence_dir = root / "dev-data"
            nested = evidence_dir / "nested"
            nested.mkdir(mode=0o700, parents=True)
            report_path = nested / "report.json"
            report_path.write_text("report\n", encoding="utf-8")
            root_identity = root.stat()
            ancestor_identities = release_gate.capture_evidence_ancestor_identities(root, report_path)
            nested_identity = nested.stat()
            report_identity = report_path.stat()
            outside = root.parent / f"{root.name}-outside"
            nested.rename(outside)
            nested.symlink_to(outside, target_is_directory=True)

            self.assertFalse(
                release_gate.public_output_path_matches(
                    root,
                    report_path,
                    (root_identity.st_dev, root_identity.st_ino),
                    (nested_identity.st_dev, nested_identity.st_ino),
                    (report_identity.st_dev, report_identity.st_ino, 0o644),
                    ancestor_identities,
                )
            )

    def test_public_output_path_matches_rejects_changed_ancestor_mode(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            evidence_dir = root / "dev-data"
            nested = evidence_dir / "nested"
            nested.mkdir(mode=0o700, parents=True)
            report_path = nested / "report.json"
            report_path.write_text("report\n", encoding="utf-8")
            root_metadata = root.stat()
            parent_metadata = nested.stat()
            ancestor_identities = release_gate.capture_evidence_ancestor_identities(root, report_path)
            report_metadata = report_path.stat()
            nested.chmod(0o777)

            self.assertFalse(
                release_gate.public_output_path_matches(
                    root,
                    report_path,
                    (root_metadata.st_dev, root_metadata.st_ino),
                    (parent_metadata.st_dev, parent_metadata.st_ino),
                    (report_metadata.st_dev, report_metadata.st_ino, 0o644),
                    ancestor_identities,
                )
            )

    def test_interrupted_recovery_failure_returns_without_printing_green_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "recovery-failure", "description": "recovery failure", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            calls = 0

            def pending_once() -> bool:
                nonlocal calls
                calls += 1
                return calls == 1

            with mock.patch.object(release_gate, "shutdown_pending", side_effect=pending_once), mock.patch.object(
                release_gate,
                "rewrite_persisted_report",
                side_effect=OSError("recovery failed"),
            ), mock.patch.object(
                sys,
                "argv",
                [
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--output",
                    "dev-data/recovery-failure.json",
                    "--json",
                ],
            ), mock.patch("builtins.print") as emit:
                result = release_gate.main()

            self.assertEqual(result, 2)
            self.assertFalse(any(call.args and str(call.args[0]).startswith("{") for call in emit.call_args_list))

    def test_interrupted_report_recovery_rejects_replacement_after_write(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            evidence_dir = root / "dev-data"
            evidence_dir.mkdir(mode=0o700)
            report_path = evidence_dir / "replaced-after-write.json"
            report_path.write_text("original\n", encoding="utf-8")
            original_identity = report_path.stat()
            real_writer = release_gate.write_evidence_descriptor

            def replace_after_write(descriptor: int, output: str, *, truncate: bool = False) -> None:
                real_writer(descriptor, output, truncate=truncate)
                report_path.unlink()
                report_path.write_text("replacement\n", encoding="utf-8")

            with mock.patch.object(release_gate, "write_evidence_descriptor", side_effect=replace_after_write):
                with self.assertRaisesRegex(GateError, "replaced during"):
                    release_gate.rewrite_persisted_report(
                        root,
                        report_path,
                        "recovered\n",
                        release_gate.report_identity(original_identity),
                    )

            self.assertEqual(report_path.read_text(encoding="utf-8"), "replacement\n")

    def test_interrupt_during_final_artifact_hash_persists_failed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            artifact = root / "release.tar.gz"
            artifact.write_bytes(b"release artifact")
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "artifact-final", "description": "artifact final phase", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            report_path = root / "dev-data/final-artifact-interrupt.json"
            original_hash = release_gate.hash_contained_file
            calls = 0

            def hash_and_interrupt(checkout: Path, relative: str) -> str:
                nonlocal calls
                calls += 1
                digest = original_hash(checkout, relative)
                if calls == 2:
                    release_gate.request_shutdown(signal.SIGTERM, None)
                return digest

            release_gate.shutdown_requested = False
            with mock.patch.object(release_gate, "hash_contained_file", side_effect=hash_and_interrupt), mock.patch.object(
                sys,
                "argv",
                [
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--artifact",
                    artifact.name,
                    "--output",
                    "dev-data/final-artifact-interrupt.json",
                    "--json",
                ],
            ):
                result = release_gate.main()

            self.assertEqual(result, 1)
            report = json.loads(report_path.read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            release_gate.shutdown_requested = False

    def test_cli_hashes_explicit_release_artifacts_into_top_level_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            artifact = root / "release.tar.gz"
            artifact.write_bytes(b"release artifact")
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{
                    "id": "fixture",
                    "description": "deterministic fixture",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "pass.py"],
                    "timeout_seconds": 5,
                }],
            )
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--artifact",
                    artifact.name,
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "passed")
            self.assertEqual(report["artifacts"][0]["path"], artifact.name)
            self.assertEqual(report["artifacts"][0]["sha256"], "133cfccb5b503cf4040c95f3dfad56d07c1574283a1e39066b594f6ee33711ba")

    def test_cli_rejects_symlinked_release_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            target = root / "real-release.tar.gz"
            target.write_bytes(b"release artifact")
            artifact = root / "release.tar.gz"
            artifact.symlink_to(target)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{
                    "id": "fixture",
                    "description": "deterministic fixture",
                    "subsystems": ["governance"],
                    "required": True,
                    "cwd": ".",
                    "argv": ["python3", "pass.py"],
                    "timeout_seconds": 5,
                }],
            )

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--artifact", artifact.name, "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("symlink", result.stderr)

    def test_cli_rejects_symlinked_parent_release_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            outside = root.parent / f"release-parent-{uuid.uuid4().hex}"
            outside.mkdir()
            try:
                (outside / "release.tar.gz").write_bytes(b"release artifact")
                (root / "artifacts").symlink_to(outside, target_is_directory=True)
                (root / "pass.py").write_text("pass\n", encoding="utf-8")
                manifest = self.write_manifest(
                    root,
                    [{"id": "fixture", "description": "fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
                )

                result = subprocess.run(
                    [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--artifact", "artifacts/release.tar.gz", "--json"],
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertIn("symlink", result.stderr)
            finally:
                outside.joinpath("release.tar.gz").unlink(missing_ok=True)
                outside.rmdir()

    def test_cli_fails_closed_when_a_passing_gate_creates_candidate_changes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            mutating_gate = root / "mutating_gate.py"
            mutating_gate.write_text("from pathlib import Path; Path('README.md').write_text('mutated\\n')\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(root), "add", "mutating_gate.py"], check=True)
            subprocess.run(["git", "-C", str(root), "commit", "-m", "fixture gate"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            manifest = self.write_manifest(
                root,
                [{"id": "mutating", "description": "mutating fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "mutating_gate.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["status"], "failed")
            self.assertIn("README.md", report["git"]["modified_paths"], result.stdout)

    def test_cli_fails_closed_when_a_gate_changes_an_explicit_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            artifact = root / "release.tar.gz"
            artifact.write_bytes(b"original artifact")
            mutating_gate = root / "mutating_gate.py"
            mutating_gate.write_text("from pathlib import Path; Path('release.tar.gz').write_bytes(b'tampered artifact')\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(root), "add", "mutating_gate.py"], check=True)
            subprocess.run(["git", "-C", str(root), "commit", "-m", "fixture gate"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            manifest = self.write_manifest(
                root,
                [{"id": "mutating", "description": "mutating fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "mutating_gate.py"], "timeout_seconds": 5}],
            )

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--artifact",
                    artifact.name,
                    "--output",
                    "dev-data/explicit-artifact-mutation.json",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads((root / "dev-data" / "explicit-artifact-mutation.json").read_text(encoding="utf-8"))
            self.assertEqual(report["status"], "failed")
            self.assertIn("changed during gate execution", report["artifact_error"])

    def test_cli_fails_closed_when_a_gate_changes_an_existing_dirty_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "README.md").write_text("pre-existing candidate edit\n", encoding="utf-8")
            mutating_gate = root / "mutating_gate.py"
            mutating_gate.write_text("from pathlib import Path; Path('README.md').write_text('gate edit\n')\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(root), "add", "mutating_gate.py"], check=True)
            subprocess.run(["git", "-C", str(root), "commit", "-m", "fixture gate"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            manifest = self.write_manifest(
                root,
                [{"id": "mutating", "description": "mutating fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "mutating_gate.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "failed")

    def test_cli_detects_same_size_dirty_path_rewrite_with_restored_mtime(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "README.md").write_bytes(b"12345678\n")
            mutating_gate = root / "mutating_gate.py"
            mutating_gate.write_text(
                "import os\n"
                "from pathlib import Path\n"
                "path = Path('README.md')\n"
                "mtime = path.stat().st_mtime_ns\n"
                "path.write_bytes(b'ABCDEFGH\\n')\n"
                "os.utime(path, ns=(mtime, mtime))\n",
                encoding="utf-8",
            )
            subprocess.run(["git", "-C", str(root), "add", "mutating_gate.py"], check=True)
            subprocess.run(["git", "-C", str(root), "commit", "-m", "fixture gate"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            manifest = self.write_manifest(
                root,
                [{"id": "mutating", "description": "mutating fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "mutating_gate.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "failed")

    def test_cli_fails_closed_when_a_gate_deletes_an_existing_dirty_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "README.md").write_text("pre-existing candidate edit\n", encoding="utf-8")
            mutating_gate = root / "mutating_gate.py"
            mutating_gate.write_text("from pathlib import Path; Path('README.md').unlink()\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(root), "add", "mutating_gate.py"], check=True)
            subprocess.run(["git", "-C", str(root), "commit", "-m", "fixture gate"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            manifest = self.write_manifest(
                root,
                [{"id": "mutating", "description": "mutating fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "mutating_gate.py"], "timeout_seconds": 5}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(json.loads(result.stdout)["status"], "failed")

    def test_report_preserves_explicit_external_gate_classification(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "fixture", "description": "fixture", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
                external_gates=[{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}],
            )

            result = self.run_runner(root, manifest)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["external_gates"], [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}])

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
                "time.sleep(10)\n"
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
                deadline = time.monotonic() + 15
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

    def test_rejects_windows_and_backslash_path_forms(self) -> None:
        for argument in ("C:outside", "\\\\outside", "foo\\\\..\\\\outside"):
            with self.subTest(argument=argument), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                make_checkout(root)
                manifest = self.write_manifest(
                    root,
                    [
                        {
                            "id": "unsafe-path",
                            "description": "cross-platform external path",
                            "subsystems": ["governance"],
                            "required": True,
                            "cwd": ".",
                            "argv": ["python3", "-m", "unittest", "discover", "-s", argument],
                            "timeout_seconds": 5,
                        }
                    ],
                )

                result = self.run_runner(root, manifest)

                self.assertEqual(result.returncode, 2)
                self.assertIn("path outside", result.stderr)

    def test_rejects_windows_and_backslash_working_directories(self) -> None:
        for cwd in ("C:outside", "C:/outside", "\\\\outside", "foo\\\\..\\\\outside"):
            with self.subTest(cwd=cwd), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                make_checkout(root)
                (root / "pass.py").write_text("pass\n", encoding="utf-8")
                manifest = self.write_manifest(
                    root,
                    [
                        {
                            "id": "unsafe-cwd",
                            "description": "cross-platform external cwd",
                            "subsystems": ["governance"],
                            "required": True,
                            "cwd": cwd,
                            "argv": ["python3", "pass.py"],
                            "timeout_seconds": 5,
                        }
                    ],
                )

                result = self.run_runner(root, manifest)

                self.assertEqual(result.returncode, 2)
                self.assertIn("cwd", result.stderr)

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
            output = root / "dev-data" / "report.json"
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", str(output), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout), json.loads(output.read_text(encoding="utf-8")))

    def test_rejects_existing_evidence_output_without_overwriting_it(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "governance", "description": "governance", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            output = root / "dev-data" / "report.json"
            output.parent.mkdir()
            output.write_text("do not overwrite\n", encoding="utf-8")

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", str(output), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("already exists", result.stderr)
            self.assertEqual(output.read_text(encoding="utf-8"), "do not overwrite\n")

    def test_rejects_dangling_symlink_evidence_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            manifest = self.write_manifest(
                root,
                [{"id": "governance", "description": "governance", "subsystems": ["governance"], "required": True, "cwd": ".", "argv": ["python3", "pass.py"], "timeout_seconds": 5}],
            )
            output = root / "dev-data" / "report.json"
            output.parent.mkdir()
            output.symlink_to(root / "not-created.json")

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--manifest", str(manifest), "--output", str(output), "--json"],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("already exists", result.stderr)
            self.assertFalse((root / "not-created.json").exists())


if __name__ == "__main__":
    unittest.main()
