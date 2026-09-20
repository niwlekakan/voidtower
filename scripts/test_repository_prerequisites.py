#!/usr/bin/env python3
"""Contract tests for repository hygiene and migration ownership gates."""

from __future__ import annotations

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parent.parent
HYGIENE = ROOT / "scripts/check-repository-hygiene.sh"
SCHEMA = ROOT / "scripts/check-schema-migration-ownership.sh"
SCHEMA_HELPER = ROOT / "scripts/check_schema_migration_ownership.py"


def run_git(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=root,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )


class RepositoryPrerequisiteTests(unittest.TestCase):
    def test_hygiene_allows_tracked_continuity_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "docs/internal/handoffs").mkdir(parents=True)
            (root / "docs/internal/agent-knowledge").mkdir(parents=True)
            (root / "docs/internal/evidence").mkdir(parents=True)
            (root / "scripts/check-repository-hygiene.sh").write_text(
                HYGIENE.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'schema ok\\n'\n",
                encoding="utf-8",
            )
            for relative in (
                "docs/internal/handoffs/2026-09-20-slice.md",
                "docs/internal/agent-knowledge/system-map.md",
                "docs/internal/evidence/report.json",
            ):
                path = root / relative
                path.write_text("evidence\n", encoding="utf-8")
            (root / "app-vault/apps").mkdir(parents=True)
            (root / "app-vault/apps/authentik.yml").write_text("app config\n", encoding="utf-8")
            (root / "README.md").write_text("readme\n", encoding="utf-8")
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)

            result = subprocess.run(
                ["bash", "scripts/check-repository-hygiene.sh"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Repository hygiene check passed", result.stdout)

    def test_hygiene_still_rejects_tracked_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "backend/migrations").mkdir(parents=True)
            (root / "backend/src/db").mkdir(parents=True)
            (root / "scripts/check-repository-hygiene.sh").write_text(
                HYGIENE.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'schema ok\\n'\n",
                encoding="utf-8",
            )
            (root / ".env").write_text("TOKEN=must-not-be-tracked\n", encoding="utf-8")
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)

            result = subprocess.run(
                ["bash", "scripts/check-repository-hygiene.sh"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn(".env", result.stderr)

    def test_hygiene_rejects_sensitive_continuity_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "docs/internal").mkdir(parents=True)
            (root / "scripts/check-repository-hygiene.sh").write_text(
                HYGIENE.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'schema ok\\n'\n",
                encoding="utf-8",
            )
            (root / "docs/internal/private.pem").write_text("not a key\n", encoding="utf-8")
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)

            result = subprocess.run(
                ["bash", "scripts/check-repository-hygiene.sh"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("docs/internal/private.pem", result.stderr)

    def test_hygiene_rejects_newline_and_symlinked_sensitive_paths(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "docs/internal").mkdir(parents=True)
            (root / "outside.env").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/leak\n.env").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/id_ed25519").write_text("not a key\n", encoding="utf-8")
            (root / "docs/internal/id_ecdsa").write_text("not a key\n", encoding="utf-8")
            (root / "docs/internal/id_ed25519_sk").write_text("not a key\n", encoding="utf-8")
            (root / "docs/internal/private-key").write_text("not a key\n", encoding="utf-8")
            (root / "docs/internal/service_credentials.json").write_text(
                "not a credential\n", encoding="utf-8"
            )
            (root / "docs/internal/api-key.json").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/token.json").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/.netrc").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/TOKEN.JSON").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/auth-config.yaml").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/key.json").write_text("not a credential\n", encoding="utf-8")
            (root / "docs/internal/linked.md").symlink_to(root / "outside.env")
            (root / "scripts/check-repository-hygiene.sh").write_text(
                HYGIENE.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf 'schema ok\\n'\n",
                encoding="utf-8",
            )
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)

            result = subprocess.run(
                ["bash", "scripts/check-repository-hygiene.sh"],
                cwd=root,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("tracked symlink", result.stderr)
            self.assertIn(".env", result.stderr)

    def test_schema_gate_detects_forbidden_ddl_without_rg(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "backend/migrations").mkdir(parents=True)
            (root / "backend/src").mkdir(parents=True)
            (root / "backend/src/unsafe.rs").write_text(
                'sqlx::query_as /* a */ :: /* b */ <_, (String,)> /* c */ '
                '("CREATE TABLE unsafe (id TEXT)\n'
                'sqlx::query_scalar!("ALTER TABLE unsafe ADD COLUMN value TEXT")\n'
                'sqlx::query_unchecked!("DROP TABLE unsafe")\n'
                'sqlx::query_as!(User, "DROP TABLE typed")\n'
                'sqlx::query_as!(Holder<\'a>, "DROP TABLE lifetime_typed")\n'
                'sqlx /* spaced */ :: /* spaced */ query /* c */ ! /* d */ '
                ' /* e */ ("CREATE INDEX unsafe_idx ON unsafe (id)")\n',
                encoding="utf-8",
            )
            for version in range(1, 6):
                (root / f"backend/migrations/{version:04d}_migration.sql").write_text(
                    "-- migration\n", encoding="utf-8"
                )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                SCHEMA.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check_schema_migration_ownership.py").write_text(
                SCHEMA_HELPER.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)
            environment = os.environ.copy()
            environment["PATH"] = "/usr/bin:/bin"

            result = subprocess.run(
                ["bash", "scripts/check-schema-migration-ownership.sh"],
                cwd=root,
                env=environment,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("production DDL", result.stderr)

    def test_schema_gate_rejects_query_file_ddl_without_rg(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "backend/migrations").mkdir(parents=True)
            (root / "backend/src").mkdir(parents=True)
            (root / "backend/src/unsafe.rs").write_text(
                'sqlx::query_file!("schema.sql")\n'
                'sqlx::query_file_scalar!("schema.sql")\n'
                'sqlx::query_file_unchecked!("schema.sql")\n'
                'sqlx::query_file_as_unchecked!("schema.sql")\n'
                'sqlx::query_file_v2!("schema.sql")\n'
                'sqlx /* a */ :: /* b */ query_file /* c */ ! /* d */ ("schema.sql")\n',
                encoding="utf-8",
            )
            for version in range(1, 6):
                (root / f"backend/migrations/{version:04d}_migration.sql").write_text(
                    "-- migration\n", encoding="utf-8"
                )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                SCHEMA.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check_schema_migration_ownership.py").write_text(
                SCHEMA_HELPER.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)

            result = subprocess.run(
                ["bash", "scripts/check-schema-migration-ownership.sh"],
                cwd=root,
                env={**os.environ, "PATH": "/usr/bin:/bin"},
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("production DDL", result.stderr)

    def test_schema_gate_requires_contiguous_tracked_migrations_without_rg(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "backend/migrations").mkdir(parents=True)
            (root / "backend/src").mkdir(parents=True)
            for version in (1, 2, 4, 5):
                (root / f"backend/migrations/{version:04d}_migration.sql").write_text(
                    "-- migration\n", encoding="utf-8"
                )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                SCHEMA.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check_schema_migration_ownership.py").write_text(
                SCHEMA_HELPER.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)
            environment = os.environ.copy()
            environment["PATH"] = "/usr/bin:/bin"

            result = subprocess.run(
                ["bash", "scripts/check-schema-migration-ownership.sh"],
                cwd=root,
                env=environment,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("contiguous", result.stderr)

    def test_schema_gate_rejects_source_and_migration_symlinks_without_rg(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "backend/migrations").mkdir(parents=True)
            (root / "backend/src").mkdir(parents=True)
            (root / "outside.rs").write_text("// outside\n", encoding="utf-8")
            (root / "outside.sql").write_text("-- outside\n", encoding="utf-8")
            (root / "backend/src/linked.rs").symlink_to(root / "outside.rs")
            for version in range(1, 5):
                (root / f"backend/migrations/{version:04d}_migration.sql").write_text(
                    "-- migration\n", encoding="utf-8"
                )
            (root / "backend/migrations/0005_migration.sql").symlink_to(root / "outside.sql")
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                SCHEMA.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check_schema_migration_ownership.py").write_text(
                SCHEMA_HELPER.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)
            environment = os.environ.copy()
            environment["PATH"] = "/usr/bin:/bin"

            result = subprocess.run(
                ["bash", "scripts/check-schema-migration-ownership.sh"],
                cwd=root,
                env=environment,
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("symlinks are not allowed", result.stderr)

    def test_schema_gate_rejects_symlinked_source_root_without_rg(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "backend").mkdir()
            (root / "external-src").mkdir()
            (root / "backend/migrations").mkdir(parents=True)
            (root / "external-src/unsafe.rs").write_text(
                'sqlx::query("CREATE TABLE unsafe (id TEXT)")\n', encoding="utf-8"
            )
            (root / "backend/src").symlink_to(root / "external-src", target_is_directory=True)
            for version in range(1, 5):
                (root / f"backend/migrations/{version:04d}_migration.sql").write_text(
                    "-- migration\n", encoding="utf-8"
                )
            (root / "scripts/check-schema-migration-ownership.sh").write_text(
                SCHEMA.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (root / "scripts/check_schema_migration_ownership.py").write_text(
                SCHEMA_HELPER.read_text(encoding="utf-8"), encoding="utf-8"
            )
            self.assertEqual(run_git(root, "init", "-q").returncode, 0)
            self.assertEqual(run_git(root, "add", ".").returncode, 0)

            result = subprocess.run(
                ["bash", "scripts/check-schema-migration-ownership.sh"],
                cwd=root,
                env={**os.environ, "PATH": "/usr/bin:/bin"},
                check=False,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("symlinked path component", result.stderr)


if __name__ == "__main__":
    unittest.main()
