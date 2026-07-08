#!/usr/bin/env bash
# scripts/devteam/gates.sh — merge gates for VoidTower dev-team work.
# Invoked by workers before committing and by the runner after each task.
# FORBIDDEN ZONE: agents must never edit this file (CLAUDE.md hard rules).
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"

echo "[gates] G0 format/lint"
( cd "$ROOT/backend" && cargo fmt --check )
( cd "$ROOT/backend" && cargo clippy --all-targets --all-features -- -D warnings )

echo "[gates] G0 forbidden-zone diff check"
CHANGED="$(git -C "$ROOT" diff --name-only origin/main...HEAD)"
BLOCKLIST='^backend/src/policy\.rs$|^backend/src/auth/|^backend/src/api/auth\.rs$|^backend/src/oidc\.rs$|^backend/src/db/mod\.rs$|^\.github/workflows/|^scripts/devteam/|^CLAUDE\.md$|^docs/edd\.md$|^docs/gap-analysis\.md$'
if echo "$CHANGED" | grep -Eq "$BLOCKLIST"; then
  # allowed only if the active task spec explicitly cites an ADR granting it
  if ! grep -Rq "ADR-" "$ROOT/.devteam/active/" 2>/dev/null; then
    echo "[gates] FAIL: forbidden-zone files changed without an ADR-bearing task spec:"
    echo "$CHANGED" | grep -E "$BLOCKLIST" | sed 's/^/  /'
    exit 1
  fi
fi

echo "[gates] G1 tests"
( cd "$ROOT/backend" && cargo test --all-features )

echo "[gates] G1 frontend (if touched)"
if echo "$CHANGED" | grep -q '^frontend/'; then
  ( cd "$ROOT/frontend" && npm run lint && npm run build )
fi

echo "[gates] G2 secret scan (basic)"
! git -C "$ROOT" diff origin/main...HEAD | grep -Eiq 'BEGIN (RSA|EC|OPENSSH) PRIVATE KEY|api[_-]?key\s*=\s*["'"'"'][A-Za-z0-9]{20,}' \
  || { echo "[gates] FAIL: possible secret in diff"; exit 1; }

echo "[gates] all green"
