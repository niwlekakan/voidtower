"""Small fail-closed Rust source parser used by the compatibility inventory.

This is deliberately not a Rust compiler or type checker.  It delegates syntax
validation to rustfmt, then parses the bounded structural facts needed by the
source-boundary inventory from comments/strings-free tokens.  Unsupported or
ambiguous forms raise ParseError instead of being guessed.
"""

from __future__ import annotations

from dataclasses import dataclass, replace
from pathlib import Path
import re
import subprocess
from typing import Iterable


class ParseError(ValueError):
    """The source could not be parsed with the bounded inventory grammar."""


@dataclass(frozen=True)
class Token:
    kind: str
    text: str
    start: int
    end: int
    line: int


@dataclass(frozen=True)
class Call:
    marker: str
    line: int
    offset: int


@dataclass(frozen=True)
class ParsedFunction:
    module: str
    name: str
    start_line: int
    end_line: int
    start: int
    end: int
    body: str
    tokens: tuple[Token, ...]
    calls: tuple[Call, ...]
    canonical_calls: tuple[Call, ...]
    aliases: tuple[tuple[str, str], ...]


@dataclass(frozen=True)
class _Scope:
    kind: str
    name: str
    start: int
    end: int


_IDENT = re.compile(r"(?:[^\W\d]\w*|_\w*)", re.UNICODE)
_MULTI_PUNCT = ("::", "=>", "->", "..=", "..", "&&", "||", "==", "!=", "<=", ">=")
_KNOWN_ATTRIBUTES = {"allow", "async_trait", "cfg", "derive", "path", "rustfmt", "serde", "test", "tokio"}
_MUTATING_NAMES = {"write", "write_all", "write_fmt", "flush", "shutdown", "create", "create_dir", "create_dir_all", "create_new", "remove_file", "remove_dir", "remove_dir_all", "rename", "copy", "set_permissions", "set_len", "set_times", "sync_all", "sync_data", "truncate", "hard_link", "soft_link", "Command", "open", "send", "post", "put", "delete", "patch", "deploy_compose", "restart_compose", "remove_compose", "stop_compose", "pull_compose", "container_action", "run", "init", "restore", "apply", "add", "set", "reload", "write_conf", "write_htpasswd"}


def _normalize_ident(text: str) -> str:
    """Compare Rust raw identifiers by their semantic identifier spelling."""
    return text[2:] if text.startswith("r#") else text


def _normalize_path(path: str) -> str:
    leading = "::" if path.startswith("::") else ""
    parts = path.removeprefix("::").split("::")
    return leading + "::".join(_normalize_ident(part) for part in parts)


def _rustfmt_parse(path: Path, text: str) -> None:
    try:
        result = subprocess.run(
            ["rustfmt", "--edition", "2021", "--emit", "stdout"],
            check=False,
            input=text,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.SubprocessError) as exc:
        raise ParseError(f"rustfmt parser unavailable for {path}") from exc
    if result.returncode:
        raise ParseError(f"Rust syntax rejected in {path}")


def _skip_string(text: str, index: int) -> int:
    quote = text[index]
    index += 1
    while index < len(text):
        if text[index] == "\\":
            index += 2
        elif text[index] == quote:
            return index + 1
        else:
            index += 1
    raise ParseError("unterminated Rust string or character literal")


def _skip_raw_string(text: str, index: int) -> int | None:
    prefix = index
    if text.startswith("br", index):
        index += 1
    elif text.startswith("r", index):
        index += 1
    else:
        return None
    hashes = 0
    while index < len(text) and text[index] == "#":
        hashes += 1
        index += 1
    if index >= len(text) or text[index] != '"':
        return None
    closing = '"' + ("#" * hashes)
    end = text.find(closing, index + 1)
    if end < 0:
        raise ParseError("unterminated Rust raw string")
    return end + len(closing)


def lex(text: str) -> tuple[Token, ...]:
    tokens: list[Token] = []
    index = 0
    line = 1
    while index < len(text):
        char = text[index]
        if char in " \t\r\n":
            if char == "\n":
                line += 1
            index += 1
            continue
        if text.startswith("//", index):
            end = text.find("\n", index + 2)
            if end < 0:
                break
            line += 1
            index = end + 1
            continue
        if text.startswith("/*", index):
            end = index + 2
            depth = 1
            while end < len(text) and depth:
                if text.startswith("/*", end):
                    depth += 1
                    end += 2
                elif text.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
            if depth:
                raise ParseError("unterminated Rust block comment")
            line += text[index:end].count("\n")
            index = end
            continue
        raw_end = _skip_raw_string(text, index)
        if raw_end is not None:
            line += text[index:raw_end].count("\n")
            tokens.append(Token("literal", text[index:raw_end], index, raw_end, line))
            index = raw_end
            continue
        if text.startswith("r#", index):
            raw_identifier = _IDENT.match(text, index + 2)
            if raw_identifier:
                end = raw_identifier.end()
                tokens.append(Token("ident", text[index:end], index, end, line))
                index = end
                continue
        if char == "'" and index + 1 < len(text) and (text[index + 1].isalpha() or text[index + 1] == "_") and not (index + 2 < len(text) and text[index + 2] == "'"):
            tokens.append(Token("punct", char, index, index + 1, line))
            index += 1
            continue
        if char in ('"', "'"):
            end = _skip_string(text, index)
            line += text[index:end].count("\n")
            tokens.append(Token("literal", text[index:end], index, end, line))
            index = end
            continue
        match = _IDENT.match(text, index)
        if match:
            end = match.end()
            tokens.append(Token("ident", text[index:end], index, end, line))
            index = end
            continue
        punctuation = next((item for item in _MULTI_PUNCT if text.startswith(item, index)), None)
        if punctuation:
            tokens.append(Token("punct", punctuation, index, index + len(punctuation), line))
            index += len(punctuation)
            continue
        tokens.append(Token("punct", char, index, index + 1, line))
        index += 1
    return tuple(tokens)


def _matching(tokens: tuple[Token, ...]) -> dict[int, int]:
    pairs: dict[int, int] = {}
    stack: list[tuple[str, int]] = []
    opens = {"{": "}", "[": "]", "(": ")"}
    closes = {value: key for key, value in opens.items()}
    for index, token in enumerate(tokens):
        if token.text in opens:
            stack.append((token.text, index))
        elif token.text in closes:
            if not stack or stack[-1][0] != closes[token.text]:
                raise ParseError(f"unbalanced Rust delimiter near line {token.line}")
            _, opening = stack.pop()
            pairs[opening] = index
            pairs[index] = opening
    if stack:
        raise ParseError("unclosed Rust delimiter")
    return pairs


def _attribute_end(tokens: tuple[Token, ...], start: int, pairs: dict[int, int]) -> int:
    if start >= len(tokens) or tokens[start].text != "#":
        return start
    cursor = start + 1
    if cursor < len(tokens) and tokens[cursor].text == "!":
        cursor += 1
    if cursor >= len(tokens) or tokens[cursor].text != "[" or cursor not in pairs:
        raise ParseError("malformed Rust attribute")
    return pairs[cursor] + 1


def _cfg_test_attributes(tokens: tuple[Token, ...], pairs: dict[int, int]) -> tuple[tuple[int, int], bool]:
    excluded: list[tuple[int, int]] = []
    inner_cfg = False
    for cursor, token in enumerate(tokens):
        if token.text != "#":
            continue
        if cursor + 1 >= len(tokens) or tokens[cursor + 1].text not in {"[", "!"}:
            continue
        attr_end = _attribute_end(tokens, cursor, pairs)
        attr = [item.text for item in tokens[cursor:attr_end]]
        attribute_name = attr[3] if len(attr) > 3 and attr[1] == "!" else attr[2] if len(attr) > 2 else ""
        if attribute_name not in _KNOWN_ATTRIBUTES:
            raise ParseError(f"unsupported Rust attribute near line {token.line}")
        is_inner = attr[:2] == ["#", "!"] and attr[2:7] == ["[", "cfg", "(", "test", ")"]
        is_outer = attr[:1] == ["#"] and attr[1:6] == ["[", "cfg", "(", "test", ")"]
        if is_inner:
            containing = [opening for opening, closing in pairs.items() if opening < cursor and closing > cursor and tokens[opening].text == "{"]
            if containing:
                opening = max(containing, key=lambda item: item)
                excluded.append((tokens[opening].start, tokens[pairs[opening]].end))
            else:
                inner_cfg = True
        if is_outer:
            item_start = attr_end
            while item_start < len(tokens) and tokens[item_start].text == "#":
                item_start = _attribute_end(tokens, item_start, pairs)
            if item_start >= len(tokens):
                raise ParseError("cfg(test) attribute has no following item")
            item_end = _item_end(tokens, item_start, pairs)
            excluded.append((tokens[cursor].start, tokens[item_end - 1].end))
    return tuple(excluded), inner_cfg


def _item_end(tokens: tuple[Token, ...], start: int, pairs: dict[int, int]) -> int:
    """Return the end-exclusive token index for one immediate Rust item."""
    cursor = start
    while cursor < len(tokens) and tokens[cursor].text == "#":
        cursor = _attribute_end(tokens, cursor, pairs)
    brace = None
    depth = 0
    while cursor < len(tokens):
        value = tokens[cursor].text
        if value in "([":
            depth += 1
        elif value in ")]":
            depth -= 1
        elif value == "{" and depth == 0:
            brace = cursor
            break
        elif value == ";" and depth == 0:
            return cursor + 1
        cursor += 1
    if brace is None:
        raise ParseError("Rust item has no body or terminator")
    return pairs[brace] + 1


def _scopes(tokens: tuple[Token, ...], pairs: dict[int, int]) -> tuple[_Scope, ...]:
    result: list[_Scope] = []
    for index, token in enumerate(tokens):
        if token.text not in {"mod", "impl"} or index + 1 >= len(tokens):
            continue
        if token.text == "mod" and tokens[index + 1].kind != "ident":
            raise ParseError("module declaration has no identifier")
        cursor = index + 1
        name = tokens[cursor].text if token.text == "mod" else "impl"
        if token.text == "impl":
            while cursor < len(tokens) and tokens[cursor].text != "{" and tokens[cursor].text != ";":
                cursor += 1
            if cursor >= len(tokens) or tokens[cursor].text != "{":
                continue
            type_tokens = [item.text for item in tokens[index + 1:cursor] if item.kind == "ident"]
            name = "impl<" + "_".join(type_tokens) + ">"
        else:
            cursor += 1
            while cursor < len(tokens) and tokens[cursor].text not in {"{", ";"}:
                cursor += 1
            if cursor >= len(tokens) or tokens[cursor].text != "{":
                continue
        result.append(_Scope(token.text, name, token.start, tokens[pairs[cursor]].end))
    return tuple(result)


def _path_at(tokens: tuple[Token, ...], start: int) -> tuple[str, int] | None:
    leading = False
    if start < len(tokens) and tokens[start].text == "::":
        if start and tokens[start - 1].kind == "ident":
            return None
        leading = True
        start += 1
    if start >= len(tokens) or tokens[start].kind != "ident":
        return None
    parts = [_normalize_ident(tokens[start].text)]
    cursor = start + 1
    while cursor + 1 < len(tokens) and tokens[cursor].text == "::" and tokens[cursor + 1].kind == "ident":
        parts.append(_normalize_ident(tokens[cursor + 1].text))
        cursor += 2
    return ("::" if leading else "") + "::".join(parts), cursor


def _imports(tokens: tuple[Token, ...]) -> dict[str, str]:
    aliases: dict[str, str] = {}
    cursor = 0
    while cursor < len(tokens):
        if tokens[cursor].text != "use":
            cursor += 1
            continue
        end = cursor + 1
        depth = 0
        while end < len(tokens):
            if tokens[end].text == "{":
                depth += 1
            elif tokens[end].text == "}":
                depth -= 1
            elif tokens[end].text == ";" and depth == 0:
                break
            end += 1
        if end >= len(tokens):
            raise ParseError("use declaration has no terminator")
        segment = tokens[cursor + 1:end]
        if "{" in [item.text for item in segment] or "*" in [item.text for item in segment]:
            _group_import(segment, aliases)
            cursor = end + 1
            continue
        path = _path_at(segment, 0)
        if path:
            full, consumed = path
            if consumed < len(segment) and segment[consumed].text == "as" and consumed + 1 < len(segment):
                aliases[_normalize_ident(segment[consumed + 1].text)] = full
            elif consumed == len(segment):
                aliases[full.split("::")[-1]] = full
            elif consumed < len(segment) and segment[consumed].text == "::" and segment[consumed + 1].text == "{":
                base = full
                close = next((i for i in range(consumed + 1, len(segment)) if segment[i].text == "}"), len(segment))
                members = segment[consumed + 2:close]
                current: list[Token] = []
                chunks: list[list[Token]] = []
                for member in list(members) + [Token("punct", ",", 0, 0, 0)]:
                    if member.text == ",":
                        if current:
                            chunks.append(current)
                        current = []
                    else:
                        current.append(member)
                for member in chunks:
                    if not member or member[0].kind != "ident":
                        continue
                    target = base if member[0].text == "self" else f"{base}::{member[0].text}"
                    if len(member) >= 3 and member[1].text == "as":
                        aliases[_normalize_ident(member[2].text)] = target
                    else:
                        aliases[_normalize_ident(member[0].text)] = target
        cursor = end + 1
    return aliases


def _group_import(tokens: tuple[Token, ...], aliases: dict[str, str], prefix: str = "") -> None:
    if tokens and tokens[0].text == "self" and len(tokens) == 1 and prefix:
        aliases[_normalize_ident(prefix.split("::")[-1])] = prefix
        return
    parts: list[str] = []
    cursor = 0
    while cursor < len(tokens) and tokens[cursor].kind == "ident":
        parts.append(tokens[cursor].text)
        cursor += 1
        if cursor < len(tokens) and tokens[cursor].text == "::":
            if cursor + 1 < len(tokens) and tokens[cursor + 1].text in {"{", "*"}:
                cursor += 1
                break
            cursor += 1
            continue
        break
    base = "::".join([item for item in [prefix, *parts] if item])
    if cursor >= len(tokens):
        if base:
            aliases[_normalize_ident(base.split("::")[-1])] = base
        return
    if tokens[cursor].text == "*":
        known_by_base = {
            "std::fs": {"write", "create_dir", "create_dir_all", "remove_file", "remove_dir", "remove_dir_all", "rename", "copy", "OpenOptions", "File"},
            "tokio::fs": {"write", "create_dir", "create_dir_all", "remove_file", "remove_dir", "remove_dir_all", "rename", "copy", "OpenOptions", "File"},
            "std::process": {"Command"},
            "tokio::process": {"Command"},
            "reqwest": {"Client", "Method"},
        }
        if base not in known_by_base:
            raise ParseError("unsupported wildcard use declaration")
        known = known_by_base[base]
        for name in known:
            aliases[_normalize_ident(name)] = f"{base}::{name}"
        return
    if tokens[cursor].text != "{":
        raise ParseError("unsupported grouped use declaration")
    depth = 0
    close = None
    for index in range(cursor, len(tokens)):
        if tokens[index].text == "{":
            depth += 1
        elif tokens[index].text == "}":
            depth -= 1
            if depth == 0:
                close = index
                break
    if close is None:
        raise ParseError("unclosed grouped use declaration")
    chunks: list[list[Token]] = []
    current: list[Token] = []
    nested = 0
    for item in list(tokens[cursor + 1:close]) + [Token("punct", ",", 0, 0, 0)]:
        if item.text == "{":
            nested += 1
        elif item.text == "}":
            nested -= 1
        if item.text == "," and nested == 0:
            if current:
                chunks.append(current)
            current = []
        else:
            current.append(item)
    for chunk in chunks:
        if len(chunk) >= 3 and chunk[-2].text == "as" and chunk[-1].kind == "ident":
            alias = _normalize_ident(chunk[-1].text)
            _group_import(tuple(chunk[:-2]), aliases, base)
            target = aliases.get(chunk[-3].text, f"{base}::{chunk[-3].text}")
            aliases[alias] = target
        else:
            _group_import(tuple(chunk), aliases, base)


def _reject_unsafe_reexports(tokens: tuple[Token, ...], excluded: tuple[tuple[int, int], ...]) -> None:
    """Reject re-exports whose provenance is outside this bounded resolver.

    The inventory cannot prove what a local or wildcard re-export ultimately
    names.  The one production re-export currently needed is a standard trait;
    every other re-export must wait for compiler-grade module resolution rather
    than being treated as safe.
    """
    safe = ("std", "::", "str", "::", "FromStr")
    for index, token in enumerate(tokens):
        if token.text != "pub" or any(left <= token.start < right for left, right in excluded):
            continue
        cursor = index + 1
        if cursor < len(tokens) and tokens[cursor].text == "(":
            cursor += 1
            while cursor < len(tokens) and tokens[cursor].text != ")":
                cursor += 1
            cursor += 1
        if cursor >= len(tokens) or tokens[cursor].text != "use":
            continue
        end = cursor + 1
        while end < len(tokens) and tokens[end].text != ";":
            end += 1
        if end >= len(tokens):
            raise ParseError("re-export declaration has no terminator")
        segment = tuple(item.text for item in tokens[cursor + 1:end])
        if segment != safe:
            raise ParseError(f"unsupported re-export near line {token.line}")


def _resolve(path: str, aliases: dict[str, str]) -> str:
    path = _normalize_path(path).removeprefix("::")
    seen: set[str] = set()
    while True:
        first, separator, remainder = path.partition("::")
        target = aliases.get(first)
        if target is None:
            return path
        if first in seen:
            return path
        seen.add(first)
        path = _normalize_path(target)
        if separator and remainder:
            path = f"{path}::{remainder}"


def _is_mutating_path(path: str) -> bool:
    normalized = _normalize_path(path).removeprefix("::")
    terminal = normalized.rsplit("::", 1)[-1]
    if terminal in {
        "write", "create", "create_dir", "create_dir_all", "create_new", "remove_file", "open",
        "remove_dir", "remove_dir_all", "rename", "copy", "set_permissions", "hard_link", "soft_link",
    }:
        return normalized.startswith(("std::fs::", "tokio::fs::")) or "::" not in normalized
    if terminal in {"post", "put", "delete", "patch", "send", "request"}:
        return normalized.startswith("reqwest::")
    if normalized.endswith(("std::process::Command::new", "tokio::process::Command::new")):
        return True
    return normalized.endswith((
        "::deploy_compose", "::restart_compose", "::remove_compose", "::stop_compose", "::pull_compose",
        "::container_action", "::apply", "::add", "::remove", "::set", "::reload", "::write",
        "::write_conf", "::write_htpasswd", "::init", "::restore",
    ))


_SAFE_MUTATION_PROVENANCE = (
    "std::fs::", "tokio::fs::", "std::process::", "tokio::process::", "reqwest::", "axum::routing::",
)
_SAFE_MUTATION_EXACT = {
    # Explicit non-provider service boundaries; module prefixes are not trust.
    "crate::cmdb::assets::create",
    "crate::cmdb::assets::create_manual",
    "crate::cmdb::assets::rename",
    "crate::cmdb::assets::set_retired",
    "crate::cmdb::catalog_admin::self::create_class",
    "crate::cmdb::catalog_admin::self::delete_class",
    "crate::cmdb::catalog_admin::self::create_type",
    "crate::cmdb::catalog_admin::self::delete_type",
    "crate::cmdb::locations::self::create",
    "crate::cmdb::locations::self::delete",
    "crate::cmdb::locations::self::update",
    "crate::cmdb::observations::self::ignore_discovery",
    "crate::cmdb::observations::self::link_discovery",
    "crate::cmdb::observations::self::register_discovery",
    "crate::cmdb::observations::ingest",
    "crate::cmdb::relationships::self::end",
    "crate::cmdb::relationships::self::start",
    "crate::cmdb::settings::self::update",
    "crate::networking::proxy::self::write_htpasswd",
    "crate::networking::proxy::self::write_conf",
    "crate::networking::proxy::self::remove_htpasswd",
    "crate::networking::proxy::self::remove_conf",
}
_SAFE_IMPORTED_PROVENANCE = (
    "std::", "tokio::", "reqwest::", "axum::", "axum_extra::", "serde::", "serde_json::", "futures_util::",
    "sqlx::", "tracing::", "anyhow::", "uuid::", "url::", "base64::", "aes_gcm::", "sha2::", "action_registry::",
    "super::", "crate::api::", "crate::agent", "crate::ai", "crate::audit", "crate::auth", "crate::collector",
    "crate::cmdb::", "crate::config", "crate::db", "crate::error",
    "crate::policy", "crate::services", "crate::containers", "crate::voidwatch", "crate::AppState",
)


def _unresolved_mutation_alias(raw: str, resolved: str, terminal: str) -> bool:
    return (
        resolved != raw
        and terminal in _MUTATING_NAMES
        and not (resolved in _SAFE_MUTATION_EXACT or resolved.startswith(_SAFE_MUTATION_PROVENANCE))
    )


def _untrusted_bare_alias(raw: str, resolved: str) -> bool:
    terminal = resolved.rsplit("::", 1)[-1]
    return (
        not raw.startswith("::")
        and len(raw.split("::")) == 1
        and resolved != raw
        and terminal in _MUTATING_NAMES
        and resolved not in _SAFE_MUTATION_EXACT
        and not resolved.startswith(_SAFE_IMPORTED_PROVENANCE)
    )


def _shadowed_aliases(tokens: tuple[Token, ...], aliases: dict[str, str]) -> set[str]:
    """Return imported names that are rebound in this function item.

    The inventory cannot model every Rust pattern binding, so it deliberately
    disables canonical proof for an item when an imported name is rebound by a
    parameter, local, loop binding, or closure parameter anywhere in that item.
    This is conservative: an ambiguous item becomes unknown rather than being
    authorized by a stale module-level import.
    """
    names = set(aliases)
    shadowed: set[str] = set()
    pairs = _matching(tokens)
    for function_index, token in enumerate(tokens):
        if token.text != "fn" or function_index + 2 >= len(tokens):
            continue
        opening = next(
            (position for position in range(function_index + 2, len(tokens)) if tokens[position].text == "("),
            None,
        )
        if opening is None or opening not in pairs:
            continue
        closing = pairs[opening]
        segment_start = opening + 1
        depth = 0
        for position in range(opening + 1, closing):
            value = tokens[position].text
            if value in {"(", "[", "{"}:
                depth += 1
            elif value in {")", "]", "}"}:
                depth = max(0, depth - 1)
            elif value == ":" and depth == 0:
                for item in tokens[segment_start:position]:
                    if item.kind == "ident" and _normalize_ident(item.text) in names:
                        shadowed.add(_normalize_ident(item.text))
            elif value == "," and depth == 0:
                segment_start = position + 1
    for index, token in enumerate(tokens):
        if token.kind != "ident":
            continue
        name = _normalize_ident(token.text)
        if name not in names:
            continue
        previous = tokens[index - 1].text if index else ""
        before_previous = tokens[index - 2].text if index >= 2 else ""
        following = tokens[index + 1].text if index + 1 < len(tokens) else ""
        if previous in {"let", "for"} or (previous == "mut" and before_previous == "let"):
            shadowed.add(name)
        elif following == ":" and not (index + 2 < len(tokens) and tokens[index + 2].text == "::"):
            shadowed.add(name)

    # Destructuring, `if let`/`while let`, and `for` patterns bind names away
    # from the simple `let name`/`for name` forms above. Conservatively treat
    # every imported alias in a pattern as shadowed.
    for index, token in enumerate(tokens):
        if token.text not in {"let", "for"}:
            continue
        cursor = index + 1
        while cursor < len(tokens) and tokens[cursor].text not in {"=", "in", ";", "=>"}:
            item = tokens[cursor]
            if item.kind == "ident" and _normalize_ident(item.text) in names:
                shadowed.add(_normalize_ident(item.text))
            cursor += 1
    for index, token in enumerate(tokens):
        if token.text != "=>":
            continue
        cursor = index - 1
        while cursor >= 0 and tokens[cursor].text not in {"{", ",", ";", "=>"}:
            item = tokens[cursor]
            if item.kind == "ident" and _normalize_ident(item.text) in names:
                shadowed.add(_normalize_ident(item.text))
            cursor -= 1

    # Closure parameters are delimited by a single `|` on each side. Ignore
    # `||` and only inspect names that occur in a parameter list.
    cursor = 0
    while cursor < len(tokens):
        if tokens[cursor].text != "|" or (cursor + 1 < len(tokens) and tokens[cursor + 1].text == "|"):
            cursor += 1
            continue
        end = cursor + 1
        while end < len(tokens) and tokens[end].text != "|":
            end += 1
        if end < len(tokens):
            for position, item in enumerate(tokens[cursor + 1:end], start=cursor + 1):
                if item.kind != "ident" or _normalize_ident(item.text) not in names:
                    continue
                following = tokens[position + 1].text if position + 1 < end else ""
                if following in {"(", "::"}:
                    continue
                shadowed.add(_normalize_ident(item.text))
            cursor = end + 1
        else:
            cursor += 1
    return shadowed


def _nested_function_ranges(tokens: tuple[Token, ...]) -> tuple[tuple[int, int], ...]:
    """Return token ranges for function items nested inside the first item."""
    pairs = _matching(tokens)
    ranges: list[tuple[int, int]] = []
    for index, token in enumerate(tokens):
        if token.text != "fn" or index == 0:
            continue
        cursor = index + 1
        depth = 0
        opening = None
        while cursor < len(tokens):
            value = tokens[cursor].text
            if value in "([<":
                depth += 1
            elif value in ")]>":
                depth -= 1
            elif value == "{" and depth == 0:
                opening = cursor
                break
            elif value == ";" and depth == 0:
                break
            cursor += 1
        if opening is not None and opening in pairs:
            ranges.append((index, pairs[opening] + 1))
    return tuple(ranges)


def _closure_and_async_ranges(tokens: tuple[Token, ...]) -> tuple[tuple[int, int], ...]:
    """Return closure and async-block bodies for lexical proof isolation."""
    pairs = _matching(tokens)
    ranges: list[tuple[int, int]] = []
    for opening, closing in pairs.items():
        if opening >= closing or tokens[opening].text != "{":
            continue
        cursor = opening - 1
        if cursor >= 0 and tokens[cursor].text == "async":
            ranges.append((opening, closing + 1))
            continue
        if cursor >= 0 and tokens[cursor].text == "move":
            cursor -= 1
            if cursor >= 0 and tokens[cursor].text == "async":
                ranges.append((opening, closing + 1))
                continue
        # A closure's return type may sit between its parameter delimiter and
        # body (`|x| -> Result<()> { ... }`). Scan the bounded prefix for a
        # matching `|` pair, while stopping at the enclosing statement scope.
        pipe_count = 0
        while cursor >= 0 and tokens[cursor].text not in {";", "{", "}"}:
            if tokens[cursor].text == "||":
                pipe_count = 2
                break
            if tokens[cursor].text == "|":
                pipe_count += 1
                if pipe_count >= 2:
                    break
            cursor -= 1
        if pipe_count >= 2:
            ranges.append((opening, closing + 1))
    return tuple(ranges)


def _scoped_ranges(tokens: tuple[Token, ...]) -> tuple[tuple[int, int], ...]:
    return _nested_function_ranges(tokens) + _closure_and_async_ranges(tokens)


def _without_ranges(tokens: tuple[Token, ...], ranges: tuple[tuple[int, int], ...]) -> tuple[Token, ...]:
    return tuple(
        token
        for index, token in enumerate(tokens)
        if not any(start <= index < end for start, end in ranges)
    )


def _type_paths(tokens: tuple[Token, ...], start: int) -> tuple[tuple[str, int], ...]:
    """Collect path-shaped candidates through wrappers such as `&mut`/`dyn`."""
    paths: list[tuple[str, int]] = []
    cursor = start
    while cursor < len(tokens) and tokens[cursor].text not in {"=", ";", ",", ")", "{", "}"}:
        path_info = _path_at(tokens, cursor)
        if path_info:
            path, end = path_info
            if path not in {"mut", "const", "dyn", "impl"} and not path.startswith("_"):
                paths.append((path, end))
            cursor = max(cursor + 1, end)
            continue
        cursor += 1
    return tuple(paths)


def _type_aliases(tokens: tuple[Token, ...]) -> dict[str, str]:
    aliases: dict[str, str] = {}
    cursor = 0
    while cursor + 3 < len(tokens):
        if tokens[cursor].text != "type" or tokens[cursor + 1].kind != "ident":
            cursor += 1
            continue
        name = _normalize_ident(tokens[cursor + 1].text)
        equals = cursor + 2
        while equals < len(tokens) and tokens[equals].text != "=":
            equals += 1
        if equals + 1 >= len(tokens):
            break
        paths = _type_paths(tokens, equals + 1)
        if paths:
            aliases[name] = _normalize_path(paths[-1][0])
        cursor = equals + 1
    return aliases


def _returned_receiver_kinds(
    tokens: tuple[Token, ...],
    pairs: dict[int, int],
    aliases: dict[str, str],
    type_aliases: dict[str, str],
) -> dict[str, frozenset[str]]:
    """Map local function names to receiver kinds proven by return types."""
    kinds: dict[str, set[str]] = {}
    for index, token in enumerate(tokens):
        if token.text != "fn" or index + 1 >= len(tokens) or tokens[index + 1].kind != "ident":
            continue
        cursor = index + 2
        depth = 0
        opening = None
        while cursor < len(tokens):
            value = tokens[cursor].text
            if value in "([<":
                depth += 1
            elif value in ")]>":
                depth -= 1
            elif value == "{" and depth == 0:
                opening = cursor
                break
            elif value == ";" and depth == 0:
                break
            cursor += 1
        if opening is None:
            continue
        arrow = next((position for position in range(index + 2, opening) if tokens[position].text == "->"), None)
        if arrow is None:
            continue
        function_kinds: set[str] = set()
        for path, _ in _type_paths(tokens, arrow + 1):
            resolved = _resolve(_resolve(path, type_aliases), aliases)
            terminal = resolved.rsplit("::", 1)[-1]
            if terminal in {"File", "OpenOptions", "DirBuilder", "AsyncWrite", "Write"}:
                function_kinds.add("filesystem")
            if terminal == "RequestBuilder":
                function_kinds.add("request")
        if function_kinds:
            kinds.setdefault(_normalize_ident(tokens[index + 1].text), set()).update(function_kinds)
    return {name: frozenset(value) for name, value in kinds.items()}


def _binding_before_equals(tokens: tuple[Token, ...], equals: int) -> str | None:
    cursor = equals - 1
    while cursor >= 0 and tokens[cursor].text not in {";", "{", "}"}:
        if tokens[cursor].kind == "ident" and cursor and tokens[cursor - 1].text == "let":
            return _normalize_ident(tokens[cursor].text)
        if tokens[cursor].text == ":" and cursor and tokens[cursor - 1].kind == "ident":
            return _normalize_ident(tokens[cursor - 1].text)
        cursor -= 1
    return None


def _is_call_expression(tokens: tuple[Token, ...], start: int, end: int) -> bool:
    """Recognize bounded direct, qualified, method, and turbofish calls."""
    cursor = end
    if cursor < len(tokens) and tokens[cursor].text == "(":
        return True
    if cursor + 1 < len(tokens) and tokens[cursor].text == ".":
        if cursor + 2 < len(tokens) and tokens[cursor + 1].kind == "ident" and tokens[cursor + 2].text == "(":
            return True
        if cursor + 3 < len(tokens) and tokens[cursor + 1].kind == "ident" and tokens[cursor + 2].text == "::" and tokens[cursor + 3].text == "<":
            cursor += 3
        else:
            return False
    else:
        if cursor + 1 >= len(tokens) or tokens[cursor].text != "::" or tokens[cursor + 1].text != "<":
            return False
        cursor += 1
    depth = 0
    while cursor < len(tokens):
        if tokens[cursor].text == "<":
            depth += 1
        elif tokens[cursor].text == ">":
            depth -= 1
            if depth == 0:
                return cursor + 1 < len(tokens) and tokens[cursor + 1].text == "("
        cursor += 1
    return False


def _call_expression_name(tokens: tuple[Token, ...], start: int) -> str | None:
    """Return the called function/method terminal for bounded receiver tracking."""
    path_info = _path_at(tokens, start)
    if path_info:
        raw, end = path_info
        if _is_call_expression(tokens, start, end):
            if end < len(tokens) and tokens[end].text == ".":
                return _normalize_ident(tokens[end + 1].text)
            return _normalize_ident(raw.rsplit("::", 1)[-1])
    if start < len(tokens) and tokens[start].text == "(":
        depth = 1
        cursor = start + 1
        while cursor < len(tokens) and depth:
            if tokens[cursor].text == "(":
                depth += 1
            elif tokens[cursor].text == ")":
                depth -= 1
            cursor += 1
        if depth == 0 and cursor < len(tokens) and tokens[cursor].text == "(":
            inner = _path_at(tokens, start + 1)
            if inner and inner[1] == cursor - 1:
                return _normalize_ident(inner[0].rsplit("::", 1)[-1])
            return _call_expression_name(tokens, start + 1)
    return None


def _destructured_bindings_before_equals(tokens: tuple[Token, ...], equals: int) -> tuple[str, ...]:
    """Collect simple tuple-pattern bindings for receiver propagation."""
    cursor = equals - 1
    while cursor >= 0 and tokens[cursor].text not in {";", "{", "}"}:
        if tokens[cursor].text == "let" and cursor + 1 < equals and tokens[cursor + 1].text == "(":
            return tuple(
                _normalize_ident(item.text)
                for item in tokens[cursor + 2 : equals]
                if item.kind == "ident" and item.text not in {"mut", "ref"}
            )
        cursor -= 1
    return ()


def _calls(
    tokens: tuple[Token, ...],
    aliases: dict[str, str],
    module: str,
    type_aliases: dict[str, str] | None = None,
    returned_receiver_kinds: dict[str, frozenset[str]] | None = None,
) -> tuple[tuple[Call, ...], tuple[Call, ...]]:
    calls: list[Call] = []
    canonical: list[Call] = []
    mutation_receivers: set[str] = set()
    non_filesystem_receivers: set[str] = set()
    indirect_receivers: set[str] = set()
    indirect_values: set[str] = set()
    request_receivers: set[str] = set()
    process_receivers: set[str] = set()
    mutation_aliases: set[str] = set()
    type_aliases = type_aliases or {}
    returned_receiver_kinds = returned_receiver_kinds or {}
    shadowed_aliases = _shadowed_aliases(tokens, aliases)
    canonical_paths = {
        "operation_adoption::submit",
        "operation_adoption::submit_with_key",
        "operation_adoption::prepare",
        "super::operation_adoption::submit",
        "super::operation_adoption::submit_with_key",
        "super::operation_adoption::prepare",
        "crate::api::operation_adoption::submit",
        "crate::api::operation_adoption::submit_with_key",
        "crate::api::operation_adoption::prepare",
        "crate::operations::invocation::submit",
        "crate::operations::invocation::prepare",
    }
    canonical_terminals = {"submit", "submit_with_key", "prepare"}

    # UFCS and angle-bracket dispatch require trait/type resolution.  A
    # mutating method in that form is therefore unknown, never canonicalized
    # from its spelling alone.
    for index, token in enumerate(tokens):
        if token.text != "<":
            continue
        depth = 1
        cursor = index + 1
        while cursor < len(tokens) and depth:
            if tokens[cursor].text == "<":
                depth += 1
            elif tokens[cursor].text == ">":
                depth -= 1
            cursor += 1
        if depth or cursor + 2 >= len(tokens):
            continue
        if tokens[cursor].text != "::" or tokens[cursor + 1].kind != "ident" or tokens[cursor + 2].text != "(":
            continue
        method = _normalize_ident(tokens[cursor + 1].text)
        if method in _MUTATING_NAMES or method in {"request", "send", "post", "put", "delete", "patch"}:
            calls.append(Call("unsupported_call_shape", tokens[cursor + 1].line, tokens[cursor + 1].start))

    # Resolve typed request-builder parameters and assignments before inspecting
    # method calls. A receiver alias is still security-relevant even when the
    # original builder expression is outside the bounded call-shape grammar.
    for index, token in enumerate(tokens):
        if token.text == "RequestBuilder" or (
            token.kind == "ident"
            and _resolve(_resolve(_normalize_ident(token.text), type_aliases), aliases).endswith("::RequestBuilder")
        ):
            cursor = index - 1
            while cursor >= 1 and tokens[cursor].text not in {";", "{", "}"}:
                if tokens[cursor].text == ":" and cursor and tokens[cursor - 1].kind == "ident":
                    request_receivers.add(_normalize_ident(tokens[cursor - 1].text))
                    break
                cursor -= 1
    for index, token in enumerate(tokens):
        if token.text != ":" or index == 0 or tokens[index - 1].kind != "ident":
            continue
        type_paths = _type_paths(tokens, index + 1)
        for type_path, _ in type_paths:
            resolved_type = _resolve(_resolve(type_path, type_aliases), aliases)
            type_terminal = resolved_type.rsplit("::", 1)[-1]
            if type_terminal in {"File", "OpenOptions", "DirBuilder", "AsyncWrite", "Write"}:
                mutation_receivers.add(_normalize_ident(tokens[index - 1].text))
                break
            if resolved_type in {"std::process::Command", "tokio::process::Command"}:
                process_receivers.add(_normalize_ident(tokens[index - 1].text))
                break
            if resolved_type in {"reqwest::Client", "reqwest::ClientBuilder"}:
                request_receivers.add(_normalize_ident(tokens[index - 1].text))
                break
            if type_terminal in {"String", "Vec", "HashMap", "HashSet", "BTreeMap", "BTreeSet"}:
                non_filesystem_receivers.add(_normalize_ident(tokens[index - 1].text))
                break
    for index, token in enumerate(tokens):
        if token.text != "=" or index + 1 >= len(tokens):
            continue
        path_info = _path_at(tokens, index + 1)
        if not path_info:
            continue
        raw, end = path_info
        resolved = _resolve(raw, aliases)
        if end < len(tokens) and tokens[end].text == "(" and resolved.endswith((
            "std::fs::OpenOptions::new", "tokio::fs::OpenOptions::new",
            "std::fs::File::options", "tokio::fs::File::options",
            "std::fs::DirBuilder::new", "tokio::fs::DirBuilder::new",
        )):
            binding = _binding_before_equals(tokens, index)
            if binding:
                mutation_receivers.add(binding)
    for index, token in enumerate(tokens):
        if token.text != "=" or index == 0:
            continue
        binding = _binding_before_equals(tokens, index)
        if binding is None and tokens[index - 1].kind == "ident":
            binding = _normalize_ident(tokens[index - 1].text)
        if index + 1 >= len(tokens):
            continue
        bindings = (binding,) if binding is not None else _destructured_bindings_before_equals(tokens, index)
        call_name = _call_expression_name(tokens, index + 1)
        if call_name is None and bindings:
            cursor = index + 1
            while cursor < len(tokens) and tokens[cursor].text not in {";", "}"}:
                call_name = _call_expression_name(tokens, cursor)
                if call_name is not None:
                    break
                cursor += 1
        if call_name:
            receiver_kinds = returned_receiver_kinds.get(call_name, ())
            for candidate in bindings:
                if "filesystem" in receiver_kinds:
                    mutation_receivers.add(candidate)
                if "request" in receiver_kinds:
                    request_receivers.add(candidate)
        if binding is None:
            continue
        if tokens[index + 1].text == "&":
            cursor = index + 2
            while cursor < len(tokens) and tokens[cursor].text not in {";", ","}:
                if tokens[cursor].kind == "ident" and _normalize_ident(tokens[cursor].text) in mutation_receivers:
                    mutation_receivers.add(binding)
                    break
                cursor += 1
        path_info = _path_at(tokens, index + 1)
        if path_info:
            raw, end = path_info
            resolved = _resolve(raw, aliases)
            if _is_mutating_path(resolved) and (end >= len(tokens) or tokens[end].text != "("):
                calls.append(Call("indirect_function_value", tokens[index + 1].line, tokens[index + 1].start))
                indirect_receivers.add(binding)
                indirect_values.add(binding)
            if binding in request_receivers:
                request_receivers.add(binding)
        cursor = index + 1
        has_request_method_reference = False
        while cursor < len(tokens) and tokens[cursor].text not in {";", "}"}:
            if (
                tokens[cursor].text == "."
                and cursor + 1 < len(tokens)
                and _normalize_ident(tokens[cursor + 1].text) in {"post", "put", "delete", "patch", "request"}
                and (cursor + 2 >= len(tokens) or tokens[cursor + 2].text != "(")
            ):
                has_request_method_reference = True
                break
            cursor += 1
        if has_request_method_reference:
            calls.append(Call("indirect_function_value", tokens[index + 1].line, tokens[index + 1].start))
            indirect_receivers.add(binding)
            indirect_values.add(binding)
        if index + 1 < len(tokens) and tokens[index + 1].kind == "ident":
            rhs_binding = _normalize_ident(tokens[index + 1].text)
            if rhs_binding in request_receivers:
                request_receivers.add(binding)
            if rhs_binding in mutation_receivers:
                mutation_receivers.add(binding)
    for index, token in enumerate(tokens):
        if index and tokens[index - 1].text == "::":
            continue
        path_info = _path_at(tokens, index)
        if not path_info:
            continue
        raw, end = path_info
        if end < len(tokens) and tokens[end].text == ";" and index >= 2 and tokens[index - 1].text == "=" and tokens[index - 2].kind == "ident" and raw.rsplit("::", 1)[-1] in _MUTATING_NAMES:
            indirect_receivers.add(_normalize_ident(tokens[index - 2].text))
    for index, token in enumerate(tokens):
        if token.text == "(" and index and tokens[index - 1].kind == "ident" and _normalize_ident(tokens[index - 1].text) in indirect_receivers:
            if _normalize_ident(tokens[index - 1].text) not in indirect_values:
                calls.append(Call("indirect_function_value", token.line, tokens[index - 1].start))
            continue
        if (
            token.text == "."
            and index
            and tokens[index - 1].kind == "ident"
            and _normalize_ident(tokens[index - 1].text) in request_receivers
            and index + 2 < len(tokens)
            and _normalize_ident(tokens[index + 1].text) == "send"
            and tokens[index + 2].text == "("
        ):
            calls.append(Call("provider_http_mutation", tokens[index + 1].line, tokens[index + 1].start))
        if index and tokens[index - 1].text == ".":
            continue
        if index and tokens[index - 1].text == "::":
            continue
        path_info = _path_at(tokens, index)
        if path_info:
            raw, end = path_info
            resolved_path = _resolve(raw, aliases)
            terminal = raw.rsplit("::", 1)[-1]
            known_prefix = raw.startswith(("std::fs::", "tokio::fs::", "std::process::", "tokio::process::", "containers::", "firewall_provider::", "proxy_provider::", "restic::"))
            basic_name = terminal in {"write", "create", "create_dir", "create_dir_all", "create_new", "remove_file", "remove_dir", "remove_dir_all", "rename", "copy", "set_permissions", "hard_link", "soft_link"} and ("::" not in raw or raw.startswith(("std::fs::", "tokio::fs::")))
            is_assignment_value = index and tokens[index - 1].text == "="
            if (((basic_name or (known_prefix and terminal in _MUTATING_NAMES)) and (end >= len(tokens) or tokens[end].text != "(")) or ("process::Command" in resolved_path and terminal == "new" and (end >= len(tokens) or tokens[end].text != "(")) or (resolved_path.endswith(("std::fs::OpenOptions::new", "std::fs::File::options", "std::fs::DirBuilder::new")) and (end >= len(tokens) or tokens[end].text != "("))) and not is_assignment_value:
                calls.append(Call("unsupported_call_shape", token.line, token.start))
                continue
            if terminal in _MUTATING_NAMES and end < len(tokens) and (tokens[end].text == "<" or (tokens[end].text == "::" and end + 1 < len(tokens) and tokens[end + 1].text == "<")):
                calls.append(Call("unsupported_call_shape", token.line, token.start))
                continue
            if terminal in _MUTATING_NAMES and end + 1 < len(tokens) and tokens[end].text == ")" and tokens[end + 1].text == "(":
                calls.append(Call("unsupported_call_shape", token.line, token.start))
                continue
            if end < len(tokens) and tokens[end].text == "(":
                path = _resolve(raw, aliases)
                terminal = path.rsplit("::", 1)[-1]
                first = raw.removeprefix("::").split("::", 1)[0]
                if first in shadowed_aliases and first in aliases and (
                    path in canonical_paths or terminal in _MUTATING_NAMES
                ):
                    calls.append(Call("unsupported_call_shape", token.line, token.start))
                    continue
                if terminal in canonical_terminals and path not in canonical_paths and ("::" in raw or path != raw):
                    calls.append(Call("unsupported_call_shape", token.line, token.start))
                    continue
                if _untrusted_bare_alias(raw, path) or _unresolved_mutation_alias(raw, path, terminal):
                    calls.append(Call("unsupported_call_shape", token.line, token.start))
                    continue
                if terminal in _MUTATING_NAMES and "::" in path and path not in _SAFE_MUTATION_EXACT and not path.startswith(_SAFE_MUTATION_PROVENANCE):
                    calls.append(Call("unsupported_call_shape", token.line, token.start))
                    continue
                if path.startswith(("std::io::", "tokio::io::")) and terminal in _MUTATING_NAMES:
                    calls.append(Call("unsupported_call_shape", token.line, token.start))
                    continue
                if path.startswith(("std::fs::File::", "tokio::fs::File::", "std::fs::OpenOptions::", "tokio::fs::OpenOptions::")) and terminal in {"write_all", "write_fmt", "flush", "shutdown", "set_len", "set_times", "sync_all", "sync_data", "truncate"}:
                    calls.append(Call("unsupported_call_shape", token.line, token.start))
                    continue
                if (
                    index >= 2
                    and tokens[index - 1].text == "="
                    and path.endswith((
                        "std::fs::DirBuilder::new", "tokio::fs::DirBuilder::new",
                        "std::fs::File::open", "tokio::fs::File::open",
                        "std::fs::File::create", "tokio::fs::File::create",
                        "std::fs::File::options", "tokio::fs::File::options",
                        "std::fs::OpenOptions::new", "tokio::fs::OpenOptions::new",
                    ))
                ):
                    binding = None
                    cursor = index - 2
                    while cursor >= 0 and tokens[cursor].text not in {";", "{", "}"}:
                        if tokens[cursor].text == ":" and cursor and tokens[cursor - 1].kind == "ident":
                            binding = tokens[cursor - 1].text
                            break
                        cursor -= 1
                    if binding is None and tokens[index - 2].kind == "ident":
                        binding = tokens[index - 2].text
                    if binding:
                        mutation_receivers.add(binding)
                if path.endswith(("std::process::Command::new", "tokio::process::Command::new")):
                    binding = _binding_before_equals(tokens, index)
                    if binding:
                        process_receivers.add(binding)
                if path.endswith(("std::process::Command", "tokio::process::Command", "std::process::Command::new", "tokio::process::Command::new")) or raw.endswith("Command::new"):
                    calls.append(Call("process_execution", token.line, token.start))
                filesystem_functions = tuple(
                    f"{prefix}::{name}"
                    for prefix in ("std::fs", "tokio::fs")
                    for name in (
                        "write", "create_dir", "create_dir_all", "remove_file", "remove_dir",
                        "remove_dir_all", "rename", "copy", "set_permissions", "hard_link", "soft_link",
                    )
                )
                if path.endswith(filesystem_functions) or path.endswith(("std::fs::File::create", "tokio::fs::File::create")) or (
                    raw.startswith(("fs::", "io::", "file_ops::"))
                    and raw.rsplit("::", 1)[-1] in {
                        "write", "create_dir", "create_dir_all", "remove_file", "remove_dir",
                        "remove_dir_all", "rename", "copy", "set_permissions", "hard_link", "soft_link",
                    }
                ):
                    calls.append(Call("filesystem_mutation", token.line, token.start))
                if path.endswith((
                    "std::fs::File::set_permissions", "tokio::fs::File::set_permissions",
                    "std::fs::DirBuilder::create", "tokio::fs::DirBuilder::create",
                    "std::fs::OpenOptions::open", "tokio::fs::OpenOptions::open",
                )):
                    calls.append(Call("filesystem_mutation", token.line, token.start))
                if path.endswith(("::OpenOptions::new", "::File::options", "::DirBuilder::new")):
                    option_methods = False
                    for pos in range(end, len(tokens)):
                        if tokens[pos].text in {";", "}"}:
                            break
                        if (
                            tokens[pos].text == "."
                            and pos + 2 < len(tokens)
                            and tokens[pos + 1].text in {"write", "create", "create_new", "truncate", "append", "create_dir"}
                            and tokens[pos + 2].text == "("
                        ):
                            option_methods = True
                            break
                    if option_methods:
                        calls.append(Call("filesystem_mutation", token.line, token.start))
                if path in {"std::process::Command", "tokio::process::Command"}:
                    calls.append(Call("process_execution", token.line, token.start))
                if path.startswith("reqwest::") and terminal in {"request", "post", "put", "delete", "patch", "send"}:
                    calls.append(Call("provider_http_mutation", token.line, token.start))
                if path.endswith(("containers::deploy_compose", "containers::restart_compose", "containers::remove_compose", "containers::stop_compose", "containers::pull_compose", "containers::container_action", "firewall_provider::apply", "firewall_provider::add", "firewall_provider::delete", "firewall_provider::remove", "firewall_provider::set", "proxy_provider::write", "proxy_provider::write_conf", "proxy_provider::write_htpasswd", "proxy_provider::reload", "proxy_provider::apply", "proxy_provider::delete", "restic::run", "restic::init", "restic::restore")) or raw.endswith(("containers::deploy_compose", "containers::restart_compose", "containers::remove_compose", "containers::stop_compose", "containers::pull_compose", "containers::container_action", "firewall_provider::apply", "firewall_provider::add", "firewall_provider::delete", "firewall_provider::remove", "firewall_provider::set", "proxy_provider::write", "proxy_provider::write_conf", "proxy_provider::write_htpasswd", "proxy_provider::reload", "proxy_provider::apply", "proxy_provider::delete", "restic::run", "restic::init", "restic::restore")):
                    calls.append(Call("provider_direct_call", token.line, token.start))
                if path in canonical_paths:
                    canonical.append(Call("canonical", token.line, token.start))
        if token.text == "." and index + 2 < len(tokens) and tokens[index + 1].kind == "ident" and tokens[index + 2].text == "(":
            method = _normalize_ident(tokens[index + 1].text)
            statement_start = max((position for position in range(index) if tokens[position].text == ";"), default=-1)
            builder_expression = any(
                tokens[position].text in {"File", "OpenOptions", "DirBuilder"}
                for position in range(statement_start + 1, index)
            )
            reference_expression = tokens[index - 1].text == ")" and any(
                tokens[position].text == "&" for position in range(statement_start + 1, index)
            )
            filesystem_methods = {
                "write", "write_all", "write_fmt", "flush", "set_len", "set_times",
                "write_vectored", "write_all_vectored", "sync_all", "sync_data", "truncate", "set_permissions", "create", "create_new",
                "create_dir", "create_dir_all", "append",
            }
            receiver = (
                _normalize_ident(tokens[index - 1].text)
                if index and tokens[index - 1].kind == "ident"
                else None
            )
            if method in {"spawn", "status", "output", "wait", "wait_with_output", "exec"} and receiver in process_receivers:
                calls.append(Call("process_execution", tokens[index + 1].line, tokens[index + 1].start))
            if method == "execute" and receiver in request_receivers:
                calls.append(Call("provider_http_mutation", tokens[index + 1].line, tokens[index + 1].start))
            if method in filesystem_methods and receiver is None and not builder_expression and not reference_expression:
                calls.append(Call("unresolved_receiver_provenance", tokens[index + 1].line, tokens[index + 1].start))
                continue
            assignment = max(
                (position for position in range(index) if tokens[position].text == "="),
                default=-1,
            )
            rhs = tokens[assignment + 1 : index] if assignment >= 0 else ()
            complex_rhs = any(item.text in {"if", "match", "=>", "|", "{"} for item in rhs)
            if (
                method in filesystem_methods
                and receiver
                and receiver not in mutation_receivers
                and receiver not in non_filesystem_receivers
                and not (index >= 2 and tokens[index - 2].text == ".")
                and (not builder_expression or complex_rhs)
            ):
                # A filesystem-looking method on an unproven receiver must not
                # disappear merely because its value came through a block,
                # branch, closure, or unsupported helper expression.  The
                # inventory is source-boundary enforcement, so uncertainty is
                # an explicit finding rather than an absent call.
                calls.append(Call("unresolved_receiver_provenance", tokens[index + 1].line, tokens[index + 1].start))
                continue
            if (
                (
                    (receiver in mutation_receivers)
                    or reference_expression
                    or (builder_expression and method in filesystem_methods)
                )
                and method in filesystem_methods
            ):
                calls.append(Call("filesystem_mutation", tokens[index + 1].line, tokens[index + 1].start))
            if method == "send" and (
                any(item.text == "RequestBuilder" for item in tokens)
                or any(type_alias.endswith("::RequestBuilder") for type_alias in type_aliases.values())
                or (index and tokens[index - 1].kind == "ident" and _normalize_ident(tokens[index - 1].text) in request_receivers)
                or (
                    index and tokens[index - 1].text == ")"
                    and any("request" in kinds for kinds in returned_receiver_kinds.values())
                )
            ):
                calls.append(Call("provider_http_mutation", tokens[index + 1].line, tokens[index + 1].start))
            if method in {"post", "put", "delete", "patch"}:
                statement_start = max((position for position in range(index) if tokens[position].text == ";"), default=-1)
                marker = "http_route_registration" if any(
                    tokens[position].text == "." and position + 2 < index and tokens[position + 1].text == "route" and tokens[position + 2].text == "("
                    for position in range(statement_start + 1, index)
                ) else "provider_http_mutation"
                calls.append(Call(marker, tokens[index + 1].line, tokens[index + 1].start))
        if token.text == "." and index + 2 < len(tokens) and tokens[index + 1].text == "request" and tokens[index + 2].text == "(":
            method_tokens = []
            cursor = index + 3
            while cursor < len(tokens) and tokens[cursor].text not in {",", ")"}:
                method_tokens.append(tokens[cursor].text)
                cursor += 1
            if tuple(method_tokens) not in {
                ("Method", "::", "GET"),
                ("reqwest", "::", "Method", "::", "GET"),
            }:
                calls.append(Call("provider_http_mutation", tokens[index + 1].line, tokens[index + 1].start))
        if (
            token.text == "."
            and index
            and tokens[index - 1].kind == "ident"
            and _normalize_ident(tokens[index - 1].text) in mutation_receivers
            and index + 2 < len(tokens)
            and tokens[index + 2].text == "("
            and _normalize_ident(tokens[index + 1].text) in {"create", "write", "create_new", "truncate", "append", "set_permissions", "create_dir", "mode"}
        ):
            calls.append(Call("filesystem_mutation", tokens[index + 1].line, tokens[index + 1].start))
        if token.kind == "ident" and token.text in {"write", "create", "create_new", "truncate", "remove_file", "remove_dir", "remove_dir_all", "rename"} and index + 1 < len(tokens) and tokens[index + 1].text == "(":
            previous = tokens[index - 1].text if index else ""
            if previous == "." and any(item.text == "OpenOptions" for item in tokens[max(0, index - 8):index]):
                calls.append(Call("filesystem_mutation", token.line, token.start))
        if (
            token.text == "!"
            and index
            and tokens[index - 1].kind == "ident"
            and tokens[index - 1].text not in {"if", "while", "match", "return", "else", "for", "loop"}
        ):
            macro = tokens[index - 1].text
            root = macro
            cursor = index - 2
            while cursor >= 1 and tokens[cursor].text == "::" and tokens[cursor - 1].kind == "ident":
                root = tokens[cursor - 1].text
                cursor -= 2
            resolved = aliases.get(macro)
            if resolved and "::" in resolved:
                root = resolved.split("::", 1)[0]
                cursor = index - 3
            safe_macros = {
                "anyhow", "assert", "assert_eq", "assert_ne", "bail", "ensure", "env", "error", "format",
                "include_str", "info", "json", "matches", "option_env", "panic", "println", "select",
                "unreachable", "vec", "warn",
            }
            if macro not in safe_macros or (cursor != index - 2 and root not in {"anyhow", "serde_json", "tokio", "tracing"}):
                calls.append(Call("unsupported_macro", tokens[index - 1].line, tokens[index - 1].start))
    calls = list(dict.fromkeys((call.marker, call.line, call.offset) for call in calls))
    calls = [Call(marker, line, offset) for marker, line, offset in calls]
    return tuple(calls), tuple(canonical)


def _local_call_names(tokens: tuple[Token, ...], candidates: set[str] | None = None) -> set[str]:
    """Return direct local-call spellings for bounded same-module resolution."""
    tokens = _without_ranges(tokens, _scoped_ranges(tokens))
    names: set[str] = set()
    shadowed: set[str] = set()
    for index, token in enumerate(tokens):
        if token.kind != "ident":
            continue
        previous = tokens[index - 1].text if index else ""
        following = tokens[index + 1].text if index + 1 < len(tokens) else ""
        before_previous = tokens[index - 2].text if index >= 2 else ""
        if previous == "let" or (previous == "mut" and before_previous == "let") or (previous == "|" and following in {"|", ",", ":"}) or following == ":":
            shadowed.add(_normalize_ident(token.text))
    for index, token in enumerate(tokens[:-1]):
        if token.kind != "ident" or tokens[index + 1].text != "(":
            continue
        if index and tokens[index - 1].text in {".", "::"}:
            continue
        if token.text not in shadowed and token.text not in {"if", "match", "while", "for", "loop", "return", "Err", "Ok"}:
            names.add(_normalize_ident(token.text))
    if candidates:
        shadowed.update(_shadowed_aliases(tokens, {name: name for name in candidates}))
    return names - shadowed


def parse_source(path: Path, text: str, module_base: str) -> tuple[ParsedFunction, ...]:
    _rustfmt_parse(path, text)
    tokens = lex(text)
    pairs = _matching(tokens)
    excluded, inner_cfg = _cfg_test_attributes(tokens, pairs)
    if inner_cfg:
        return ()
    _reject_unsafe_reexports(tokens, excluded)
    scopes = _scopes(tokens, pairs)
    has_local_canonical_module = any(
        token.text == "mod"
        and index + 1 < len(tokens)
        and _normalize_ident(tokens[index + 1].text) == "operation_adoption"
        for index, token in enumerate(tokens)
    )
    has_external_canonical_alias = any(
        token.text == "extern"
        and index + 2 < len(tokens)
        and tokens[index + 1].text == "crate"
        and any(item.text == "operation_adoption" for item in tokens[index + 2:index + 8])
        for index, token in enumerate(tokens)
    )
    has_local_canonical_module = has_local_canonical_module or has_external_canonical_alias
    aliases = _imports(tuple(token for token in tokens if not any(left <= token.start < right for left, right in excluded)))
    type_aliases = _type_aliases(tokens)
    returned_receiver_kinds = _returned_receiver_kinds(tokens, pairs, aliases, type_aliases)
    functions: list[ParsedFunction] = []
    for index, token in enumerate(tokens):
        if token.text != "fn" or index + 1 >= len(tokens) or tokens[index + 1].kind != "ident":
            continue
        name = tokens[index + 1].text
        cursor = index + 2
        depth = 0
        opening = None
        while cursor < len(tokens):
            value = tokens[cursor].text
            if value in "([<":
                depth += 1
            elif value in ")]>":
                depth -= 1
            elif value == "{" and depth == 0:
                opening = cursor
                break
            elif value == ";" and depth == 0:
                break
            cursor += 1
        if opening is None:
            continue
        end_index = pairs.get(opening)
        if end_index is None:
            raise ParseError(f"function {name} has no matching body")
        start = token.start
        end = tokens[end_index].end
        if any(start >= left and end <= right for left, right in excluded):
            continue
        containing = [scope for scope in scopes if scope.start < start and end <= scope.end]
        modules = [scope.name for scope in sorted(containing, key=lambda scope: scope.start) if scope.kind == "mod"]
        impls = [scope.name for scope in sorted(containing, key=lambda scope: scope.start) if scope.kind == "impl"]
        module = "::".join([part for part in [module_base, *modules, *impls] if part])
        body_tokens = tokens[index:end_index + 1]
        nested_ranges = _nested_function_ranges(body_tokens)
        scoped_ranges = _scoped_ranges(body_tokens)
        analysis_tokens = _without_ranges(body_tokens, nested_ranges)
        calls, canonical = _calls(analysis_tokens, aliases, module, type_aliases, returned_receiver_kinds)
        scoped_offsets = tuple(
            (body_tokens[start].start, body_tokens[end - 1].end)
            for start, end in scoped_ranges
            if start < end
        )
        canonical = tuple(
            call
            for call in canonical
            if not any(start <= call.offset < end for start, end in scoped_offsets)
        )
        has_nested_canonical_module = any(
            body_tokens[index].text == "mod"
            and index + 1 < len(body_tokens)
            and _normalize_ident(body_tokens[index + 1].text) == "operation_adoption"
            for index in range(len(body_tokens))
        )
        if has_local_canonical_module or has_nested_canonical_module:
            canonical = ()
        nested_item_has_feature = any(
            any(token.text == "FeatureUnavailable" for token in body_tokens[start:end])
            for start, end in nested_ranges
        )
        if nested_item_has_feature:
            calls = (*calls, Call("unsupported_call_shape", token.line, token.start))
        calls = tuple(Call(call.marker, call.line, call.offset - start) for call in calls)
        canonical = tuple(Call(call.marker, call.line, call.offset - start) for call in canonical)
        functions.append(ParsedFunction(module, name, token.line, tokens[end_index].line, start, end, text[start:end], body_tokens, calls, canonical, tuple(sorted(aliases.items()))))
    # Resolve only exact same-module helper calls whose own body reaches a
    # canonical adapter.  A helper merely named `prepare_or_submit` is not a
    # delegation; its definition must be proven by this bounded call graph.
    # Nested Rust functions have lexical scope and cannot serve as same-module
    # helpers for their enclosing item.  Keep parsing them independently, but
    # do not let their canonical calls authorize the parent through the helper
    # graph.
    nested_function_starts = {
        function.tokens[start].start
        for function in functions
        for start, _ in _scoped_ranges(function.tokens)
    }
    trusted_helpers = {
        (function.module, function.name)
        for function in functions
        if function.canonical_calls and function.start not in nested_function_starts
    }
    changed = True
    while changed:
        changed = False
        for function in functions:
            identity = (function.module, function.name)
            if identity in trusted_helpers:
                continue
            same_module_names = {item.name for item in functions if item.module == function.module}
            if any((function.module, name) in trusted_helpers for name in _local_call_names(function.tokens, same_module_names)):
                trusted_helpers.add(identity)
                changed = True
    for index, function in enumerate(functions):
        if (function.module, function.name) not in trusted_helpers or function.canonical_calls:
            continue
        call = Call("canonical", function.start_line, 0)
        functions[index] = replace(function, canonical_calls=(call,))
    function_ranges = tuple((function.start, function.end) for function in functions)
    for index, token in enumerate(tokens):
        if (
            token.text not in {"const", "static"}
            or (token.text == "static" and index and tokens[index - 1].text == "'")
            or any(start <= token.start < end for start, end in function_ranges)
        ):
            continue
        cursor = index + 1
        depth = 0
        while cursor < len(tokens):
            value = tokens[cursor].text
            if value in "([{":
                depth += 1
            elif value in ")]}":
                depth -= 1
            elif value == ";" and depth == 0:
                break
            cursor += 1
        if cursor >= len(tokens):
            raise ParseError("item-level initializer has no terminator")
        for item_index in range(index + 1, cursor):
            if any(left <= tokens[item_index].start < right for left, right in excluded):
                continue
            path_info = _path_at(tokens, item_index)
            if not path_info:
                continue
            raw, end = path_info
            resolved = _resolve(raw, aliases)
            if _is_mutating_path(resolved):
                raise ParseError("unsupported executable item-level mutation")
    for index, token in enumerate(tokens):
        if token.text != "!" or index == 0 or tokens[index - 1].text == "#":
            continue
        if any(start <= token.start < end for start, end in function_ranges):
            continue
        if any(left <= token.start < right for left, right in excluded):
            continue
        raise ParseError(f"unsupported executable item-level mutation: unsupported item-level macro near line {token.line}")
    return tuple(functions)


def normalized_token_digest(function: ParsedFunction) -> str:
    import hashlib

    material = "\x1f".join(token.text for token in function.tokens).encode("utf-8")
    return hashlib.sha256(material).hexdigest()
