"""Contract tests for the generated API/OpenAPI drift check.

`backend/contracts/api-v1-envelope-contract.json` is the source of truth;
`frontend/src/api/generatedApiContract.ts` and `docs/api-v1.openapi.json` are
generated from it. These tests prove the toolchain-independent check detects real
drift and that the source-owned release gate manifest and CI actually enforce it,
rather than leaving a generate command that nobody runs.
"""

from __future__ import annotations

import json
import shutil
import tempfile
import unittest
from pathlib import Path

from scripts import api_contract_drift

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts" / "release-gates.json"
WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
GATE_ID = "api-contract-drift"
GATE_ARGV = ["python3", "-m", "unittest", "scripts.test_api_contract_drift", "-v"]
ARTIFACTS = (
    api_contract_drift.SOURCE_REL,
    api_contract_drift.GENERATED_TYPES_REL,
    api_contract_drift.GENERATED_OPENAPI_REL,
)


def _copy_artifacts(destination: Path) -> None:
    for relative in ARTIFACTS:
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / relative, target)


def _workflow_job(workflow: str, job: str) -> list[str]:
    lines = workflow.splitlines()
    start = lines.index(f"  {job}:")
    end = next(
        (
            index
            for index in range(start + 1, len(lines))
            if lines[index].startswith("  ") and not lines[index].startswith("    ")
        ),
        len(lines),
    )
    return lines[start:end]


class ApiContractDriftTests(unittest.TestCase):
    def test_generated_artifacts_match_the_source_contract(self) -> None:
        self.assertEqual(api_contract_drift.check(ROOT), [])

    def test_drift_is_detected_when_the_source_changes_without_regeneration(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _copy_artifacts(root)
            source_path = root / api_contract_drift.SOURCE_REL
            source = json.loads(source_path.read_text(encoding="utf-8"))
            source["api_version"] = "2"
            source_path.write_text(json.dumps(source, indent=2) + "\n", encoding="utf-8")

            problems = api_contract_drift.check(root)

        self.assertTrue(problems, "a changed source contract must be reported as drift")
        self.assertIn("stamps a stale source digest", " ".join(problems))
        self.assertIn("embeds a stale contract payload", " ".join(problems))

    def test_digest_drift_alone_is_detected_when_only_source_bytes_move(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _copy_artifacts(root)
            source_path = root / api_contract_drift.SOURCE_REL
            source = json.loads(source_path.read_text(encoding="utf-8"))
            source_path.write_text(json.dumps(source, indent=4) + "\n", encoding="utf-8")

            problems = api_contract_drift.check(root)

        self.assertEqual(problems, ["generated client artifact stamps a stale source digest"])

    def test_drift_is_detected_when_the_published_openapi_document_is_edited(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _copy_artifacts(root)
            openapi_path = root / api_contract_drift.GENERATED_OPENAPI_REL
            openapi = json.loads(openapi_path.read_text(encoding="utf-8"))
            openapi["info"]["version"] = "2"
            openapi_path.write_text(json.dumps(openapi, indent=2) + "\n", encoding="utf-8")

            problems = api_contract_drift.check(root)

        self.assertEqual(problems, ["published OpenAPI artifact is stale against the source contract"])

    def test_drift_is_detected_when_a_generated_artifact_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _copy_artifacts(root)
            (root / api_contract_drift.GENERATED_TYPES_REL).unlink()

            problems = api_contract_drift.check(root)

        self.assertTrue(problems, "a missing generated client artifact must fail closed")
        self.assertIn("generated client artifact is unusable", problems[0])

    def test_malformed_generated_client_artifact_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _copy_artifacts(root)
            generated = root / api_contract_drift.GENERATED_TYPES_REL
            generated.write_text(api_contract_drift.HEADER, encoding="utf-8")

            problems = api_contract_drift.check(root)

        self.assertEqual(len(problems), 1)
        self.assertIn("no valid source digest stamp", problems[0])

    def test_reports_are_bounded_and_never_echo_absolute_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            _copy_artifacts(root)
            (root / api_contract_drift.GENERATED_TYPES_REL).unlink()
            (root / api_contract_drift.GENERATED_OPENAPI_REL).unlink()

            problems = api_contract_drift.check(root)

        joined = " ".join(problems)
        self.assertNotIn(str(root), joined, joined)
        self.assertIn("frontend/src/api/generatedApiContract.ts", joined)
        self.assertIn("docs/api-v1.openapi.json", joined)

    def test_local_release_gate_manifest_enforces_api_contract_drift(self) -> None:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        matching = [gate for gate in manifest["gates"] if gate.get("id") == GATE_ID]

        self.assertEqual(len(matching), 1, [gate.get("id") for gate in manifest["gates"]])
        gate = matching[0]
        self.assertTrue(gate["required"])
        self.assertEqual(gate["cwd"], ".")
        self.assertEqual(gate["argv"], GATE_ARGV)
        self.assertIn("governance", gate["subsystems"])
        self.assertIn("frontend", gate["subsystems"])

    def test_ci_runs_the_gate_and_the_byte_exact_check_in_the_frontend_job(self) -> None:
        workflow = WORKFLOW.read_text(encoding="utf-8")
        hygiene_job = _workflow_job(workflow, "repository-hygiene")
        frontend_job = _workflow_job(workflow, "frontend")

        self.assertIn(
            "PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_api_contract_drift -v",
            [line.strip().removeprefix("run: ") for line in hygiene_job if line.strip().startswith("run: ")],
        )
        self.assertTrue(
            any("working-directory: frontend" in line for line in frontend_job),
            frontend_job,
        )
        self.assertIn(
            "npm run contracts:check",
            [line.strip().removeprefix("run: ") for line in frontend_job if line.strip().startswith("run: ")],
        )


if __name__ == "__main__":
    unittest.main()
