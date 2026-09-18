#!/usr/bin/env python3
"""Contract tests for the Linux agent release and installer seams."""

from __future__ import annotations

from pathlib import Path
import io
import os
import subprocess
import tarfile
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
BUILD = (ROOT / "scripts/build-release.sh").read_text(encoding="utf-8")
INSTALL = (ROOT / "scripts/install.sh").read_text(encoding="utf-8")
AGENT_UNIT = (ROOT / "packaging/systemd/voidtower-agent.service").read_text(encoding="utf-8")
SERVER_UNIT = (ROOT / "packaging/systemd/voidtower.service").read_text(encoding="utf-8")
RELEASE_WORKFLOW = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")


class AgentPackageContractTests(unittest.TestCase):
    def test_release_archive_contains_frontend_and_both_service_units(self) -> None:
        self.assertIn('VERSION="${VERSION#v}"', BUILD)
        self.assertIn('cp -r "$ROOT/packaging/systemd/." "$TMP/packaging/systemd/"', BUILD)
        self.assertIn('cp -r frontend/dist pkg/frontend', RELEASE_WORKFLOW)
        self.assertIn('cp -r packaging/systemd/. pkg/packaging/systemd/', RELEASE_WORKFLOW)
        self.assertIn('find artifacts -type f -name \'voidtower-*.tar.gz\'', RELEASE_WORKFLOW)
        self.assertIn('basename "$archive"', RELEASE_WORKFLOW)
        self.assertNotIn('path: |\n            voidtower-*.tar.gz\n            SHA256SUMS', RELEASE_WORKFLOW)
        self.assertIn('python3 scripts/test_release_gate.py', RELEASE_WORKFLOW)

    def test_build_release_creates_archive_with_runtime_assets(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "scripts").mkdir()
            (root / "frontend").mkdir()
            (root / "backend").mkdir()
            (root / "packaging/systemd").mkdir(parents=True)
            (root / "frontend/dist").mkdir()
            (root / "frontend/dist/index.html").write_text("assets", encoding="utf-8")
            (root / "backend/target/x86_64-unknown-linux-musl/release").mkdir(parents=True)
            (root / "backend/target/x86_64-unknown-linux-musl/release/voidtower").write_text(
                "binary", encoding="utf-8"
            )
            (root / "scripts/build-release.sh").write_text(
                (ROOT / "scripts/build-release.sh").read_text(encoding="utf-8"), encoding="utf-8"
            )
            for unit in ("voidtower.service", "voidtower-agent.service"):
                (root / "packaging/systemd" / unit).write_text("[Unit]\n", encoding="utf-8")

            fake_bin = root / "fake-bin"
            fake_bin.mkdir()
            (fake_bin / "npm").symlink_to("/bin/true")
            (fake_bin / "cargo").symlink_to("/bin/true")

            environment = os.environ.copy()
            environment.update(
                {"PATH": f"{fake_bin}:/usr/bin:/bin", "TARGETS": "x86_64-unknown-linux-musl", "VERSION": "test"}
            )
            result = subprocess.run(
                ["bash", str(root / "scripts/build-release.sh")],
                cwd=root,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            archive = root / "dist/voidtower-test-x86_64-unknown-linux-musl.tar.gz"
            self.assertTrue(archive.is_file())
            with tarfile.open(archive, "r:gz") as package:
                self.assertEqual(
                    set(package.getnames()),
                    {".", "./voidtower", "./frontend", "./frontend/index.html", "./packaging", "./packaging/systemd", "./packaging/systemd/voidtower.service", "./packaging/systemd/voidtower-agent.service"},
                )

    def test_binary_install_extracts_frontend_and_agent_unit(self) -> None:
        self.assertIn('cp -r "$tmp_dir/frontend" "${VT_INSTALL_DIR}/frontend"', INSTALL)
        self.assertIn('"$tmp_dir/packaging/systemd/voidtower-agent.service"', INSTALL)
        self.assertIn('install -m 644', INSTALL)

    def test_installer_installs_and_enables_agent_without_starting_unenrolled_state(self) -> None:
        self.assertIn('install_agent_service()', INSTALL)
        self.assertIn('voidtower-agent.service', INSTALL)
        self.assertIn('systemctl enable voidtower-agent.service', INSTALL)
        self.assertIn('agent_service_stop', INSTALL)
        self.assertIn('agent_service_start_if_enrolled', INSTALL)
        self.assertIn('for unit in voidtower voidtower-agent odysseus voidtower-llama', INSTALL)
        self.assertIn('"$SKIP_SYSTEMD" != true', INSTALL)
        self.assertIn('SHA256SUMS', INSTALL)
        self.assertIn('tolower($1)', INSTALL)
        self.assertIn('checksum_count', INSTALL)
        self.assertIn('--no-same-owner --no-same-permissions', INSTALL)
        self.assertIn('[[ -f "${VT_DATA_DIR}/agent/state.json" ]]', INSTALL)
        self.assertIn('validate_tar_archive()', INSTALL)
        self.assertIn('systemctl show --property=Version', INSTALL)
        self.assertIn('Release checksum manifest is unavailable', INSTALL)
        self.assertIn('validate_tar_archive "$_tarball" "Source archive"', INSTALL)
        self.assertIn('Offline mode: local source tree required', INSTALL)
        self.assertIn('VT_VERSION="${VT_VERSION#v}"', INSTALL)
        self.assertIn('Offline mode: skipping Playwright MCP pre-cache', INSTALL)
        self.assertIn('systemctl stop voidtower-agent.service 2>/dev/null || true', INSTALL)
        self.assertIn('systemctl stop voidtower.service 2>/dev/null || true', INSTALL)
        self.assertGreaterEqual(INSTALL.count('"$SKIP_SYSTEMD" != true'), 6)
        self.assertIn('Catalog archive extraction failed', INSTALL)
        self.assertIn('Offline mode: skipping dependency installation', INSTALL)
        self.assertIn('CARGO_NET_OFFLINE="$OFFLINE"', INSTALL)
        self.assertIn('npm ci --offline', INSTALL)
        self.assertIn('install_catalog() {', INSTALL)
        self.assertIn('Offline requested version requires local checkout at tag', INSTALL)
        self.assertIn('supported releases: x86_64 and aarch64', INSTALL)

    def test_archive_validator_rejects_traversal_and_symlink_members(self) -> None:
        start = INSTALL.index("validate_tar_archive()")
        end = INSTALL.index("\ndownload_binary()", start)
        validator = INSTALL[start:end]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            wrapper = root / "validate.sh"
            wrapper.write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "die() { printf '%s\\n' \"$*\" >&2; exit 1; }\n"
                f"{validator}\n"
                "validate_tar_archive \"$1\" \"test archive\"\n",
                encoding="utf-8",
            )
            safe = root / "safe.tar.gz"
            with tarfile.open(safe, "w:gz") as package:
                safe_entry = tarfile.TarInfo("safe.txt")
                safe_entry.size = 4
                package.addfile(safe_entry, io.BytesIO(b"safe"))
            safe_result = subprocess.run(["bash", str(wrapper), str(safe)], check=False, capture_output=True, text=True)
            self.assertEqual(safe_result.returncode, 0, safe_result.stderr)

            traversal = root / "traversal.tar.gz"
            with tarfile.open(traversal, "w:gz") as package:
                entry = tarfile.TarInfo("../escape")
                entry.size = 4
                package.addfile(entry, io.BytesIO(b"oops"))
            traversal_result = subprocess.run(["bash", str(wrapper), str(traversal)], check=False, capture_output=True, text=True)
            self.assertNotEqual(traversal_result.returncode, 0)

            symlink = root / "symlink.tar.gz"
            with tarfile.open(symlink, "w:gz") as package:
                package.addfile(tarfile.TarInfo("link"))
                link = tarfile.TarInfo("link")
                link.type = tarfile.SYMTYPE
                link.linkname = "/etc/passwd"
                package.addfile(link)
            symlink_result = subprocess.run(["bash", str(wrapper), str(symlink)], check=False, capture_output=True, text=True)
            self.assertNotEqual(symlink_result.returncode, 0)

    def test_agent_unit_preserves_device_visibility_and_protected_state(self) -> None:
        self.assertNotIn("PrivateDevices=true", AGENT_UNIT)
        self.assertIn("ConditionPathExists=/var/lib/voidtower/agent/state.json", AGENT_UNIT)
        self.assertIn("ConditionPathExists=/var/lib/voidtower/agent/state.json", (ROOT / "docs/agent/linux-agent-service.md").read_text(encoding="utf-8"))
        self.assertIn("ReadWritePaths=/var/lib/voidtower/agent", AGENT_UNIT)
        self.assertIn("--agent-state=/var/lib/voidtower/agent/state.json", AGENT_UNIT)
        self.assertIn("NoNewPrivileges=true", AGENT_UNIT)

    def test_server_unit_uses_supported_configuration_environment(self) -> None:
        self.assertNotIn("--data-dir", SERVER_UNIT)
        self.assertNotIn("--config-dir", SERVER_UNIT)
        self.assertIn("Environment=VOIDTOWER_DATA_DIR=/var/lib/voidtower", SERVER_UNIT)
        self.assertIn("Environment=VOIDTOWER_CONFIG_DIR=/etc/voidtower", SERVER_UNIT)

    def test_shell_scripts_parse(self) -> None:
        for script in (ROOT / "scripts/build-release.sh", ROOT / "scripts/install.sh"):
            result = subprocess.run(
                ["bash", "-n", str(script)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
