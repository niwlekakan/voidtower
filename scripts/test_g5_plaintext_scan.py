"""Contract tests for the G5 provider-plaintext scan harness.

G5 requires that database and log scans find no provider plaintext. This suite
proves the scan harness detects provider-shaped residue -- including a deleted
Slack incoming-webhook URL and a deleted Proxmox API token -- in a database file
and its sidecars and in an application log, classifies pre-migration snapshots
through the operator seam, reports every authoritative legacy plaintext settings
key and an AI-provider reference to one, states the classes no byte pattern
covers, never echoes a matched value, leaves the scanned database content
unchanged, is fail-closed on bounded, non-regular, partial, or unverifiable
input, and is enforced as a required release gate rather than an optional helper
nobody runs.
"""

from __future__ import annotations

import json
import os
import re
import sqlite3
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import g5_plaintext_scan

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts" / "release-gates.json"
GATE_ID = "g5-plaintext-scan"
GATE_ARGV = ["python3", "-m", "unittest", "scripts.test_g5_plaintext_scan", "-v"]

PROVIDER_KEY = "sk-" + "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0"
SNAPSHOT_NAME = "voidtower.db.pre-migration-v1-20260101T000000Z.bak"

# Values that match no provider pattern, so only the live-row check can report
# them. Nothing here is a real credential.
SYNTHETIC_LEGACY_VALUE = "synthetic-unpatterned-value-0001"


def build_database_with_deleted_key(directory: Path, name: str = "voidtower.db") -> Path:
    """Reproduce the residue an older release left: a provider key row that was
    deleted by a connection which did not zero freed cell content."""
    return build_database_with_deleted_value(
        directory, "legacy.provider_key", PROVIDER_KEY, name
    )


def build_database_with_deleted_value(
    directory: Path, key: str, value: str, name: str = "voidtower.db"
) -> Path:
    """A legacy plaintext row deleted by a connection that did not zero freed
    cell content, so the value stays in the file's freed pages."""
    path = directory / name
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA secure_delete = OFF")
    connection.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)")
    connection.execute(
        "INSERT INTO settings (key, value) VALUES (?, ?)", (key, value)
    )
    for index in range(40):
        connection.execute(
            "INSERT INTO settings (key, value) VALUES (?, ?)",
            (f"pad.{index}", "p" * 400),
        )
    connection.commit()
    connection.execute("DELETE FROM settings WHERE key = ?", (key,))
    connection.commit()
    connection.close()
    return path


def build_clean_database(directory: Path) -> Path:
    """A migrated database: only ciphertext and non-credential settings remain.

    It is left in the product's WAL journal mode, so a later read-only scan
    exercises the sidecar path rather than a rollback-journal database that
    creates none.
    """
    path = directory / "voidtower.db"
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA journal_mode = WAL")
    connection.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)")
    connection.execute("CREATE TABLE secrets (id TEXT PRIMARY KEY, value_enc BLOB)")
    connection.execute(
        "CREATE TABLE ai_providers (id TEXT PRIMARY KEY, api_key_ref TEXT)"
    )
    connection.execute(
        "INSERT INTO settings (key, value) VALUES ('notif.ntfy_url_secret_id', 'sec-1')"
    )
    connection.execute(
        "INSERT INTO settings (key, value) VALUES ('theme', 'dark')"
    )
    connection.execute(
        "INSERT INTO secrets (id, value_enc) VALUES ('sec-1', ?)",
        (bytes(range(256)) * 4,),
    )
    connection.execute(
        "INSERT INTO ai_providers (id, api_key_ref) VALUES ('provider-1', 'sec-1')"
    )
    connection.commit()
    connection.close()
    return path


def legacy_setting_keys_declared_by_the_backend() -> set[str]:
    """The legacy credential keys the backend itself migrates.

    Declarations are the contract: a credential migration names its legacy
    settings key with a `legacy_setting` field or a `*_LEGACY_SETTING` constant.
    A migration module may also address one key as a bare string, so literal
    `settings WHERE key = '<key>'` uses are collected only inside
    `migrate_legacy_*` functions, never from ordinary reads or test fixtures.
    """
    keys: set[str] = set()
    for path in sorted((ROOT / "backend" / "src").rglob("*.rs")):
        text = path.read_text(encoding="utf-8")
        keys.update(re.findall(r'legacy_setting:\s*"([^"]+)"', text))
        keys.update(
            re.findall(
                r"\b[A-Z0-9_]*LEGACY_SETTING[A-Z0-9_]*\s*:\s*&str\s*=\s*\"([^\"]+)\"",
                text,
            )
        )
    current_function = ""
    secrets_source = (ROOT / "backend" / "src" / "api" / "secrets.rs").read_text(
        encoding="utf-8"
    )
    for line in secrets_source.splitlines():
        declaration = re.search(r"\bfn\s+([a-z0-9_]+)", line)
        if declaration:
            current_function = declaration.group(1)
        if current_function.startswith("migrate_legacy_"):
            keys.update(re.findall(r"settings WHERE key = '([^']+)'", line))
    return keys


class G5PlaintextScanTests(unittest.TestCase):
    # --- detection -------------------------------------------------------
    def test_detects_provider_key_residue_in_the_database_file(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            database = build_database_with_deleted_key(Path(raw))

            report = g5_plaintext_scan.scan_database(database)

            self.assertFalse(report.ok())
            self.assertIn("residue", {finding.kind for finding in report.findings})
            self.assertIn(
                "openai_api_key", {finding.pattern for finding in report.findings}
            )

    def test_detects_provider_key_residue_in_the_wal_sidecar(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "voidtower.db"
            connection = sqlite3.connect(path)
            connection.execute("PRAGMA journal_mode = WAL")
            connection.execute("PRAGMA wal_autocheckpoint = 0")
            connection.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)")
            connection.execute(
                "INSERT INTO settings (key, value) VALUES ('legacy.provider_key', ?)",
                (PROVIDER_KEY,),
            )
            connection.commit()
            connection.execute("DELETE FROM settings WHERE key = 'legacy.provider_key'")
            connection.commit()
            # The connection stays open so the WAL is not checkpointed away.
            try:
                report = g5_plaintext_scan.scan_database(path)
            finally:
                connection.close()

            self.assertFalse(report.ok())
            self.assertIn(
                "voidtower.db-wal",
                {
                    finding.target
                    for finding in report.findings
                    if finding.kind == "residue"
                },
            )

    def test_detects_a_deleted_slack_incoming_webhook_url(self) -> None:
        # The backend migrates `notif_slack_webhook` into an encrypted secret.
        # A Slack incoming-webhook URL has a documented shape, so the byte scan
        # must find a deleted copy, not only a live row.
        webhook = (
            "https://hooks.slack.com/services/T0000000000/B0000000000/"
            "abcdefghijklmnopqrstuvwx"
        )
        with tempfile.TemporaryDirectory() as raw:
            database = build_database_with_deleted_value(
                Path(raw), "notif_slack_webhook", webhook
            )

            report = g5_plaintext_scan.scan_database(database)

            self.assertFalse(report.ok())
            self.assertIn(
                "slack_webhook_url",
                {finding.pattern for finding in report.findings},
            )
            self.assertNotIn(webhook, json.dumps(report.as_dict()))

    def test_detects_a_deleted_proxmox_api_token(self) -> None:
        # The legacy `proxmox_token` setting holds the documented
        # `user@realm!tokenid=uuid` API-token string. Its shape is discoverable,
        # so a deleted copy must be found by the byte scan.
        token = "root@pam!voidtower=8f0a5a0e-2f0b-4a1c-9d2e-3b4c5d6e7f80"
        with tempfile.TemporaryDirectory() as raw:
            database = build_database_with_deleted_value(
                Path(raw), "proxmox_token", token
            )

            report = g5_plaintext_scan.scan_database(database)

            self.assertFalse(report.ok())
            self.assertIn(
                "proxmox_api_token",
                {finding.pattern for finding in report.findings},
            )

    def test_report_states_the_classes_no_byte_pattern_covers(self) -> None:
        # A clean byte scan must not imply coverage it does not have: the ntfy
        # topic URL and the Odysseus webhook secret have no discoverable shape.
        with tempfile.TemporaryDirectory() as raw:
            database = build_clean_database(Path(raw))

            report = g5_plaintext_scan.scan_candidate([database])

            self.assertTrue(report.ok(), report.as_dict())
            self.assertEqual(
                ["notif_ntfy_url", "odysseus.webhook_secret"],
                report.as_dict()["coverage_limits"],
            )

    def test_detects_plaintext_in_the_application_log(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            log = Path(raw) / "voidtower.log"
            log.write_text(
                f"2026-11-26T10:00:00Z WARN provider request failed key={PROVIDER_KEY}\n",
                encoding="utf-8",
            )

            report = g5_plaintext_scan.scan_log(log)

            self.assertFalse(report.ok())
            self.assertEqual({"log"}, {finding.kind for finding in report.findings})

    def test_reports_every_authoritative_legacy_plaintext_key(self) -> None:
        # A legacy migration that did not complete leaves a still-readable
        # setting. The values match no byte pattern, so only the live-row check
        # can report them. The fixture keys come from the backend migration
        # source, not from the scanner's own list, so this test cannot agree
        # with a rotten list by construction.
        authoritative_keys = legacy_setting_keys_declared_by_the_backend()
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "voidtower.db"
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)")
            for key in authoritative_keys:
                connection.execute(
                    "INSERT INTO settings (key, value) VALUES (?, ?)",
                    (key, SYNTHETIC_LEGACY_VALUE),
                )
            connection.commit()
            connection.close()

            report = g5_plaintext_scan.scan_candidate([path])

            live_findings = [
                finding
                for finding in report.findings
                if finding.kind == "live_plaintext_setting"
            ]
            self.assertEqual(len(authoritative_keys), len(live_findings))
            details = [finding.detail for finding in live_findings]
            for key in authoritative_keys:
                self.assertEqual(
                    1,
                    sum(key in detail for detail in details),
                    f"the legacy key {key} must be reported exactly once",
                )

    def test_legacy_key_list_tracks_the_backend_migration_source(self) -> None:
        # A hardcoded list rots silently: it once named three keys the product
        # never migrated while omitting two it did. Pin the set against the
        # migration source in both directions.
        declared = legacy_setting_keys_declared_by_the_backend()

        self.assertEqual(
            {"odysseus.webhook_secret", "proxmox_token"}
            | {"notif_ntfy_url", "notif_discord_webhook", "notif_slack_webhook"},
            declared,
            "the drift extraction must keep seeing the backend's legacy keys",
        )
        self.assertEqual(
            declared,
            set(g5_plaintext_scan.LEGACY_PLAINTEXT_SETTING_KEYS),
        )

    def test_reports_an_ai_provider_reference_to_a_plaintext_setting(self) -> None:
        # AI-provider legacy keys are addressed through `ai_providers.api_key_ref`,
        # so no static key list can enumerate them.
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "voidtower.db"
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)")
            connection.execute(
                "CREATE TABLE ai_providers (id TEXT PRIMARY KEY, api_key_ref TEXT)"
            )
            connection.execute(
                "INSERT INTO settings (key, value) VALUES ('legacy.ai.key', ?)",
                (SYNTHETIC_LEGACY_VALUE,),
            )
            connection.execute(
                "INSERT INTO ai_providers (id, api_key_ref) VALUES ('provider-1', 'legacy.ai.key')"
            )
            connection.commit()
            connection.close()

            report = g5_plaintext_scan.scan_candidate([path])

            self.assertFalse(report.ok())
            self.assertIn(
                "legacy_ai_provider_reference",
                {finding.pattern for finding in report.findings},
            )

    # --- classification --------------------------------------------------
    def test_classifies_a_pre_migration_snapshot_instead_of_ignoring_it(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            database = build_clean_database(directory)
            (directory / SNAPSHOT_NAME).write_bytes(
                b"SQLite format 3\x00" + b"padding" * 40 + PROVIDER_KEY.encode()
            )

            report = g5_plaintext_scan.scan_candidate([database])

            snapshot_findings = [
                finding for finding in report.findings if finding.kind == "snapshot"
            ]
            self.assertEqual(1, len(snapshot_findings))
            self.assertEqual(SNAPSHOT_NAME, snapshot_findings[0].target)

    # --- no false positives ----------------------------------------------
    def test_clean_database_with_encrypted_secrets_reports_clean(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            database = build_clean_database(Path(raw))

            report = g5_plaintext_scan.scan_candidate([database])

            self.assertTrue(report.ok(), report.as_dict())

    def test_no_pattern_matches_the_products_redaction_forms(self) -> None:
        # The scanner has no placeholder special-case. That is only safe while
        # no provider pattern can match text the product itself writes after
        # redaction, so pin the pattern set against those forms.
        redaction_forms = (
            b"[REDACTED]",
            b"bearer ***",
            b"token <redacted>",
            b"Bearer [REDACTED]",
            b"***",
        )

        for form in redaction_forms:
            for name, pattern in g5_plaintext_scan.PATTERNS:
                self.assertIsNone(
                    pattern.search(form),
                    f"pattern {name} matches redaction text {form!r}",
                )

    def test_empty_legacy_setting_value_is_still_reported(self) -> None:
        # Presence, not length, is the signal: an empty row also means the
        # migration did not complete.
        with tempfile.TemporaryDirectory() as raw:
            path = Path(raw) / "voidtower.db"
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT)")
            connection.execute(
                "INSERT INTO settings (key, value) VALUES ('notif_ntfy_url', '')"
            )
            connection.commit()
            connection.close()

            report = g5_plaintext_scan.scan_candidate([path])

            self.assertFalse(report.ok())
            details = [
                finding.detail
                for finding in report.findings
                if finding.kind == "live_plaintext_setting"
            ]
            self.assertEqual(1, len(details))
            self.assertIn("notif_ntfy_url", details[0])
            self.assertIn("0 bytes", details[0])

    # --- report hygiene --------------------------------------------------
    def test_report_never_contains_the_matched_value(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            database = build_database_with_deleted_key(Path(raw))

            report = g5_plaintext_scan.scan_candidate([database])
            rendered = json.dumps(report.as_dict(), sort_keys=True)

            self.assertFalse(report.ok())
            self.assertNotIn(PROVIDER_KEY, rendered)
            for finding in report.as_dict()["findings"]:
                self.assertNotIn("value", finding)

    def test_scanning_leaves_the_database_content_unchanged(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            database = build_clean_database(directory)
            before_bytes = database.read_bytes()
            before_mtime = database.stat().st_mtime_ns
            before_names = {path.name for path in directory.iterdir()}

            g5_plaintext_scan.scan_candidate([database])

            self.assertEqual(before_bytes, database.read_bytes())
            self.assertEqual(before_mtime, database.stat().st_mtime_ns)
            # A read-only connection to a WAL-mode database creates SQLite's
            # shared-memory index and an empty write-ahead log after the sidecar
            # scan already ran. They must carry no database content. Assert the
            # sidecars really appear, so this loop cannot pass vacuously.
            created = {path.name for path in directory.iterdir()} - before_names
            self.assertEqual(
                {f"{database.name}-shm", f"{database.name}-wal"},
                created,
                "a WAL-mode scan must create exactly the empty sidecars",
            )
            for path in directory.iterdir():
                if path.name in before_names:
                    continue
                if path.name.endswith("-wal"):
                    self.assertEqual(0, path.stat().st_size)
                for _, pattern in g5_plaintext_scan.PATTERNS:
                    self.assertIsNone(pattern.search(path.read_bytes()))

    # --- bounded streaming ----------------------------------------------
    def test_detects_a_credential_that_straddles_a_chunk_boundary(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            log = Path(raw) / "voidtower.log"
            # The key starts at offset 50, so a 64-byte first chunk cannot
            # contain it whole; only the carried overlap can complete it.
            log.write_bytes(b"x" * 50 + PROVIDER_KEY.encode() + b"x" * 40)

            with mock.patch.object(g5_plaintext_scan, "CHUNK_BYTES", 64):
                report = g5_plaintext_scan.scan_log(log)

            starts = [
                finding.offset
                for finding in report.findings
                if finding.pattern == "openai_api_key"
            ]
            self.assertEqual([50], starts, "the straddling key must be reported once")

    def test_detects_a_match_starting_on_the_last_byte_of_a_chunk(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            log = Path(raw) / "voidtower.log"
            log.write_bytes(b"x" * 63 + PROVIDER_KEY.encode() + b"x" * 10)

            with mock.patch.object(g5_plaintext_scan, "CHUNK_BYTES", 64):
                report = g5_plaintext_scan.scan_log(log)

            starts = [
                finding.offset
                for finding in report.findings
                if finding.pattern == "openai_api_key"
            ]
            self.assertEqual([63], starts)

    def test_oversized_single_match_fails_closed_instead_of_passing(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            log = Path(raw) / "voidtower.log"
            log.write_bytes(b"Bearer " + b"A" * 102400)

            with self.assertRaises(g5_plaintext_scan.ScanError):
                g5_plaintext_scan.scan_log(log)
            self.assertEqual(
                2, g5_plaintext_scan.main(["--log", str(log), "--json"])
            )

    # --- fail closed -----------------------------------------------------
    def test_oversized_input_fails_closed_instead_of_passing(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            database = build_database_with_deleted_key(Path(raw))

            with mock.patch.object(g5_plaintext_scan, "MAX_SCAN_BYTES", 8):
                with self.assertRaises(g5_plaintext_scan.ScanError):
                    g5_plaintext_scan.scan_candidate([database])

    def test_unreadable_input_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw) / "voidtower.db"
            directory.mkdir()

            with self.assertRaises(g5_plaintext_scan.ScanError):
                g5_plaintext_scan.scan_candidate([directory])

    def test_database_without_a_settings_table_fails_closed(self) -> None:
        # An explicitly named --db that cannot be verified as a VoidTower
        # database must not look like a clean scan.
        with tempfile.TemporaryDirectory() as raw:
            unrelated = Path(raw) / "unrelated.db"
            connection = sqlite3.connect(unrelated)
            connection.execute("CREATE TABLE other (id INTEGER PRIMARY KEY)")
            connection.commit()
            connection.close()
            empty = Path(raw) / "empty.db"
            empty.write_bytes(b"")
            log = Path(raw) / "voidtower.log"
            log.write_text("clean\n", encoding="utf-8")

            for path in (unrelated, empty):
                with self.assertRaises(g5_plaintext_scan.ScanError):
                    g5_plaintext_scan.scan_candidate([path])
                self.assertEqual(
                    2,
                    g5_plaintext_scan.main(
                        ["--db", str(path), "--log", str(log), "--json"]
                    ),
                )

    def test_a_settings_table_outside_the_column_contract_fails_closed(self) -> None:
        # Identification is a readable `settings(key, value)` table. A foreign
        # database with a `settings` table of another shape is not verifiable.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            path = directory / "foreign.db"
            connection = sqlite3.connect(path)
            connection.execute("CREATE TABLE settings (name TEXT, payload TEXT)")
            connection.execute(
                "INSERT INTO settings (name, payload) VALUES ('theme', 'dark')"
            )
            connection.commit()
            connection.close()
            log = directory / "voidtower.log"
            log.write_text("clean\n", encoding="utf-8")

            with self.assertRaises(g5_plaintext_scan.ScanError):
                g5_plaintext_scan.scan_candidate([path])
            self.assertEqual(
                2,
                g5_plaintext_scan.main(
                    ["--db", str(path), "--log", str(log), "--json"]
                ),
            )

    def test_partial_input_is_not_a_clean_g5_scan(self) -> None:
        # G5 requires a database and a log scan. Naming only one must not exit 0
        # and read as a clean result.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            database = build_clean_database(directory)
            log = directory / "voidtower.log"
            log.write_text("2026-11-26T10:00:00Z INFO started\n", encoding="utf-8")

            self.assertEqual(
                2, g5_plaintext_scan.main(["--db", str(database), "--json"])
            )
            self.assertEqual(
                2, g5_plaintext_scan.main(["--log", str(log), "--json"])
            )
            self.assertEqual(
                0,
                g5_plaintext_scan.main(
                    ["--db", str(database), "--log", str(log), "--json"]
                ),
            )

    @unittest.skipUnless(hasattr(os, "mkfifo"), "requires POSIX FIFOs")
    def test_non_regular_input_fails_closed(self) -> None:
        # A FIFO has no size to bound and would block forever, so it must be
        # rejected instead of hanging an evidence run.
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            database = build_clean_database(directory)
            fifo = directory / "voidtower.log"
            os.mkfifo(fifo)

            with self.assertRaises(g5_plaintext_scan.ScanError):
                g5_plaintext_scan.scan_candidate([database], [fifo])
            self.assertEqual(
                2,
                g5_plaintext_scan.main(
                    ["--db", str(database), "--log", str(fifo), "--json"]
                ),
            )

    def test_missing_explicit_input_fails_closed_instead_of_reporting_clean(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            missing_db = Path(raw) / "voidtower.db"
            missing_log = Path(raw) / "voidtower.log"

            with self.assertRaises(g5_plaintext_scan.ScanError):
                g5_plaintext_scan.scan_candidate([missing_db])
            with self.assertRaises(g5_plaintext_scan.ScanError):
                g5_plaintext_scan.scan_candidate([], [missing_log])
            self.assertEqual(
                2,
                g5_plaintext_scan.main(["--log", str(missing_log), "--json"]),
            )

    # --- gate enforcement ------------------------------------------------
    def test_gate_is_declared_required_in_the_release_manifest(self) -> None:
        manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
        gates = {gate["id"]: gate for gate in manifest["gates"]}

        self.assertIn(GATE_ID, gates)
        gate = gates[GATE_ID]
        self.assertTrue(gate["required"])
        self.assertEqual(GATE_ARGV, gate["argv"])
        self.assertEqual(".", gate["cwd"])

    def test_cli_reports_clean_and_residue_with_distinct_statuses(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            directory = Path(raw)
            clean = build_clean_database(directory)
            dirty = build_database_with_deleted_key(directory, name="dirty.db")
            log = directory / "voidtower.log"
            log.write_text("2026-11-26T10:00:00Z INFO started\n", encoding="utf-8")

            self.assertEqual(
                0,
                g5_plaintext_scan.main(
                    ["--db", str(clean), "--log", str(log), "--json"]
                ),
            )
            self.assertEqual(
                1,
                g5_plaintext_scan.main(
                    ["--db", str(dirty), "--log", str(log), "--json"]
                ),
            )


if __name__ == "__main__":
    unittest.main()
