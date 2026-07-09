#!/usr/bin/env bash
# scripts/devteam/adr.sh — ADR lifecycle. Agents DRAFT (Status: Proposed);
# only the operator ACCEPTS. Acceptance is a signature, not a document edit,
# and it is the one step in this harness that is never automated.
#
#   adr.sh list                    # all ADRs + status
#   adr.sh show <id>               # print one
#   adr.sh accept <id>             # operator signs a proposed ADR
#   adr.sh revoke <id>             # close a grant (e.g. at phase exit)
#   adr.sh check <id> <files...>   # are these paths granted by an ACCEPTED adr?
#
# ADRs carry a machine-readable grant block that `gates.sh` and `check` parse:
#
#   ```granted-paths
#   backend/src/policy.rs
#   backend/src/voidwatch/**
#   backend/src/api/mcp.rs
#   ```
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
ADR_DIR="$ROOT/docs/adr"
mkdir -p "$ADR_DIR"

adr_file() { ls "$ADR_DIR"/${1}*.md 2>/dev/null | head -1; }
adr_status() { grep -m1 -oE '^\*\*Status:\*\*[[:space:]]*[A-Za-z]+' "$1" | awk '{print $NF}'; }

granted_paths() {  # $1 = adr file → newline-separated glob patterns
  awk '/^```granted-paths/{f=1;next} /^```/{f=0} f' "$1" | sed '/^[[:space:]]*$/d'
}

case "${1:-}" in
  list)
    printf '%-12s %-10s %s\n' ID STATUS TITLE
    for f in "$ADR_DIR"/ADR-*.md; do
      [[ -e "$f" ]] || continue
      id="$(basename "$f" | grep -oE '^ADR-[0-9]+')"
      printf '%-12s %-10s %s\n' "$id" "$(adr_status "$f")" "$(head -1 "$f" | sed 's/^# *//')"
    done ;;

  show) f="$(adr_file "${2:?id}")"; cat "$f" ;;

  accept)
    id="${2:?usage: adr.sh accept ADR-002}"
    f="$(adr_file "$id")" || { echo "no such ADR: $id"; exit 1; }
    st="$(adr_status "$f")"
    [[ "$st" == "Proposed" ]] || { echo "ADR $id is '$st', not Proposed — nothing to sign."; exit 1; }

    echo "─── Granted paths ───"; granted_paths "$f" | sed 's/^/  /'
    echo "─── Constraints ───"; sed -n '/^## Constraints/,/^## /p' "$f" | sed '1d;$d' | sed 's/^/  /'
    echo
    read -rp "Sign this grant as ACCEPTED? Type the ADR id to confirm: " confirm
    [[ "$confirm" == "$id" ]] || { echo "aborted."; exit 1; }

    sed -i "s|^\*\*Status:\*\*.*|**Status:** Accepted (signed by operator $(git config user.name) on $(date -I))|" "$f"
    echo "✔ $id accepted. Commit and push it before running the loop:"
    echo "  git add $f && git commit -m 'docs(adr): accept $id' && git push origin main" ;;

  revoke)
    id="${2:?id}"; f="$(adr_file "$id")"
    sed -i "s|^\*\*Status:\*\*.*|**Status:** Revoked ($(date -I)) — grant closed|" "$f"
    echo "✔ $id revoked; its zones are closed again." ;;

  check)  # used by gates.sh: adr.sh check ADR-001[,ADR-002,...] file1 file2 ...
    # A spec may legitimately cite more than one ADR (e.g. one grant for
    # policy.rs, a separate one for a db/mod.rs schema addition) — validate
    # each file against the UNION of every cited ADR's granted paths, not
    # just the first one. Every cited ADR must independently be Accepted;
    # one Proposed grant in the list still blocks the whole check, since a
    # spec citing an unsigned ADR alongside a signed one is not evidence the
    # unsigned one was meant to be skippable.
    ids="${2:?id}"; shift 2
    ALL_PATTERNS=()
    IFS=',' read -ra ID_LIST <<<"$ids"
    for id in "${ID_LIST[@]}"; do
      f="$(adr_file "$id")" || { echo "gates: cited $id does not exist"; exit 1; }
      st="$(adr_status "$f")"
      [[ "$st" == "Accepted" ]] || { echo "gates: $id has status '$st' — only Accepted grants authorize forbidden-zone changes"; exit 1; }
      mapfile -t PATTERNS < <(granted_paths "$f")
      ALL_PATTERNS+=("${PATTERNS[@]}")
    done
    rc=0
    for file in "$@"; do
      ok=1
      for p in "${ALL_PATTERNS[@]}"; do
        # shellcheck disable=SC2053
        [[ "$file" == $p ]] && { ok=0; break; }
      done
      [[ $ok -eq 0 ]] || { echo "gates: '$file' is NOT in any of ${ids}'s granted paths"; rc=1; }
    done
    exit $rc ;;

  *) echo "usage: adr.sh {list|show <id>|accept <id>|revoke <id>|check <id> <files...>}"; exit 1 ;;
esac
