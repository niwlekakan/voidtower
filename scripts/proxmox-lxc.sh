#!/usr/bin/env bash
# VoidTower — Proxmox LXC installer
# Run ON the Proxmox host as root:
#
#   bash -c "$(curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/voidtower-aio/scripts/proxmox-lxc.sh)"
#
# Options (all have interactive fallback):
#   --yes              Non-interactive, accept all defaults
#   --id ID            Container ID (default: next available)
#   --hostname NAME    Container hostname (default: voidtower)
#   --storage POOL     Proxmox storage pool (default: auto-detected)
#   --disk GB          Root disk size in GB (default: 20)
#   --ram MB           RAM in MB (default: 2048)
#   --swap MB          Swap in MB (default: 512)
#   --cores N          CPU cores (default: 2)
#   --bridge BR        Network bridge (default: vmbr0)
#   --ip CIDR|dhcp     IP address or dhcp (default: dhcp)
#   --gw GW            Gateway (required for static IP)
#   --port PORT        VoidTower port inside container (default: 8743)
#   --no-docker        Skip Docker/nesting setup (App Vault disabled)
#   --all-in-one       Install full stack: VoidTower + Odysseus + AI
#   --pull-model       Pull Ollama model during install (implies --all-in-one AI)
#   --vt-flags FLAGS   Extra flags passed verbatim to install.sh
set -euo pipefail
trap 'echo -e "\033[0;31m[ERROR]\033[0m Unexpected exit at line $LINENO: $BASH_COMMAND" >&2' ERR

# ─── Colours ─────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
die()     { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }
step()    { echo -e "\n${BOLD}${CYAN}── $* ──${RESET}"; }
ask()     { echo -e "${BOLD}$*${RESET}"; }

# ─── Defaults ─────────────────────────────────────────────────────────────────
YES=false
CT_ID=""
CT_HOSTNAME="voidtower"
CT_STORAGE=""
CT_DISK=20
CT_RAM=2048
CT_SWAP=512
CT_CORES=2
CT_BRIDGE="vmbr0"
CT_IP="dhcp"
CT_GW=""
CT_PASS=""
VT_PORT=8743
DOCKER=true
ALL_IN_ONE=false
PULL_MODEL=false
VT_EXTRA_FLAGS=""
TEMPLATE_STORAGE="local"   # where to store the downloaded template

# ─── Args ─────────────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes)         YES=true ;;
    --id)          CT_ID="$2";          shift ;;
    --hostname)    CT_HOSTNAME="$2";    shift ;;
    --storage)     CT_STORAGE="$2";     shift ;;
    --disk)        CT_DISK="$2";        shift ;;
    --ram)         CT_RAM="$2";         shift ;;
    --swap)        CT_SWAP="$2";        shift ;;
    --cores)       CT_CORES="$2";       shift ;;
    --bridge)      CT_BRIDGE="$2";      shift ;;
    --ip)          CT_IP="$2";          shift ;;
    --gw)          CT_GW="$2";          shift ;;
    --port)        VT_PORT="$2";        shift ;;
    --no-docker)   DOCKER=false ;;
    --all-in-one)  ALL_IN_ONE=true ;;
    --pull-model)  PULL_MODEL=true; ALL_IN_ONE=true ;;
    --vt-flags)    VT_EXTRA_FLAGS="$2"; shift ;;
    --help|-h)
      head -20 "$0" | grep "^#" | sed 's/^# \?//'
      exit 0 ;;
    *) die "Unknown option: $1" ;;
  esac
  shift
done

# ─── Proxmox host check ───────────────────────────────────────────────────────
[[ $EUID -eq 0 ]] || die "Must be run as root on the Proxmox host."
command -v pct      >/dev/null 2>&1 || die "pct not found — run this on a Proxmox VE host."
command -v pveam    >/dev/null 2>&1 || die "pveam not found — run this on a Proxmox VE host."
command -v pvesm    >/dev/null 2>&1 || die "pvesm not found — run this on a Proxmox VE host."

# ─── Banner ───────────────────────────────────────────────────────────────────
echo
echo -e "${BOLD}${CYAN}▓▓▒░ VoidTower — Proxmox LXC Installer ░▒▓▓${RESET}"
[[ "$ALL_IN_ONE" == true ]] && echo -e "     ${CYAN}+ Odysseus + Voidwatch + Local AI (Ollama)${RESET}"
[[ "$DOCKER"     == false ]] && echo -e "     ${YELLOW}Docker disabled — App Vault will be limited${RESET}"
echo

# ─── Auto-detect defaults ─────────────────────────────────────────────────────

# Next available container ID
if [[ -z "$CT_ID" ]]; then
  CT_ID=$(pct nextid 2>/dev/null || true)
  # pct nextid is absent or broken on some PVE versions — scan manually
  if [[ -z "$CT_ID" || ! "$CT_ID" =~ ^[0-9]+$ ]]; then
    for _id in $(seq 100 999); do
      if ! pct status "$_id" >/dev/null 2>&1 && ! qm status "$_id" >/dev/null 2>&1; then
        CT_ID="$_id"; break
      fi
    done
  fi
  [[ -n "$CT_ID" ]] || die "Could not find a free container ID — set one with --id"
fi

# Storage: prefer local-lvm or local-zfs, fall back to local
if [[ -z "$CT_STORAGE" ]]; then
  # pvesm status columns: Name  Type  Status  Total  Used  Available  %
  CT_STORAGE=$(pvesm status 2>/dev/null \
    | awk 'NR>1 && $2=="lvm-thin" && $3=="active" {print $1; exit}')
  if [[ -z "$CT_STORAGE" ]]; then
    CT_STORAGE=$(pvesm status 2>/dev/null \
      | awk 'NR>1 && $2=="zfspool" && $3=="active" {print $1; exit}')
  fi
  if [[ -z "$CT_STORAGE" ]]; then
    CT_STORAGE=$(pvesm status 2>/dev/null \
      | awk 'NR>1 && $2=="dir" && $3=="active" {print $1; exit}')
  fi
  [[ -n "$CT_STORAGE" ]] || CT_STORAGE="local"
fi

# Find the best storage for templates (needs content type vztmpl)
TEMPLATE_STORAGE=$(pvesm status --content vztmpl 2>/dev/null \
  | awk 'NR>1 && $3=="active" {print $1; exit}')
[[ -n "$TEMPLATE_STORAGE" ]] || TEMPLATE_STORAGE="local"

# ─── Interactive config ───────────────────────────────────────────────────────
if [[ "$YES" == false ]]; then
  echo -e "${BOLD}Container configuration${RESET} (press Enter to accept defaults)\n"

  read -rp "  Container ID        [${CT_ID}]: " _v
  [[ -n "$_v" ]] && CT_ID="$_v"

  read -rp "  Hostname            [${CT_HOSTNAME}]: " _v
  [[ -n "$_v" ]] && CT_HOSTNAME="$_v"

  read -rp "  Storage pool        [${CT_STORAGE}]: " _v
  [[ -n "$_v" ]] && CT_STORAGE="$_v"

  read -rp "  Disk size (GB)      [${CT_DISK}]: " _v
  [[ -n "$_v" ]] && CT_DISK="$_v"

  read -rp "  RAM (MB)            [${CT_RAM}]: " _v
  [[ -n "$_v" ]] && CT_RAM="$_v"

  read -rp "  CPU cores           [${CT_CORES}]: " _v
  [[ -n "$_v" ]] && CT_CORES="$_v"

  read -rp "  Network bridge      [${CT_BRIDGE}]: " _v
  [[ -n "$_v" ]] && CT_BRIDGE="$_v"

  read -rp "  IP (cidr or dhcp)   [${CT_IP}]: " _v
  [[ -n "$_v" ]] && CT_IP="$_v"

  if [[ "$CT_IP" != "dhcp" && -z "$CT_GW" ]]; then
    read -rp "  Gateway             : " CT_GW
  fi

  read -rp "  VoidTower port      [${VT_PORT}]: " _v
  [[ -n "$_v" ]] && VT_PORT="$_v"

  echo -e "\n  ${BOLD}Install mode${RESET}"
  echo -e "  [1] VoidTower only"
  echo -e "  [2] Full stack — VoidTower + Odysseus + Voidwatch + AI (Ollama)"
  read -rp "  Choice [1-2, default 1]: " _mode
  case "${_mode:-1}" in
    2) ALL_IN_ONE=true
       read -rp "  Pull Ollama model now? [y/N]: " _yn
       [[ "${_yn,,}" == "y"* ]] && PULL_MODEL=true ;;
  esac

  read -rsp "  Root password for container (leave blank = no password): " CT_PASS
  echo

  echo
fi

# ─── Validate ─────────────────────────────────────────────────────────────────
[[ "$CT_ID" =~ ^[0-9]+$ ]]   || die "Container ID must be a number: ${CT_ID}"
[[ "$CT_DISK" =~ ^[0-9]+$ ]] || die "Disk size must be a number: ${CT_DISK}"
[[ "$CT_RAM"  =~ ^[0-9]+$ ]] || die "RAM must be a number: ${CT_RAM}"

if pct status "$CT_ID" >/dev/null 2>&1; then
  die "Container ${CT_ID} already exists. Choose a different ID with --id."
fi

# ─── Template ─────────────────────────────────────────────────────────────────
step "Preparing Ubuntu 24.04 LXC template"

# Find the best available Ubuntu 24.04 template name
TEMPLATE_FILE=$(pveam available --section system 2>/dev/null \
  | awk '/ubuntu-24\.04/ {print $2; exit}')

# Fall back to 22.04 if 24.04 isn't listed yet
if [[ -z "$TEMPLATE_FILE" ]]; then
  TEMPLATE_FILE=$(pveam available --section system 2>/dev/null \
    | awk '/ubuntu-22\.04/ {print $2; exit}')
fi

[[ -n "$TEMPLATE_FILE" ]] || die "No Ubuntu template found in pveam. Run: pveam update"

TEMPLATE_PATH="${TEMPLATE_STORAGE}:vztmpl/${TEMPLATE_FILE}"

# Download only if not already present
if ! pveam list "$TEMPLATE_STORAGE" 2>/dev/null | grep -q "$TEMPLATE_FILE"; then
  info "Downloading ${TEMPLATE_FILE} to ${TEMPLATE_STORAGE}…"
  pveam download "$TEMPLATE_STORAGE" "$TEMPLATE_FILE" \
    || die "Template download failed. Check storage and network."
  success "Template downloaded"
else
  success "Template already available: ${TEMPLATE_FILE}"
fi

# ─── Create container ─────────────────────────────────────────────────────────
step "Creating LXC container ${CT_ID}"

PCT_ARGS=(
  "$CT_ID" "$TEMPLATE_PATH"
  --hostname   "$CT_HOSTNAME"
  --storage    "$CT_STORAGE"
  --rootfs     "${CT_STORAGE}:${CT_DISK}"
  --memory     "$CT_RAM"
  --swap       "$CT_SWAP"
  --cores      "$CT_CORES"
  --net0       "name=eth0,bridge=${CT_BRIDGE},firewall=1"
  --unprivileged 1
  --onboot     1
  --start      0
)

# Network: static or dhcp
if [[ "$CT_IP" == "dhcp" ]]; then
  PCT_ARGS+=(--net0 "name=eth0,bridge=${CT_BRIDGE},ip=dhcp,ip6=auto,firewall=1")
else
  [[ -n "$CT_GW" ]] || die "Static IP requires --gw <gateway>"
  PCT_ARGS+=(--net0 "name=eth0,bridge=${CT_BRIDGE},ip=${CT_IP},gw=${CT_GW},firewall=1")
fi

# Root password
if [[ -n "$CT_PASS" ]]; then
  PCT_ARGS+=(--password "$CT_PASS")
elif [[ -f /root/.ssh/authorized_keys ]]; then
  PCT_ARGS+=(--ssh-public-keys /root/.ssh/authorized_keys)
fi

# Docker / nesting
if [[ "$DOCKER" == true ]]; then
  PCT_ARGS+=(--features "keyctl=1,nesting=1")
fi

# pct create has --net0 twice above (default then override) — fix that
# Rebuild without the first --net0 default
PCT_ARGS_CLEAN=("$CT_ID" "$TEMPLATE_PATH"
  --hostname   "$CT_HOSTNAME"
  --storage    "$CT_STORAGE"
  --rootfs     "${CT_STORAGE}:${CT_DISK}"
  --memory     "$CT_RAM"
  --swap       "$CT_SWAP"
  --cores      "$CT_CORES"
  --unprivileged 1
  --onboot     1
  --start      0
)

if [[ "$CT_IP" == "dhcp" ]]; then
  PCT_ARGS_CLEAN+=(--net0 "name=eth0,bridge=${CT_BRIDGE},ip=dhcp,ip6=auto,firewall=1")
else
  PCT_ARGS_CLEAN+=(--net0 "name=eth0,bridge=${CT_BRIDGE},ip=${CT_IP},gw=${CT_GW},firewall=1")
fi

[[ -n "$CT_PASS" ]] && PCT_ARGS_CLEAN+=(--password "$CT_PASS")
[[ "$DOCKER" == true ]] && PCT_ARGS_CLEAN+=(--features "keyctl=1,nesting=1")

info "Running: pct create ${PCT_ARGS_CLEAN[*]}"
pct create "${PCT_ARGS_CLEAN[@]}" || die "pct create failed"
success "Container ${CT_ID} created"

# Extra LXC config for Docker (apparmor + cgroup device access)
if [[ "$DOCKER" == true ]]; then
  CT_CONF="/etc/pve/lxc/${CT_ID}.conf"
  cat >> "$CT_CONF" <<'LXCEOF'
# Docker support — added by VoidTower LXC installer
lxc.apparmor.profile: unconfined
lxc.cap.drop:
lxc.cgroup2.devices.allow: a
lxc.mount.auto: proc:rw sys:rw
LXCEOF
  success "Docker LXC config applied"
fi

# ─── Start and wait ───────────────────────────────────────────────────────────
step "Starting container ${CT_ID}"
pct start "$CT_ID" || die "Failed to start container"

# Wait until the container's network is up (max 60 s)
info "Waiting for container to boot…"
for i in $(seq 1 30); do
  if pct exec "$CT_ID" -- bash -c "command -v apt-get" >/dev/null 2>&1; then
    success "Container ready"
    break
  fi
  sleep 2
  [[ $i -eq 30 ]] && die "Container did not become ready in 60 s — check: pct status ${CT_ID}"
done

# ─── Bootstrap inside container ───────────────────────────────────────────────
step "Installing VoidTower inside container ${CT_ID}"

# Build the installer flags
INSTALLER_FLAGS="--unattended --port ${VT_PORT}"
[[ "$ALL_IN_ONE" == true  ]] && INSTALLER_FLAGS+=" --all-in-one"
[[ "$PULL_MODEL" == true  ]] && INSTALLER_FLAGS+=" --pull-model"
[[ -n "$VT_EXTRA_FLAGS"   ]] && INSTALLER_FLAGS+=" ${VT_EXTRA_FLAGS}"

info "Installer flags: ${INSTALLER_FLAGS}"

# Install curl inside the container first, then run the VoidTower installer
pct exec "$CT_ID" -- bash -c "
  set -e
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq curl ca-certificates
  curl -fsSL https://raw.githubusercontent.com/niwlekakan/voidtower/voidtower-aio/scripts/install.sh \
    | bash -s -- ${INSTALLER_FLAGS}
" || die "VoidTower installation inside container failed — check: pct console ${CT_ID}"

# ─── Retrieve access info ─────────────────────────────────────────────────────
# Grab the container IP for the summary
CT_ACTUAL_IP=$(pct exec "$CT_ID" -- bash -c \
  "ip -4 addr show eth0 2>/dev/null | awk '/inet /{print \$2}' | cut -d/ -f1 | head -1" \
  2>/dev/null || echo "")

[[ -z "$CT_ACTUAL_IP" ]] && CT_ACTUAL_IP="${CT_IP%/*}"

# ─── Summary ─────────────────────────────────────────────────────────────────
echo
echo -e "${BOLD}${GREEN}══════════════════════════════════════════════════${RESET}"
echo -e "${BOLD}${GREEN}  VoidTower LXC Ready!${RESET}"
echo -e "${BOLD}${GREEN}══════════════════════════════════════════════════${RESET}"
echo
echo -e "  ${BOLD}Container${RESET}    CT${CT_ID}  (${CT_HOSTNAME})"
echo -e "  ${BOLD}Resources${RESET}   ${CT_CORES} cores · ${CT_RAM} MB RAM · ${CT_DISK} GB disk"
echo -e "  ${BOLD}Docker${RESET}      $([ "$DOCKER" == true ] && echo "enabled (nesting + keyctl)" || echo "disabled")"
echo
echo -e "  ${BOLD}VoidTower${RESET}"
echo -e "    URL:     ${CYAN}http://${CT_ACTUAL_IP}:${VT_PORT}/bootstrap${RESET}"
echo -e "    Creds:   ${CYAN}pct exec ${CT_ID} -- cat /root/voidtower-bootstrap-token${RESET}"
echo -e "    Logs:    ${CYAN}pct exec ${CT_ID} -- journalctl -u voidtower -f${RESET}"
echo -e "    Console: ${CYAN}pct console ${CT_ID}${RESET}"

if [[ "$ALL_IN_ONE" == true ]]; then
  echo
  echo -e "  ${BOLD}Odysseus${RESET}"
  echo -e "    URL:     ${CYAN}http://${CT_ACTUAL_IP}:7000${RESET}"
  echo -e "    Creds:   ${CYAN}pct exec ${CT_ID} -- cat /root/odysseus-bootstrap-token${RESET}"
  echo -e "    Logs:    ${CYAN}pct exec ${CT_ID} -- journalctl -u odysseus -f${RESET}"
fi

echo
echo -e "  ${BOLD}Manage container${RESET}"
echo -e "    Start:   ${CYAN}pct start ${CT_ID}${RESET}"
echo -e "    Stop:    ${CYAN}pct stop ${CT_ID}${RESET}"
echo -e "    Shell:   ${CYAN}pct enter ${CT_ID}${RESET}"
echo
