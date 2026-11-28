#!/usr/bin/env python3
"""Behavioural tests for the agent binary upgrade/rollback seam in install.sh.

Every test sources the real installer, which is why the root precondition lives
in `main` and why `main` runs only when the script is executed rather than
sourced. Each test then executes the rollback functions against a throwaway
install tree. Nothing here starts a service, rewrites a real prefix, or contacts
a network.

Test doubles used by this suite: the "retained binary" is a shell stub that
mimics `voidtower --version` and `agent check-state`, and the digest-mismatch
test prepends a `sha256sum` shim to PATH. Both are test-only; no product code
path treats them specially.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
INSTALL = ROOT / "scripts" / "install.sh"
BINARY_NAME = "voidtower"

FAKE_BINARY = """#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "voidtower {version}"
  exit 0
fi
if [ "$1" = "agent" ] && [ "$2" = "check-state" ]; then
{support}fi
echo "unknown invocation" >&2
exit 2
"""

SUPPORTS_CHECK_STATE = """  for argument in "$@"; do
    if [ "$argument" = "--help" ]; then
      echo "Validate the local agent state file"
      exit 0
    fi
  done
  echo "state: ok"
  exit {state_rc}
"""

PREDATES_CHECK_STATE = """  exit 2
"""


def write_binary(
    path: Path,
    version: str = "0.9.0",
    *,
    supports_check_state: bool = True,
    state_rc: int = 0,
) -> bytes:
    if supports_check_state:
        support = SUPPORTS_CHECK_STATE.format(state_rc=state_rc)
    else:
        support = PREDATES_CHECK_STATE
    content = FAKE_BINARY.format(version=version, support=support).encode()
    path.write_bytes(content)
    path.chmod(0o755)
    return content


def digest(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


class InstallerTree:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.use_install_dir(root / "opt")
        self.data_dir = root / "data"
        self.config_dir = root / "config"
        self.data_dir.mkdir()
        self.config_dir.mkdir()
        self.agent_dir = self.data_dir / "agent"
        self.agent_dir.mkdir()
        self.state = self.agent_dir / "state.json"

    def use_install_dir(self, install_dir: Path) -> None:
        self.install_dir = install_dir
        self.install_dir.mkdir(parents=True, exist_ok=True)
        # The default mkdir mode is umask-dependent: with the common 002 umask on
        # Debian and GitHub runners the directory lands at 0775, which the installer
        # then correctly refuses to restore into. Pin the mode the tests mean.
        self.install_dir.chmod(0o755)
        self.installed = self.install_dir / BINARY_NAME
        self.previous = self.install_dir / f"{BINARY_NAME}.previous"
        self.manifest = self.install_dir / f"{BINARY_NAME}.previous.manifest"

    def write_state(self, mode: int = 0o600) -> None:
        self.state.write_text('{"node_id":"fixture"}', encoding="utf-8")
        self.state.chmod(mode)

    def environment(self, extra: dict[str, str] | None = None) -> dict[str, str]:
        environment = os.environ.copy()
        environment.pop("VOIDTOWER_INSTALL_LIB_ONLY", None)
        environment.update(
            {
                "VT_INSTALL_SCRIPT": str(INSTALL),
                "VT_INSTALL_DIR": str(self.install_dir),
                "VT_DATA_DIR": str(self.data_dir),
                "VT_CONFIG_DIR": str(self.config_dir),
            }
        )
        if extra:
            environment.update(extra)
        return environment

    def call(
        self,
        function: str,
        extra: dict[str, str] | None = None,
        arguments: list[str] | None = None,
        timeout: int = 120,
    ) -> subprocess.CompletedProcess:
        positional = "set --" + "".join(f" {shlex.quote(value)}" for value in arguments or [])
        return invoke(
            ["-c", f'{positional}\nsource "$VT_INSTALL_SCRIPT"\n{function}'],
            cwd=self.root,
            environment=self.environment(extra),
            timeout=timeout,
        )


def invoke(
    arguments: list[str],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout: int = 120,
) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["bash", *arguments],
        cwd=cwd,
        env=environment,
        capture_output=True,
        text=True,
        timeout=timeout,
    )


class AgentRollbackTests(unittest.TestCase):
    def setUp(self) -> None:
        self._temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporary.cleanup)
        self.tree = InstallerTree(Path(self._temporary.name))

    def assert_unchanged(self, before: bytes) -> None:
        self.assertEqual(self.tree.installed.read_bytes(), before)

    def test_installer_script_parses(self) -> None:
        result = invoke(["-n", str(INSTALL)], cwd=self.tree.root, environment=os.environ.copy())

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_sourcing_the_installer_does_not_run_it(self) -> None:
        result = self.tree.call('echo "sourced"')

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "sourced")

    def test_rollback_flags_are_parsed_without_running_the_installer(self) -> None:
        command = (
            'set -- --rollback --allow-incompatible-state --install-dir "$TARGET_INSTALL_DIR"\n'
            'source "$VT_INSTALL_SCRIPT"\n'
            'echo "mode=$INSTALL_MODE"\n'
            'echo "allow=$VT_ROLLBACK_ALLOW_INCOMPATIBLE_STATE"\n'
            'echo "install=$VT_INSTALL_DIR"\n'
        )
        result = invoke(
            ["-c", command],
            cwd=self.tree.root,
            environment=self.tree.environment(
                {"TARGET_INSTALL_DIR": str(self.tree.install_dir)}
            ),
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("mode=rollback", result.stdout)
        self.assertIn("allow=true", result.stdout)
        self.assertIn(f"install={self.tree.install_dir}", result.stdout)

    def test_retain_previous_binary_records_a_verified_digest(self) -> None:
        payload = write_binary(self.tree.installed, "1.2.3")

        result = self.tree.call("retain_previous_binary")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.tree.previous.read_bytes(), payload)
        self.assertEqual(stat.S_IMODE(self.tree.previous.stat().st_mode), 0o755)
        self.assertEqual(stat.S_IMODE(self.tree.manifest.stat().st_mode), 0o600)
        text = self.tree.manifest.read_text(encoding="utf-8")
        self.assertIn(f"binary_sha256={digest(payload)}", text)
        self.assertIn("binary_version=voidtower 1.2.3", text)
        self.assertIn("schema=1", text)
        self.assertFalse((self.tree.install_dir / f"{BINARY_NAME}.previous.tmp").exists())

    def test_retain_previous_binary_is_a_no_op_without_an_installed_binary(self) -> None:
        result = self.tree.call("retain_previous_binary")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(self.tree.previous.exists())
        self.assertFalse(self.tree.manifest.exists())

    def test_retain_previous_binary_refuses_a_symlinked_install(self) -> None:
        write_binary(self.tree.root / "elsewhere")
        self.tree.installed.symlink_to(self.tree.root / "elsewhere")

        result = self.tree.call("retain_previous_binary")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("symlink", (result.stdout + result.stderr).lower())
        self.assertFalse(self.tree.previous.exists())

    def test_rollback_restores_the_retained_artifact(self) -> None:
        retained = write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback")

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.tree.installed.read_bytes(), retained)
        self.assertIn("voidtower 1.2.3", self.tree.call('"$VT_INSTALL_DIR/voidtower" --version').stdout)
        rejected = sorted(self.tree.install_dir.glob(f"{BINARY_NAME}.replaced-*"))
        self.assertEqual(len(rejected), 1, [path.name for path in rejected])
        self.assertEqual(rejected[0].read_bytes(), upgraded)
        self.assertFalse(self.tree.previous.exists())
        self.assertFalse(self.tree.manifest.exists())

    def test_rollback_refuses_without_a_retained_artifact(self) -> None:
        installed = write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("retained", (result.stdout + result.stderr).lower())
        self.assert_unchanged(installed)

    def test_rollback_refuses_a_digest_mismatch(self) -> None:
        installed = write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        self.tree.previous.write_bytes(b"tampered")

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("digest", (result.stdout + result.stderr).lower())
        self.assert_unchanged(upgraded)

    def test_rollback_refuses_a_symlinked_retained_artifact(self) -> None:
        installed = write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        real = self.tree.root / "moved.previous"
        self.tree.previous.rename(real)
        self.tree.previous.symlink_to(real)

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0)
        self.assert_unchanged(upgraded)
        self.assertNotEqual(installed, b"")

    def test_rollback_refuses_a_group_writable_install_directory(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        self.tree.install_dir.chmod(0o775)

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("writable", (result.stdout + result.stderr).lower())
        self.assert_unchanged(upgraded)

    def test_rollback_refuses_state_the_retained_binary_cannot_read(self) -> None:
        write_binary(self.tree.installed, "1.2.3", state_rc=1)
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        self.tree.write_state()

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("state", (result.stdout + result.stderr).lower())
        self.assert_unchanged(upgraded)
        self.assertTrue(self.tree.previous.exists())

    def test_rollback_proceeds_when_the_state_override_flag_is_passed(self) -> None:
        retained = write_binary(self.tree.installed, "1.2.3", state_rc=1)
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        write_binary(self.tree.installed, "1.3.0")
        self.tree.write_state()

        result = self.tree.call(
            "cmd_rollback",
            arguments=["--rollback", "--allow-incompatible-state"],
        )

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.tree.installed.read_bytes(), retained)
        self.assertIn("override", (result.stdout + result.stderr).lower())

    def test_override_variable_is_not_read_from_the_environment(self) -> None:
        installed = write_binary(self.tree.installed, "1.2.3", state_rc=1)
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        self.tree.write_state()

        result = self.tree.call(
            "cmd_rollback",
            {"VT_ROLLBACK_ALLOW_INCOMPATIBLE_STATE": "true"},
        )

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("override", (result.stdout + result.stderr).lower())
        self.assert_unchanged(upgraded)

    def test_rollback_refuses_when_the_retained_binary_cannot_be_probed(self) -> None:
        write_binary(self.tree.installed, "1.2.3", supports_check_state=False)
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        self.tree.write_state()

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("check-state", result.stdout + result.stderr)
        self.assert_unchanged(upgraded)
        self.assertTrue(self.tree.previous.exists())

    def test_rollback_proceeds_when_an_unprobeable_retained_binary_is_overridden(self) -> None:
        retained = write_binary(self.tree.installed, "1.2.3", supports_check_state=False)
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        write_binary(self.tree.installed, "1.3.0")
        self.tree.write_state()

        result = self.tree.call(
            "cmd_rollback",
            arguments=["--rollback", "--allow-incompatible-state"],
        )

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.tree.installed.read_bytes(), retained)
        self.assertIn("override", (result.stdout + result.stderr).lower())

    def test_rollback_skips_the_state_gate_when_no_state_exists(self) -> None:
        retained = write_binary(self.tree.installed, "1.2.3", state_rc=1)
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback")

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.tree.installed.read_bytes(), retained)

    def test_manifest_is_parsed_not_executed(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        marker = self.tree.root / "manifest-was-sourced"
        with self.tree.manifest.open("a", encoding="utf-8") as handle:
            handle.write(f"injected=$(touch {marker})\n")

        result = self.tree.call("cmd_rollback")

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertFalse(marker.exists())
        self.assertNotEqual(self.tree.installed.read_bytes(), upgraded)

    def test_update_retains_the_installed_binary_before_replacing_it(self) -> None:
        retained = write_binary(self.tree.installed, "1.2.3")
        stubs = "\n".join(
            [
                "download_binary() { return 1; }",
                "build_from_source() { :; }",
                "install_catalog() { :; }",
                "install_service() { :; }",
                "install_path_symlink() { :; }",
                "cmd_update",
            ]
        )

        result = self.tree.call(stubs)

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.tree.previous.read_bytes(), retained)
        self.assertIn("binary_version=voidtower 1.2.3", self.tree.manifest.read_text(encoding="utf-8"))

    def test_repair_retains_the_installed_binary_before_replacing_it(self) -> None:
        retained = write_binary(self.tree.installed, "1.2.3")
        stubs = "\n".join(
            [
                "download_binary() { return 1; }",
                "build_from_source() { :; }",
                "install_catalog() { :; }",
                "install_service() { :; }",
                "install_path_symlink() { :; }",
                "cmd_repair",
            ]
        )

        result = self.tree.call(stubs)

        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.tree.previous.read_bytes(), retained)

    def test_retain_previous_binary_replaces_an_older_retained_artifact(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        newest = write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("retain_previous_binary")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.tree.previous.read_bytes(), newest)
        self.assertIn(
            f"binary_sha256={digest(newest)}",
            self.tree.manifest.read_text(encoding="utf-8"),
        )


    def test_executed_installer_is_not_silenced_by_an_inherited_library_variable(self) -> None:
        environment = self.tree.environment({"VOIDTOWER_INSTALL_LIB_ONLY": "1"})

        result = invoke([str(INSTALL), "--dry-run"], cwd=self.tree.root, environment=environment)

        combined = result.stdout + result.stderr
        self.assertTrue(result.returncode != 0 or "DRY-RUN" in combined, combined)
        self.assertFalse(
            result.returncode == 0 and combined.strip() == "",
            "an executed installer must never exit silently",
        )

    def test_piped_installer_still_runs_main(self) -> None:
        result = subprocess.run(
            ["bash", "-s", "--", "--dry-run"],
            cwd=self.tree.root,
            env=self.tree.environment(),
            input=INSTALL.read_text(encoding="utf-8"),
            capture_output=True,
            text=True,
            timeout=120,
        )

        combined = result.stdout + result.stderr
        self.assertTrue(result.returncode != 0 or "DRY-RUN" in combined, combined)

    def test_override_flag_is_refused_outside_rollback_mode(self) -> None:
        result = self.tree.call(
            'echo "mode=$INSTALL_MODE"',
            arguments=["--allow-incompatible-state"],
        )

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("--rollback", result.stdout + result.stderr)

    def test_rollback_refuses_a_group_writable_ancestor(self) -> None:
        shared = self.tree.root / "shared"
        shared.mkdir()
        shared.chmod(0o777)
        self.tree.use_install_dir(shared / "opt")
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_unchanged(upgraded)
        self.assertTrue(self.tree.previous.exists())

    def test_rollback_allows_a_sticky_ancestor(self) -> None:
        shared = self.tree.root / "sticky"
        shared.mkdir()
        shared.chmod(0o1777)
        self.tree.use_install_dir(shared / "opt")
        retained = write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.tree.installed.read_bytes(), retained)

    @unittest.skipUnless(
        hasattr(os, "geteuid") and os.geteuid() == 0,
        "creating a foreign-owned prefix requires root",
    )
    def test_rollback_refuses_a_prefix_owned_by_another_user(self) -> None:
        foreign = self.tree.root / "foreign"
        foreign.mkdir()
        os.chown(foreign, 65534, 65534)
        foreign.chmod(0o755)
        self.tree.use_install_dir(foreign)
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback")

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assert_unchanged(upgraded)

    def test_rollback_dry_run_changes_nothing(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")

        result = self.tree.call("cmd_rollback", arguments=["--rollback", "--dry-run"])

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("dry-run", (result.stdout + result.stderr).lower())
        self.assert_unchanged(upgraded)
        self.assertTrue(self.tree.previous.exists())
        self.assertTrue(self.tree.manifest.exists())
        self.assertEqual(list(self.tree.install_dir.glob(f"{BINARY_NAME}.replaced-*")), [])

    def test_rollback_restores_the_pre_rollback_binary_when_the_restore_digest_is_wrong(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")
        shim = self.tree.root / "shim"
        shim.mkdir()
        sha256sum = shim / "sha256sum"
        sha256sum.write_text(
            "#!/bin/sh\n"
            "for last; do :; done\n"
            'case "$last" in\n'
            f"  {self.tree.installed}) printf '%s  %s\\n' "
            '"0000000000000000000000000000000000000000000000000000000000000000" "$last" ;;\n'
            '  *) exec /usr/bin/sha256sum "$@" ;;\n'
            "esac\n",
            encoding="utf-8",
        )
        sha256sum.chmod(0o755)
        environment = {"PATH": f"{shim}:{os.environ['PATH']}"}

        result = self.tree.call("cmd_rollback", environment)

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.tree.installed.read_bytes(), upgraded)
        self.assertFalse(self.tree.manifest.exists())
        self.assertIn("restored", (result.stdout + result.stderr).lower())
        failed = list(self.tree.install_dir.glob(f"{BINARY_NAME}.failed-*"))
        self.assertEqual(len(failed), 1, [path.name for path in failed])

    def test_two_rollbacks_keep_distinct_rejected_artifacts(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        first_upgrade = write_binary(self.tree.installed, "1.3.0")
        self.assertEqual(self.tree.call("cmd_rollback").returncode, 0)

        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        second_upgrade = write_binary(self.tree.installed, "1.4.0")
        result = self.tree.call("cmd_rollback")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        rejected = sorted(self.tree.install_dir.glob(f"{BINARY_NAME}.replaced-*"))
        self.assertEqual(len(rejected), 2, [path.name for path in rejected])
        self.assertEqual(
            {path.read_bytes() for path in rejected},
            {first_upgrade, second_upgrade},
        )

    def test_verify_previous_binary_rejects_a_manifest_with_a_wrong_schema_or_size(self) -> None:
        payload = write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        original = self.tree.manifest.read_text(encoding="utf-8")
        tampered_manifests = (
            original.replace("schema=1", "schema=2"),
            original.replace(f"binary_size={len(payload)}", f"binary_size={len(payload) + 1}"),
            original.replace("binary_size=", "binary_size=not-a-number-"),
        )

        for tampered in tampered_manifests:
            self.assertNotEqual(tampered, original)
            self.tree.manifest.write_text(tampered, encoding="utf-8")
            self.tree.manifest.chmod(0o600)
            result = self.tree.call('verify_previous_binary && echo accepted || echo rejected')
            self.assertIn("rejected", result.stdout, tampered + result.stdout + result.stderr)

    def test_manifest_version_is_not_interpreted_by_the_installer(self) -> None:
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        write_binary(self.tree.installed, "1.3.0")
        text = self.tree.manifest.read_text(encoding="utf-8").replace(
            "binary_version=voidtower 1.2.3",
            "binary_version=bad\\x1b[31mred\\nend",
        )
        self.tree.manifest.write_text(text, encoding="utf-8")
        self.tree.manifest.chmod(0o600)

        result = self.tree.call("cmd_rollback")

        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        plain = re.sub(r"\x1b\[[0-9;]*m", "", result.stdout + result.stderr)
        self.assertNotIn("\x1b", plain)
        self.assertNotIn("[31m", plain)
        self.assertIn("Retained artifact verified", plain)

    def test_rollback_refuses_a_symlinked_install_prefix_with_either_spelling(self) -> None:
        real = self.tree.root / "real-prefix"
        real.mkdir()
        link = self.tree.root / "linked-prefix"
        link.symlink_to(real, target_is_directory=True)
        self.tree.use_install_dir(real)
        write_binary(self.tree.installed, "1.2.3")
        self.assertEqual(self.tree.call("retain_previous_binary").returncode, 0)
        upgraded = write_binary(self.tree.installed, "1.3.0")

        for naming in (str(link), f"{link}{os.sep}"):
            with self.subTest(prefix=naming):
                result = self.tree.call("cmd_rollback", {"VT_INSTALL_DIR": naming})

                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("canonical", (result.stdout + result.stderr).lower())
                self.assert_unchanged(upgraded)
                self.assertTrue(self.tree.previous.exists())
                self.assertTrue(self.tree.manifest.exists())

    def test_rollback_refuses_a_relative_install_prefix_instead_of_hanging(self) -> None:
        relative = self.tree.root / "relative-prefix"
        relative.mkdir()
        write_binary(relative / BINARY_NAME, "1.2.3")
        absolute = str(relative)
        self.assertEqual(
            self.tree.call("retain_previous_binary", {"VT_INSTALL_DIR": absolute}).returncode, 0
        )
        upgraded = write_binary(relative / BINARY_NAME, "1.3.0")

        result = self.tree.call(
            "cmd_rollback", {"VT_INSTALL_DIR": relative.name}, timeout=20
        )

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("absolute", (result.stdout + result.stderr).lower())
        self.assertEqual((relative / BINARY_NAME).read_bytes(), upgraded)
        self.assertTrue((relative / f"{BINARY_NAME}.previous").exists())

    def test_local_release_gate_manifest_runs_the_agent_rollback_suite(self) -> None:
        manifest = json.loads((ROOT / "scripts" / "release-gates.json").read_text(encoding="utf-8"))
        matching = [
            gate for gate in manifest["gates"] if "scripts.test_agent_rollback" in gate.get("argv", [])
        ]

        self.assertEqual(len(matching), 1, manifest["gates"])
        gate = matching[0]
        self.assertTrue(gate["required"])
        self.assertEqual(gate["cwd"], ".")
        self.assertEqual(
            gate["argv"], ["python3", "-m", "unittest", "scripts.test_agent_rollback", "-v"]
        )

    def test_ci_workflow_runs_the_agent_rollback_suite(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")

        self.assertIn("scripts.test_agent_rollback", workflow)


if __name__ == "__main__":
    unittest.main()
