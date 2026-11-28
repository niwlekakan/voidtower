import json
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parent.parent
GENERATOR = ROOT / "scripts/generate-action-schema-catalog.py"
CATALOG = ROOT / "backend/contracts/action-schema-catalog.json"
GENERATOR_SPEC = importlib.util.spec_from_file_location("action_schema_catalog_generator", GENERATOR)
assert GENERATOR_SPEC and GENERATOR_SPEC.loader
GENERATOR_MODULE = importlib.util.module_from_spec(GENERATOR_SPEC)
GENERATOR_SPEC.loader.exec_module(GENERATOR_MODULE)


class ActionSchemaCatalogTests(unittest.TestCase):
    def test_generated_catalog_is_current(self):
        result = subprocess.run(
            [sys.executable, str(GENERATOR), "--check"],
            cwd=ROOT,
            check=False,
            capture_output=True,
            text=True,
            env={"PATH": ":".join([str(Path(sys.executable).parent), "/usr/bin", "/bin"]), "PYTHONDONTWRITEBYTECODE": "1"},
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_catalog_has_unique_bounded_v1_identities(self):
        catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
        actions = catalog["actions"]
        names = [entry["action"] for entry in actions]
        self.assertEqual(names, sorted(names))
        self.assertEqual(len(names), len(set(names)))
        for entry in actions:
            self.assertRegex(entry["action"], r"^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}$")
            self.assertEqual(entry["input_schema_id"], f"{entry['action']}.input.v1")
            self.assertEqual(entry["result_schema_id"], f"{entry['action']}.result.v1")
            self.assertEqual(entry["lifecycle"], "active")

    def test_catalog_declares_fail_closed_deprecation_policy(self):
        policy = json.loads(CATALOG.read_text(encoding="utf-8"))["deprecation_policy"]
        self.assertIn("replacement", policy["deprecated"])
        self.assertIn("sunset", policy["deprecated"])
        self.assertIn("fail closed", policy["unknown"])

    def test_unknown_registry_macro_fails_closed(self):
        source = GENERATOR_MODULE.SOURCE.read_text(encoding="utf-8")
        source = source.replace(
            '    durable_mutation!(\n        "firewall.disable",',
            '    durable_future_action!(\n        "firewall.disable",',
            1,
        )
        with self.assertRaisesRegex(ValueError, "unknown action metadata macro"):
            GENERATOR_MODULE.catalog(source)

    def test_inline_unknown_registry_macro_fails_closed(self):
        source = GENERATOR_MODULE.SOURCE.read_text(encoding="utf-8")
        source = source.replace(
            GENERATOR_MODULE.ACTIONS_START,
            GENERATOR_MODULE.ACTIONS_START + '\n    durable_future_action!("inline"),',
            1,
        )
        with self.assertRaisesRegex(ValueError, "unknown action metadata macro"):
            GENERATOR_MODULE.catalog(source)

    def test_whitespace_before_macro_bang_is_not_ignored(self):
        source = GENERATOR_MODULE.SOURCE.read_text(encoding="utf-8")
        source = source.replace(
            GENERATOR_MODULE.ACTIONS_START,
            GENERATOR_MODULE.ACTIONS_START + '\n    durable_future_action !("spaced"),',
            1,
        )
        with self.assertRaisesRegex(ValueError, "unknown action metadata macro"):
            GENERATOR_MODULE.catalog(source)

    def test_each_wrapper_schema_expansion_is_validated(self):
        source = GENERATOR_MODULE.SOURCE.read_text(encoding="utf-8")
        source = source.replace(
            '            concat!($name, ".input.v1"),',
            '            concat!($name, ".input.v2"),',
            1,
        )
        with self.assertRaisesRegex(ValueError, "wrapper durable_mutation"):
            GENERATOR_MODULE.catalog(source)

    def test_swapped_wrapper_schema_expansions_fail_closed(self):
        source = GENERATOR_MODULE.SOURCE.read_text(encoding="utf-8")
        source = source.replace(
            '            concat!($name, ".input.v1"),\n            concat!($name, ".result.v1"),',
            '            concat!($name, ".result.v1"),\n            concat!($name, ".input.v1"),',
            1,
        )
        with self.assertRaisesRegex(ValueError, "wrapper durable_mutation"):
            GENERATOR_MODULE.catalog(source)

    def test_explicit_registry_schema_ids_are_validated(self):
        source = GENERATOR_MODULE.SOURCE.read_text(encoding="utf-8")
        source = source.replace(
            '        "automation.run.input.v1",',
            '        "automation.run.input.forged.v1",',
            1,
        )
        with self.assertRaisesRegex(ValueError, "input schema identity"):
            GENERATOR_MODULE.catalog(source)

    def test_catalog_records_generator_and_source_provenance(self):
        catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
        self.assertEqual(catalog["generator_version"], GENERATOR_MODULE.GENERATOR_VERSION)
        self.assertRegex(catalog["source_sha256"], r"^[0-9a-f]{64}$")


if __name__ == "__main__":
    unittest.main()
