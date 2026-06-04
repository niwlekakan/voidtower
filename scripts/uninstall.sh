#!/usr/bin/env bash
# VoidTower uninstaller
set -euo pipefail

RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; RESET='\033[0m'
info()  { echo -e "\033[0;36m[INFO]\033[0m  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
die()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "Must be run as root"

PURGE=false
[[ "${1:-}" == "--purge" ]] && PURGE=true

VT_INSTALL_DIR="${VT_INSTALL_DIR:-/opt/voidtower}"
VT_DATA_DIR="${VT_DATA_DIR:-/var/lib/voidtower}"
VT_CONFIG_DIR="${VT_CONFIG_DIR:-/etc/voidtower}"
VT_USER="${VT_USER:-voidtower}"

if command -v systemctl &>/dev/null; then
  if systemctl is-active --quiet voidtower 2>/dev/null; then
    info "Stopping voidtower service…"
    systemctl stop voidtower
  fi
  if systemctl is-enabled --quiet voidtower 2>/dev/null; then
    info "Disabling voidtower service…"
    systemctl disable voidtower
  fi
  [[ -f /etc/systemd/system/voidtower.service ]] && rm -f /etc/systemd/system/voidtower.service
  systemctl daemon-reload
fi

info "Removing binary and install dir…"
rm -rf "$VT_INSTALL_DIR"

if [[ "$PURGE" == true ]]; then
  warn "Purging data and config directories…"
  rm -rf "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  if id "$VT_USER" &>/dev/null; then
    userdel "$VT_USER" 2>/dev/null || true
  fi
  echo -e "${GREEN}VoidTower fully removed (data purged).${RESET}"
else
  echo -e "${GREEN}VoidTower removed. Data preserved at ${VT_DATA_DIR}${RESET}"
  echo -e "Run with --purge to also remove data and config."
fi
