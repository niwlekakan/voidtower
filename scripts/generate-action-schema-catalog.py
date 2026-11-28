#!/usr/bin/env python3
"""Generate the durable action schema catalog from the Rust action registry.

The registry is the source of truth. This bounded parser only accepts the durable
macro forms used in ACTIONS and fails closed when a durable invocation cannot be
identified, so the JSON catalog cannot silently omit a schema.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "backend/src/action_registry.rs"
OUTPUT = ROOT / "backend/contracts/action-schema-catalog.json"
GENERATOR_VERSION = "1"
ACTIONS_START = "pub const ACTIONS: &[ActionMetadata] = &["
ACTION_INVOCATION = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*!\s*\(")
NON_DURABLE_MACROS = {"action_metadata", "internal_action_metadata"}
DURABLE_MACROS = {
    "durable_action_metadata",
    "durable_mutation",
    "durable_scoped_mutation",
    "durable_webhook_scoped_mutation",
    "durable_mcp_scoped_mutation",
    "durable_read_job",
}


def _macro_invocations(region: str) -> list[tuple[str, list[str]]]:
    invocations = []
    for match in ACTION_INVOCATION.finditer(region):
        depth = 0
        in_string = False
        escaped = False
        close = None
        for index in range(match.end() - 1, len(region)):
            char = region[index]
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
                continue
            if char == '"':
                in_string = True
            elif char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0:
                    close = index
                    break
        if close is None:
            raise ValueError(f"unterminated action metadata macro: {match.group(1)}")
        body = region[match.end():close]
        args = []
        start = 0
        depth = 0
        in_string = False
        escaped = False
        for index, char in enumerate(body):
            if in_string:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == '"':
                    in_string = False
            elif char == '"':
                in_string = True
            elif char in "([{":
                depth += 1
            elif char in ")]}":
                depth -= 1
            elif char == "," and depth == 0:
                args.append(body[start:index].strip())
                start = index + 1
        args.append(body[start:].strip())
        invocations.append((match.group(1), args))
    return invocations


def _schema_lifecycle(source: str) -> str:
    if re.search(r"schema_lifecycle:\s*Some\(SchemaLifecycle::Active\)", source):
        return "active"
    raise ValueError("durable action metadata has no supported schema lifecycle")


def _macro_definition(source: str, name: str) -> str:
    start = source.find(f"macro_rules! {name}")
    if start < 0:
        raise ValueError(f"action metadata macro definition is missing: {name}")
    end = source.find("\n}\n", start)
    if end < 0:
        raise ValueError(f"action metadata macro definition is unterminated: {name}")
    return source[start:end]


def durable_actions(source: str) -> list[str]:
    start = source.find(ACTIONS_START)
    if start < 0:
        raise ValueError("action registry ACTIONS source region is missing")
    end = source.find("\n];", start)
    if end < 0:
        raise ValueError("action registry ACTIONS source region is unterminated")
    region = source[start:end]
    invocations = _macro_invocations(region)
    unknown = sorted({macro for macro, _args in invocations} - NON_DURABLE_MACROS - DURABLE_MACROS)
    if unknown:
        raise ValueError(f"unknown action metadata macro: {', '.join(unknown)}")
    actions = []
    lifecycle = _schema_lifecycle(source)
    for macro, args in invocations:
        if macro not in DURABLE_MACROS:
            continue
        if not args or not re.fullmatch(r'"[^"\\]+"', args[0]):
            raise ValueError(f"durable action {macro} has no literal name")
        name = args[0][1:-1]
        if macro == "durable_action_metadata":
            if len(args) < 9:
                raise ValueError(f"durable action {name} has incomplete schema metadata")
            input_id = args[7].strip().strip('"')
            result_id = args[8].strip().strip('"')
            if input_id != f"{name}.input.v1":
                raise ValueError(f"input schema identity for {name} is not source-consistent")
            if result_id != f"{name}.result.v1":
                raise ValueError(f"result schema identity for {name} is not source-consistent")
        else:
            definition = _macro_definition(source, macro)
            expansions = re.findall(r'concat!\(\$name,\s*"([^"]+)"\)', definition)
            if expansions != [".input.v1", ".result.v1"]:
                raise ValueError(f"durable action wrapper {macro} has non-v1 input schema identity")
        if lifecycle != "active":
            raise ValueError(f"unsupported lifecycle for durable action {name}")
        actions.append(name)
    if not actions:
        raise ValueError("no durable action invocations were found")
    if len(actions) != len(set(actions)):
        raise ValueError("duplicate durable action schema identity")
    if "schema_lifecycle: Some(SchemaLifecycle::Active)" not in source:
        raise ValueError("durable action metadata does not declare schema lifecycle")
    return sorted(actions)


def catalog(source: str) -> dict[str, object]:
    actions = durable_actions(source)
    return {
        "contract": "voidtower.action.schemas",
        "schema_version": 1,
        "generator_version": GENERATOR_VERSION,
        "source": "backend/src/action_registry.rs",
        "source_sha256": hashlib.sha256(source.encode("utf-8")).hexdigest(),
        "deprecation_policy": {
            "active": "v1 schemas are accepted at the canonical durable boundary",
            "deprecated": "a deprecated schema must name a replacement and optional RFC 8594 sunset date",
            "unknown": "unknown or forged schema identities fail closed before adapter planning",
        },
        "actions": [
            {
                "action": name,
                "input_schema_id": f"{name}.input.v1",
                "result_schema_id": f"{name}.result.v1",
                "lifecycle": "active",
            }
            for name in actions
        ],
    }


def render(value: dict[str, object]) -> str:
    return json.dumps(value, indent=2, sort_keys=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail when the catalog is stale")
    args = parser.parse_args()
    value = catalog(SOURCE.read_text(encoding="utf-8"))
    expected = render(value)
    if args.check:
        try:
            actual = OUTPUT.read_text(encoding="utf-8")
        except FileNotFoundError:
            print(f"Generated action schema catalog is missing: {OUTPUT}")
            return 1
        if actual != expected:
            print(f"Generated action schema catalog is stale: {OUTPUT}")
            return 1
        print(f"Generated action schema catalog is current: {OUTPUT}")
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(expected, encoding="utf-8")
    print(f"Generated {OUTPUT} from {SOURCE}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
