#!/usr/bin/env python3
"""Contract tests for the release-owned standalone MCP dependency set."""

from __future__ import annotations

import json
from pathlib import Path
import re
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
REQUIREMENTS = ROOT / "odysseus-mcp-servers" / "requirements.txt"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "release.yml"
CI_WORKFLOW = ROOT / ".github" / "workflows" / "ci.yml"
GOLDEN_PATH = ROOT / "backend" / "examples" / "golden_path.rs"

# This is the reviewed Linux/Python dependency closure used by the standalone
# MCP contract suite. Any upgrade must change this allowlist and be reviewed as
# a release dependency change rather than being selected by a lower bound.
EXPECTED = {
    "PyJWT": "2.14.0",
    "annotated-types": "0.8.0",
    "anyio": "4.15.1",
    "attrs": "26.1.0",
    "certifi": "2026.7.22",
    "cffi": "2.1.1",
    "click": "8.5.0",
    "cryptography": "50.0.1",
    "h11": "0.16.0",
    "httpcore": "1.0.9",
    "httpcore2": "2.13.0",
    "httpx": "0.28.1",
    "httpx2": "2.13.0",
    "idna": "3.20",
    "jsonschema": "4.26.0",
    "jsonschema-specifications": "2025.9.1",
    "mcp": "2.2.0",
    "mcp-types": "2.2.0",
    "opentelemetry-api": "1.44.0",
    "pycparser": "3.0",
    "pydantic": "2.13.5",
    "pydantic_core": "2.46.5",
    "python-multipart": "0.0.32",
    "referencing": "0.37.0",
    "rpds-py": "2026.6.3",
    "sse-starlette": "3.4.11",
    "starlette": "1.6.0",
    "truststore": "0.10.4",
    "typing-inspection": "0.4.4",
    "typing_extensions": "4.16.0",
    "uvicorn": "0.53.0",
}


class StandaloneMcpDependencyContractTests(unittest.TestCase):
    def test_requirements_pin_the_complete_reviewed_dependency_closure(self) -> None:
        entries: dict[str, str] = {}
        for raw_line in REQUIREMENTS.read_text(encoding="utf-8").splitlines():
            line = raw_line.strip()
            self.assertTrue(line, "requirements must not contain blank lines")
            self.assertIsNone(re.search(r"[<>~!*]|===|;", line), line)
            match = re.fullmatch(r"([A-Za-z0-9_.-]+)==([0-9]+(?:\.[0-9]+){1,3})", line)
            self.assertIsNotNone(match, line)
            assert match is not None
            name, version = match.groups()
            self.assertNotIn(name.lower().replace("_", "-"), {key.lower().replace("_", "-") for key in entries})
            entries[name] = version

        normalized = {key.lower().replace("_", "-"): value for key, value in entries.items()}
        expected = {key.lower().replace("_", "-"): value for key, value in EXPECTED.items()}
        self.assertEqual(normalized, expected)

    def test_release_workflow_installs_the_source_owned_manifest(self) -> None:
        workflow = RELEASE_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("python-version: '3.13'", workflow)
        self.assertIn(
            "python3 -m pip install --disable-pip-version-check --requirement odysseus-mcp-servers/requirements.txt",
            workflow,
        )
        self.assertLess(
            workflow.index("Install standalone MCP contract dependencies"),
            workflow.index("Run release gate manifest"),
        )
        self.assertNotIn("mcp>=", workflow)
        self.assertNotIn("httpx>=", workflow)

    def test_dependency_contract_runs_before_standalone_mcp_tests(self) -> None:
        manifest = json.loads((ROOT / "scripts" / "release-gates.json").read_text(encoding="utf-8"))
        gate_ids = [entry["id"] for entry in manifest["gates"]]
        self.assertLess(
            gate_ids.index("standalone-mcp-dependency-contracts"),
            gate_ids.index("standalone-mcp-tests"),
        )

    def test_golden_path_workflows_generate_and_scope_ephemeral_restic_credentials(self) -> None:
        workflows = {
            "ci": CI_WORKFLOW.read_text(encoding="utf-8"),
            "release": RELEASE_WORKFLOW.read_text(encoding="utf-8"),
        }
        for name, workflow in workflows.items():
            with self.subTest(workflow=name):
                self.assertNotIn("voidtower-golden-path-ci-only", workflow)
                self.assertNotIn("GITHUB_ENV", workflow)
                self.assertNotRegex(workflow, r"(?im)^\s*['\"]?RESTIC_PASSWORD['\"]?\s*:")

                marker = "Run real-infrastructure golden paths" if name == "ci" else "Run and record real-infrastructure golden path"
                step_header = f"      - name: {marker}\n"
                step_start = workflow.index(step_header)
                step_tail = workflow[step_start:]
                step_body_start = len(step_header)
                next_step = re.search(r"^      - ", step_tail[step_body_start:], re.MULTILINE)
                step = step_tail if next_step is None else step_tail[: step_body_start + next_step.start()]
                run_marker = "        run: |\n"
                run_start = step.index(run_marker) + len(run_marker)
                run_lines = []
                for line in step[run_start:].splitlines():
                    if not line.startswith("          "):
                        break
                    run_lines.append(line[10:])
                run_script = "\n".join(run_lines)

                assignments = re.findall(
                    r'(?m)^(?:[A-Z0-9_]+=(?:"[^"]*"|\'[^\']*\'|\S+)\s+)?RESTIC_PASSWORD="[^"]*"',
                    run_script,
                )
                expected_assignment = 'RESTIC_PASSWORD="$restic_password"'
                if name == "release":
                    expected_assignment = 'VOIDTOWER_REQUIRE_PACKAGED=1 RESTIC_PASSWORD="$restic_password"'
                self.assertEqual(assignments, [expected_assignment])
                generation = run_script.index("restic_password=\"$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')\"")
                trap = run_script.index("trap 'unset restic_password' EXIT")
                masking = run_script.index('echo "::add-mask::$restic_password"')
                assignment = run_script.index('RESTIC_PASSWORD="$restic_password"')
                cleanup = run_script.index("unset restic_password", assignment)
                self.assertLess(generation, masking)
                self.assertLess(trap, masking)
                self.assertLess(masking, assignment)
                self.assertLess(assignment, cleanup)

                invocation = "cargo run --example golden_path" if name == "ci" else "python3 scripts/write_external_gate_evidence.py"
                self.assertIn(invocation, run_script)
                expected_assignment = (
                    'RESTIC_PASSWORD="$restic_password" cargo run --example golden_path'
                    if name == "ci"
                    else 'VOIDTOWER_REQUIRE_PACKAGED=1 RESTIC_PASSWORD="$restic_password" python3 scripts/write_external_gate_evidence.py'
                )
                self.assertIn(expected_assignment, run_script)

    def test_packaged_runtime_selection_is_identity_bound_and_fail_closed(self) -> None:
        source = GOLDEN_PATH.read_text(encoding="utf-8")

        self.assertIn("openat", source)
        self.assertIn("O_NOFOLLOW", source)
        self.assertIn("_identity_fds", source)
        self.assertIn("/proc/self/fd/", source)
        self.assertNotIn("packaged.is_file()", source)
        self.assertNotIn("VOIDTOWER_BINARY", source)
        self.assertLess(source.index("VOIDTOWER_REQUIRE_PACKAGED"), source.index("current_exe"))

    def test_packaged_runtime_selection_executes_symlink_and_fallback_cases(self) -> None:
        result = subprocess.run(
            ["cargo", "test", "--example", "golden_path", "binary_selection_tests", "--all-features"],
            cwd=ROOT / "backend",
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
