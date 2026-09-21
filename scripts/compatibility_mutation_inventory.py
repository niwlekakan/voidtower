#!/usr/bin/env python3
"""Inventory provider/destructive callsites outside canonical operation adapters.

This is a source-boundary check, not a runtime or provider qualification. It validates
Rust syntax with rustfmt, parses production Rust functions under backend/src/api,
excludes only cfg(test)-selected items, and emits only file/function/line metadata.
It intentionally never prints source lines, request bodies, URLs, or string literals.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
from typing import Iterable

from rust_source_parser import ParseError, _matching, _normalize_ident, lex, normalized_token_digest, parse_source


SCHEMA_VERSION = "voidtower.compatibility-mutation-inventory.v1"
API_ROOT = Path("backend/src/api")
REGISTRY = Path("backend/src/operations/registry.rs")
EXCEPTION_EVIDENCE = Path("scripts/compatibility_mutation_exception_evidence.json")

# These are intentionally narrow, source-visible classifications. They are not
# a bypass allowlist: adding a new function still produces an unknown finding.
READ_ONLY_FUNCTIONS = {
    ("ai", "nvidia_smi_info"),
    ("ai_context", "search_project_code"),
    ("apps", "detect_cuda_major_version"),
    ("apps", "detect_gpu"),
    ("apps", "run_probe"),
    ("capabilities", "which"),
    ("capabilities", "cmd_version"),
    ("capabilities", "detect_systemd"),
    ("capabilities", "detect_docker"),
    ("capabilities", "detect_libvirt"),
    ("capabilities", "detect_wireguard"),
    ("capabilities", "detect_ufw"),
    ("capabilities", "detect_nginx"),
    ("capabilities", "detect_nvidia"),
    ("capabilities", "detect_docker_compose"),
    ("containers", "get_compose"),
    ("containers", "inspect_label"),
    ("diagnostics", "check_disk_space"),
    ("diagnostics", "check_docker"),
    ("diagnostics", "check_restic"),
    ("diagnostics", "check_nginx"),
    ("diagnostics", "check_config_dir"),
    ("members", "statvfs_bytes"),
    ("members", "dir_size_bytes"),
    ("mods", "run"),
    ("network", "read_ip_neigh"),
    ("network", "lookup_hostnames"),
    ("proxy", "docker_host_ip"),
    ("proxy", "check_nginx_setup"),
    ("proxy", "ai_auto_proxy"),
    ("proxy", "nginx_logs"),
    ("proxy", "nginx_status"),
    ("studio", "gpu_summary"),
    ("system", "git"),
    ("vms", "list_local"),
    ("lxc", "get_config"),
    ("wireguard", "wg_available"),
}

READ_ONLY_CALLS = {
    ("lxc", "list"),
}

OUT_OF_SCOPE_FUNCTIONS = {
    ("webhooks", "dispatch_one"): "generic notification-webhook delivery remains separate outbound-egress work",
    ("webhooks", "fire_webhooks"): "generic notification-webhook scheduling remains separate outbound-egress work",
    ("mods", "run_checked"): "dead compatibility helper has no registered production route",
    ("plugins", "extract_zip"): "dead plugin helper has no registered production route",
}

EPHEMERAL_FUNCTIONS = {
    ("proxmox", "vm_vncproxy"): "explicit ephemeral Proxmox VNC ticket exception; no durable mutation",
}

INFERENCE_FUNCTIONS = {
    ("ai_ask", "legacy_odysseus_fallback"): "legacy local inference forwarding; it does not select or reload models",
    ("models", "openai_chat_completions"): "OpenAI-compatible inference forwarding; it does not select or reload models",
}

SYNCHRONOUS_FUNCTIONS = {
    ("auth", "provision_voidwatch"): "bounded local bootstrap token provisioning, outside provider mutation",
    ("disaster", "cli_export"): "explicit local configuration export; no provider or destructive resource mutation",
    ("members", "resolve_member_storage_root"): "bounded local member-directory provisioning, not a provider operation",
    ("models", "models_dir"): "bounded local model-directory initialization, not provider execution",
}

DEFERRED_FUNCTIONS = {
    ("apps", "ensure_vt_proxy_network"): "App Vault lifecycle remains a bounded deferred direct-provider exception",
    ("apps", "recreate_vt_proxy_network_ipv4"): "App Vault lifecycle remains a bounded deferred direct-provider exception",
    ("apps", "rewrite_named_volumes_to_storage_root"): "App Vault storage lifecycle remains a bounded deferred local exception",
    ("apps", "ensure_volume_dirs"): "App Vault storage lifecycle remains a bounded deferred local exception",
    ("apps", "spawn_post_deploy_hook"): "App Vault lifecycle remains a bounded deferred direct-provider exception",
    ("models", "download_file"): "model lifecycle remains a bounded deferred direct-provider exception",
    ("models", "do_ollama_pull"): "model lifecycle remains a bounded deferred provider exception",
    ("models", "do_ollama_create"): "model lifecycle remains a bounded deferred provider exception",
    ("plugins", "extract_zip"): "plugin lifecycle remains a bounded deferred local filesystem exception",
    ("studio", "gallery_delete"): "AI Studio file lifecycle remains a bounded deferred local filesystem exception",
    ("wireguard", "wg_cmd"): "local WireGuard lifecycle remains a bounded deferred provider exception",
}

CANONICAL_HANDLER_FUNCTIONS = {
    ("proxmox", "upload_storage_content"),
}

CANONICAL_HANDLER_MARKERS = {
    ("proxmox", "upload_storage_content"): {"filesystem_mutation"},
    ("proxy", "write_htpasswd_file"): {"provider_direct_call"},
    ("proxy", "write_nginx_conf"): {"provider_direct_call"},
    ("proxy", "remove_htpasswd_file_checked"): {"provider_direct_call"},
    ("proxy", "remove_nginx_conf_checked"): {"provider_direct_call"},
}

ROUTE_REGISTRATION_FUNCTIONS = {
    ("", "router"),
}

CANONICAL_ADAPTER_FUNCTIONS = {
    ("proxy", "write_htpasswd_file"),
    ("proxy", "write_nginx_conf"),
    ("proxy", "remove_htpasswd_file_checked"),
    ("proxy", "remove_nginx_conf_checked"),
}

EVIDENCE_BOUND_FUNCTIONS = (
    set(READ_ONLY_FUNCTIONS)
    | set(READ_ONLY_CALLS)
    | set(OUT_OF_SCOPE_FUNCTIONS)
    | set(EPHEMERAL_FUNCTIONS)
    | set(INFERENCE_FUNCTIONS)
    | set(SYNCHRONOUS_FUNCTIONS)
    | set(DEFERRED_FUNCTIONS)
    | set(CANONICAL_HANDLER_FUNCTIONS)
    | set(CANONICAL_ADAPTER_FUNCTIONS)
)

@dataclass(frozen=True)
class Function:
    module: str
    name: str
    start_line: int
    end_line: int
    body: str
    calls: tuple[tuple[str, int, int], ...]
    canonical_calls: tuple[tuple[str, int, int], ...]
    digest: str
    aliases: tuple[tuple[str, str], ...]


def evidence_digest(function: object) -> str:
    base = normalized_token_digest(function) if hasattr(function, "tokens") else function.digest
    aliases = getattr(function, "aliases", ())
    return hashlib.sha256((base + "\n" + json.dumps(aliases, separators=(",", ":"))).encode()).hexdigest()


def _production_source(text: str) -> str:
    """Mask cfg(test) modules while preserving production source after them."""
    masked = _mask_non_code(text)
    chars = list(text)
    if re.search(r"(?m)^\s*#!\[cfg\(test\)\]", masked):
        return "".join("\n" if char == "\n" else "\r" if char == "\r" else " " for char in chars)
    for attribute in re.finditer(r"#\[cfg\(test\)\]", masked):
        module = re.search(
            r"\bmod\s+[A-Za-z_][A-Za-z0-9_]*\s*(?P<open>\{|;)",
            masked[attribute.end() :],
        )
        if not module:
            raise ValueError("cfg(test) attribute is not attached to a module")
        module_end = attribute.end() + module.end()
        if module.group("open") == ";":
            end = module_end
        else:
            opening = module_end - 1
            end = _find_body_end(masked, opening)
        for position in range(attribute.start(), end):
            if chars[position] not in "\n\r":
                chars[position] = " "
    return "".join(chars)


def _find_body_end(text: str, opening: int) -> int:
    depth = 0
    in_string = False
    escaped = False
    in_line_comment = False
    in_block_comment = False
    index = opening
    while index < len(text):
        char = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""
        if in_line_comment:
            if char == "\n":
                in_line_comment = False
        elif in_block_comment:
            if char == "*" and next_char == "/":
                in_block_comment = False
                index += 1
        elif in_string:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                in_string = False
        elif char == "/" and next_char == "/":
            in_line_comment = True
            index += 1
        elif char == "/" and next_char == "*":
            in_block_comment = True
            index += 1
        elif char == '"':
            in_string = True
        elif char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index + 1
        index += 1
    return len(text)


def _mask_raw_strings(text: str) -> str:
    """Mask Rust raw and byte-raw strings while preserving offsets/newlines."""
    chars = list(text)
    index = 0
    while index < len(text):
        prefix = index
        if index > 0 and (text[index - 1].isalnum() or text[index - 1] == "_"):
            index += 1
            continue
        if text[index] == "b" and index + 1 < len(text) and text[index + 1] == "r":
            index += 1
        if text[index] != "r":
            index = prefix + 1
            continue
        quote = index + 1
        while quote < len(text) and text[quote] == "#":
            quote += 1
        if quote >= len(text) or text[quote] != '"':
            index = prefix + 1
            continue
        hashes = text[index + 1 : quote]
        closing = '"' + hashes
        end = text.find(closing, quote + 1)
        if end < 0:
            raise ValueError("unterminated Rust raw string in scanned source")
        for position in range(prefix, end + len(closing)):
            if chars[position] not in "\n\r":
                chars[position] = " "
        index = end + len(closing)
    return "".join(chars)


def _mask_non_code(text: str) -> str:
    """Mask strings/comments while preserving offsets and line endings."""
    chars = list(_mask_raw_strings(text))
    index = 0
    in_string = False
    escaped = False
    in_line_comment = False
    in_block_comment = False
    while index < len(chars):
        char = chars[index]
        next_char = chars[index + 1] if index + 1 < len(chars) else ""
        if in_line_comment:
            if char == "\n":
                in_line_comment = False
            elif char != "\r":
                chars[index] = " "
        elif in_block_comment:
            if char == "*" and next_char == "/":
                chars[index] = " "
                chars[index + 1] = " "
                in_block_comment = False
                index += 1
            elif char not in "\n\r":
                chars[index] = " "
        elif in_string:
            if escaped:
                escaped = False
                if char not in "\n\r":
                    chars[index] = " "
            elif char == "\\":
                escaped = True
                chars[index] = " "
            elif char == '"':
                in_string = False
                chars[index] = " "
            elif char not in "\n\r":
                chars[index] = " "
        elif char == "/" and next_char == "/":
            chars[index] = " "
            chars[index + 1] = " "
            in_line_comment = True
            index += 1
        elif char == "/" and next_char == "*":
            chars[index] = " "
            chars[index + 1] = " "
            in_block_comment = True
            index += 1
        elif char == '"':
            chars[index] = " "
            in_string = True
        index += 1
    return "".join(chars)


def functions_in(path: Path, text: str) -> Iterable[Function]:
    parts = path.parts
    if "api" in parts:
        module_index = max(index for index, part in enumerate(parts) if part == "api")
    elif "src" in parts:
        module_index = max(index for index, part in enumerate(parts) if part == "src")
    else:
        raise ValueError(f"source path is outside backend/src: {path}")
    relative = Path(*parts[module_index + 1 :])
    module_parts = list(relative.with_suffix("").parts)
    if module_parts[-1] == "mod":
        module_parts.pop()
    module = "::".join(module_parts)
    try:
        parsed = parse_source(path, text, module)
    except ParseError as exc:
        raise ValueError(str(exc)) from exc
    for function in parsed:
        yield Function(
            module=function.module,
            name=function.name,
            start_line=function.start_line,
            end_line=function.end_line,
            body=function.body,
            calls=tuple((call.marker, call.line, call.offset) for call in function.calls),
            canonical_calls=()
            if _has_canonical_shadow_declaration(path)
            else tuple((call.marker, call.line, call.offset) for call in function.canonical_calls),
            digest=normalized_token_digest(function),
            aliases=function.aliases,
        )


def _deferred_sources_from_text(text: str) -> set[str]:
    tokens = lex(text)
    pairs = _matching(tokens)
    sources: set[str] = set()
    for index, token in enumerate(tokens[:-1]):
        if token.text != "DeferredMutationException" or tokens[index + 1].text != "{" or index + 1 not in pairs:
            continue
        body = tokens[index + 2:pairs[index + 1]]
        source_index = next(
            (
                position
                for position, item in enumerate(body[:-2])
                if item.text == "source" and body[position + 1].text == ":" and body[position + 2].kind == "literal"
            ),
            None,
        )
        if source_index is None:
            if any(item.text == "pub" and position + 1 < len(body) and body[position + 1].text == "source" for position, item in enumerate(body)):
                continue
            raise ValueError("deferred exception registry record has no source")
        literal = body[source_index + 2].text
        if not literal.startswith('"'):
            raise ValueError("deferred exception registry contains an invalid source string")
        try:
            source = json.loads(literal)
        except json.JSONDecodeError as exc:
            raise ValueError("exception registry contains an invalid source string") from exc
        if not isinstance(source, str) or not re.fullmatch(r"(?:r#)?[A-Za-z_][A-Za-z0-9_]*(?:::(?:r#)?[A-Za-z_][A-Za-z0-9_]*)+", source):
            raise ValueError("exception registry contains an invalid source identity")
        sources.add(source)
    return sources


def deferred_sources(root: Path) -> set[str]:
    path = root / REGISTRY
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"missing or unsafe exception registry: {REGISTRY}")
    root_resolved = root.resolve()
    registry_resolved = path.resolve()
    if root_resolved != registry_resolved and root_resolved not in registry_resolved.parents:
        raise ValueError(f"exception registry escapes checkout: {REGISTRY}")
    return _deferred_sources_from_text(path.read_text(encoding="utf-8"))


def _marker_matches(function: Function) -> Iterable[tuple[str, int, int]]:
    for marker, line, offset in function.calls:
        yield marker, line, offset


def _load_exception_evidence(root: Path) -> dict[str, dict[str, str]]:
    path = root / EXCEPTION_EVIDENCE
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"missing immutable exception evidence: {EXCEPTION_EVIDENCE}")
    root_resolved = root.resolve()
    evidence_resolved = path.resolve()
    if root_resolved != evidence_resolved and root_resolved not in evidence_resolved.parents:
        raise ValueError(f"immutable exception evidence escapes checkout: {EXCEPTION_EVIDENCE}")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError("invalid immutable exception evidence") from exc
    if not isinstance(payload, dict) or payload.get("schema_version") != "voidtower.compatibility-mutation-exception-evidence.v1":
        raise ValueError("unsupported immutable exception evidence")
    entries = payload.get("exceptions")
    if not isinstance(entries, dict):
        raise ValueError("immutable exception evidence has no exception map")
    result: dict[str, dict[str, str]] = {}
    for source, entry in entries.items():
        if not isinstance(source, str) or not isinstance(entry, dict):
            raise ValueError("immutable exception evidence contains invalid entry")
        file_name = entry.get("file")
        digest = entry.get("sha256")
        if not isinstance(file_name, str) or not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            raise ValueError("immutable exception evidence contains invalid digest")
        result[source] = {"file": file_name, "sha256": digest}
    return result


def _resolve_approval_base(root: Path, requested: str | None) -> str | None:
    """Resolve the trusted Git base used to approve exception bodies."""
    if requested is None and not (root / ".git").exists():
        raise ValueError("immutable exception approval requires a Git base")
    revision = requested or "HEAD"
    try:
        object_format = subprocess.run(
            ["git", "rev-parse", "--show-object-format"],
            cwd=root,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=10,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.SubprocessError) as exc:
        raise ValueError("cannot determine Git object format for immutable exception approval") from exc
    commit_width = {"sha1": 40, "sha256": 64}.get(object_format)
    if requested is not None and (commit_width is None or not re.fullmatch(rf"[0-9a-f]{{{commit_width}}}", requested)):
        raise ValueError("immutable exception approval base must be a full commit ID")
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--verify", "--quiet", "--end-of-options", f"{revision}^{{commit}}"],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise ValueError("cannot resolve the immutable exception approval base") from exc
    commit = result.stdout.decode("ascii", errors="ignore").strip()
    if result.returncode != 0 or not re.fullmatch(r"[0-9a-f]{40,64}", commit):
        raise ValueError("invalid immutable exception approval base")
    return commit


def _approved_registry_error(root: Path, commit: str, current: set[str]) -> str | None:
    """Require the active exception registry itself to be approved by the base."""
    relative = REGISTRY.as_posix()
    approved_text = _read_approved_source(root, commit, relative)
    if approved_text is None:
        return "exception registry has no approved Git base"
    approved = _deferred_sources_from_text(approved_text)
    current_text = (root / REGISTRY).read_text(encoding="utf-8")
    current_digest = hashlib.sha256("\x1f".join(item.text for item in lex(current_text)).encode("utf-8")).hexdigest()
    approved_digest = hashlib.sha256("\x1f".join(item.text for item in lex(approved_text)).encode("utf-8")).hexdigest()
    if approved != current or approved_digest != current_digest:
        return "exception registry differs from approved base"
    return None


def _read_approved_source(root: Path, commit: str, relative: str) -> str | None:
    """Read one source file from the trusted base without invoking a shell."""
    try:
        result = subprocess.run(
            ["git", "show", "--format=", "--no-ext-diff", "--no-textconv", f"{commit}:{relative}"],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise ValueError("cannot read the immutable exception approval base") from exc
    if result.returncode != 0:
        return None
    try:
        return result.stdout.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ValueError("approved exception source is not valid UTF-8") from exc


def _approved_body_error(
    root: Path,
    commit: str | None,
    relative: str,
    source_name: str,
    path: Path,
    function: Function,
    cache: dict[str, dict[str, Function] | None],
) -> str | None:
    """Reject edits to an evidence-bound body unless the Git base already contains them."""
    if commit is None:
        return None
    if relative not in cache:
        approved_text = _read_approved_source(root, commit, relative)
        if approved_text is None:
            cache[relative] = None
        else:
            approved_functions = functions_in(path, approved_text)
            cache[relative] = {
                f"{item.module}::{item.name}": item
                for item in approved_functions
            }
    approved = cache[relative]
    if approved is None or source_name not in approved:
        return "exception body has no approved Git base"
    if evidence_digest(approved[source_name]) != evidence_digest(function):
        return "exception body differs from approved base"
    return None


def _has_unconditional_feature_error(body: str, feature_offset: int) -> bool:
    prefix = body[:feature_offset]
    # A closure may return FeatureUnavailable while the enclosing handler
    # continues to execute.  Without compiler control-flow resolution, reject
    # any closure marker rather than treating its error as an unconditional
    # handler boundary.
    if (
        "||" in prefix
        or re.search(r"\|[^|\n{};]*\|", prefix)
        or re.search(r"\b(?:if|match|while|for|loop)\b", prefix)
        or re.search(r"\basync(?:\s+move)?\s*\{", prefix)
    ):
        return False
    return bool(
        re.search(
            r"\breturn\s+Err\s*\(\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*FeatureUnavailable\b",
            prefix,
        )
    )


def _production_source_paths(api_root: Path) -> tuple[Path, ...]:
    """Exclude external files attached only to cfg(test) modules."""
    candidates = set(api_root.rglob("*.rs"))
    source_root = api_root.parent.resolve()
    test_targets: set[Path] = set()
    production_targets: set[Path] = set()
    for module_file in sorted(api_root.rglob("*.rs")):
        original = module_file.read_text(encoding="utf-8")
        text = _mask_non_code(original)
        for match in re.finditer(
            r"(?P<attrs>(?:#\[[^\]]+\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?mod\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*;",
            text,
        ):
            attrs = match.group("attrs")
            original_attrs = original[match.start("attrs"):match.end("attrs")]
            path_match = re.search(
                r"#\[\s*path\s*=\s*(?P<literal>r\#*\".*?\"\#*|\"(?:\\\\.|[^\"\\\\])*\")\s*\]",
                original_attrs,
            )
            base = module_file.parent if module_file.name == "mod.rs" else module_file.parent / module_file.stem
            if path_match:
                relative = Path(_decode_path_literal(path_match.group("literal")))
                target = (base / relative).resolve()
                if relative.is_absolute() or source_root not in target.parents:
                    raise ValueError(f"unsupported external module path: {module_file}:{relative}")
                targets = (target,)
            else:
                name = match.group("name")
                targets = (base / f"{name}.rs", base / name / "mod.rs")
            destination = test_targets if re.search(r"#\[\s*cfg\s*\(\s*test\s*\)\s*\]", attrs) else production_targets
            destination.update(targets)
    for target in production_targets:
        if target.is_file():
            candidates.add(target)
    for target in test_targets - production_targets:
        candidates.discard(target)
    return tuple(sorted(candidates))


def _decode_path_literal(literal: str) -> str:
    """Decode a Rust path attribute literal or reject it conservatively."""
    if literal.startswith("r"):
        quote = literal.find('"')
        hashes = quote - 1
        closing = '"' + ("#" * hashes)
        if quote < 1 or not literal.endswith(closing):
            raise ValueError("invalid raw external module path literal")
        return literal[quote + 1 : -len(closing)]
    try:
        value = json.loads(literal)
    except json.JSONDecodeError as exc:
        raise ValueError("invalid external module path literal") from exc
    if not isinstance(value, str):
        raise ValueError("invalid external module path literal")
    return value


def _has_canonical_shadow_declaration(path: Path) -> bool:
    """Reject canonical proof below a parent declaring a local same-named module."""
    parts = path.parts
    if "api" not in parts:
        return False
    api_dir = Path(*parts[: parts.index("api") + 1])
    cursor = path.parent
    while cursor != api_dir and api_dir in cursor.parents:
        for candidate in (cursor / "mod.rs", cursor.with_suffix(".rs")):
            if not candidate.is_file() or candidate.is_symlink():
                continue
            tokens = lex(candidate.read_text(encoding="utf-8"))
            for index, token in enumerate(tokens):
                if token.text == "mod" and index + 1 < len(tokens) and _normalize_ident(tokens[index + 1].text) == "operation_adoption":
                    return True
                if (
                    token.text == "extern"
                    and index + 2 < len(tokens)
                    and tokens[index + 1].text == "crate"
                    and any(item.text == "operation_adoption" for item in tokens[index + 2 : index + 8])
                ):
                    return True
                if (
                    token.text == "as"
                    and index + 1 < len(tokens)
                    and _normalize_ident(tokens[index + 1].text) == "operation_adoption"
                ):
                    return True
                if token.text == "use":
                    token_cursor = index + 1
                    while token_cursor < len(tokens) and tokens[token_cursor].text not in {";", "{"}:
                        if _normalize_ident(tokens[token_cursor].text) == "operation_adoption":
                            return True
                        token_cursor += 1
        cursor = cursor.parent
    return False


def inventory(root: Path, approval_base: str | None = None) -> dict[str, object]:
    deferred = deferred_sources(root)
    classified: list[dict[str, object]] = []
    unknown: list[dict[str, object]] = []
    seen_evidence_sources: set[str] = set()
    discovered_sources: set[str] = set()
    api_root = root / API_ROOT
    if not api_root.is_dir():
        raise ValueError(f"missing source directory: {API_ROOT}")
    if api_root.is_symlink():
        raise ValueError(f"source directory is a symlink: {API_ROOT}")
    root_resolved = root.resolve()
    api_resolved = api_root.resolve()
    if root_resolved != api_resolved and root_resolved not in api_resolved.parents:
        raise ValueError(f"source directory escapes checkout: {API_ROOT}")
    for candidate in api_root.rglob("*"):
        if candidate.is_symlink():
            raise ValueError(f"source path is a symlink: {candidate.relative_to(root)}")
    evidence = _load_exception_evidence(root)
    approved_commit = _resolve_approval_base(root, approval_base)
    registry_error = _approved_registry_error(root, approved_commit, deferred)
    if registry_error:
        raise ValueError(registry_error)
    approved_cache: dict[str, dict[str, Function] | None] = {}

    for path in _production_source_paths(api_root):
        if path.is_symlink() or not path.is_file():
            raise ValueError(f"source path is not a regular file: {path.relative_to(root)}")
        relative = path.relative_to(root).as_posix()
        text = path.read_text(encoding="utf-8")
        for function in functions_in(path, text):
            source_name = f"{function.module}::{function.name}"
            discovered_sources.add(source_name)
            masked_body = _mask_non_code(function.body)
            markers = tuple(_marker_matches(function))
            if (function.module, function.name) in EVIDENCE_BOUND_FUNCTIONS or source_name in deferred:
                seen_evidence_sources.add(source_name)
                expected = evidence.get(source_name)
                evidence_error = None
                if expected is None:
                    evidence_error = "missing immutable body evidence"
                elif expected["file"] != relative or expected["sha256"] != evidence_digest(function):
                    evidence_error = "immutable body evidence does not match the function"
                if evidence_error is None:
                    evidence_error = _approved_body_error(
                        root,
                        approved_commit,
                        relative,
                        source_name,
                        path,
                        function,
                        approved_cache,
                    )
                if evidence_error and source_name in deferred and not markers:
                    unknown.append({
                        "file": relative,
                        "function": function.name,
                        "line": function.start_line,
                        "marker": "exception_evidence",
                        "reason": evidence_error,
                    })
                    continue
                if evidence_error and (function.module, function.name) in EVIDENCE_BOUND_FUNCTIONS:
                    raise ValueError(f"{evidence_error} for {source_name}")
            for marker, line, marker_offset in markers:
                finding: dict[str, object] = {
                    "file": relative,
                    "function": function.name,
                    "line": line,
                    "marker": marker,
                }
                feature_offset = masked_body.find("FeatureUnavailable")
                evidence_error = None
                identity = (function.module, function.name)
                if source_name in deferred or identity in EVIDENCE_BOUND_FUNCTIONS:
                    expected = evidence.get(source_name)
                    if expected is None:
                        evidence_error = "missing immutable body evidence"
                    elif expected is not None and expected["file"] != relative:
                        evidence_error = "immutable body evidence names a different file"
                    elif expected is not None and expected["sha256"] != evidence_digest(function):
                        evidence_error = "immutable body evidence does not match the function"
                    if evidence_error is None:
                        evidence_error = _approved_body_error(
                            root,
                            approved_commit,
                            relative,
                            source_name,
                            path,
                            function,
                            approved_cache,
                        )
                if evidence_error is not None:
                    finding.update(classification="unknown", reason=evidence_error)
                    unknown.append(finding)
                    continue
                unsupported_reasons = {
                    "unsupported_macro": "unsupported macro expansion",
                    "unsupported_call_shape": "unsupported mutation call shape",
                    "indirect_function_value": "indirect mutation function value",
                    "unresolved_receiver_provenance": "indirect or unresolved receiver provenance",
                }
                if marker in unsupported_reasons:
                    finding.update(classification="unknown", reason=unsupported_reasons[marker])
                    unknown.append(finding)
                elif marker == "http_route_registration" and identity in ROUTE_REGISTRATION_FUNCTIONS:
                    finding.update(
                        classification="route_registration",
                        reason="framework route declaration, not an outbound provider request",
                    )
                    classified.append(finding)
                elif (
                    source_name in deferred
                    and feature_offset >= 0
                    and feature_offset < marker_offset
                    and _has_unconditional_feature_error(masked_body, feature_offset + len("FeatureUnavailable"))
                ):
                    finding.update(
                        classification="deferred_exception",
                        reason="registered handler fails closed before provider or destructive execution",
                    )
                    classified.append(finding)
                elif (function.module, function.name) in OUT_OF_SCOPE_FUNCTIONS:
                    finding.update(
                        classification="out_of_scope",
                        reason=OUT_OF_SCOPE_FUNCTIONS[(function.module, function.name)],
                    )
                    classified.append(finding)
                elif (function.module, function.name) in EPHEMERAL_FUNCTIONS:
                    finding.update(
                        classification="ephemeral_exception",
                        reason=EPHEMERAL_FUNCTIONS[(function.module, function.name)],
                    )
                    classified.append(finding)
                elif (function.module, function.name) in INFERENCE_FUNCTIONS:
                    finding.update(
                        classification="inference_proxy",
                        reason=INFERENCE_FUNCTIONS[(function.module, function.name)],
                    )
                    classified.append(finding)
                elif (function.module, function.name) in SYNCHRONOUS_FUNCTIONS:
                    finding.update(
                        classification="synchronous_exception",
                        reason=SYNCHRONOUS_FUNCTIONS[(function.module, function.name)],
                    )
                    classified.append(finding)
                elif (function.module, function.name) in DEFERRED_FUNCTIONS:
                    finding.update(
                        classification="deferred_exception",
                        reason=DEFERRED_FUNCTIONS[(function.module, function.name)],
                    )
                    classified.append(finding)
                elif (
                    marker in CANONICAL_HANDLER_MARKERS.get((function.module, function.name), set())
                    and (
                        (function.module, function.name) in CANONICAL_ADAPTER_FUNCTIONS
                        or (
                            (function.module, function.name) in CANONICAL_HANDLER_FUNCTIONS
                            and function.canonical_calls
                        )
                    )
                ):
                    finding.update(
                        classification="canonical_adapter",
                        reason="helper is called only by the canonical operation adapter",
                    )
                    classified.append(finding)
                elif (
                    ((function.module, function.name) in READ_ONLY_FUNCTIONS
                     or (function.module, function.name) in READ_ONLY_CALLS)
                    and marker != "provider_http_mutation"
                ):
                    finding.update(
                        classification="read_only_probe",
                        reason="capability, diagnostics, search, or provider-observation probe",
                    )
                    classified.append(finding)
                else:
                    unknown.append(finding)

    missing_registry = sorted(deferred - discovered_sources)
    if missing_registry:
        raise ValueError(f"deferred exception registry has {len(missing_registry)} missing source functions")
    extra = sorted(set(evidence) - (seen_evidence_sources | deferred))
    if extra:
        raise ValueError(f"immutable exception evidence has {len(extra)} stale identities")

    key = lambda item: (str(item["file"]), int(item["line"]), str(item["function"]), str(item["marker"]))
    classified.sort(key=key)
    unknown.sort(key=key)
    return {
        "schema_version": SCHEMA_VERSION,
        "scope": "backend/src/api production Rust source; operation adapters are the approved external execution boundary",
        "exception_approval": {
            "mode": "git_base",
            "base": approved_commit,
        },
        "classified": classified,
        "unknown": unknown,
        "status": "passed" if not unknown else "failed",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--check", action="store_true", help="exit 1 when unknown callsites exist")
    parser.add_argument(
        "--base",
        help="trusted Git commit whose evidence-bound exception bodies must match",
    )
    args = parser.parse_args(argv)
    root = args.repo.resolve()
    try:
        report = inventory(root, args.base)
    except (OSError, ValueError) as exc:
        print(f"compatibility inventory: {exc}", file=sys.stderr)
        return 2
    print(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 1 if args.check and report["unknown"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
