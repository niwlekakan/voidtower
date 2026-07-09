#!/usr/bin/env bash
# scripts/devteam/gates.sh — merge gates for VoidTower dev-team work.
# Invoked by workers before committing and by the runner after each task.
# FORBIDDEN ZONE: agents must never edit this file (CLAUDE.md hard rules).
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"

CHANGED="$(git -C "$ROOT" diff --name-only origin/main...HEAD)"

echo "[gates] G0 format (diff-scoped)"
# NOTE: `cargo fmt -- path.rs` walks the whole `mod` graph from the crate root and
# reformats everything. Invoke rustfmt directly on the changed files instead, so a
# PR is never held hostage by pre-existing formatting debt elsewhere in the repo
# (and never needs to touch forbidden-zone files to satisfy a whitespace gate).
#
# rustfmt is inherently whole-file, not diff-scoped: `--check` reports every
# non-conforming line in a touched file, including pre-existing debt this PR
# never touched, so any task touching an already-messy file fails G0 forever
# regardless of whether its own new lines are clean. Workers also have no
# standing permission to invoke `rustfmt` as a separate top-level command —
# only this script, already allowlisted, can. So: apply the formatting (not
# just check it) here, then fail with a clear diff if that changed anything,
# forcing a deliberate review-and-commit rather than a silent pass or a
# permanent dead end.
CHANGED_RS="$(echo "$CHANGED" | grep '\.rs$' | grep -v '^backend/target/' || true)"
if [[ -n "$CHANGED_RS" ]]; then
  MISSING=""
  for f in $CHANGED_RS; do [[ -f "$ROOT/$f" ]] && MISSING="$MISSING $ROOT/$f"; done
  if [[ -n "$MISSING" ]]; then
    # shellcheck disable=SC2086
    rustfmt --edition 2021 $MISSING
    # shellcheck disable=SC2086
    if ! git -C "$ROOT" diff --quiet -- $CHANGED_RS; then
      echo "[gates] FAIL: rustfmt reformatted your changed file(s) — this also cleans up any"
      echo "  pre-existing debt in them, since rustfmt is whole-file, not diff-scoped. Review"
      echo "  the diff below, commit it, and re-run gates.sh:"
      # shellcheck disable=SC2086
      git -C "$ROOT" diff --stat -- $CHANGED_RS | sed 's/^/    /'
      exit 1
    fi
  fi
fi

echo "[gates] G0 lint"
( cd "$ROOT/backend" && cargo clippy --all-targets --all-features -- -D warnings )

echo "[gates] G0 forbidden-zone diff check"
BLOCKLIST='^backend/src/policy\.rs$|^backend/src/auth/|^backend/src/api/auth\.rs$|^backend/src/oidc\.rs$|^backend/src/db/mod\.rs$|^\.github/workflows/|^scripts/devteam/|^CLAUDE\.md$|^docs/edd\.md$|^docs/gap-analysis\.md$'
if echo "$CHANGED" | grep -Eq "$BLOCKLIST"; then
  # Forbidden-zone changes require an ACCEPTED ADR cited by the active spec,
  # and every changed path must fall inside that ADR's granted-paths block.
  # Citing a Proposed (unsigned) ADR is not authorization.
  # NOTE: a spec may legitimately cite more than one ADR (e.g. one for
  # policy.rs, another for db/mod.rs's schema addition) — pass every cited id
  # to adr.sh check and let it validate each file against the union of all
  # their granted-paths, not just whichever ADR id happened to appear first.
  ADR_IDS="$(grep -rhoE 'ADR-[0-9]{3}' "$ROOT/.devteam/active/" 2>/dev/null | sort -u | paste -sd, - || true)"
  if [[ -z "$ADR_IDS" ]]; then
    echo "[gates] FAIL: forbidden-zone files changed but the active task spec cites no ADR:"
    echo "$CHANGED" | grep -E "$BLOCKLIST" | sed 's/^/  /'
    exit 1
  fi
  FZ_FILES="$(echo "$CHANGED" | grep -E "$BLOCKLIST")"
  # shellcheck disable=SC2086
  "$ROOT/scripts/devteam/adr.sh" check "$ADR_IDS" $FZ_FILES \
    || { echo "[gates] FAIL: forbidden-zone changes not authorized by $ADR_IDS"; exit 1; }
  echo "[gates]   forbidden-zone changes authorized by $ADR_IDS"
fi

echo "[gates] G1 tests"
( cd "$ROOT/backend" && cargo test --all-features )

echo "[gates] G1 frontend (if touched)"
if echo "$CHANGED" | grep -q '^frontend/'; then
  ( cd "$ROOT/frontend" && npm run lint && npm run build )
fi

echo "[gates] G1 ADR integrity"
# ADRs are authorization records. A diff may ADD them; it may never delete or downgrade one.
DELETED_ADR="$(git -C "$ROOT" diff --diff-filter=D --name-only origin/main...HEAD -- docs/adr/ || true)"
[[ -z "$DELETED_ADR" ]] || { echo "[gates] FAIL: diff deletes ADR file(s):"; echo "$DELETED_ADR" | sed 's/^/  /'; exit 1; }
for f in $(git -C "$ROOT" diff --name-only origin/main...HEAD -- docs/adr/ || true); do
  # nobody may flip an Accepted ADR back to Proposed, or edit a granted-paths block, in a code PR
  # NOTE: capture then test, don't pipe `git show` into `grep -q` — under `set -o pipefail`,
  # an early grep match can SIGPIPE `git show` before it finishes writing, which makes the
  # pipeline report failure even though a match was found (reproduced against this exact
  # failure mode in preflight_deps; see devteam.sh).
  MAIN_ADR_CONTENT="$(git -C "$ROOT" show "origin/main:$f" 2>/dev/null)"
  if grep -qE '^\*\*Status:\*\*[[:space:]]*Accepted' <<<"$MAIN_ADR_CONTENT"; then
    grep -qE '^\*\*Status:\*\*[[:space:]]*Accepted' "$ROOT/$f" \
      || { echo "[gates] FAIL: $f was Accepted on main and is no longer — grants are not revocable by PR"; exit 1; }
  fi
done

echo "[gates] G2 secret scan (basic)"
# Capture then test — do not pipe `git diff` into `grep -q`. Under `set -o pipefail`, grep
# exits as soon as it finds the first match, which can SIGPIPE `git diff` before it finishes
# writing the rest of the diff; the pipeline then reports failure for the wrong reason, but
# worse, `!` would then treat the (SIGPIPE, non-grep) failure as "no secret found" even when
# grep's own match was real. This is the one gate that must never fail open.
FULL_DIFF="$(git -C "$ROOT" diff origin/main...HEAD)"
! grep -Eiq 'BEGIN (RSA|EC|OPENSSH) PRIVATE KEY|api[_-]?key\s*=\s*["'"'"'][A-Za-z0-9]{20,}' <<<"$FULL_DIFF" \
  || { echo "[gates] FAIL: possible secret in diff"; exit 1; }

echo "[gates] all green"