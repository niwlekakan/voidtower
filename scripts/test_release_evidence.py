#!/usr/bin/env python3
"""Contract tests for release archive/report publication validation."""

from __future__ import annotations

import base64
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import stat
import tarfile
import tempfile
import time
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).parent))
from scripts import release_gate
from scripts import validate_release_evidence
from scripts.validate_release_evidence import EvidenceError, load_manifest, read_text_bounded
from scripts.test_repo_truth import make_checkout


SCRIPT = Path(__file__).with_name("validate_release_evidence.py")
EXTERNAL_HELPER = Path(__file__).with_name("write_external_gate_evidence.py")
EXTRACTOR = Path(__file__).with_name("extract_release_binary.py")
WORKFLOW = Path(__file__).parents[1] / ".github/workflows/release.yml"
CI_WORKFLOW = Path(__file__).parents[1] / ".github/workflows/ci.yml"
DOCKER_WORKFLOW = Path(__file__).parents[1] / ".github/workflows/docker.yml"
DEPENDABOT = Path(__file__).parents[1] / ".github/dependabot.yml"
EXPECTED_COMMIT = "a" * 40

# The complete required gate inventory, frozen as a reviewed contract. Adding,
# removing, renaming, or demoting any gate in scripts/release-gates.json must be
# a visible change here; `all(gate["required"])` alone cannot observe a deletion.
REQUIRED_GATE_IDS = (
    "repository-truth-tests",
    "repository-truth-check",
    "compose-contract",
    "repository-prerequisites",
    "action-schema-catalog",
    "api-contract-drift",
    "release-evidence-contracts",
    "agent-package-contracts",
    "agent-rollback-contracts",
    "repository-hygiene",
    "schema-migration-ownership",
    "g5-plaintext-scan",
    "schema-tests",
    "backend-tests",
    "backend-clippy",
    "backend-format",
    "supply-chain",
    "frontend-tests",
    "frontend-type-check",
    "frontend-lint",
    "frontend-build",
    "standalone-mcp-dependency-contracts",
    "standalone-mcp-tests",
    "diff-check",
)


class ReleaseEvidenceValidatorTests(unittest.TestCase):
    def test_malformed_evidence_validation_closes_retained_descriptors(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            report = Path(temporary) / "malformed.json"
            report.write_text("{", encoding="utf-8")
            before = len(list(Path("/proc/self/fd").iterdir()))
            for _ in range(25):
                with self.assertRaises(EvidenceError):
                    validate_release_evidence.load_report(report, EXPECTED_COMMIT)
            after = len(list(Path("/proc/self/fd").iterdir()))
            self.assertLessEqual(after, before + 1)

    def test_validator_fails_closed_when_posix_descriptor_support_is_unavailable(self) -> None:
        evidence_root = Path("evidence")
        manifest_path = Path("manifest.json")
        with mock.patch.object(validate_release_evidence.os, "name", "nt"):
            status, message = validate_release_evidence.validate(
                evidence_root,
                1,
                manifest_path,
                EXPECTED_COMMIT,
            )

        self.assertEqual(status, 1)
        self.assertIn("safe evidence-root validation is unavailable", message)

    def run_validator(
        self,
        root: Path,
        *extra: str,
        external_gates: list[dict[str, object]] | None = None,
        manifest_gate: dict[str, object] | None = None,
        restore_modes: bool = True,
    ) -> subprocess.CompletedProcess[str]:
        manifest_path = root / "scripts/release-gates.json"
        manifest_path.parent.mkdir(exist_ok=True)
        manifest_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "gates": [
                        manifest_gate
                        or {"id": "fixture", "required": True, "subsystems": ["governance"], "argv": ["python3", "-m", "unittest"], "cwd": "."}
                    ],
                    "external_gates": external_gates or [],
                }
            ),
            encoding="utf-8",
        )
        root_mode = root.stat().st_mode & 0o777
        parent_mode = root.parent.stat().st_mode & 0o777
        try:
            return subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--root",
                    str(root),
                    "--manifest",
                    str(manifest_path),
                    "--expected-reports",
                    "2",
                    "--expected-commit",
                    EXPECTED_COMMIT,
                    *extra,
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
        finally:
            if restore_modes:
                os.chmod(root, root_mode)
                os.chmod(root.parent, parent_mode)

    @staticmethod
    def write_report(
        root: Path,
        archive_name: str,
        *,
        digest: str | None = None,
        path: str | None = None,
        status: str = "passed",
        report_name: str | None = None,
        external_gates: list[dict[str, object]] | None = None,
        selected_subsystems: list[str] | None = None,
        git_state: dict[str, object] | None = None,
        extra_fields: dict[str, object] | None = None,
    ) -> None:
        archive = root / archive_name
        actual_digest = hashlib.sha256(archive.read_bytes()).hexdigest() if digest is None else digest
        report = {
            "artifacts": [{"path": archive_name if path is None else path, "sha256": actual_digest}],
            "evidence_scope": "release_gate_execution",
            "external_gates": external_gates or [],
            "gates": [{"id": "fixture", "required": True, "status": "passed", "description": "fixture", "evidence_limit": "unit-verified", "stdout": "", "stderr": "", "artifacts": []}],
            "git": git_state or {"head": EXPECTED_COMMIT, "branch": "release", "staged_paths": [], "modified_paths": []},
            "manifest": "scripts/release-gates.json",
            "platform": {"machine": "x86_64", "system": "Linux", "release": "fixture"},
            "selected_subsystems": selected_subsystems or ["governance"],
            "skipped_gates": [],
            "status": status,
            "schema_version": 1,
        }
        report["gates"][0].update({"argv": ["python3", "-m", "unittest"], "cwd": ".", "exit_code": 0})
        report.update(extra_fields or {})
        name = report_name if report_name is not None else f"release-gate-{archive_name}.json"
        (root / name).write_text(json.dumps(report), encoding="utf-8")

    @staticmethod
    def write_external_report(root: Path, gate_id: str, *, workflow_job: str = "runtime", status: str = "passed", candidate_commit: str = EXPECTED_COMMIT, evidence_file: str | None = None, artifact: str | None = None) -> None:
        name = evidence_file or f"release-external-{gate_id}.json"
        report = {
            "candidate_commit": candidate_commit,
            "command": ["fixture-gate", "--checked"],
            "cwd": ".",
            "evidence_file": name,
            "evidence_scope": "release_external_gate",
            "exit_code": 0,
            "gate_id": gate_id,
            "platform": {"machine": "x86_64", "system": "Linux", "release": "fixture"},
            "schema_version": 1,
            "status": status,
            "workflow_job": workflow_job,
        }
        if artifact is not None:
            report["artifact"] = artifact
            report["artifact_sha256"] = hashlib.sha256((root / artifact).read_bytes()).hexdigest()
        (root / name).write_text(json.dumps(report), encoding="utf-8")

    @staticmethod
    def write_archive(path: Path, members: list[tarfile.TarInfo], payloads: dict[str, bytes]) -> None:
        with tarfile.open(path, "w:gz") as archive:
            for member in members:
                data = payloads.get(member.name, b"")
                if member.isreg():
                    member.size = len(data)
                archive.addfile(member, io.BytesIO(data) if member.isreg() else None)

    def test_extractor_writes_only_the_bounded_regular_binary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "release.tar.gz"
            (root / "extracted").mkdir()
            binary = tarfile.TarInfo("./voidtower")
            frontend = tarfile.TarInfo("./frontend/index.html")
            self.write_archive(
                archive,
                [binary, frontend],
                {"./voidtower": b"packaged binary", "./frontend/index.html": b"frontend"},
            )

            result = subprocess.run(
                [sys.executable, str(EXTRACTOR), "--archive", archive.name, "--output", "extracted/voidtower"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "extracted/voidtower").read_bytes(), b"packaged binary")
            self.assertEqual((root / "extracted/voidtower").stat().st_mode & 0o777, 0o700)

    def test_extractor_rejects_traversal_and_non_regular_members(self) -> None:
        for member in (tarfile.TarInfo("../outside"), tarfile.TarInfo("./voidtower")):
            with self.subTest(member=member.name), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = root / "release.tar.gz"
                if member.name == "./voidtower":
                    member.type = tarfile.SYMTYPE
                    member.linkname = "/etc/passwd"
                self.write_archive(archive, [member], {})
                result = subprocess.run(
                    [sys.executable, str(EXTRACTOR), "--archive", archive.name, "--output", "voidtower"],
                    cwd=root,
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertFalse((root / "voidtower").exists())

    def test_extractor_rejects_duplicate_normalized_members(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "release.tar.gz"
            first = tarfile.TarInfo("./frontend/index.html")
            duplicate = tarfile.TarInfo("frontend/index.html")
            binary = tarfile.TarInfo("voidtower")
            self.write_archive(
                archive,
                [first, duplicate, binary],
                {
                    "./frontend/index.html": b"first",
                    "frontend/index.html": b"duplicate",
                    "voidtower": b"packaged binary",
                },
            )

            result = subprocess.run(
                [sys.executable, str(EXTRACTOR), "--archive", archive.name, "--output", "voidtower"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("duplicate", result.stderr)
            self.assertFalse((root / "voidtower").exists())

    def test_extractor_rejects_symlinked_output_and_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as outside:
            root = Path(temporary)
            archive = root / "release.tar.gz"
            binary = tarfile.TarInfo("voidtower")
            self.write_archive(archive, [binary], {"voidtower": b"packaged binary"})
            outside_file = Path(outside) / "outside"
            outside_file.write_bytes(b"must remain unchanged")
            (root / "voidtower").symlink_to(outside_file)
            result = subprocess.run(
                [sys.executable, str(EXTRACTOR), "--archive", archive.name, "--output", "voidtower"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(outside_file.read_bytes(), b"must remain unchanged")

            (root / "voidtower").unlink()
            (root / "linked").symlink_to(Path(outside), target_is_directory=True)
            result = subprocess.run(
                [sys.executable, str(EXTRACTOR), "--archive", archive.name, "--output", "linked/voidtower"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertFalse((Path(outside) / "voidtower").exists())

    def test_runner_and_validator_use_the_same_trusted_script_argv(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            trusted_script = root / "scripts/check-repository-hygiene.sh"
            trusted_script.chmod(0o755)
            manifest_path = root / "scripts/release-gates.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "hygiene",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["scripts/check-repository-hygiene.sh"],
                                "cwd": ".",
                            }
                        ],
                        "external_gates": [],
                    }
                ),
                encoding="utf-8",
            )

            with mock.patch("scripts.release_gate.os.access", return_value=True), mock.patch("scripts.validate_release_evidence.os.access", return_value=True):
                runner_gates, _ = release_gate.parse_manifest(root, manifest_path)
                validator_gates, _ = load_manifest(manifest_path)

            self.assertEqual(runner_gates[0]["argv"], validator_gates["hygiene"]["report_argv"])

    def test_real_all_scope_report_validates_through_publication_boundary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            (root / "pass.py").write_text("pass\n", encoding="utf-8")
            archive_name = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive_name).write_bytes(b"packaged archive")
            manifest_path = root / "scripts/release-gates.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "fixture",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["python3", "pass.py"],
                                "cwd": ".",
                            }
                        ],
                        "external_gates": [],
                    }
                ),
                encoding="utf-8",
            )
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(
                ["git", "-C", str(root), "commit", "-m", "fixture release candidate"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            candidate = subprocess.run(
                ["git", "-C", str(root), "rev-parse", "HEAD"],
                check=True,
                stdout=subprocess.PIPE,
                text=True,
            ).stdout.strip()
            report_path = root / "dev-data/release-gate-fixture.json"
            runner = subprocess.run(
                [
                    sys.executable,
                    str(release_gate.__file__),
                    "--repo",
                    str(root),
                    "--manifest",
                    str(manifest_path),
                    "--scope",
                    "all",
                    "--artifact",
                    archive_name,
                    "--output",
                    "dev-data/release-gate-fixture.json",
                    "--json",
                ],
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(runner.returncode, 0, runner.stderr)
            self.assertEqual(json.loads(report_path.read_text(encoding="utf-8"))["status"], "passed")
            report_for_publication = root / "release-gate-fixture.json"
            report_for_publication.write_bytes(report_path.read_bytes())
            report_path.unlink()
            validated = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--root",
                    str(root),
                    "--manifest",
                    str(manifest_path),
                    "--expected-reports",
                    "1",
                    "--expected-commit",
                    candidate,
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(validated.returncode, 0, validated.stderr)

    def test_validates_each_archive_against_one_passed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ("voidtower-1-x86_64-unknown-linux-musl.tar.gz", "voidtower-1-aarch64-unknown-linux-musl.tar.gz"):
                (root / name).write_bytes(name.encode("utf-8"))
                self.write_report(root, name)

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("validated 2 release-gate reports and 2 archive digests", result.stdout)

    def test_rejects_dirty_candidate_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(
                root,
                archive,
                git_state={"head": EXPECTED_COMMIT, "branch": "release", "staged_paths": [], "modified_paths": ["README.md"]},
            )

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("candidate checkout is dirty", result.stderr)

    def test_rejects_unknown_passed_report_fields(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive, extra_fields={"unreviewed_metadata": "must not publish"})

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("unknown fields", result.stderr)

    def test_recognizes_failed_artifact_diagnostic_field_before_rejecting_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(
                root,
                archive,
                extra_fields={"artifact_error": "artifact changed during gate execution: release.tar.gz"},
            )

            contradictory = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(contradictory.returncode, 1)
            self.assertIn("cannot accompany a passed report", contradictory.stderr)
            self.assertNotIn("unknown fields", contradictory.stderr)

            self.write_report(
                root,
                archive,
                status="failed",
                extra_fields={"artifact_error": "artifact changed during gate execution: release.tar.gz"},
            )
            failed = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(failed.returncode, 1)
            self.assertIn("release gate is not passed", failed.stderr)
            self.assertNotIn("unknown fields", failed.stderr)

    def test_requires_exact_supported_archive_matrix_for_two_reports(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            unexpected = "voidtower-1-armv7-unknown-linux-musl.tar.gz"
            for archive in (first, unexpected):
                (root / archive).write_bytes(archive.encode("utf-8"))
                self.write_report(root, archive)

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 1)
            self.assertIn("supported archive matrix", result.stderr)

    def test_rejects_archive_reports_from_different_release_versions(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            second = "voidtower-2-aarch64-unknown-linux-musl.tar.gz"
            for archive in (first, second):
                (root / archive).write_bytes(archive.encode("utf-8"))
                self.write_report(root, archive)

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 1)
            self.assertIn("same release version", result.stderr)

    def test_requires_passed_external_gate_evidence_bound_to_manifest_and_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)

            missing = self.run_validator(root, "--expected-reports", "1", external_gates=external)
            self.assertEqual(missing.returncode, 1)
            self.assertIn("external gate evidence", missing.stderr)

            self.write_external_report(root, "runtime")
            valid = self.run_validator(root, "--expected-reports", "1", external_gates=external)
            self.assertEqual(valid.returncode, 0, valid.stderr)

            external_report = root / "release-external-runtime.json"
            payload = json.loads(external_report.read_text(encoding="utf-8"))
            payload["candidate_commit"] = "b" * 40
            external_report.write_text(json.dumps(payload), encoding="utf-8")
            mismatch = self.run_validator(root, "--expected-reports", "1", external_gates=external)
            self.assertEqual(mismatch.returncode, 1)
            self.assertIn("candidate commit", mismatch.stderr)

    def test_runtime_external_evidence_binds_the_exact_packaged_archive(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "golden-path-runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-golden-path-runtime.json", "workflow_job": "golden-path-runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            self.write_external_report(root, "golden-path-runtime", workflow_job="golden-path-runtime", artifact=archive)

            valid = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(valid.returncode, 0, valid.stderr)

            evidence = root / "release-external-golden-path-runtime.json"
            payload = json.loads(evidence.read_text(encoding="utf-8"))
            payload["artifact_sha256"] = "0" * 64
            evidence.write_text(json.dumps(payload), encoding="utf-8")
            mismatch = self.run_validator(root, "--expected-reports", "1", external_gates=external)
            self.assertEqual(mismatch.returncode, 1)
            self.assertIn("external gate artifact digest", mismatch.stderr)

    def test_publication_root_retains_an_immutable_archive_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / archive
            archive_path.write_bytes(b"validated archive")
            self.write_report(root, archive)
            publication_root = root / "published-artifacts"
            report_name = f"release-gate-{archive}.json"
            report_bytes = (root / report_name).read_bytes()
            root_mode = root.stat().st_mode & 0o777
            parent_mode = root.parent.stat().st_mode & 0o777

            source_writer = os.open(archive_path, os.O_WRONLY | os.O_NOFOLLOW)
            try:
                result = self.run_validator(
                    root,
                    "--expected-reports",
                    "1",
                    "--publication-root",
                    publication_root.name,
                    "--checksums-output",
                    "SHA256SUMS",
                    restore_modes=False,
                )

                self.assertEqual(result.returncode, 0, result.stderr)
                published = publication_root / archive
                self.assertTrue(published.is_file())
                self.assertNotEqual(published.stat().st_ino, archive_path.stat().st_ino)
                self.assertEqual(published.stat().st_mode & 0o222, 0)
                self.assertEqual(publication_root.stat().st_mode & 0o222, 0)
                published_report = publication_root / report_name
                self.assertEqual(published_report.read_bytes(), report_bytes)
                self.assertEqual(published_report.stat().st_mode & 0o222, 0)
                self.assertEqual((root / "SHA256SUMS").read_text(encoding="utf-8"), f"{hashlib.sha256(b'validated archive').hexdigest()}  {archive}\n")
                self.assertEqual(root.stat().st_mode & 0o222, 0)
                with self.assertRaises(PermissionError):
                    publication_root.rename(root / "replacement-publication")
                os.pwrite(source_writer, b"tampered source", 0)
                self.assertEqual(published.read_bytes(), b"validated archive")
            finally:
                os.close(source_writer)
                os.chmod(root, root_mode)
                os.chmod(root.parent, parent_mode)
            archive_path.unlink()
            archive_path.write_bytes(b"replaced source pathname")
            self.assertEqual(published.read_bytes(), b"validated archive")

    def test_publication_requires_a_checksum_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)

            result = self.run_validator(root, "--expected-reports", "1", "--publication-root", "published-artifacts")

            self.assertEqual(result.returncode, 1)
            self.assertIn("checksum output", result.stderr)
            self.assertFalse((root / "published-artifacts").exists())

    def test_publication_seals_nested_evidence_and_replaces_preopened_reports(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary) / "checkout"
            checkout.mkdir()
            make_checkout(checkout)
            evidence = checkout / "artifacts"
            evidence.mkdir()
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = evidence / archive
            archive_path.write_bytes(b"validated archive")
            self.write_report(evidence, archive)
            report_path = evidence / f"release-gate-{archive}.json"
            report_bytes = report_path.read_bytes()
            manifest_path = checkout / "scripts/release-gates.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "fixture",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["python3", "-m", "unittest"],
                                "cwd": ".",
                            }
                        ],
                        "external_gates": [],
                    }
                ),
                encoding="utf-8",
            )
            evidence_mode = evidence.stat().st_mode & 0o777
            checkout_mode = checkout.stat().st_mode & 0o777
            parent_mode = checkout.parent.stat().st_mode & 0o777
            report_writer = os.open(report_path, os.O_WRONLY | os.O_NOFOLLOW)
            try:
                result = subprocess.run(
                    [
                        sys.executable,
                        str(SCRIPT),
                        "--root",
                        "artifacts",
                        "--manifest",
                        "scripts/release-gates.json",
                        "--expected-reports",
                        "1",
                        "--expected-commit",
                        EXPECTED_COMMIT,
                        "--publication-root",
                        "published-artifacts",
                        "--checksums-output",
                        "SHA256SUMS",
                    ],
                    cwd=checkout,
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(evidence.stat().st_mode & 0o222, 0)
                self.assertEqual(checkout.stat().st_mode & 0o222, 0)
                self.assertNotEqual(os.fstat(report_writer).st_ino, report_path.stat().st_ino)
                os.pwrite(report_writer, b"forged", 0)
                self.assertEqual(report_path.read_bytes(), report_bytes)
                self.assertEqual((checkout / "published-artifacts" / report_path.name).read_bytes(), report_bytes)
                with self.assertRaises(PermissionError):
                    checkout.rename(checkout.parent / "replacement-checkout")
            finally:
                os.close(report_writer)
                os.chmod(evidence, evidence_mode)
                os.chmod(checkout, checkout_mode)
                os.chmod(checkout.parent, parent_mode)

    def test_report_snapshot_uses_validated_bytes_after_same_inode_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            report = root / "release-gate-fixture.json"
            original = b"validated report bytes"
            report.write_bytes(original)
            validated_descriptor = os.open(report, os.O_RDONLY | os.O_NOFOLLOW)
            writer = os.open(report, os.O_WRONLY | os.O_NOFOLLOW)
            directory_descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            try:
                os.pwrite(writer, b"forged report bytes", 0)
                validate_release_evidence._replace_with_sealed_snapshot(
                    directory_descriptor,
                    report.name,
                    validated_descriptor,
                    original,
                )
                self.assertEqual(report.read_bytes(), original)
                self.assertNotEqual(os.fstat(writer).st_ino, report.stat().st_ino)
            finally:
                os.close(directory_descriptor)
                os.close(writer)
                os.close(validated_descriptor)

    def test_rejects_an_archive_over_the_publication_size_bound(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / archive
            with archive_path.open("wb") as stream:
                stream.truncate(validate_release_evidence.MAX_ARCHIVE_BYTES + 1)
            self.write_report(root, archive, digest=hashlib.sha256(b"").hexdigest())

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("bounded size", result.stderr)

    def test_rejects_archive_growth_during_retained_hashing(self) -> None:
        with mock.patch.object(validate_release_evidence.os, "pread", return_value=b"12345"):
            with self.assertRaises(EvidenceError) as raised:
                validate_release_evidence.sha256_descriptor(123, maximum_bytes=4)

        self.assertIn("bounded size", str(raised.exception))

    def test_rejects_archive_mutation_during_final_revalidation_without_publication_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / archive
            archive_path.write_bytes(b"validated archive")
            self.write_report(root, archive)
            manifest_path = root / "scripts/release-gates.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "fixture",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["python3", "-m", "unittest"],
                                "cwd": ".",
                            }
                        ],
                        "external_gates": [],
                    }
                ),
                encoding="utf-8",
            )
            original_hash = validate_release_evidence.sha256_descriptor
            calls = 0

            def hash_and_mutate(descriptor: int) -> str:
                nonlocal calls
                digest = original_hash(descriptor)
                calls += 1
                if calls == 1:
                    archive_path.write_bytes(b"mutated after first hash")
                return digest

            previous_cwd = Path.cwd()
            try:
                os.chdir(root)
                with mock.patch.object(validate_release_evidence, "sha256_descriptor", side_effect=hash_and_mutate):
                    status, message = validate_release_evidence.validate(
                        root,
                        1,
                        manifest_path,
                        EXPECTED_COMMIT,
                    )
            finally:
                os.chdir(previous_cwd)

            self.assertEqual(status, 1)
            self.assertIn("changed during validation", message)
            self.assertGreaterEqual(calls, 2)

    def test_rejects_report_mutation_during_final_revalidation_without_publication_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / archive
            archive_path.write_bytes(b"validated archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            manifest_path = root / "scripts/release-gates.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "fixture",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["python3", "-m", "unittest"],
                                "cwd": ".",
                            }
                        ],
                        "external_gates": [],
                    }
                ),
                encoding="utf-8",
            )
            original_load_report = validate_release_evidence.load_report

            def load_and_mutate(path: Path, expected_commit: str) -> tuple[dict[str, object], dict[str, object], int, bytes]:
                result = original_load_report(path, expected_commit)
                retained_bytes = report_path.read_bytes()
                replacement = report_path.with_name("replacement-report.json")
                report_path.rename(replacement)
                report_path.write_bytes(retained_bytes)
                return result

            previous_cwd = Path.cwd()
            try:
                os.chdir(root)
                with mock.patch.object(validate_release_evidence, "load_report", side_effect=load_and_mutate):
                    status, message = validate_release_evidence.validate(
                        root,
                        1,
                        manifest_path,
                        EXPECTED_COMMIT,
                    )
            finally:
                os.chdir(previous_cwd)

            self.assertEqual(status, 1)
            self.assertIn("release evidence pathname changed during validation", message)

    def test_rejects_external_evidence_mutation_during_final_revalidation_without_publication_root(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            make_checkout(root)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / archive
            archive_path.write_bytes(b"validated archive")
            external = [
                {
                    "id": "runtime",
                    "reason": "host boundary",
                    "required": True,
                    "evidence_file": "release-external-runtime.json",
                    "workflow_job": "runtime",
                    "command": ["fixture-gate", "--checked"],
                    "cwd": ".",
                }
            ]
            self.write_report(root, archive, external_gates=external)
            self.write_external_report(root, "runtime")
            external_path = root / "release-external-runtime.json"
            manifest_path = root / "scripts/release-gates.json"
            manifest_path.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "fixture",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["python3", "-m", "unittest"],
                                "cwd": ".",
                            }
                        ],
                        "external_gates": external,
                    }
                ),
                encoding="utf-8",
            )
            original_load_external_report = validate_release_evidence.load_external_report

            def load_and_mutate(path: Path, descriptor: dict[str, object], expected_commit: str) -> tuple[dict[str, str] | None, int, bytes]:
                result = original_load_external_report(path, descriptor, expected_commit)
                retained_bytes = external_path.read_bytes()
                replacement = external_path.with_name("replacement-external.json")
                external_path.rename(replacement)
                external_path.write_bytes(retained_bytes)
                return result

            previous_cwd = Path.cwd()
            try:
                os.chdir(root)
                with mock.patch.object(validate_release_evidence, "load_external_report", side_effect=load_and_mutate):
                    status, message = validate_release_evidence.validate(
                        root,
                        1,
                        manifest_path,
                        EXPECTED_COMMIT,
                    )
            finally:
                os.chdir(previous_cwd)

            self.assertEqual(status, 1)
            self.assertIn("release evidence pathname changed during validation", message)

    def test_publication_root_rejects_destination_replacement_with_matching_content(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            parent_mode = root.parent.stat().st_mode & 0o777
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive_path = root / archive
            archive_path.write_bytes(b"validated archive")
            publication_root = root / "published-artifacts"
            archive_descriptor = os.open(archive_path, os.O_RDONLY | os.O_NOFOLLOW)
            try:
                archive_digest = validate_release_evidence.sha256_descriptor(archive_descriptor)
                original_fchmod = validate_release_evidence.os.fchmod
                replaced = False

                def fchmod_with_replacement(file_descriptor: int, mode: int) -> None:
                    nonlocal replaced
                    if mode == (stat.S_IRUSR | stat.S_IXUSR) and publication_root.exists() and not replaced:
                        original_publication_root = publication_root.with_name("published-artifacts-original")
                        publication_root.rename(original_publication_root)
                        publication_root.mkdir()
                        (publication_root / archive).write_bytes(b"attacker content")
                        replaced = True
                    original_fchmod(file_descriptor, mode)

                with mock.patch.object(validate_release_evidence.os, "fchmod", side_effect=fchmod_with_replacement):
                    with self.assertRaises(EvidenceError) as raised:
                        validate_release_evidence.publish_archive_links(
                            {archive: archive_descriptor},
                            {archive: archive_digest},
                            publication_root,
                            root,
                            root / "SHA256SUMS",
                        )
            finally:
                os.close(archive_descriptor)
                os.chmod(root.parent, parent_mode)

            self.assertTrue(replaced)
            self.assertIn("publication root pathname identity changed", str(raised.exception))

    def test_publication_root_rejects_parent_traversal(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            outside = root / "outside-publication"

            result = self.run_validator(root, "--expected-reports", "1", "--publication-root", "nested/../outside-publication")

            self.assertEqual(result.returncode, 1)
            self.assertFalse(outside.exists())

    def test_publication_root_rejects_nested_writable_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            nested = root / "nested" / "published-artifacts"

            result = self.run_validator(root, "--expected-reports", "1", "--publication-root", str(nested.relative_to(root)), "--checksums-output", "SHA256SUMS")

            self.assertEqual(result.returncode, 1)
            self.assertIn("direct", result.stderr)
            self.assertFalse(nested.exists())

    def test_publication_root_rejects_symlinked_intermediate_directory(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            outside = root / "outside-publication"
            outside.mkdir()
            (root / "linked-parent").symlink_to(outside, target_is_directory=True)

            result = self.run_validator(root, "--expected-reports", "1", "--publication-root", "linked-parent/published-artifacts")

            self.assertEqual(result.returncode, 1)
            self.assertFalse((outside / "published-artifacts").exists())

    def test_external_evidence_helper_writes_bounded_candidate_bound_record(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "release-external-runtime.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    ".",
                    "--output",
                    output.name,
                    "--",
                    "python3",
                    "-c",
                    "pass",
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["candidate_commit"], EXPECTED_COMMIT)
            self.assertEqual(payload["evidence_scope"], "release_external_gate")
            self.assertEqual(payload["command"], ["python3", "-c", "pass"])

    def test_external_evidence_helper_preserves_observed_step_command(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            marker = root / "successful-step.txt"
            marker.write_text("passed\n", encoding="utf-8")
            output = root / "release-external-runtime.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    ".",
                    "--observed-file",
                    marker.name,
                    "--output",
                    output.name,
                    "--",
                    "fixture-gate",
                    "--checked",
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["command"], ["fixture-gate", "--checked"])
            self.assertEqual(payload["observed_file"], marker.name)
            self.assertEqual(payload["observed_size"], len(b"passed\n"))
            self.assertEqual(payload["observed_sha256"], hashlib.sha256(b"passed\n").hexdigest())
            self.assertNotIn("observed_content_b64", payload)

    def test_external_evidence_helper_records_packaged_archive_digest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            archive.write_bytes(b"archive")
            output = root / "release-external-runtime.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    ".",
                    "--artifact",
                    archive.name,
                    "--output",
                    output.name,
                    "--",
                    sys.executable,
                    "-c",
                    "pass",
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(payload["artifact"], archive.name)
            self.assertEqual(payload["artifact_sha256"], hashlib.sha256(b"archive").hexdigest())

    def test_release_workflow_publishes_only_descriptor_bound_archive_links(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("--publication-root published-artifacts", workflow)
        self.assertIn("--checksums-output SHA256SUMS", workflow)
        self.assertNotIn("sha256sum \"$archive\"", workflow)
        self.assertIn("files: |\n            published-artifacts/voidtower-*.tar.gz", workflow)
        self.assertIn("published-artifacts/release-gate-*.json", workflow)
        self.assertIn("published-artifacts/release-external-*.json", workflow)
        self.assertNotIn("artifacts/**/release-gate-*.json", workflow)
        self.assertNotIn("find artifacts -type f -name 'voidtower-*.tar.gz'", workflow)

    def test_helper_output_is_accepted_by_publication_validator(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            marker = root / "successful-step.txt"
            marker.write_text("passed\n", encoding="utf-8")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["observed-ci-step"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            output = root / "release-external-runtime.json"

            helper = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    ".",
                    "--observed-file",
                    marker.name,
                    "--output",
                    output.name,
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(helper.returncode, 0, helper.stderr)

            result = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_external_evidence_that_embeds_observed_content(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            self.write_external_report(root, "runtime")
            external_path = root / "release-external-runtime.json"
            payload = json.loads(external_path.read_text(encoding="utf-8"))
            payload["observed_file"] = "successful-step.txt"
            payload["observed_size"] = len(b"passed\n")
            payload["observed_sha256"] = hashlib.sha256(b"passed\n").hexdigest()
            payload["observed_content_b64"] = base64.b64encode(b"passed\n").decode("ascii")
            external_path.write_text(json.dumps(payload), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(result.returncode, 1)
            self.assertIn("observed evidence metadata", result.stderr)

            payload.pop("observed_content_b64")
            payload["unexpected_content"] = "secret marker text"
            external_path.write_text(json.dumps(payload), encoding="utf-8")
            unknown_field = self.run_validator(root, "--expected-reports", "1", external_gates=external)
            self.assertEqual(unknown_field.returncode, 1)
            self.assertIn("observed evidence metadata", unknown_field.stderr)

            payload.pop("unexpected_content")
            payload["observed_content_b64"] = None
            external_path.write_text(json.dumps(payload), encoding="utf-8")
            null_legacy_field = self.run_validator(root, "--expected-reports", "1", external_gates=external)
            self.assertEqual(null_legacy_field.returncode, 1)
            self.assertIn("observed evidence metadata", null_legacy_field.stderr)

    def test_rejects_oversized_external_evidence_record(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            (root / "release-external-runtime.json").write_bytes(b"x" * (64 * 1024 + 1))

            result = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(result.returncode, 1)
            self.assertIn("oversized", result.stderr)

    def test_external_helper_timeout_cleans_forked_descendants(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    ".",
                    "--timeout-seconds",
                    "1",
                    "--output",
                    "release-external-runtime.json",
                    "--",
                    sys.executable,
                    "-c",
                    "import pathlib, subprocess, sys, time; subprocess.Popen([sys.executable, '-c', \"import pathlib, time; time.sleep(2); pathlib.Path('orphan-marker').write_text('orphan')\"], start_new_session=True); time.sleep(5)",
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2)
            time.sleep(2.5)
            self.assertFalse((root / "orphan-marker").exists())

    def test_rejects_manifest_executable_and_external_id_collision(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            external = {
                "id": "fixture",
                "reason": "collision",
                "required": True,
                "evidence_file": "release-external-fixture.json",
                "workflow_job": "fixture",
                "command": ["fixture-gate"],
                "cwd": ".",
            }

            result = self.run_validator(root, "--expected-reports", "1", external_gates=[external])

            self.assertEqual(result.returncode, 1)
            self.assertIn("overlap", result.stderr)

    def test_rejects_unsafe_manifest_executable_at_publication_boundary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            manifest_gate = {
                "id": "fixture",
                "required": True,
                "subsystems": ["governance"],
                "argv": ["sh", "-c", "true"],
                "cwd": ".",
            }

            result = self.run_validator(root, "--expected-reports", "1", manifest_gate=manifest_gate)

            self.assertEqual(result.returncode, 1)
            self.assertIn("shell interpreter", result.stderr)

    def test_canonicalizes_source_manifest_script_argv_like_release_runner(self) -> None:
        gates, external = load_manifest(Path(__file__).with_name("release-gates.json"))

        self.assertEqual(
            sorted(REQUIRED_GATE_IDS),
            sorted(gates),
            "the required gate inventory is a reviewed contract",
        )
        self.assertTrue(all(gate["required"] for gate in gates.values()))
        self.assertIn("g5-plaintext-scan", gates)
        self.assertTrue(gates["g5-plaintext-scan"]["required"])
        self.assertEqual(
            ["python3", "-m", "unittest", "scripts.test_g5_plaintext_scan", "-v"],
            gates["g5-plaintext-scan"]["argv"],
        )
        self.assertEqual(len(external), 2)
        self.assertEqual(gates["repository-hygiene"]["argv"][0], "scripts/check-repository-hygiene.sh")
        self.assertEqual(gates["repository-hygiene"]["report_argv"][0], "scripts/check-repository-hygiene.sh")
        self.assertEqual(gates["schema-migration-ownership"]["argv"][0], "scripts/check-schema-migration-ownership.sh")
        self.assertEqual(external["secret-history-scan"]["command"], ["observed-ci-step"])

    def test_rejects_equals_prefixed_path_escape_at_publication_boundary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            manifest_gate = {
                "id": "fixture",
                "required": True,
                "subsystems": ["governance"],
                "argv": ["cargo", "--manifest-path=/tmp=outside"],
                "cwd": ".",
            }

            result = self.run_validator(root, "--expected-reports", "1", manifest_gate=manifest_gate)

            self.assertEqual(result.returncode, 1)
            self.assertIn("outside the repository", result.stderr)

    def test_rejects_windows_and_backslash_path_forms_at_publication_boundary(self) -> None:
        for argument in ("C:outside", "\\\\outside", "foo\\\\..\\\\outside"):
            with self.subTest(argument=argument), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
                (root / archive).write_bytes(b"archive")
                self.write_report(root, archive)
                manifest_gate = {
                    "id": "fixture",
                    "required": True,
                    "subsystems": ["governance"],
                    "argv": ["cargo", argument],
                    "cwd": ".",
                }

                result = self.run_validator(root, "--expected-reports", "1", manifest_gate=manifest_gate)

                self.assertEqual(result.returncode, 1)
                self.assertIn("outside the repository", result.stderr)

    def test_rejects_non_source_owned_manifest_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            manifest = root / "other-manifest.json"
            manifest.write_text(json.dumps({"schema_version": 1, "gates": []}), encoding="utf-8")

            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--root",
                    str(root),
                    "--manifest",
                    str(manifest),
                    "--expected-reports",
                    "1",
                    "--expected-commit",
                    EXPECTED_COMMIT,
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 1)
            self.assertIn("source-owned", result.stderr)

    def test_rejects_non_regular_trusted_script(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            checkout = Path(temporary)
            scripts = checkout / "scripts"
            scripts.mkdir()
            trusted = scripts / "check-repository-hygiene.sh"
            trusted.mkdir()
            manifest = scripts / "release-gates.json"
            manifest.write_text(
                json.dumps(
                    {
                        "schema_version": 1,
                        "gates": [
                            {
                                "id": "unsafe",
                                "required": True,
                                "subsystems": ["governance"],
                                "argv": ["scripts/check-repository-hygiene.sh"],
                                "cwd": ".",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(EvidenceError, "regular executable"):
                load_manifest(manifest)

    def test_rejects_report_selected_subsystems_that_do_not_match_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive, selected_subsystems=["backend"])

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("subsystem", result.stderr)

    def test_external_helper_rejects_symlinked_cwd_parent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as outside:
            root = Path(temporary)
            (root / "linked").symlink_to(outside, target_is_directory=True)
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    "linked",
                    "--output",
                    "release-external-runtime.json",
                    "--",
                    "python3",
                    "-c",
                    "pass",
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("symlink", result.stderr)
            self.assertFalse((root / "release-external-runtime.json").exists())

    def test_external_helper_rejects_windows_cwd_forms(self) -> None:
        for cwd in ("C:outside", "C:/outside", "\\\\outside", "foo\\\\..\\\\outside"):
            with self.subTest(cwd=cwd), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                result = subprocess.run(
                    [
                        sys.executable,
                        str(EXTERNAL_HELPER),
                        "--gate-id",
                        "runtime",
                        "--workflow-job",
                        "runtime",
                        "--candidate-commit",
                        EXPECTED_COMMIT,
                        "--cwd",
                        cwd,
                        "--output",
                        "release-external-runtime.json",
                        "--",
                        "python3",
                        "-c",
                        "pass",
                    ],
                    cwd=root,
                    check=False,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )

                self.assertEqual(result.returncode, 2)
                self.assertIn("checkout", result.stderr)

    def test_rejects_windows_cwd_form_in_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            manifest_gate = {
                "id": "fixture",
                "required": True,
                "subsystems": ["governance"],
                "argv": ["python3", "-m", "unittest"],
                "cwd": "C:/outside",
            }

            result = self.run_validator(root, "--expected-reports", "1", manifest_gate=manifest_gate)

            self.assertEqual(result.returncode, 1)
            self.assertIn("working-directory", result.stderr)

    def test_rejects_forged_observed_file_cross_platform_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            self.write_external_report(root, "runtime")
            external_path = root / "release-external-runtime.json"
            payload = json.loads(external_path.read_text(encoding="utf-8"))
            payload["observed_sha256"] = hashlib.sha256(b"passed\n").hexdigest()
            payload["observed_content_b64"] = base64.b64encode(b"passed\n").decode("ascii")
            for observed_file in ("C:outside", "C:/outside", "\\\\outside", "foo\\\\..\\\\outside", "bad\x00path", "bad\npath", "x" * 513):
                with self.subTest(observed_file=observed_file):
                    payload["observed_file"] = observed_file
                    external_path.write_text(json.dumps(payload), encoding="utf-8")

                    result = self.run_validator(root, "--expected-reports", "1", external_gates=external)

                    self.assertEqual(result.returncode, 1)
                    self.assertIn("observed evidence metadata", result.stderr)

    def test_rejects_forged_observed_file_metadata_and_boolean_schema(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            self.write_external_report(root, "runtime")
            external_path = root / "release-external-runtime.json"
            external_payload = json.loads(external_path.read_text(encoding="utf-8"))
            external_payload["schema_version"] = True
            external_payload["observed_file"] = "successful-step.txt"
            external_payload["observed_sha256"] = "f" * 64
            external_payload["observed_content_b64"] = base64.b64encode(b"passed\n").decode("ascii")
            external_path.write_text(json.dumps(external_payload), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(result.returncode, 1)
            self.assertIn("observed evidence metadata", result.stderr)
            external_payload["schema_version"] = 1
            external_path.write_text(json.dumps(external_payload), encoding="utf-8")

            forged_observed = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(forged_observed.returncode, 1)
            self.assertIn("observed evidence metadata", forged_observed.stderr)

    def test_rejects_repository_gate_command_forgery(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["gates"][0]["argv"] = ["python3", "-c", "forged"]
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("execution metadata", result.stderr)

    def test_external_helper_rejects_oversized_command_vector(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            result = subprocess.run(
                [
                    sys.executable,
                    str(EXTERNAL_HELPER),
                    "--gate-id",
                    "runtime",
                    "--workflow-job",
                    "runtime",
                    "--candidate-commit",
                    EXPECTED_COMMIT,
                    "--cwd",
                    ".",
                    "--output",
                    "release-external-runtime.json",
                    "--",
                    *(["argument"] * 65),
                ],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 2)
            self.assertIn("unbounded", result.stderr)
            self.assertFalse((root / "release-external-runtime.json").exists())

    def test_rejects_digest_mismatch_and_non_passed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            second = "voidtower-1-aarch64-unknown-linux-musl.tar.gz"
            (root / first).write_bytes(b"first")
            (root / second).write_bytes(b"second")
            self.write_report(root, first, digest="0" * 64)
            self.write_report(root, second)

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 1)
            self.assertIn("digest mismatch", result.stderr)

    def test_rejects_a_report_that_did_not_pass(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            second = "voidtower-1-aarch64-unknown-linux-musl.tar.gz"
            (root / first).write_bytes(b"first")
            (root / second).write_bytes(b"second")
            self.write_report(root, first, status="failed")
            self.write_report(root, second)

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 1)
            self.assertIn("not passed", result.stderr)

    def test_rejects_missing_candidate_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["git"] = {}
            report["platform"] = {}
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("candidate provenance", result.stderr)

    def test_rejects_report_from_a_different_candidate_commit(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)

            result = self.run_validator(root, "--expected-reports", "1", "--expected-commit", "b" * 40)

            self.assertEqual(result.returncode, 1)
            self.assertIn("candidate commit", result.stderr)

    def test_rejects_gate_without_execution_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            del report["gates"][0]["argv"]
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("execution evidence", result.stderr)

    def test_rejects_unsafe_execution_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["gates"][0]["cwd"] = "../outside"
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("execution evidence", result.stderr)

    def test_rejects_a_structurally_incomplete_passed_report(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            (root / "release-gate-one.json").write_text(
                json.dumps({
                    "artifacts": [{"path": archive, "sha256": hashlib.sha256(b"archive").hexdigest()}],
                    "schema_version": 1,
                    "status": "passed",
                }),
                encoding="utf-8",
            )

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("structurally incomplete", result.stderr)

    def test_rejects_a_report_with_an_incomplete_manifest_gate_set(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["gates"] = [{"id": "forged", "required": True, "status": "passed", "description": "forged", "evidence_limit": "unit-verified", "stdout": "", "stderr": "", "artifacts": []}]
            report["gates"][0].update({"argv": ["python3", "-c", "pass"], "cwd": ".", "exit_code": 0})
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("executable manifest gate set", result.stderr)

    def test_rejects_report_evidence_limit_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["gates"][0]["evidence_limit"] = "runtime-verified"
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("evidence-limit metadata does not match", result.stderr)

    def test_rejects_unknown_nested_report_fields(self) -> None:
        mutations = (
            ("platform", lambda report: report["platform"].update({"unexpected": "metadata"})),
            ("git", lambda report: report["git"].update({"unexpected": "metadata"})),
            (
                "git path semantics",
                lambda report: report["git"].update({"path_semantics": {"impact_paths": "all", "modified_paths": "worktree", "staged_paths": "index", "unexpected": "metadata"}}),
            ),
            ("gate", lambda report: report["gates"][0].update({"unexpected": "metadata"})),
            (
                "gate artifact",
                lambda report: report["gates"][0].update({"artifacts": [{"path": "archive.bin", "sha256": "0" * 64, "unexpected": "metadata"}]}),
            ),
            (
                "top-level artifact",
                lambda report: report["artifacts"][0].update({"unexpected": "metadata"}),
            ),
        )
        for label, mutate in mutations:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
                (root / archive).write_bytes(b"archive")
                self.write_report(root, archive)
                report_path = root / f"release-gate-{archive}.json"
                report = json.loads(report_path.read_text(encoding="utf-8"))
                mutate(report)
                report_path.write_text(json.dumps(report), encoding="utf-8")

                result = self.run_validator(root, "--expected-reports", "1")

                self.assertEqual(result.returncode, 1)
                self.assertIn("unknown fields", result.stderr)

    def test_rejects_unknown_nested_external_descriptor_fields(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = {
                "id": "runtime",
                "reason": "host boundary",
                "required": True,
                "evidence_file": "release-external-runtime.json",
                "workflow_job": "runtime",
                "command": ["fixture-gate", "--checked"],
                "cwd": ".",
                "unexpected": "metadata",
            }
            self.write_report(root, archive, external_gates=[external])

            result = self.run_validator(root, "--expected-reports", "1", external_gates=[external])

            self.assertEqual(result.returncode, 1)
            self.assertIn("unknown fields", result.stderr)

    def test_rejects_unknown_nested_external_report_fields(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            external = [{"id": "runtime", "reason": "host boundary", "required": True, "evidence_file": "release-external-runtime.json", "workflow_job": "runtime", "command": ["fixture-gate", "--checked"], "cwd": "."}]
            self.write_report(root, archive, external_gates=external)
            self.write_external_report(root, "runtime")
            external_path = root / "release-external-runtime.json"
            payload = json.loads(external_path.read_text(encoding="utf-8"))
            payload["platform"]["unexpected"] = "metadata"
            external_path.write_text(json.dumps(payload), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1", external_gates=external)

            self.assertEqual(result.returncode, 1)
            self.assertIn("unknown fields", result.stderr)

    def test_rejects_malformed_known_nested_report_values(self) -> None:
        mutations = (
            ("git", lambda report: report["git"].update({"ahead": {"metadata": "object"}})),
            ("git path traversal", lambda report: report["git"].update({"impact_paths": ["../../secret"]})),
            ("git path semantics null", lambda report: report["git"].update({"path_semantics": None})),
            ("git path semantics partial", lambda report: report["git"].update({"path_semantics": {"impact_paths": "all"}})),
            ("gate", lambda report: report["gates"][0].update({"stdout": {"metadata": "object"}})),
            ("gate missing description", lambda report: report["gates"][0].pop("description")),
            ("gate artifact traversal", lambda report: report["gates"][0].update({"artifacts": [{"path": "../../secret", "sha256": "0" * 64}]})),
            ("gate artifact absolute", lambda report: report["gates"][0].update({"artifacts": [{"path": "/etc/passwd", "sha256": "0" * 64}]})),
            ("gate artifact nul", lambda report: report["gates"][0].update({"artifacts": [{"path": "artifact\x00.bin", "sha256": "0" * 64}]})),
        )
        for label, mutate in mutations:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
                (root / archive).write_bytes(b"archive")
                self.write_report(root, archive)
                report_path = root / f"release-gate-{archive}.json"
                report = json.loads(report_path.read_text(encoding="utf-8"))
                mutate(report)
                report_path.write_text(json.dumps(report), encoding="utf-8")

                result = self.run_validator(root, "--expected-reports", "1")

                self.assertEqual(result.returncode, 1)
                self.assertIn("invalid", result.stderr)

    def test_rejects_malformed_manifest_nested_values_at_publication_boundary(self) -> None:
        for artifacts in ([{"unexpected": "metadata"}], ["C:artifact"]):
            with self.subTest(artifacts=artifacts), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
                (root / archive).write_bytes(b"archive")
                self.write_report(root, archive)
                manifest_gate = {"id": "fixture", "required": True, "subsystems": ["governance"], "argv": ["python3", "-m", "unittest"], "cwd": ".", "artifacts": artifacts}

                result = self.run_validator(root, "--expected-reports", "1", manifest_gate=manifest_gate)

                self.assertEqual(result.returncode, 1)
                self.assertIn("artifact", result.stderr)

    def test_rejects_undeclared_gate_artifact_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            report_path = root / f"release-gate-{archive}.json"
            report = json.loads(report_path.read_text(encoding="utf-8"))
            report["gates"][0]["artifacts"] = [{"path": "artifact.bin", "sha256": "0" * 64}]
            report_path.write_text(json.dumps(report), encoding="utf-8")

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("does not match the manifest", result.stderr)

    def test_rejects_nested_archive_layout_instead_of_matching_by_basename(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            nested = root / "nested"
            nested.mkdir()
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (nested / archive).write_bytes(b"archive")
            self.write_report(root, archive, digest=hashlib.sha256(b"archive").hexdigest())

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("directly under evidence root", result.stderr)

    def test_rejects_symlinked_evidence_members(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / archive).write_bytes(b"archive")
            self.write_report(root, archive)
            (root / "alias").symlink_to(root / archive)

            result = self.run_validator(root, "--expected-reports", "1")

            self.assertEqual(result.returncode, 1)
            self.assertIn("symlinked evidence path", result.stderr)

    def test_bounded_evidence_reads_reject_file_and_parent_symlinks(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, tempfile.TemporaryDirectory() as outside:
            root = Path(temporary)
            outside_path = Path(outside) / "evidence.json"
            outside_path.write_text("{}", encoding="utf-8")

            file_alias = root / "file-alias.json"
            file_alias.symlink_to(outside_path)
            with self.assertRaisesRegex(EvidenceError, "cannot read evidence file"):
                read_text_bounded(file_alias, 1024)

            parent_alias = root / "parent-alias"
            parent_alias.symlink_to(Path(outside), target_is_directory=True)
            with self.assertRaisesRegex(EvidenceError, "cannot read evidence file"):
                read_text_bounded(parent_alias / "evidence.json", 1024)

            fifo = root / "evidence.fifo"
            os.mkfifo(fifo)
            with self.assertRaisesRegex(EvidenceError, "evidence file is not a regular file"):
                read_text_bounded(fifo, 1024)

    def test_rejects_unsafe_report_path_and_invalid_digest_shape(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            second = "voidtower-1-aarch64-unknown-linux-musl.tar.gz"
            (root / first).write_bytes(b"first")
            (root / second).write_bytes(b"second")
            self.write_report(root, first, path=f"../{first}")
            self.write_report(root, second)

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 1)
            self.assertIn("unsafe artifact path", result.stderr)

    def test_rejects_duplicate_archive_names_and_unmatched_archives(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            duplicate_name = "voidtower-1-x86_64-unknown-linux-musl.tar.gz"
            (root / duplicate_name).write_bytes(b"first")
            duplicate_dir = root / "nested"
            duplicate_dir.mkdir()
            (duplicate_dir / duplicate_name).write_bytes(b"second")
            self.write_report(root, duplicate_name, report_name="release-gate-first.json")
            self.write_report(root, duplicate_name, report_name="release-gate-second.json")

            result = self.run_validator(root)

            self.assertEqual(result.returncode, 1)
            self.assertIn("duplicate archive name", result.stderr)

    def test_release_workflow_uses_tested_validator_instead_of_inline_policy(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("jobs:\n  build:", workflow)
        build_job = workflow.split("\n  golden-path-runtime:\n", 1)[0]
        self.assertIn("--require-clean", build_job)
        self.assertIn("permissions:\n      contents: read", workflow)
        release_job = workflow.split("\n  release:\n", 1)[1]
        self.assertIn("permissions:\n      contents: write", release_job)
        self.assertRegex(release_job, r"- uses: actions/checkout@[0-9a-f]{40} # v5\.")
        self.assertEqual(workflow.count("ref: ${{ github.sha }}"), 4)
        self.assertIn("python3 scripts/validate_release_evidence.py", workflow)
        self.assertIn("--manifest scripts/release-gates.json", workflow)
        self.assertIn("--expected-commit \"$GITHUB_SHA\"", release_job)
        self.assertIn("Prepare release evidence artifact", workflow)
        self.assertIn("release-gate-${{ matrix.target }}.json", workflow)
        self.assertIn("--root artifacts", release_job)
        self.assertIn("--expected-reports 2", release_job)
        self.assertIn("environment:\n      name: release", release_job)
        self.assertIn("id-token: write", release_job)
        self.assertIn("attestations: write", release_job)
        self.assertIn("uses: actions/attest@", release_job)
        self.assertIn("subject-path: published-artifacts/voidtower-*.tar.gz", release_job)
        self.assertIn("- x86_64-unknown-linux-musl", workflow)
        self.assertIn("- aarch64-unknown-linux-musl", workflow)
        self.assertIn('ARCHIVE="voidtower-${VERSION}-${ARCH%%-*}-unknown-linux-musl.tar.gz"', workflow)
        self.assertIn("rm -rf dist", workflow)
        self.assertIn('tar -czf "$ARCHIVE" -C dist/pkg .', workflow)
        self.assertIn('sha256sum "$ARCHIVE" > dist/SHA256SUMS', workflow)
        self.assertGreaterEqual(workflow.count('PYTHONDONTWRITEBYTECODE: "1"'), 2)
        self.assertIn("needs: [build, golden-path-runtime, secret-history-scan]", workflow)
        self.assertIn("startsWith(github.ref, 'refs/tags/v') && github.ref_protected", workflow)
        self.assertIn("name: Release golden path runtime", workflow)
        self.assertIn("name: Release secret history scan", workflow)
        self.assertRegex(workflow, r"actions/setup-python@[0-9a-f]{40} # v5\.")
        self.assertIn("odysseus-mcp-servers/requirements.txt", workflow)
        self.assertIn("release-external-golden-path-runtime.json", workflow)
        self.assertIn("release-external-secret-history-scan.json", workflow)
        self.assertIn("scripts/write_external_gate_evidence.py", workflow)
        self.assertIn("needs: build", workflow.split("\n  golden-path-runtime:\n", 1)[1].split("\n  secret-history-scan:\n", 1)[0])
        self.assertIn("actions/download-artifact@", workflow.split("\n  golden-path-runtime:\n", 1)[1].split("\n  secret-history-scan:\n", 1)[0])
        self.assertIn("--artifact", workflow.split("\n  golden-path-runtime:\n", 1)[1].split("\n  secret-history-scan:\n", 1)[0])
        self.assertIn("mv -- \"$ARCHIVE_PATH\" \"$ARCHIVE\"", workflow)
        self.assertIn("scripts/extract_release_binary.py", workflow)
        self.assertIn("VOIDTOWER_REQUIRE_PACKAGED=1", workflow)
        self.assertIn("-- cargo run --example golden_path", workflow)
        self.assertNotIn("env VOIDTOWER_BINARY=", workflow)
        self.assertNotIn("-- gitleaks-history-scan", workflow)
        self.assertIn("gitleaks/gitleaks-action@e0c47f4f8be36e29cdc102c57e68cb5cbf0e8d1e", workflow)
        self.assertNotIn("python3 - <<'PY'", workflow)

    def test_ci_invokes_the_source_owned_all_scope_release_gate(self) -> None:
        workflow = CI_WORKFLOW.read_text(encoding="utf-8")
        release_job = workflow.split("\n  release-gates:\n", 1)[1]
        self.assertIn("scripts/release_gate.py", release_job)
        self.assertIn("--manifest scripts/release-gates.json", release_job)
        self.assertIn("--scope all", release_job)
        self.assertIn("--require-clean", release_job)
        self.assertIn("odysseus-mcp-servers/requirements.txt", release_job)

    def test_workflow_publication_boundaries_and_dependabot_contract(self) -> None:
        docker = DOCKER_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("push: false", docker)
        self.assertNotIn("docker/login-action@", docker)
        self.assertNotIn("packages: write", docker)
        self.assertNotIn("Build & push", docker)

        dependabot = DEPENDABOT.read_text(encoding="utf-8")
        self.assertIn("version: 2", dependabot)
        for ecosystem, directory in (("cargo", "/backend"), ("npm", "/frontend"), ("pip", "/odysseus-mcp-servers"), ("github-actions", "/")):
            self.assertIn(f"package-ecosystem: {ecosystem}", dependabot)
            self.assertIn(f"directory: {directory}", dependabot)


if __name__ == "__main__":
    unittest.main()
