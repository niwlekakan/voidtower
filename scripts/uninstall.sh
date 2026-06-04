#!/usr/bin/env bash
# VoidTower uninstaller
set -euo pipefail

RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; RESET='\033[0m'
info()  { echo -e "${CYAN}[INFO]${RESET}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
die()   { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || die "Must be run as root"

PURGE=false
REMOVE_ODYSSEUS=false
REMOVE_VOIDWATCH=false
REMOVE_AI=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --purge)            PURGE=true ;;
    --remove-odysseus)  REMOVE_ODYSSEUS=true ;;
    --remove-voidwatch) REMOVE_VOIDWATCH=true; REMOVE_ODYSSEUS=true ;;
    --remove-ai)        REMOVE_AI=true ;;
    --all)              PURGE=true; REMOVE_ODYSSEUS=true; REMOVE_VOIDWATCH=true; REMOVE_AI=true ;;
    --help|-h)
      cat <<EOF
Usage: uninstall.sh [OPTIONS]

  --purge              Also remove data and config directories
  --remove-odysseus    Stop and remove Odysseus service and files
  --remove-voidwatch   Remove Voidwatch integration (implies --remove-odysseus)
  --remove-ai          Remove Ollama and llama.cpp
  --all                Remove everything with purge
EOF
      exit 0 ;;
    *) die "Unknown option: $1" ;;
  esac
  shift
done

VT_INSTALL_DIR="${VT_INSTALL_DIR:-/opt/voidtower}"
VT_DATA_DIR="${VT_DATA_DIR:-/var/lib/voidtower}"
VT_CONFIG_DIR="${VT_CONFIG_DIR:-/etc/voidtower}"
VT_USER="${VT_USER:-voidtower}"
ODYSSEUS_INSTALL_DIR="${ODYSSEUS_INSTALL_DIR:-/opt/odysseus}"
ODYSSEUS_DATA_DIR="${ODYSSEUS_DATA_DIR:-/var/lib/odysseus}"
ODYSSEUS_CONFIG_DIR="${ODYSSEUS_CONFIG_DIR:-/etc/odysseus}"
ODYSSEUS_USER="${ODYSSEUS_USER:-odysseus}"

_stop_disable() {
  local svc="$1"
  command -v systemctl &>/dev/null || return
  systemctl is-active --quiet "$svc" 2>/dev/null && { info "Stopping ${svc}…"; systemctl stop "$svc"; }
  systemctl is-enabled --quiet "$svc" 2>/dev/null && { info "Disabling ${svc}…"; systemctl disable "$svc"; }
  [[ -f "/etc/systemd/system/${svc}.service" ]] && rm -f "/etc/systemd/system/${svc}.service"
}

# ── Revoke Voidwatch token before removing ────────────────────────────────────
if [[ "$REMOVE_VOIDWATCH" == true ]]; then
  info "Revoking Voidwatch integration in VoidTower…"
  local vt_port="${VT_PORT:-8743}"
  # Disable Odysseus integration in VoidTower if it's still running
  if systemctl is-active --quiet voidtower 2>/dev/null; then
    curl -fsSL --max-time 5 \
      -H "Content-Type: application/json" \
      -d '{"enabled":false,"emergency_disable":true}' \
      "http://localhost:${vt_port}/api/integrations/odysseus/config" &>/dev/null || true
    info "Odysseus integration disabled in VoidTower"
  fi
fi

# ── Stop VoidTower ────────────────────────────────────────────────────────────
_stop_disable voidtower
_stop_disable voidtower-llama
command -v systemctl &>/dev/null && systemctl daemon-reload

info "Removing VoidTower binary and install dir…"
rm -rf "$VT_INSTALL_DIR"

# ── Remove Odysseus ───────────────────────────────────────────────────────────
if [[ "$REMOVE_ODYSSEUS" == true ]]; then
  info "Removing Odysseus…"
  _stop_disable odysseus

  if [[ "$PURGE" == true ]]; then
    rm -rf "$ODYSSEUS_INSTALL_DIR" "$ODYSSEUS_DATA_DIR" "$ODYSSEUS_CONFIG_DIR"
    id "$ODYSSEUS_USER" &>/dev/null && userdel "$ODYSSEUS_USER" 2>/dev/null || true
    success "Odysseus fully removed"
  else
    # Remove install dir but preserve data
    rm -rf "$ODYSSEUS_INSTALL_DIR"
    success "Odysseus removed (data preserved at ${ODYSSEUS_DATA_DIR})"
  fi
fi

# ── Remove AI runtime ──────────────────────────────────────────────────────────
if [[ "$REMOVE_AI" == true ]]; then
  if command -v ollama &>/dev/null; then
    _stop_disable ollama
    # Ask about models (large downloads)
    if [[ -d "${HOME}/.ollama/models" || -d "/usr/share/ollama/models" ]]; then
      warn "Ollama models are large files. Remove them?"
      read -rp "  Remove Ollama models? [y/N]: " _yn
      case "${_yn:-N}" in
        [Yy]*) rm -rf "${HOME}/.ollama" "/usr/share/ollama" 2>/dev/null || true ;;
        *) info "Models preserved. Remove manually: rm -rf ~/.ollama/models" ;;
      esac
    fi
    # Remove Ollama binary
    rm -f /usr/local/bin/ollama /usr/bin/ollama 2>/dev/null || true
    success "Ollama removed"
  fi
fi

# ── Purge VoidTower data ───────────────────────────────────────────────────────
if [[ "$PURGE" == true ]]; then
  warn "Purging VoidTower data and config…"
  rm -rf "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  id "$VT_USER" &>/dev/null && userdel "$VT_USER" 2>/dev/null || true
  rm -f /root/voidlink-bootstrap-token /root/odysseus-bootstrap-token /root/voidwatch-recovery-info 2>/dev/null || true
  success "VoidTower fully removed (all data purged)"
else
  success "VoidTower removed. Data preserved at ${VT_DATA_DIR}"
  info "Run with --purge to also remove data and config"
fi
