#!/usr/bin/env python3
"""Scan a VoidTower candidate database and log set for provider plaintext.

G5 of the release gates requires that database and log scans find no provider
plaintext. Deleting a credential row does not erase its bytes from the SQLite
file, so a scan must read the raw bytes of the database, its write-ahead log, and
any retained pre-migration snapshot, not only the live rows.

Byte-pattern coverage is deliberately narrow: a pattern is included only when the
product or the provider that issued the credential gives the value a discoverable
shape. Covered shapes are OpenAI keys, Google API keys, Slack `xox*` tokens,
Slack incoming-webhook URLs, Discord webhook URLs, Proxmox API tokens in the
documented `user@realm!tokenid=uuid` form, PEM private-key blocks, and `Bearer`
values.

Two authoritative legacy classes have no discoverable shape: the ntfy topic URL
(`notif_ntfy_url` -- an arbitrary self-hosted URL and topic) and the Odysseus
webhook secret (`odysseus.webhook_secret` -- 48 hex characters, a width shared by
unrelated identifiers, so a pattern for it would also match values that are not
credentials). The live-row check reports both while their settings row
remains, and the one-time plaintext-residue rebuild (`docs/recovery.md`) is the
control for copies deleted before it ran. A copy deleted after that rebuild is
outside byte-pattern coverage, so a clean byte scan is not proof that those two
classes are absent; every report names them in `coverage_limits`.

This module is a read-only evidence collector:

* it never writes, truncates, vacuums, or repairs a scanned file, and the scanned
  database file's content and mtime are unchanged by a scan;
* it never prints or returns a matched value -- findings carry a pattern name, a
  byte offset where one applies, and a redacted length so an operator can locate
  the residue without copying the credential into a report;
* every input must be a regular file and is bounded, and a missing, unreadable,
  oversized, or unverifiable input is an explicit error rather than a silent pass.

"Read-only" is about file content. Opening a WAL-mode database through SQLite
with `mode=ro` can create the `-shm` shared-memory index and an empty `-wal`
sidecar; when a writer already holds an uncheckpointed WAL, the wal-index
read-mark slot in an existing `-shm` can be rewritten. Every sidecar is scanned
before that connection is opened, and none carries database content.

The live-row check complements the byte scan: a legacy plaintext settings key
that is still readable through SQL is reported even if its bytes happen to be
outside the scanned window. The key set is the authoritative set the backend
migrates, and both the set and the scanner are pinned by a drift test against
`backend/src/api/secrets.rs`; an `ai_providers.api_key_ref` that still names a
settings row is reported too, because those legacy keys are dynamic and no
static list can enumerate them.

An explicitly named database must be recognizable as a VoidTower database: a
readable `settings` table with `key` and `value` columns. Anything else fails
closed with exit 2. A foreign database that happens to have that exact shape
cannot be told apart, so the operator must name the right file. The CLI requires
at least one `--db` and one `--log`; a partial scan is not a G5 evidence run.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
from pathlib import Path
import re
import sqlite3
import stat
import sys
from typing import Any, Iterable, Sequence

SCHEMA_VERSION = 2  # 2 adds `coverage_limits` to the report
MAX_SCAN_BYTES = 512 * 1024 * 1024
MAX_FINDINGS = 200
CHUNK_BYTES = 1024 * 1024
# Longest credential pattern is bounded well below this; the overlap keeps a
# match that straddles a chunk boundary detectable.
OVERLAP_BYTES = 512
# Widened carry when a credential runs past a chunk boundary. A single match
# that grows beyond this bound fails closed instead of being truncated.
MAX_CARRY_BYTES = 64 * 1024
SNAPSHOT_SUFFIX = ".bak"
SNAPSHOT_MARKER = ".pre-migration-v1-"

# Provider-shaped credential patterns. Each entry is (name, compiled bytes
# pattern). Names appear in findings; matched bytes never do.
PATTERNS: tuple[tuple[str, re.Pattern[bytes]], ...] = (
    ("openai_api_key", re.compile(rb"sk-(?:proj-|ant-)?[A-Za-z0-9_-]{20,}")),
    ("google_api_key", re.compile(rb"AIza[0-9A-Za-z_-]{35}")),
    ("slack_token", re.compile(rb"xox[baprs]-[0-9A-Za-z-]{10,}")),
    (
        "slack_webhook_url",
        re.compile(
            rb"https://hooks\.slack\.com/services/[A-Z0-9]{6,}/[A-Z0-9]{6,}/"
            rb"[A-Za-z0-9]{16,}"
        ),
    ),
    (
        "discord_webhook",
        re.compile(
            rb"https://(?:ptb\.|canary\.)?discord(?:app)?\.com/api/webhooks/"
            rb"[0-9]{5,}/[A-Za-z0-9_-]{20,}"
        ),
    ),
    (
        "proxmox_api_token",
        re.compile(
            rb"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+![A-Za-z0-9._-]+="
            rb"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-"
            rb"[0-9a-fA-F]{4}-[0-9a-fA-F]{12}"
        ),
    ),
    ("private_key_block", re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
    ("bearer_credential", re.compile(rb"(?i)bearer\s+[A-Za-z0-9._~+/-]{20,}")),
)

# Authoritative legacy plaintext classes whose value shape is not discoverable,
# so no byte pattern can cover them. The live-row check reports them while a row
# remains; a deleted copy is only covered by the one-time residue rebuild.
BYTE_PATTERN_COVERAGE_LIMITS = (
    "notif_ntfy_url",
    "odysseus.webhook_secret",
)

# The authoritative legacy plaintext credential settings the backend migrates
# (backend/src/api/secrets.rs). A drift test in the contract suite fails when
# this tuple and the migration source disagree in either direction, so a key
# cannot be invented here or silently dropped from here.
LEGACY_PLAINTEXT_SETTING_KEYS = (
    "notif_ntfy_url",
    "notif_discord_webhook",
    "notif_slack_webhook",
    "odysseus.webhook_secret",
    "proxmox_token",
)


class ScanError(RuntimeError):
    """A scan could not be completed, so no clean result may be claimed."""


@dataclass(frozen=True)
class Finding:
    kind: str
    target: str
    pattern: str
    offset: int | None
    detail: str

    def as_dict(self) -> dict[str, object]:
        return {
            "kind": self.kind,
            "target": self.target,
            "pattern": self.pattern,
            "offset": self.offset,
            "detail": self.detail,
        }


@dataclass
class Report:
    findings: list[Finding] = field(default_factory=list)
    scanned: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)

    def ok(self) -> bool:
        return not self.findings

    def as_dict(self) -> dict[str, object]:
        return {
            "schema_version": SCHEMA_VERSION,
            "ok": self.ok(),
            "findings": [finding.as_dict() for finding in self.findings],
            "scanned": list(self.scanned),
            "skipped": list(self.skipped),
            "coverage_limits": list(BYTE_PATTERN_COVERAGE_LIMITS),
        }


def _bounded_path(path: Path, target: str) -> int:
    """Validate one bounded input; unreadable or oversized input fails closed."""
    try:
        status = path.stat()
    except OSError as exc:
        raise ScanError(f"input is unreadable: {target}") from exc
    if not stat.S_ISREG(status.st_mode):
        # A FIFO or character device has no size to bound and would block or
        # stream forever, so it is not a scannable evidence input.
        raise ScanError(f"input is not a regular file: {target}")
    if status.st_size > MAX_SCAN_BYTES:
        raise ScanError(f"input exceeds the bounded scan size: {target}")
    return status.st_size


def _scan_stream(handle: Any, target: str, kind: str, report: Report) -> None:
    """Scan a binary stream in bounded chunks without loading the whole input.

    A credential may straddle a chunk boundary, so each chunk is scanned together
    with the tail of the previous one. A match that runs to the end of the buffer
    may be extended by the next chunk, so it is deferred and the carry is widened
    back to its start. `OVERLAP_BYTES` bounds the prefix carried across a normal
    boundary and `MAX_CARRY_BYTES` bounds the widened carry. A single match that
    grows beyond that bound is not a clean result, so the scan fails closed
    instead of dropping the match and reporting silence. A match is reported once
    per absolute offset, so the overlap cannot duplicate a finding.
    """
    seen: set[tuple[str, int]] = set()
    overlap = OVERLAP_BYTES
    carry = b""
    base = 0
    while True:
        chunk = handle.read(CHUNK_BYTES)
        final = not chunk
        buffer = carry + chunk
        deferred_start: int | None = None
        for name, pattern in PATTERNS:
            for match in pattern.finditer(buffer):
                if not final and match.end() >= len(buffer):
                    deferred_start = (
                        match.start()
                        if deferred_start is None
                        else min(deferred_start, match.start())
                    )
                    continue
                start = base + match.start()
                if (name, start) in seen:
                    continue
                seen.add((name, start))
                value = match.group(0)
                if len(report.findings) >= MAX_FINDINGS:
                    report.findings.append(
                        Finding(
                            kind=kind,
                            target=target,
                            pattern="finding_limit",
                            offset=None,
                            detail=(
                                f"stopped after {MAX_FINDINGS} findings; the input "
                                "needs manual review"
                            ),
                        )
                    )
                    return
                report.findings.append(
                    Finding(
                        kind=kind,
                        target=target,
                        pattern=name,
                        offset=start,
                        detail=f"{len(value)} bytes matched {name}",
                    )
                )
        if final:
            return
        keep_from = len(buffer) - min(overlap, len(buffer))
        if deferred_start is not None:
            keep_from = min(keep_from, deferred_start)
            if keep_from < len(buffer) - MAX_CARRY_BYTES:
                # The deferred match already spans more than the bounded carry.
                # Truncating it would drop the pattern anchor and report silence,
                # so no clean result may be claimed for this input.
                raise ScanError(
                    f"a single match exceeds the bounded carry: {target}"
                )
        carry = buffer[keep_from:]
        base += keep_from


def scan_file(path: Path, kind: str, report: Report, *, required: bool = False) -> None:
    """Scan one raw file, in bounded chunks, without modifying it.

    A sidecar may legitimately be absent, so it is recorded as skipped. An input
    the operator explicitly named must exist: a scan pointed at the wrong path
    must fail closed instead of reporting a clean result.
    """
    target = path.name
    if not path.exists():
        if required:
            raise ScanError(f"input is missing: {target}")
        report.skipped.append(target)
        return
    _bounded_path(path, target)
    try:
        with path.open("rb") as handle:
            _scan_stream(handle, target, kind, report)
    except OSError as exc:
        raise ScanError(f"input is unreadable: {target}") from exc
    report.scanned.append(target)


def scan_database(db_path: Path, report: Report | None = None) -> Report:
    """Scan a SQLite database and every sidecar SQLite may have written."""
    report = report if report is not None else Report()
    if not db_path.exists():
        raise ScanError(f"input is missing: {db_path.name}")
    scan_file(db_path, "residue", report, required=True)
    for suffix in ("-wal", "-shm", "-journal"):
        scan_file(Path(f"{db_path}{suffix}"), "residue", report)
    return report


def scan_snapshots(db_path: Path, report: Report) -> None:
    """Classify retained pre-migration snapshots instead of ignoring them.

    The controller writes `<database name>.pre-migration-v1-<timestamp>.bak`
    (`backend/src/db/mod.rs`); every file in the database directory whose name
    matches `*.pre-migration-v1-*.bak` is classified. A pre-migration copy
    renamed outside that pattern is not classified, so name it explicitly as a
    `--db` input when one is retained under another name.
    """
    directory = db_path.parent
    for candidate in sorted(directory.glob(f"*{SNAPSHOT_MARKER}*{SNAPSHOT_SUFFIX}")):
        scan_file(candidate, "snapshot", report)


def scan_log(log_path: Path, report: Report | None = None) -> Report:
    report = report if report is not None else Report()
    scan_file(log_path, "log", report, required=True)
    return report


def scan_live_settings(db_path: Path, report: Report) -> None:
    """Report legacy plaintext still readable through SQL.

    Presence, not value length, is the signal: an empty legacy row also means the
    migration did not complete. A database without a readable `settings` table
    cannot be verified as a VoidTower database, so it fails closed instead of
    being recorded as a skip that still exits clean.
    """
    target = db_path.name
    try:
        uri = f"{db_path.resolve().as_uri()}?mode=ro"
        connection = sqlite3.connect(uri, uri=True)
    except sqlite3.Error as exc:
        raise ScanError(f"database is unreadable: {target}") from exc
    try:
        try:
            tables = {
                row[0]
                for row in connection.execute(
                    "SELECT name FROM sqlite_master WHERE type = 'table'"
                )
            }
        except sqlite3.Error as exc:
            raise ScanError(f"database schema is unreadable: {target}") from exc
        if "settings" not in tables:
            raise ScanError(f"database has no settings table: {target}")
        for key in LEGACY_PLAINTEXT_SETTING_KEYS:
            try:
                row = connection.execute(
                    "SELECT length(value) FROM settings WHERE key = ?", (key,)
                ).fetchone()
            except sqlite3.Error as exc:
                # A settings table that cannot answer `key`/`value` is not the
                # VoidTower contract, and a table that cannot be read must not
                # look clean either way.
                raise ScanError(
                    f"database settings table is unverifiable: {target}"
                ) from exc
            if row is None:
                continue
            size = row[0]
            size_text = "unknown size" if size is None else f"{size} bytes"
            report.findings.append(
                Finding(
                    kind="live_plaintext_setting",
                    target=target,
                    pattern="legacy_plaintext_setting",
                    offset=None,
                    detail=f"settings key {key} is still readable ({size_text})",
                )
            )
        if "ai_providers" in tables:
            try:
                providers = connection.execute(
                    "SELECT id, api_key_ref FROM ai_providers "
                    "WHERE api_key_ref IS NOT NULL"
                ).fetchall()
                dangling = []
                for provider_id, reference in providers:
                    row = connection.execute(
                        "SELECT 1 FROM settings WHERE key = ?", (reference,)
                    ).fetchone()
                    if row is not None:
                        dangling.append(provider_id)
            except sqlite3.Error as exc:
                raise ScanError(f"ai provider rows are unreadable: {target}") from exc
            for provider_id in sorted(dangling):
                if len(report.findings) >= MAX_FINDINGS:
                    report.findings.append(
                        Finding(
                            kind="live_plaintext_setting",
                            target=target,
                            pattern="finding_limit",
                            offset=None,
                            detail=(
                                f"stopped after {MAX_FINDINGS} findings; the input "
                                "needs manual review"
                            ),
                        )
                    )
                    break
                report.findings.append(
                    Finding(
                        kind="live_plaintext_setting",
                        target=target,
                        pattern="legacy_ai_provider_reference",
                        offset=None,
                        detail=(
                            f"ai provider {provider_id} still references a "
                            "plaintext settings key"
                        ),
                    )
                )
    finally:
        connection.close()


def scan_candidate(
    database_paths: Sequence[Path], log_paths: Iterable[Path] = ()
) -> Report:
    """Scan every supplied database, its sidecars and snapshots, and every log.

    For each database the raw byte scan (file, then its existing sidecars) runs
    before the read-only SQL check, so the empty `-shm`/`-wal` sidecars that a
    read-only WAL connection can create are outside the byte scan by
    construction and carry no content.
    """
    report = Report()
    for db_path in database_paths:
        scan_database(db_path, report)
        scan_snapshots(db_path, report)
        scan_live_settings(db_path, report)
    for log_path in log_paths:
        scan_log(log_path, report)
    return report


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Scan for provider plaintext in a VoidTower database and logs."
    )
    parser.add_argument("--db", action="append", default=[], type=Path)
    parser.add_argument("--log", action="append", default=[], type=Path)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    if not args.db or not args.log:
        # A G5 evidence run needs both halves; a partial scan must not exit 0
        # and read as a clean result.
        print(
            "g5-plaintext-scan: --db and --log are both required",
            file=sys.stderr,
        )
        return 2
    try:
        report = scan_candidate(args.db, args.log)
    except ScanError as exc:
        print(f"g5-plaintext-scan: {exc}", file=sys.stderr)
        return 2
    if args.json:
        print(json.dumps(report.as_dict(), indent=2, sort_keys=True))
    else:
        for finding in report.findings:
            offset = "" if finding.offset is None else f" @{finding.offset}"
            print(f"{finding.kind}: {finding.target}{offset}: {finding.detail}")
        if report.ok():
            print(
                "g5-plaintext-scan: clean; byte-pattern coverage excludes "
                + ", ".join(BYTE_PATTERN_COVERAGE_LIMITS)
                + " (the live-row check reports them while a row remains)"
            )
    return 0 if report.ok() else 1


if __name__ == "__main__":
    raise SystemExit(main())
