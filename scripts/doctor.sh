#!/usr/bin/env bash
# VoidTower environment health check
set -euo pipefail

CYAN='\033[0;36m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; RED='\033[0;31m'; RESET='\033[0m'
ok()   { echo -e "  ${GREEN}✓${RESET} $*"; }
warn() { echo -e "  ${YELLOW}!${RESET} $*"; }
fail() { echo -e "  ${RED}✗${RESET} $*"; FAILED=1; }
FAILED=0

echo -e "${CYAN}VoidTower Doctor${RESET}"
echo

echo "── Runtime ──────────────────────────"
if command -v voidtower &>/dev/null; then
  ok "voidtower binary found: $(command -v voidtower)"
else
  fail "voidtower binary not found in PATH"
fi

echo
echo "── System ───────────────────────────"
if command -v systemctl &>/dev/null; then
  ok "systemd available"
  if systemctl is-active --quiet voidtower 2>/dev/null; then
    ok "voidtower.service is running"
  else
    warn "voidtower.service is not running"
  fi
else
  warn "systemd not found — service management unavailable"
fi

# Disk space
AVAIL=$(df -k /var/lib/voidtower 2>/dev/null | awk 'NR==2{print $4}' || echo 0)
if [[ $AVAIL -gt 524288 ]]; then
  ok "Disk space: $((AVAIL / 1024)) MB free"
else
  warn "Low disk space: $((AVAIL / 1024)) MB free (< 512 MB)"
fi

# Port check
PORT="${VT_PORT:-8743}"
if command -v ss &>/dev/null; then
  if ss -tlnp 2>/dev/null | grep -q ":${PORT} "; then
    ok "Port ${PORT} is listening"
  else
    warn "Port ${PORT} is not listening — is VoidTower running?"
  fi
fi

echo
echo "── Connectivity ─────────────────────"
if curl -fsS --max-time 3 "http://localhost:${PORT}/api/health" &>/dev/null; then
  ok "HTTP health check passed"
else
  warn "HTTP health check failed (service may be down or unreachable)"
fi

echo
if [[ $FAILED -eq 0 ]]; then
  echo -e "${GREEN}All checks passed.${RESET}"
else
  echo -e "${RED}Some checks failed. Review the output above.${RESET}"
  exit 1
fi
