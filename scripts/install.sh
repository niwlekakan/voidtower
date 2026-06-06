#!/usr/bin/env bash
# VoidTower installer — supports standalone VoidTower and full integrated stack
# (VoidTower + Odysseus + Voidwatch + local AI via Ollama)
#
# Usage examples:
#   sudo bash install.sh                          # VoidTower only
#   sudo bash install.sh --with-odysseus          # + Odysseus AI workspace
#   sudo bash install.sh --all-in-one --pull-model # full stack with model
set -euo pipefail
trap 'echo -e "\033[0;31m[ERROR]\033[0m Unexpected exit at line $LINENO: $BASH_COMMAND" >&2' ERR

# ─── Colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
die()     { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }
step()    { echo -e "\n${BOLD}${CYAN}── $* ──${RESET}"; }

# ─── Defaults ────────────────────────────────────────────────────────────────
VT_VERSION="${VT_VERSION:-latest}"
VT_INSTALL_DIR="${VT_INSTALL_DIR:-/opt/voidtower}"
VT_DATA_DIR="${VT_DATA_DIR:-/var/lib/voidtower}"
VT_CONFIG_DIR="${VT_CONFIG_DIR:-/etc/voidtower}"
VT_USER="${VT_USER:-voidtower}"
VT_GROUP="${VT_GROUP:-voidtower}"
VT_PORT="${VT_PORT:-8743}"
VT_BIND="${VT_BIND:-127.0.0.1}"
SYSTEMD_DIR="/etc/systemd/system"
BINARY_NAME="voidtower"
REPO="niwlekakan/voidtower"
ODYSSEUS_REPO="niwlekakan/odysseus"
ODYSSEUS_BRANCH="odysseus-voidlink"

UNATTENDED=false
SKIP_SYSTEMD=false
NO_TLS=false
SKIP_AI=false
INSTALL_NGINX=""
AI_SETUP_DONE=false
LLM_REMOTE_URL=""
MODEL_PATH=""
VOIDTOWER_DOMAIN=""
MDNS_ENABLED=false
HAVE_SYSTEMD=false
HAVE_DOCKER=false
HAVE_COMPOSE=false
MUSL_BUILD="${MUSL_BUILD:-false}"

# Integrated-stack flags
WITH_ODYSSEUS=false
WITH_VOIDWATCH=false
WITH_AI=false
WITH_MCP=false
NO_MCP=false
NO_WEBHOOKS=false
NO_TOOLPACKS=false
ALL_IN_ONE=false
DRY_RUN=false
OFFLINE=false
YES_MODE=false
PULL_MODEL=false
SKIP_MODEL_PULL=false
AI_PROVIDER="ollama"
INSTALL_MODE="install"  # install | uninstall | reset | repair | update
AI_MODEL=""
ODYSSEUS_PORT="${ODYSSEUS_PORT:-7000}"
ODYSSEUS_INSTALL_DIR="${ODYSSEUS_INSTALL_DIR:-/opt/odysseus}"
ODYSSEUS_DATA_DIR="${ODYSSEUS_DATA_DIR:-/var/lib/odysseus}"
ODYSSEUS_CONFIG_DIR="${ODYSSEUS_CONFIG_DIR:-/etc/odysseus}"
ODYSSEUS_USER="${ODYSSEUS_USER:-odysseus}"
VOIDWATCH_TOKEN=""
VOIDWATCH_WEBHOOK_SECRET=""
VOIDWATCH_POLICY_FILE="${ODYSSEUS_CONFIG_DIR:-/etc/odysseus}/voidwatch/policy.json"
OLLAMA_PORT=11434

# ─── Argument parsing ────────────────────────────────────────────────────────
usage() {
  cat <<EOF
Usage: install.sh [OPTIONS]

Core:
  --unattended / --yes   Non-interactive with defaults
  --port PORT            VoidTower port (default: 8743)
  --bind ADDR            Bind address (default: 127.0.0.1)
  --install-dir DIR      VoidTower install dir (default: /opt/voidtower)
  --data-dir DIR         VoidTower data dir (default: /var/lib/voidtower)
  --no-tls               No TLS (use behind a reverse proxy)
  --skip-systemd         Skip systemd service install
  --skip-ai              Skip AI setup
  --version VER          Specific VoidTower version (default: latest)
  --musl                 Build a fully-static musl binary (TrueNAS Scale, Alpine)

Integrated stack:
  --with-odysseus        Install Odysseus AI workspace
  --with-voidwatch       Install Voidwatch integration (implies --with-odysseus)
  --with-ai              Set up local AI runtime (Ollama by default)
  --all-in-one           Shorthand: --with-odysseus --with-voidwatch --with-ai
  --ai-provider P        ollama | openai-compatible | none  (default: ollama)
  --ai-model MODEL       Ollama model  (e.g. qwen2.5-coder:7b-instruct)
  --pull-model           Pull the model during install
  --skip-model-pull      Never pull a model
  --odysseus-port P      Odysseus port (default: 7000)
  --voidlink-port P      Same as --port
  --no-mcp               Skip MCP tool registration
  --no-webhooks          Skip webhook configuration
  --no-toolpacks         Skip toolpack installation
  --offline              No network calls (use local pkg manager only)
  --dry-run              Print what would be done, make no changes
  --help                 Show this help

Maintenance:
  --uninstall            Remove VoidTower (interactive: choose what to keep)
  --reset                Wipe VoidTower state (data/config), keep binary & service
  --repair               Re-install binary + service unit, fix permissions
  --update               Download & apply latest VoidTower binary
EOF
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --unattended|--yes) UNATTENDED=true; YES_MODE=true ;;
    --port|--voidlink-port) VT_PORT="$2"; shift ;;
    --bind)        VT_BIND="$2"; shift ;;
    --install-dir) VT_INSTALL_DIR="$2"; shift ;;
    --data-dir)    VT_DATA_DIR="$2"; shift ;;
    --no-tls)      NO_TLS=true ;;
    --skip-systemd) SKIP_SYSTEMD=true ;;
    --skip-ai)     SKIP_AI=true ;;
    --with-nginx)  INSTALL_NGINX="yes" ;;
    --skip-nginx)  INSTALL_NGINX="no" ;;
    --version)     VT_VERSION="$2"; shift ;;
    --with-odysseus)    WITH_ODYSSEUS=true ;;
    --with-voidwatch)   WITH_VOIDWATCH=true; WITH_ODYSSEUS=true ;;
    --with-ai)          WITH_AI=true ;;
    --all-in-one)       ALL_IN_ONE=true; WITH_ODYSSEUS=true; WITH_VOIDWATCH=true; WITH_AI=true ;;
    --ai-provider)      AI_PROVIDER="$2"; shift ;;
    --ai-model)         AI_MODEL="$2"; shift ;;
    --pull-model)       PULL_MODEL=true ;;
    --skip-model-pull)  SKIP_MODEL_PULL=true ;;
    --odysseus-port)    ODYSSEUS_PORT="$2"; shift ;;
    --no-ai)            WITH_AI=false; SKIP_AI=true ;;
    --musl)             MUSL_BUILD=true ;;
    --no-mcp)           NO_MCP=true ;;
    --no-webhooks)      NO_WEBHOOKS=true ;;
    --no-toolpacks)     NO_TOOLPACKS=true ;;
    --offline)          OFFLINE=true ;;
    --dry-run)          DRY_RUN=true ;;
    --uninstall)        INSTALL_MODE="uninstall" ;;
    --reset)            INSTALL_MODE="reset" ;;
    --repair)           INSTALL_MODE="repair" ;;
    --update)           INSTALL_MODE="update" ;;
    --help|-h)     usage ;;
    *) die "Unknown option: $1" ;;
  esac
  shift
done

[[ "$WITH_VOIDWATCH" == true && "$NO_MCP" != true ]] && WITH_MCP=true

_dry_run_check() {
  if [[ "$DRY_RUN" == true ]]; then
    info "[DRY-RUN] Would: $*"
    return 0
  fi
  return 1
}

# ─── Root check ───────────────────────────────────────────────────────────────
[[ $EUID -eq 0 ]] || die "This installer must be run as root. Try: sudo bash install.sh"

# ─── OS detection ────────────────────────────────────────────────────────────
detect_os() {
  if [[ -f /etc/os-release ]]; then
    source /etc/os-release
    OS_ID="${ID:-unknown}"
    OS_ID_LIKE="${ID_LIKE:-}"
  else
    OS_ID="unknown"; OS_ID_LIKE=""
  fi

  case "$OS_ID" in
    ubuntu|debian|linuxmint|pop|raspbian|dietpi|armbian|proxmox) PKG_MGR="apt" ;;
    fedora)                        PKG_MGR="dnf" ;;
    rhel|centos|rocky|almalinux|ol) PKG_MGR="dnf" ;;
    arch|manjaro|endeavouros)      PKG_MGR="pacman" ;;
    opensuse*|sles)                PKG_MGR="zypper" ;;
    alpine)                        PKG_MGR="apk" ;;
    void)                          PKG_MGR="xbps" ;;
    gentoo)                        PKG_MGR="emerge" ;;
    solus)                         PKG_MGR="eopkg" ;;
    clear-linux-os)                PKG_MGR="swupd" ;;
    nixos)                         PKG_MGR="nix"; warn "NixOS: service install may need manual nix derivation" ;;
    *)
      if   [[ "$OS_ID_LIKE" == *debian* || "$OS_ID_LIKE" == *ubuntu* ]]; then PKG_MGR="apt"
      elif [[ "$OS_ID_LIKE" == *rhel*   || "$OS_ID_LIKE" == *fedora* ]]; then PKG_MGR="dnf"
      elif [[ "$OS_ID_LIKE" == *arch*   ]]; then PKG_MGR="pacman"
      elif [[ "$OS_ID_LIKE" == *suse*   ]]; then PKG_MGR="zypper"
      elif command -v apt-get &>/dev/null; then PKG_MGR="apt"
      elif command -v dnf     &>/dev/null; then PKG_MGR="dnf"
      elif command -v yum     &>/dev/null; then PKG_MGR="yum"
      elif command -v pacman  &>/dev/null; then PKG_MGR="pacman"
      elif command -v apk     &>/dev/null; then PKG_MGR="apk"
      else warn "Unknown package manager — basic deps must be pre-installed"; PKG_MGR="generic"
      fi
      ;;
  esac

  command -v systemctl &>/dev/null && systemctl --version &>/dev/null 2>&1 && HAVE_SYSTEMD=true || true
  command -v docker &>/dev/null && HAVE_DOCKER=true || true
  ( command -v docker-compose &>/dev/null || docker compose version &>/dev/null 2>&1 ) && HAVE_COMPOSE=true || true

  info "OS: ${PRETTY_NAME:-$OS_ID}  pkg: $PKG_MGR  systemd: $HAVE_SYSTEMD  docker: $HAVE_DOCKER"
}

# ─── Arch detection ───────────────────────────────────────────────────────────
detect_arch() {
  case "$(uname -m)" in
    x86_64)  ARCH="x86_64" ;;
    aarch64) ARCH="aarch64" ;;
    armv7l)  ARCH="armv7" ;;
    *) die "Unsupported architecture: $(uname -m)" ;;
  esac
  info "Architecture: $ARCH"
}

# ─── Dependency install ───────────────────────────────────────────────────────
install_deps() {
  local pkgs="curl tar ca-certificates unzip git"
  info "Installing base dependencies…"
  case "$PKG_MGR" in
    apt)
      DEBIAN_FRONTEND=noninteractive apt-get update -qq
      DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $pkgs pciutils python3 python3-pip python3-venv
      ;;
    dnf|yum)
      $PKG_MGR install -y -q $pkgs pciutils python3 python3-pip
      ;;
    pacman)
      pacman -Sy --noconfirm --needed $pkgs pciutils python python-pip
      ;;
    zypper)
      zypper --non-interactive install -q $pkgs pciutils python3 python3-pip
      ;;
    apk)
      apk add --no-cache $pkgs pciutils python3 py3-pip
      ;;
    xbps)
      xbps-install -Sy $pkgs pciutils python3 python3-pip
      ;;
    *)
      warn "Generic mode: ensure curl, tar, git, python3 are installed"
      ;;
  esac
}

# ─── Binary download ──────────────────────────────────────────────────────────
download_binary() {
  [[ "$OFFLINE" == true ]] && { warn "Offline mode: skipping binary download"; return 1; }

  if [[ "$VT_VERSION" == "latest" ]]; then
    info "Fetching latest release info…"
    VT_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | sed 's/.*"tag_name": *"v\([^"]*\)".*/\1/')
    [[ -n "$VT_VERSION" ]] || { warn "No published release found for ${REPO} — will build from source" >&2; return 1; }
  fi

  local archive="voidtower-${VT_VERSION}-${ARCH}-unknown-linux-musl.tar.gz"
  local download_url="https://github.com/${REPO}/releases/download/v${VT_VERSION}/${archive}"

  info "Downloading VoidTower v${VT_VERSION} for ${ARCH}…"
  local tmp_dir; tmp_dir=$(mktemp -d)

  curl -fsSL --progress-bar "$download_url" -o "$tmp_dir/$archive" || {
    rm -rf "$tmp_dir"
    die "Download failed. Check https://github.com/${REPO}/releases"
  }

  tar -xzf "$tmp_dir/$archive" -C "$tmp_dir"
  install -m 755 "$tmp_dir/${BINARY_NAME}" "${VT_INSTALL_DIR}/${BINARY_NAME}"
  rm -rf "$tmp_dir"
  success "Binary installed to ${VT_INSTALL_DIR}/${BINARY_NAME}"
}

# ─── Build from source ────────────────────────────────────────────────────────
build_from_source() {
  info "No pre-built binary available. Attempting build from source…"
  command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust: https://rustup.rs"
  command -v npm   >/dev/null 2>&1 || die "npm not found. Install Node.js: https://nodejs.org"

  local SRC; SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

  # When piped via curl | bash, BASH_SOURCE[0] is empty so SRC resolves to the
  # current directory which has no source tree — clone the repo instead.
  if [[ ! -d "$SRC/frontend" || ! -d "$SRC/backend" ]]; then
    info "Downloading VoidTower source…"
    SRC=$(mktemp -d -p /var/tmp 2>/dev/null || mktemp -d)
    local _tarball="$SRC/source.tar.gz"
    curl -fsSL --max-time 120 -o "$_tarball" \
      "https://github.com/${REPO}/archive/refs/heads/voidtower-aio.tar.gz" 2>&1
    tar -xz --strip-components=1 -C "$SRC" -f "$_tarball"
    rm -f "$_tarball"
    success "Source downloaded"
  fi

  info "Building frontend…"
  (cd "$SRC/frontend" && npm ci && npm run build) \
    || die "Frontend build failed"
  success "Frontend built"

  info "Building backend (this can take 10–15 min on first build)…"
  [[ -d "$SRC/backend" ]] || die "Backend source directory not found at $SRC/backend"

  # Detect if we should build a musl static binary (TrueNAS, Alpine, or user-requested)
  local _use_musl=false
  if grep -qi "truenas\|alpine" /etc/os-release 2>/dev/null; then
    _use_musl=true
  fi
  [[ "${MUSL_BUILD:-}" == "true" ]] && _use_musl=true

  local CARGO_BUILD_TARGET="" RUSTFLAGS=""
  if [[ "$_use_musl" == "true" ]]; then
    info "Building musl static binary (x86_64-unknown-linux-musl)…"
    rustup target add x86_64-unknown-linux-musl 2>/dev/null || true
    # musl-gcc is required to link any C deps (e.g. libsqlite3 bundled build)
    if ! command -v musl-gcc >/dev/null 2>&1; then
      case "$PKG_MGR" in
        apt)    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq musl-tools ;;
        apk)    apk add --no-cache musl-dev ;;
        dnf)    dnf install -y -q musl-gcc musl-libc-static 2>/dev/null || warn "musl-gcc not available in dnf repos — install manually" ;;
        *)      warn "Cannot auto-install musl-gcc for pkg manager: $PKG_MGR — ensure musl-tools is installed" ;;
      esac
    fi
    CARGO_BUILD_TARGET="x86_64-unknown-linux-musl"
    RUSTFLAGS="-C target-feature=+crt-static"
  fi

  if [[ -n "$CARGO_BUILD_TARGET" ]]; then
    (cd "$SRC/backend" && TMPDIR=/var/tmp RUSTFLAGS="$RUSTFLAGS" cargo build --release --target "$CARGO_BUILD_TARGET" 2>&1) \
      || die "Backend build failed"
    local bin="$SRC/backend/target/${CARGO_BUILD_TARGET}/release/${BINARY_NAME}"
  else
    (cd "$SRC/backend" && TMPDIR=/var/tmp cargo build --release 2>&1) \
      || die "Backend build failed (exit $?)"
    local bin="$SRC/backend/target/release/${BINARY_NAME}"
  fi
  success "Backend compiled"

  [[ -f "$bin" ]] || die "Binary not found after build: $bin"
  install -m 755 "$bin" "${VT_INSTALL_DIR}/${BINARY_NAME}" \
    || die "Failed to install binary to ${VT_INSTALL_DIR}"
  success "Binary installed"

  [[ -d "$SRC/frontend/dist" ]] || die "Frontend dist not found at $SRC/frontend/dist"
  cp -r "$SRC/frontend/dist" "${VT_INSTALL_DIR}/frontend" \
    || die "Failed to copy frontend dist"
  success "Frontend assets installed"

  git -C "$SRC" rev-parse HEAD 2>/dev/null > "${VT_INSTALL_DIR}/.commit" || true
  success "Built and installed from source"
}

# ─── App catalog ──────────────────────────────────────────────────────────────
install_catalog() {
  local catalog_dir="/usr/share/voidtower/apps"
  mkdir -p "$catalog_dir"

  # Prefer local source tree if already cloned by build_from_source
  local src_catalog
  src_catalog=$(dirname "$(realpath "${BASH_SOURCE[0]}" 2>/dev/null || echo ".")")/../app-vault/apps
  if [[ -d "$src_catalog" ]]; then
    cp "$src_catalog/"*.yml "$catalog_dir/" 2>/dev/null || true
  else
    # Download catalog tarball from GitHub (works for binary installs).
    # Two-step: download to file first, then extract — avoids SIGPIPE from
    # nested pipe-within-pipe when this script is itself run via curl|bash.
    local tmp_cat; tmp_cat=$(mktemp -d)
    local _cat_tarball="$tmp_cat/catalog.tar.gz"
    if curl -fsSL -o "$_cat_tarball" \
        "https://github.com/${REPO}/archive/refs/heads/voidtower-aio.tar.gz" 2>/dev/null; then
      tar -xz -C "$catalog_dir" --strip-components=3 --wildcards \
        "*/app-vault/apps/*.yml" -f "$_cat_tarball" 2>/dev/null || true
    fi
    rm -rf "$tmp_cat"
  fi

  local count; count=$(ls "$catalog_dir/"*.yml 2>/dev/null | wc -l)
  [[ "$count" -gt 0 ]] && success "App catalog installed (${count} apps)" \
                        || warn "App catalog empty — apps will not appear in the vault"
}

# ─── System setup ────────────────────────────────────────────────────────────
setup_system() {
  info "Creating directories and system user…"
  mkdir -p "$VT_INSTALL_DIR" "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  if ! id "$VT_USER" &>/dev/null; then
    useradd --system --no-create-home --shell /usr/sbin/nologin \
      --home-dir "$VT_DATA_DIR" "$VT_USER"
  fi
  chown -R "${VT_USER}:${VT_GROUP}" "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  chmod 750 "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  success "Directories and user ready"
}

# ─── VoidTower systemd service ────────────────────────────────────────────────
install_service() {
  [[ "$SKIP_SYSTEMD" == true ]] && return
  [[ "$HAVE_SYSTEMD" == false ]] && { warn "systemd not found, skipping service install"; return; }

  local EXTRA_FLAGS=""
  [[ "$NO_TLS" == true ]] && EXTRA_FLAGS=" --no-tls"

  info "Installing voidtower.service…"
  cat > "${SYSTEMD_DIR}/voidtower.service" <<EOF
[Unit]
Description=VoidTower Infrastructure Management
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${VT_USER}
Group=${VT_GROUP}
ExecStart=${VT_INSTALL_DIR}/${BINARY_NAME} --bind ${VT_BIND} --port ${VT_PORT}${EXTRA_FLAGS}
Environment=VOIDTOWER_DATA_DIR=${VT_DATA_DIR}
Environment=VOIDTOWER_CONFIG_DIR=${VT_CONFIG_DIR}
Environment=VOIDTOWER_FRONTEND_DIR=${VT_INSTALL_DIR}/frontend
Environment=VOIDTOWER_CATALOG_DIR=/usr/share/voidtower/apps
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=voidtower
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${VT_DATA_DIR} ${VT_CONFIG_DIR} -/etc/nginx/conf.d -/etc/nginx/sites-enabled
SupplementaryGroups=docker
PrivateTmp=true
CapabilityBoundingSet=
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
EOF
  # Give voidtower docker socket access
  if getent group docker &>/dev/null; then
    usermod -aG docker "${VT_USER}" 2>/dev/null || true
  fi
  # Give voidtower read access to system logs and common directories
  # (adm group = /var/log access on Debian/Ubuntu/Arch)
  if getent group adm &>/dev/null; then
    usermod -aG adm "${VT_USER}" 2>/dev/null || true
  fi

  # Grant voidtower write access to nginx conf.d so it can manage proxy configs
  if [[ -d /etc/nginx/conf.d ]]; then
    chown -R "${VT_USER}:${VT_GROUP}" /etc/nginx/conf.d || true
  fi

  # Allow voidtower to manage nginx without a password
  local sudoers_file="/etc/sudoers.d/voidtower-nginx"
  local ng; ng=$(command -v nginx || echo /usr/sbin/nginx)
  local sc; sc=$(command -v systemctl || echo /usr/bin/systemctl)
  printf '%s ALL=(root) NOPASSWD: %s start nginx\n'                              "${VT_USER}" "$sc"  > "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: %s stop nginx\n'                               "${VT_USER}" "$sc" >> "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: %s restart nginx\n'                            "${VT_USER}" "$sc" >> "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: %s reload nginx\n'                             "${VT_USER}" "$sc" >> "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: %s -t\n'                                       "${VT_USER}" "$ng" >> "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: %s -s reload\n'                                "${VT_USER}" "$ng" >> "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: /usr/bin/tail -n 100 /var/log/nginx/error.log\n'  "${VT_USER}"    >> "${sudoers_file}"
  printf '%s ALL=(root) NOPASSWD: /usr/bin/tail -n 100 /var/log/nginx/access.log\n' "${VT_USER}"   >> "${sudoers_file}"
  chmod 440 "${sudoers_file}"

  local ody_sudoers="/etc/sudoers.d/voidtower-odysseus"
  sc=$(command -v systemctl 2>/dev/null || echo /bin/systemctl)
  printf '%s ALL=(root) NOPASSWD: %s restart odysseus.service\n' "${VT_USER}" "$sc" > "${ody_sudoers}"
  chmod 440 "${ody_sudoers}"

  # Drop the Voidwatch auto-configure helper (triggered by path unit after bootstrap)
  cat > /opt/voidtower/configure-voidwatch.sh <<'SCRIPT'
#!/usr/bin/env bash
set -euo pipefail
TOKEN_FILE="/etc/voidtower/voidwatch-pending-token"
ODYSSEUS_DATA_DIR="${ODYSSEUS_DATA_DIR:-/var/lib/odysseus}"
ODYSSEUS_USER="${ODYSSEUS_USER:-odysseus}"
VT_PORT="${VT_PORT:-8743}"
ODYSSEUS_PORT="${ODYSSEUS_PORT:-7000}"

[[ -f "$TOKEN_FILE" ]] || exit 0
token=$(cat "$TOKEN_FILE")
[[ -n "$token" ]] || exit 0

webhook_secret=$(openssl rand -hex 32 2>/dev/null || cat /proc/sys/kernel/random/uuid | tr -d '-')

mkdir -p "$ODYSSEUS_DATA_DIR"
cat > "${ODYSSEUS_DATA_DIR}/voidwatch.json" <<EOF
{
  "enabled": true,
  "base_url": "http://localhost:${VT_PORT}",
  "api_token": "${token}",
  "webhook_secret": "${webhook_secret}",
  "allowed_scopes": [
    "metrics:read","services:read","services:restart","containers:read",
    "containers:restart","containers:logs","apps:read","apps:restart",
    "backups:read","backups:run","alerts:read","alerts:ack",
    "automation:read","automation:run","timeline:read","network:read",
    "storage:read","diagnostics:read","proxy:read","tags:read"
  ],
  "auto_action_policy": "read_only",
  "require_dry_run_before_apply": true,
  "webhook_enabled": true,
  "create_tasks_from_events": true,
  "emergency_disabled": false
}
EOF
chown "${ODYSSEUS_USER}:${ODYSSEUS_USER}" "${ODYSSEUS_DATA_DIR}/voidwatch.json" 2>/dev/null || true
chmod 600 "${ODYSSEUS_DATA_DIR}/voidwatch.json"

# Register Odysseus in VoidTower
curl -fsSL --max-time 8 --retry 3 \
  -H "Authorization: Bearer ${token}" \
  -H "Content-Type: application/json" \
  -d "{\"enabled\":true,\"mcp_enabled\":true,\"allowed_url\":\"http://localhost:${ODYSSEUS_PORT}\",\"webhook_secret\":\"${webhook_secret}\"}" \
  "http://localhost:${VT_PORT}/api/integrations/odysseus/config" >/dev/null 2>&1 || true

systemctl restart odysseus.service 2>/dev/null || true
rm -f "$TOKEN_FILE"
SCRIPT
  chmod 755 /opt/voidtower/configure-voidwatch.sh

  # Path unit watches for the pending token written by VoidTower after bootstrap
  cat > "${SYSTEMD_DIR}/voidwatch-configure.path" <<EOF
[Unit]
Description=Watch for VoidTower bootstrap completion
ConditionPathExists=/opt/odysseus/app.py

[Path]
PathExists=/etc/voidtower/voidwatch-pending-token
Unit=voidwatch-configure.service

[Install]
WantedBy=multi-user.target
EOF

  cat > "${SYSTEMD_DIR}/voidwatch-configure.service" <<EOF
[Unit]
Description=Auto-configure Voidwatch after VoidTower bootstrap
After=voidtower.service odysseus.service

[Service]
Type=oneshot
ExecStart=/opt/voidtower/configure-voidwatch.sh
EOF

  systemctl daemon-reload || die "systemctl daemon-reload failed"
  systemctl enable voidtower.service || die "Failed to enable voidtower.service"
  success "voidtower.service installed and enabled"
}

# ─── GPU / hardware detection ────────────────────────────────────────────────
detect_gpu() {
  GPU_VENDOR="cpu"; GPU_VRAM_MB=0; GPU_NAME="CPU"

  if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null 2>&1; then
    GPU_VENDOR="nvidia"
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 | sed 's/^ *//')
    GPU_VRAM_MB=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null | head -1 | tr -d ' ')
  elif command -v rocm-smi &>/dev/null 2>&1; then
    GPU_VENDOR="amd"
    GPU_NAME=$(rocm-smi --showproductname 2>/dev/null | grep -i "card\|gpu" | head -1 | sed 's/.*: //')
    GPU_VRAM_MB=$(rocm-smi --showmeminfo vram 2>/dev/null | awk '/Total Memory/ {printf "%d", $NF/1024/1024; exit}')
  elif lspci 2>/dev/null | grep -qi "nvidia"; then
    GPU_VENDOR="nvidia"; GPU_NAME=$(lspci 2>/dev/null | grep -i nvidia | head -1 | sed 's/.*: //')
  elif lspci 2>/dev/null | grep -qi "amd\|radeon"; then
    GPU_VENDOR="amd"; GPU_NAME=$(lspci 2>/dev/null | grep -i "amd\|radeon" | grep -i "vga\|3d\|display" | head -1 | sed 's/.*: //')
  fi

  GPU_VRAM_MB=$(( ${GPU_VRAM_MB:-0} + 0 ))
  SYSTEM_RAM_MB=$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 0)
  DISK_FREE_MB=$(df -m "${VT_DATA_DIR:-/var/lib}" 2>/dev/null | awk 'NR==2{print $4}' || echo 0)
}

# ─── Ollama model recommendation ─────────────────────────────────────────────
recommend_ollama_model() {
  if   [[ $SYSTEM_RAM_MB -ge 32000 ]]; then echo "qwen2.5-coder:14b-instruct"
  elif [[ $SYSTEM_RAM_MB -ge 16000 ]]; then echo "qwen2.5-coder:7b-instruct"
  elif [[ $SYSTEM_RAM_MB -ge 8000  ]]; then echo "qwen2.5-coder:3b-instruct"
  else echo ""
  fi
}

# ─── Model tier (llama.cpp) ───────────────────────────────────────────────────
select_model_tier() {
  if   [[ $GPU_VRAM_MB -ge 22000 ]]; then
    MODEL_NAME="Llama 3.3 70B Q4_K_M"; MODEL_SIZE="~40 GB"
    MODEL_FILE="Llama-3.3-70B-Instruct-Q4_K_M.gguf"
    MODEL_URL="https://huggingface.co/bartowski/Llama-3.3-70B-Instruct-GGUF/resolve/main/Llama-3.3-70B-Instruct-Q4_K_M.gguf"
  elif [[ $GPU_VRAM_MB -ge 11000 ]]; then
    MODEL_NAME="Qwen 2.5 14B Q6_K"; MODEL_SIZE="~11 GB"
    MODEL_FILE="Qwen2.5-14B-Instruct-Q6_K.gguf"
    MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-14B-Instruct-GGUF/resolve/main/Qwen2.5-14B-Instruct-Q6_K.gguf"
  elif [[ $GPU_VRAM_MB -ge 7000 ]]; then
    MODEL_NAME="Llama 3.1 8B Q8_0"; MODEL_SIZE="~8.5 GB"
    MODEL_FILE="Meta-Llama-3.1-8B-Instruct-Q8_0.gguf"
    MODEL_URL="https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q8_0.gguf"
  elif [[ $GPU_VRAM_MB -ge 3500 ]]; then
    MODEL_NAME="Mistral 7B v0.2 Q4_K_M"; MODEL_SIZE="~4.1 GB"
    MODEL_FILE="mistral-7b-instruct-v0.2.Q4_K_M.gguf"
    MODEL_URL="https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF/resolve/main/mistral-7b-instruct-v0.2.Q4_K_M.gguf"
  elif [[ $SYSTEM_RAM_MB -ge 28000 ]]; then
    MODEL_NAME="Mistral 7B v0.2 Q4_K_M (CPU)"; MODEL_SIZE="~4.1 GB"
    MODEL_FILE="mistral-7b-instruct-v0.2.Q4_K_M.gguf"
    MODEL_URL="https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF/resolve/main/mistral-7b-instruct-v0.2.Q4_K_M.gguf"
  else
    MODEL_NAME="Qwen 2.5 3B Q4_K_M (CPU)"; MODEL_SIZE="~1.9 GB"
    MODEL_FILE="Qwen2.5-3B-Instruct-Q4_K_M.gguf"
    MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf"
  fi
}

_set_model() {
  case "$1" in
    1) MODEL_NAME="Qwen 2.5 3B Q4_K_M (CPU)"; MODEL_SIZE="~1.9 GB"
       MODEL_FILE="Qwen2.5-3B-Instruct-Q4_K_M.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf" ;;
    2) MODEL_NAME="Mistral 7B v0.2 Q4_K_M"; MODEL_SIZE="~4.1 GB"
       MODEL_FILE="mistral-7b-instruct-v0.2.Q4_K_M.gguf"
       MODEL_URL="https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF/resolve/main/mistral-7b-instruct-v0.2.Q4_K_M.gguf" ;;
    3) MODEL_NAME="Llama 3.1 8B Q8_0"; MODEL_SIZE="~8.5 GB"
       MODEL_FILE="Meta-Llama-3.1-8B-Instruct-Q8_0.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q8_0.gguf" ;;
    4) MODEL_NAME="Qwen 2.5 14B Q6_K"; MODEL_SIZE="~11 GB"
       MODEL_FILE="Qwen2.5-14B-Instruct-Q6_K.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-14B-Instruct-GGUF/resolve/main/Qwen2.5-14B-Instruct-Q6_K.gguf" ;;
    5) MODEL_NAME="Llama 3.3 70B Q4_K_M"; MODEL_SIZE="~40 GB"
       MODEL_FILE="Llama-3.3-70B-Instruct-Q4_K_M.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Llama-3.3-70B-Instruct-GGUF/resolve/main/Llama-3.3-70B-Instruct-Q4_K_M.gguf" ;;
  esac
}

# ─── llama.cpp download ───────────────────────────────────────────────────────
download_llama_cpp() {
  [[ "$OFFLINE" == true ]] && return 1
  local LLAMA_DIR="${VT_INSTALL_DIR}/llama.cpp"
  local LLAMA_TAG
  LLAMA_TAG=$(curl -fsSL "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest" \
    | grep '"tag_name"' | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
  [[ -n "$LLAMA_TAG" ]] || { warn "Could not fetch llama.cpp release info"; return 1; }

  local ASSET
  case "$GPU_VENDOR" in
    nvidia) ASSET="llama-${LLAMA_TAG}-bin-ubuntu-cuda-cu12.4-x64.zip" ;;
    amd)    ASSET="llama-${LLAMA_TAG}-bin-ubuntu-vulkan-x64.zip" ;;
    *)      ASSET="llama-${LLAMA_TAG}-bin-ubuntu-x64.zip" ;;
  esac

  mkdir -p "$LLAMA_DIR"
  local tmp; tmp=$(mktemp -d)
  info "Downloading llama.cpp ${LLAMA_TAG}…"
  if ! curl -fL --progress-bar "https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_TAG}/${ASSET}" -o "$tmp/llama.zip"; then
    ASSET="llama-${LLAMA_TAG}-bin-ubuntu-x64.zip"
    curl -fL --progress-bar "https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_TAG}/${ASSET}" -o "$tmp/llama.zip" || { rm -rf "$tmp"; return 1; }
  fi
  unzip -q "$tmp/llama.zip" -d "$tmp/extracted"
  local server_bin
  server_bin=$(find "$tmp/extracted" \( -name "llama-server" -o -name "server" \) -type f | head -1)
  [[ -n "$server_bin" ]] || { warn "llama-server binary not found"; rm -rf "$tmp"; return 1; }
  install -m 755 "$server_bin" "${LLAMA_DIR}/llama-server"
  find "$tmp/extracted" -name "*.so*" -exec cp -n {} "$LLAMA_DIR/" \; 2>/dev/null || true
  rm -rf "$tmp"
  chown -R "${VT_USER}:${VT_GROUP}" "$LLAMA_DIR"
  success "llama-server installed to ${LLAMA_DIR}/llama-server"
}

download_model() {
  local MODELS_DIR="${VT_DATA_DIR}/models"
  mkdir -p "$MODELS_DIR"
  info "Downloading ${MODEL_NAME} (${MODEL_SIZE})…"
  warn "Large download — time depends on connection speed"
  if curl -fL --progress-bar "$MODEL_URL" -o "${MODELS_DIR}/${MODEL_FILE}"; then
    ln -sf "${MODELS_DIR}/${MODEL_FILE}" "${MODELS_DIR}/default.gguf"
    chown -R "${VT_USER}:${VT_GROUP}" "$MODELS_DIR"
    MODEL_PATH="${MODELS_DIR}/${MODEL_FILE}"
    success "Model saved to ${MODEL_PATH}"
  else
    warn "Model download failed. Configure AI later in Settings."
    return 1
  fi
}

install_llama_service() {
  [[ "$HAVE_SYSTEMD" == false || "$SKIP_SYSTEMD" == true ]] && return
  [[ -f "${VT_INSTALL_DIR}/llama.cpp/llama-server" && -n "$MODEL_PATH" ]] || return 0
  local N_GPU_LAYERS=0
  [[ "$GPU_VENDOR" != "cpu" ]] && N_GPU_LAYERS=99
  local N_THREADS; N_THREADS=$(nproc 2>/dev/null || echo 4)
  cat > "${SYSTEMD_DIR}/voidtower-llama.service" <<EOF
[Unit]
Description=VoidTower AI (llama-server)
After=network-online.target

[Service]
Type=simple
User=${VT_USER}
Group=${VT_GROUP}
Environment=LD_LIBRARY_PATH=${VT_INSTALL_DIR}/llama.cpp
ExecStart=${VT_INSTALL_DIR}/llama.cpp/llama-server \
  --model ${MODEL_PATH} --host 127.0.0.1 --port 8080 \
  --ctx-size 4096 --n-gpu-layers ${N_GPU_LAYERS} --threads ${N_THREADS} --log-disable
Restart=on-failure
RestartSec=10
SyslogIdentifier=voidtower-llama
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
EOF
  systemctl daemon-reload
  systemctl enable voidtower-llama.service
  success "llama-server service installed (port 8080)"
}

write_llm_config() {
  local endpoint="$1" model_label="${2:-}"
  printf 'VOIDTOWER_LLM_ENDPOINT=%s\nVOIDTOWER_LLM_MODEL=%s\n' "$endpoint" "$model_label" \
    > "${VT_CONFIG_DIR}/llm.env"
  chmod 640 "${VT_CONFIG_DIR}/llm.env"
  chown "${VT_USER}:${VT_GROUP}" "${VT_CONFIG_DIR}/llm.env"
}

# ─── Ollama installation ──────────────────────────────────────────────────────
install_ollama() {
  [[ "$OFFLINE" == true ]] && { warn "Offline: skipping Ollama install"; return 1; }

  if command -v ollama &>/dev/null; then
    success "Ollama already installed: $(ollama --version 2>/dev/null | head -1)"
    return 0
  fi

  info "Installing Ollama…"
  curl -fsSL https://ollama.com/install.sh | sh
  if [[ "$HAVE_SYSTEMD" == true ]]; then
    systemctl enable --now ollama 2>/dev/null || true
  fi
  success "Ollama installed"
}

pull_ollama_model() {
  local model="$1"
  [[ -z "$model" ]] && return 0
  [[ "$SKIP_MODEL_PULL" == true ]] && { info "Model pull skipped (--skip-model-pull)"; return 0; }

  info "Pulling Ollama model: ${model}…"
  if ollama pull "$model"; then
    success "Model pulled: $model"
    AI_MODEL="$model"
  else
    warn "Model pull failed. Pull manually later: ollama pull $model"
    return 1
  fi
}

configure_odysseus_for_ollama() {
  local model="$1"
  local env_file="${ODYSSEUS_INSTALL_DIR}/.env"
  [[ -f "$env_file" ]] || return 0

  # Set or update OLLAMA_BASE_URL and default model
  if grep -q "OLLAMA_BASE_URL" "$env_file" 2>/dev/null; then
    sed -i "s|^OLLAMA_BASE_URL=.*|OLLAMA_BASE_URL=http://localhost:${OLLAMA_PORT}|" "$env_file"
  else
    echo "OLLAMA_BASE_URL=http://localhost:${OLLAMA_PORT}" >> "$env_file"
  fi

  if [[ -n "$model" ]]; then
    if grep -q "DEFAULT_MODEL" "$env_file" 2>/dev/null; then
      sed -i "s|^DEFAULT_MODEL=.*|DEFAULT_MODEL=${model}|" "$env_file"
    else
      echo "DEFAULT_MODEL=${model}" >> "$env_file"
    fi
  fi
  success "Odysseus configured to use Ollama (model: ${model:-auto})"
}

# ─── Odysseus installation ────────────────────────────────────────────────────
install_odysseus() {
  step "Installing Odysseus AI Workspace"

  if [[ "$DRY_RUN" == true ]]; then
    info "[DRY-RUN] Would install Odysseus to ${ODYSSEUS_INSTALL_DIR}, port ${ODYSSEUS_PORT}"
    return 0
  fi

  # Check for existing install — must be the right branch, not just any Odysseus clone
  local needs_clone=true
  if [[ -d "$ODYSSEUS_INSTALL_DIR" && -f "${ODYSSEUS_INSTALL_DIR}/app.py" ]]; then
    local current_branch
    current_branch=$(git -C "$ODYSSEUS_INSTALL_DIR" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "unknown")
    if [[ "$current_branch" == "$ODYSSEUS_BRANCH" ]]; then
      info "Odysseus (${ODYSSEUS_BRANCH}) already installed — pulling latest"
      git -C "$ODYSSEUS_INSTALL_DIR" pull --quiet 2>/dev/null || true
      needs_clone=false
    else
      warn "Odysseus at ${ODYSSEUS_INSTALL_DIR} is on branch '${current_branch}', need '${ODYSSEUS_BRANCH}' — replacing"
      rm -rf "$ODYSSEUS_INSTALL_DIR"
    fi
  fi

  if [[ "$needs_clone" == true ]]; then
    if [[ "$OFFLINE" == true ]]; then
      die "Offline mode: Odysseus not found at ${ODYSSEUS_INSTALL_DIR}. Provide a local clone first."
    fi
    info "Cloning Odysseus (${ODYSSEUS_BRANCH})…"
    git clone --depth 1 --branch "${ODYSSEUS_BRANCH}" "https://github.com/${ODYSSEUS_REPO}" "$ODYSSEUS_INSTALL_DIR" \
      || die "Failed to clone ${ODYSSEUS_REPO}@${ODYSSEUS_BRANCH}"
    success "Odysseus cloned to ${ODYSSEUS_INSTALL_DIR}"
  fi

  # Create system user
  if ! id "$ODYSSEUS_USER" &>/dev/null; then
    useradd --system --no-create-home --shell /usr/sbin/nologin \
      --home-dir "$ODYSSEUS_DATA_DIR" "$ODYSSEUS_USER"
  fi

  mkdir -p "$ODYSSEUS_DATA_DIR" "$ODYSSEUS_CONFIG_DIR"
  chown -R "${ODYSSEUS_USER}:${ODYSSEUS_USER}" "$ODYSSEUS_INSTALL_DIR" "$ODYSSEUS_DATA_DIR" "$ODYSSEUS_CONFIG_DIR"

  # Python venv
  info "Setting up Python virtual environment…"
  python3 -m venv "${ODYSSEUS_INSTALL_DIR}/venv" \
    || die "Failed to create Python venv at ${ODYSSEUS_INSTALL_DIR}/venv"
  "${ODYSSEUS_INSTALL_DIR}/venv/bin/pip" install --quiet --upgrade pip \
    || die "Failed to upgrade pip in Odysseus venv"
  "${ODYSSEUS_INSTALL_DIR}/venv/bin/pip" install --quiet -r "${ODYSSEUS_INSTALL_DIR}/requirements.txt" \
    || die "Failed to install Odysseus Python dependencies"

  # Create .env if missing
  local env_file="${ODYSSEUS_INSTALL_DIR}/.env"
  if [[ ! -f "$env_file" ]]; then
    local odysseus_admin_pass
    odysseus_admin_pass=$(openssl rand -hex 16 2>/dev/null || cat /proc/sys/kernel/random/uuid | tr -d '-')
    cat > "$env_file" <<EOF
# Odysseus configuration — managed by VoidTower installer
APP_BIND=127.0.0.1
APP_PORT=${ODYSSEUS_PORT}
AUTH_ENABLED=true
SECURE_COOKIES=false
LOCALHOST_BYPASS=false
DATABASE_URL=sqlite:///./data/app.db
ODYSSEUS_ADMIN_USER=admin
ODYSSEUS_ADMIN_PASSWORD=${odysseus_admin_pass}
EOF
    chmod 600 "$env_file"
    chown "${ODYSSEUS_USER}:${ODYSSEUS_USER}" "$env_file"

    # Save bootstrap credentials
    local creds_file="/root/odysseus-bootstrap-token"
    cat > "$creds_file" <<EOF
Odysseus bootstrap credentials — keep this file safe and delete after first login
URL:      http://localhost:${ODYSSEUS_PORT}
Username: admin
Password: ${odysseus_admin_pass}
Generated: $(date -u)
EOF
    chmod 600 "$creds_file"

    echo
    echo -e "${BOLD}${YELLOW}── Odysseus Credentials ──${RESET}"
    echo -e "  URL:      ${CYAN}http://localhost:${ODYSSEUS_PORT}${RESET}"
    echo -e "  Username: ${CYAN}admin${RESET}"
    echo -e "  Password: ${CYAN}${odysseus_admin_pass}${RESET}"
    echo -e "  Saved to: ${creds_file}"
    echo
  fi

  # Symlink data dir
  if [[ ! -e "${ODYSSEUS_INSTALL_DIR}/data" ]]; then
    ln -sf "$ODYSSEUS_DATA_DIR" "${ODYSSEUS_INSTALL_DIR}/data"
    chown -h "${ODYSSEUS_USER}:${ODYSSEUS_USER}" "${ODYSSEUS_INSTALL_DIR}/data"
  fi

  # Bootstrap admin user from env vars — setup.py writes data/auth.json
  if [[ ! -f "${ODYSSEUS_DATA_DIR}/auth.json" ]]; then
    info "Creating Odysseus admin user…"
    local _admin_user _admin_pass
    _admin_user=$(grep "^ODYSSEUS_ADMIN_USER=" "${ODYSSEUS_INSTALL_DIR}/.env" | cut -d= -f2-)
    _admin_pass=$(grep "^ODYSSEUS_ADMIN_PASSWORD=" "${ODYSSEUS_INSTALL_DIR}/.env" | cut -d= -f2-)
    (cd "$ODYSSEUS_INSTALL_DIR" && \
      ODYSSEUS_ADMIN_USER="$_admin_user" \
      ODYSSEUS_ADMIN_PASSWORD="$_admin_pass" \
      ODYSSEUS_SKIP_RUN_HINT=1 \
      "${ODYSSEUS_INSTALL_DIR}/venv/bin/python" setup.py 2>/dev/null) \
      && success "Odysseus admin user created" \
      || warn "Odysseus setup.py failed — login may not work on first run"
  fi

  # systemd service
  if [[ "$HAVE_SYSTEMD" == true && "$SKIP_SYSTEMD" != true ]]; then
    cat > "${SYSTEMD_DIR}/odysseus.service" <<EOF
[Unit]
Description=Odysseus AI Workspace
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${ODYSSEUS_USER}
Group=${ODYSSEUS_USER}
WorkingDirectory=${ODYSSEUS_INSTALL_DIR}
EnvironmentFile=-${ODYSSEUS_INSTALL_DIR}/.env
ExecStart=${ODYSSEUS_INSTALL_DIR}/venv/bin/uvicorn app:app --host 127.0.0.1 --port ${ODYSSEUS_PORT}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=odysseus
NoNewPrivileges=true
ProtectSystem=full
ReadWritePaths=${ODYSSEUS_DATA_DIR} ${ODYSSEUS_INSTALL_DIR}/data

[Install]
WantedBy=multi-user.target
EOF
    systemctl daemon-reload || die "systemctl daemon-reload failed"
    systemctl enable odysseus.service || die "Failed to enable odysseus.service"
    systemctl enable voidwatch-configure.path || true
    systemctl start voidwatch-configure.path 2>/dev/null || true
    success "odysseus.service installed and enabled"
  else
    warn "systemd not available — start Odysseus manually:"
    warn "  cd ${ODYSSEUS_INSTALL_DIR} && venv/bin/uvicorn app:app --port ${ODYSSEUS_PORT}"
  fi

  success "Odysseus installed at ${ODYSSEUS_INSTALL_DIR}"
}

# ─── Secret generation helpers ────────────────────────────────────────────────
_gen_secret() {
  openssl rand -hex 32 2>/dev/null || cat /dev/urandom | tr -dc 'a-f0-9' | head -c 64
}

_wait_for_service() {
  local url="$1" label="$2" tries=20
  info "Waiting for ${label} to be ready…"
  for ((i=1; i<=tries; i++)); do
    if curl -fsSL --max-time 3 "$url" &>/dev/null; then
      success "${label} is ready"
      return 0
    fi
    sleep 2
  done
  warn "${label} did not respond at ${url} after ${tries} attempts"
  return 1
}

# ─── Generate Voidwatch API token via VoidTower API ───────────────────────────
_create_voidwatch_token() {
  local bootstrap_token_file="${VT_DATA_DIR}/bootstrap.token"
  [[ -f "$bootstrap_token_file" ]] || { warn "Bootstrap token not found — token creation skipped"; return 1; }
  local bt; bt=$(cat "$bootstrap_token_file")

  # First complete bootstrap if needed
  local me_resp
  me_resp=$(curl -fsSL --max-time 8 \
    -H "Authorization: Bearer $bt" \
    "http://localhost:${VT_PORT}/api/auth/me" 2>/dev/null || echo "")

  if [[ -z "$me_resp" || "$me_resp" == *"error"* ]]; then
    warn "VoidTower not responding or bootstrap not completed — token creation skipped"
    warn "  Complete VoidTower setup at http://localhost:${VT_PORT}/bootstrap"
    warn "  Then run: voidlink voidwatch configure"
    return 1
  fi

  # Create a cookie session by using the bootstrap-generated admin credentials
  # (VoidTower will have created admin during bootstrap — we use the VT API tokens endpoint)
  local session_cookie_file; session_cookie_file=$(mktemp)
  local scopes='["metrics:read","services:read","services:restart","containers:read","containers:restart","containers:logs","apps:read","apps:restart","backups:read","backups:run","alerts:read","alerts:ack","automation:read","automation:run","timeline:read","network:read","storage:read","diagnostics:read","proxy:read","tags:read","secrets:list","vms:read"]'

  local token_resp
  token_resp=$(curl -fsSL --max-time 10 \
    -H "Authorization: Bearer $bt" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"voidwatch-integration\",\"scopes\":${scopes}}" \
    "http://localhost:${VT_PORT}/api/integrations/tokens" 2>/dev/null || echo "")

  rm -f "$session_cookie_file"

  local token
  token=$(echo "$token_resp" | grep -o '"token":"[^"]*"' | sed 's/"token":"\([^"]*\)"/\1/' | head -1)
  if [[ -z "$token" ]]; then
    warn "Could not create API token automatically — create one manually in VoidTower"
    warn "  Scopes needed: metrics:read services:read containers:read apps:read backups:read alerts:read alerts:ack automation:read timeline:read"
    return 1
  fi

  VOIDWATCH_TOKEN="$token"
  success "Voidwatch API token created"
}

# ─── Install toolpacks ────────────────────────────────────────────────────────
install_toolpacks() {
  [[ "$NO_TOOLPACKS" == true ]] && return
  [[ "$DRY_RUN" == true ]] && { info "[DRY-RUN] Would install toolpacks to ${ODYSSEUS_INSTALL_DIR}/voidwatch/toolpacks/"; return; }

  local src_dir="${ODYSSEUS_INSTALL_DIR}/voidwatch/toolpacks"
  [[ -d "$src_dir" ]] || { warn "Toolpack directory not found at ${src_dir}"; return; }

  local count valid invalid
  count=$(find "$src_dir" -name "*.yml" -o -name "*.yaml" | wc -l)
  info "Validating ${count} toolpacks…"

  valid=0; invalid=0
  while IFS= read -r -d '' f; do
    if grep -q "^id:" "$f" && grep -q "^display_name:" "$f" && grep -q "^deployment_types:" "$f"; then
      ((valid++)) || true
    else
      warn "Invalid toolpack (missing required fields): $(basename "$f")"
      ((invalid++)) || true
    fi
  done < <(find "$src_dir" \( -name "*.yml" -o -name "*.yaml" \) -print0)

  success "Toolpacks: ${valid} valid, ${invalid} invalid"
}

# ─── Generate default policy file ────────────────────────────────────────────
write_voidwatch_policy() {
  [[ "$DRY_RUN" == true ]] && { info "[DRY-RUN] Would write policy to ${VOIDWATCH_POLICY_FILE}"; return; }

  mkdir -p "$(dirname "$VOIDWATCH_POLICY_FILE")"
  cat > "$VOIDWATCH_POLICY_FILE" <<'EOF'
{
  "_comment": "Voidwatch default safe policy — edit via Settings > Integrations > Voidwatch",
  "auto_action_policy": "read_only",
  "require_dry_run_before_apply": true,
  "require_backup_before_apply": false,
  "allow_diagnostics": true,
  "allow_acknowledge_alerts": true,
  "allow_run_backups": true,
  "allow_restart_noncritical": false,
  "allow_run_approved_automations": true,
  "shell_execution_enabled": false,
  "protected_tags": ["critical", "database", "prod", "ai-no-touch"],
  "auto_allowed_tags": ["lab", "non-critical"],
  "forbidden_actions": [
    "run_shell_command", "delete_container", "delete_vm", "delete_backup",
    "delete_app", "rotate_secret", "apply_firewall_change", "remove_node",
    "edit_auth_config"
  ],
  "confirmation_rules": {
    "edit_config": true,
    "expose_publicly": true,
    "restart_critical": true,
    "deploy_app": true,
    "restore_backup": true
  }
}
EOF
  chmod 640 "$VOIDWATCH_POLICY_FILE"
  chown "${ODYSSEUS_USER}:${ODYSSEUS_USER}" "$VOIDWATCH_POLICY_FILE" 2>/dev/null || true
  success "Voidwatch default policy written to ${VOIDWATCH_POLICY_FILE}"
}

# ─── Configure Voidwatch integration ─────────────────────────────────────────
configure_voidwatch() {
  step "Configuring Voidwatch Integration"

  [[ "$DRY_RUN" == true ]] && {
    info "[DRY-RUN] Would link VoidTower ↔ Odysseus via token + webhook"
    return 0
  }

  VOIDWATCH_WEBHOOK_SECRET=$(_gen_secret)

  # Wait for services
  _wait_for_service "http://localhost:${VT_PORT}/api/health" "VoidTower" || true
  _wait_for_service "http://localhost:${ODYSSEUS_PORT}/api/health" "Odysseus" || true

  # Create token
  _create_voidwatch_token || true

  # Write Odysseus voidwatch config
  local vw_config_file="${ODYSSEUS_DATA_DIR}/voidwatch.json"
  local token_to_write="${VOIDWATCH_TOKEN:-REPLACE_WITH_TOKEN}"
  cat > "$vw_config_file" <<EOF
{
  "enabled": $([ -n "$VOIDWATCH_TOKEN" ] && echo "true" || echo "false"),
  "base_url": "http://localhost:${VT_PORT}",
  "api_token": "${token_to_write}",
  "webhook_secret": "${VOIDWATCH_WEBHOOK_SECRET}",
  "allowed_scopes": [
    "metrics:read","services:read","services:restart","containers:read",
    "containers:restart","containers:logs","apps:read","apps:restart",
    "backups:read","backups:run","alerts:read","alerts:ack",
    "automation:read","automation:run","timeline:read","network:read",
    "storage:read","diagnostics:read","proxy:read","tags:read"
  ],
  "auto_action_policy": "read_only",
  "require_dry_run_before_apply": true,
  "webhook_enabled": true,
  "create_tasks_from_events": true,
  "emergency_disabled": false
}
EOF
  chmod 600 "$vw_config_file"
  chown "${ODYSSEUS_USER}:${ODYSSEUS_USER}" "$vw_config_file" 2>/dev/null || true

  # Configure VoidTower Odysseus integration (enable, set webhook secret + allowed URL)
  if [[ -n "$VOIDWATCH_TOKEN" ]]; then
    curl -fsSL --max-time 8 \
      -H "Authorization: Bearer ${VOIDWATCH_TOKEN}" \
      -H "Content-Type: application/json" \
      -d "{\"enabled\":true,\"mcp_enabled\":true,\"allowed_url\":\"http://localhost:${ODYSSEUS_PORT}\",\"webhook_secret\":\"${VOIDWATCH_WEBHOOK_SECRET}\"}" \
      "http://localhost:${VT_PORT}/api/integrations/odysseus/config" &>/dev/null || true
  fi

  write_voidwatch_policy
  install_toolpacks

  # Save recovery info
  cat > /root/voidwatch-recovery-info <<EOF
Voidwatch Integration Info — $(date -u)
VoidTower URL:       http://localhost:${VT_PORT}
Odysseus URL:        http://localhost:${ODYSSEUS_PORT}
API Token:           ${VOIDWATCH_TOKEN:-<create manually in VoidTower Settings>}
Webhook Secret:      ${VOIDWATCH_WEBHOOK_SECRET}
Config file:         ${ODYSSEUS_DATA_DIR}/voidwatch.json
Policy file:         ${VOIDWATCH_POLICY_FILE}

Emergency disable:   curl -X POST http://localhost:${ODYSSEUS_PORT}/api/voidwatch/emergency-disable
Revoke token:        DELETE http://localhost:${VT_PORT}/api/integrations/tokens/<id>
EOF
  chmod 600 /root/voidwatch-recovery-info
  success "Voidwatch recovery info saved to /root/voidwatch-recovery-info"
}

# ─── Integrated AI setup (Ollama path) ───────────────────────────────────────
setup_ai_integrated() {
  [[ "$WITH_AI" != true ]] && return
  step "Setting Up Local AI Runtime"

  detect_gpu
  local recommended_model; recommended_model=$(recommend_ollama_model)

  if [[ "$AI_PROVIDER" == "ollama" ]]; then
    install_ollama

    # Determine model
    local model_to_use="${AI_MODEL:-$recommended_model}"

    if [[ -z "$model_to_use" ]]; then
      warn "System RAM < 8 GB — not recommending a model. Configure AI manually later."
      warn "  ollama pull phi3.5  (recommended for low RAM)"
    elif [[ "$PULL_MODEL" == true && "$SKIP_MODEL_PULL" != true ]]; then
      pull_ollama_model "$model_to_use" || true
    elif [[ "$YES_MODE" != true ]]; then
      echo
      echo -e "  Recommended model: ${GREEN}${model_to_use}${RESET} (RAM: $(( SYSTEM_RAM_MB / 1024 )) GB)"
      read -rp "  Pull this model now? (~$(( SYSTEM_RAM_MB / 1024 / 2 )) GB download) [y/N]: " _yn
      case "${_yn:-N}" in
        [Yy]*) pull_ollama_model "$model_to_use" || true ;;
        *) info "Model pull skipped. Pull later: ollama pull ${model_to_use}" ;;
      esac
    else
      info "Model pull skipped in --yes mode. Add --pull-model to pull automatically."
      info "  Pull later: ollama pull ${model_to_use}"
    fi

    # Wire Odysseus to Ollama
    if [[ "$WITH_ODYSSEUS" == true ]]; then
      configure_odysseus_for_ollama "$model_to_use"
    fi
    write_llm_config "http://localhost:${OLLAMA_PORT}/v1" "${model_to_use:-ollama}"
    AI_SETUP_DONE=true

  elif [[ "$AI_PROVIDER" == "openai-compatible" ]]; then
    echo
    read -rp "  OpenAI-compatible base URL (e.g. http://192.168.1.5:8080): " LLM_REMOTE_URL
    if [[ -n "$LLM_REMOTE_URL" ]]; then
      write_llm_config "${LLM_REMOTE_URL}/v1" "remote"
      AI_SETUP_DONE=true
      success "AI configured: ${LLM_REMOTE_URL}"
    fi
  fi
}

# ─── Domain / mDNS / nginx (existing) ────────────────────────────────────────
_write_domain_cfg() {
  printf 'VOIDTOWER_DOMAIN=%s\n' "$1" > "${VT_CONFIG_DIR}/domain.env"
  chmod 644 "${VT_CONFIG_DIR}/domain.env"
}

_setup_nginx_voidtower() {
  local domain="$1"
  _install_nginx || return 0
  local sites_dir="/etc/nginx/sites-enabled"
  [[ -d "$sites_dir" ]] || sites_dir="/etc/nginx/conf.d"
  cat > "${sites_dir}/voidtower.conf" <<NGXEOF
server {
    listen 80;
    server_name ${domain};
    location / {
        proxy_pass http://127.0.0.1:${VT_PORT};
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
        proxy_hide_header X-Frame-Options;
        add_header X-Frame-Options "ALLOWALL" always;
        add_header Content-Security-Policy "frame-ancestors *" always;
    }
}
NGXEOF
  nginx -t &>/dev/null && { nginx -s reload &>/dev/null || systemctl reload nginx &>/dev/null || true; success "nginx proxy configured for ${domain}"; }
}

_install_avahi() {
  command -v avahi-daemon &>/dev/null && return
  local pkg
  case "$PKG_MGR" in
    apt)    pkg="avahi-daemon libnss-mdns" ;;
    dnf)    pkg="avahi avahi-tools nss-mdns" ;;
    pacman) pkg="avahi nss-mdns" ;;
    zypper) pkg="avahi" ;;
    *)      warn "Cannot auto-install avahi"; return 1 ;;
  esac
  case "$PKG_MGR" in
    apt)    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $pkg ;;
    dnf)    dnf install -y -q $pkg ;;
    pacman) pacman -S --noconfirm --needed $pkg ;;
    zypper) zypper install -y -q $pkg ;;
  esac
}

_install_nginx() {
  command -v nginx &>/dev/null && return 0
  [[ "$INSTALL_NGINX" == "no" ]] && return 1
  if [[ "$INSTALL_NGINX" == "" && "$UNATTENDED" == false ]]; then
    read -rp "  nginx not installed. Install it? [Y/n]: " _yn
    case "${_yn:-Y}" in [Yy]*) INSTALL_NGINX="yes" ;; *) INSTALL_NGINX="no"; return 1 ;; esac
  fi
  case "$PKG_MGR" in
    apt)    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nginx ;;
    dnf)    dnf install -y -q nginx; systemctl enable nginx ;;
    pacman) pacman -S --noconfirm --needed nginx ;;
    zypper) zypper --non-interactive install -q nginx; systemctl enable nginx ;;
  esac
  systemctl enable --now nginx &>/dev/null || true
}

setup_domain() {
  [[ "$UNATTENDED" == true ]] && return
  local cur_host; cur_host=$(hostname -f 2>/dev/null || hostname)
  echo; echo -e "${BOLD}${CYAN}── Network & Discovery ──${RESET}"
  echo -e "  Hostname: ${CYAN}${cur_host}${RESET}"
  echo -e "  [1] localhost only  [2] mDNS (${cur_host}.local)  [3] Custom hostname + mDNS  [4] Public domain"
  local choice; read -rp "  Choice [1-4]: " choice
  case "${choice:-1}" in
    2) _install_avahi && { systemctl enable --now avahi-daemon &>/dev/null || true; VOIDTOWER_DOMAIN="${cur_host}.local"; MDNS_ENABLED=true; _write_domain_cfg "${VOIDTOWER_DOMAIN}"; success "mDNS: http://${VOIDTOWER_DOMAIN}"; } ;;
    3) read -rp "  New hostname: " new_host
       [[ "$new_host" =~ ^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}$ ]] && hostnamectl set-hostname "$new_host" 2>/dev/null || hostname "$new_host" 2>/dev/null || true
       _install_avahi && { VOIDTOWER_DOMAIN="${new_host}.local"; MDNS_ENABLED=true; _write_domain_cfg "$VOIDTOWER_DOMAIN"; } ;;
    4) read -rp "  Domain (e.g. vt.example.com): " pub_domain
       [[ "$pub_domain" =~ ^[a-zA-Z0-9*._-]+$ ]] && { VOIDTOWER_DOMAIN="$pub_domain"; _write_domain_cfg "$pub_domain"; _setup_nginx_voidtower "$pub_domain"; } ;;
  esac
}

# ─── Legacy llama.cpp AI setup (non-Ollama) ────────────────────────────────────
setup_ai_legacy() {
  [[ "$SKIP_AI" == true || "$WITH_AI" == true ]] && return

  detect_gpu
  select_model_tier
  echo; echo -e "${BOLD}${CYAN}── AI Setup (llama.cpp) ──${RESET}"
  [[ "$GPU_VENDOR" != "cpu" ]] && echo -e "  GPU: ${CYAN}${GPU_NAME}${RESET}" || echo -e "  CPU-only | RAM: ${CYAN}$(( SYSTEM_RAM_MB / 1024 )) GB${RESET}"
  echo -e "  Recommended: ${GREEN}${MODEL_NAME}${RESET} (${MODEL_SIZE})"

  [[ "$UNATTENDED" == true ]] && { info "Unattended: skipping legacy AI setup"; return; }

  echo -e "  [1] Download recommended  [2] Choose model  [3] Remote endpoint  [4] Skip"
  local choice; read -rp "  Choice [1-4]: " choice
  case "${choice:-4}" in
    1) download_llama_cpp && download_model && { install_llama_service; write_llm_config "http://127.0.0.1:8080/v1" "$MODEL_NAME"; AI_SETUP_DONE=true; } ;;
    2) echo -e "  [1] Qwen2.5 3B  [2] Mistral 7B  [3] Llama3.1 8B  [4] Qwen2.5 14B  [5] Llama3.3 70B"
       local mc; read -rp "  Model: " mc; _set_model "${mc:-2}"
       download_llama_cpp && download_model && { install_llama_service; write_llm_config "http://127.0.0.1:8080/v1" "$MODEL_NAME"; AI_SETUP_DONE=true; } ;;
    3) read -rp "  llama.cpp URL: " LLM_REMOTE_URL
       [[ -n "$LLM_REMOTE_URL" ]] && { write_llm_config "${LLM_REMOTE_URL}/v1" "remote"; AI_SETUP_DONE=true; } ;;
  esac
}

# ─── Bootstrap token ─────────────────────────────────────────────────────────
show_token() {
  local token_file="${VT_CONFIG_DIR}/bootstrap-token"
  [[ -f "$token_file" ]] || return 0
  local token; token=$(cat "$token_file")
  echo; echo -e "${BOLD}${YELLOW}── Bootstrap Token ──${RESET}"
  echo -e "  ${CYAN}${token}${RESET}"
  echo -e "  Use at: http://localhost:${VT_PORT}/bootstrap"
  echo -e "  Saved:  /root/voidtower-bootstrap-token"
  echo "${token}" > /root/voidtower-bootstrap-token
  chmod 600 /root/voidtower-bootstrap-token
}

# ─── Readiness check ──────────────────────────────────────────────────────────
run_doctor() {
  local label="$1" url="$2" ok fail
  if curl -fsSL --max-time 5 "$url" &>/dev/null; then
    echo -e "  ${GREEN}✓${RESET} ${label}"
    return 0
  else
    echo -e "  ${RED}✗${RESET} ${label}"
    return 1
  fi
}

run_readiness_check() {
  step "Readiness Check"
  local all_ok=true

  # VoidTower
  run_doctor "VoidTower service" "http://localhost:${VT_PORT}/api/health" || all_ok=false

  # Odysseus
  if [[ "$WITH_ODYSSEUS" == true ]]; then
    run_doctor "Odysseus service" "http://localhost:${ODYSSEUS_PORT}/api/health" || all_ok=false
  fi

  # Ollama
  if [[ "$WITH_AI" == true && "$AI_PROVIDER" == "ollama" ]]; then
    run_doctor "Ollama" "http://localhost:${OLLAMA_PORT}/api/tags" || all_ok=false
    if [[ -n "${AI_MODEL:-}" ]]; then
      if ollama list 2>/dev/null | grep -q "$AI_MODEL"; then
        echo -e "  ${GREEN}✓${RESET} Model available: ${AI_MODEL}"
      else
        echo -e "  ${YELLOW}!${RESET} Model not yet pulled: ${AI_MODEL}"
        echo -e "      Pull with: ollama pull ${AI_MODEL}"
      fi
    fi
  fi

  # Voidwatch
  if [[ "$WITH_VOIDWATCH" == true ]]; then
    if [[ -n "$VOIDWATCH_TOKEN" ]]; then
      local vw_resp
      vw_resp=$(curl -fsSL --max-time 5 \
        -H "Authorization: Bearer ${VOIDWATCH_TOKEN}" \
        "http://localhost:${VT_PORT}/api/auth/me" 2>/dev/null || echo "")
      if [[ -n "$vw_resp" && "$vw_resp" != *"error"* ]]; then
        echo -e "  ${GREEN}✓${RESET} Voidwatch token valid"
      else
        echo -e "  ${YELLOW}!${RESET} Voidwatch token not yet valid (complete VoidTower bootstrap first)"
        all_ok=false
      fi
    else
      echo -e "  ${YELLOW}!${RESET} Voidwatch token not created (complete VoidTower bootstrap first)"
    fi

    local policy_file="${VOIDWATCH_POLICY_FILE:-${ODYSSEUS_CONFIG_DIR}/voidwatch/policy.json}"
    if [[ -f "$policy_file" ]]; then
      echo -e "  ${GREEN}✓${RESET} Policy file exists"
    else
      echo -e "  ${YELLOW}!${RESET} Policy file missing: ${policy_file}"
    fi

    local toolpack_dir="${ODYSSEUS_INSTALL_DIR}/voidwatch/toolpacks"
    if [[ -d "$toolpack_dir" ]]; then
      local count; count=$(find "$toolpack_dir" -name "*.yml" | wc -l)
      echo -e "  ${GREEN}✓${RESET} Toolpacks: ${count} found"
    else
      echo -e "  ${YELLOW}!${RESET} Toolpack directory not found"
    fi
  fi

  echo
  if [[ "$all_ok" == true ]]; then
    success "All checks passed"
  else
    warn "Some checks failed — see above. Re-run: bash scripts/doctor.sh --with-odysseus"
  fi
}

# ─── Final summary ────────────────────────────────────────────────────────────
print_summary() {
  echo
  echo -e "${BOLD}${GREEN}══════════════════════════════════════════════════${RESET}"
  echo -e "${BOLD}${GREEN}  Installation Complete!${RESET}"
  echo -e "${BOLD}${GREEN}══════════════════════════════════════════════════${RESET}"
  echo
  echo -e "  ${BOLD}VoidTower${RESET}"
  echo -e "    URL:    ${CYAN}http://localhost:${VT_PORT}${RESET}"
  [[ -n "$VOIDTOWER_DOMAIN" ]] && echo -e "    Domain: ${CYAN}http://${VOIDTOWER_DOMAIN}${RESET}"
  echo -e "    Setup:  ${CYAN}http://localhost:${VT_PORT}/bootstrap${RESET}"
  echo -e "    Creds:  ${CYAN}/root/voidtower-bootstrap-token${RESET}"
  echo -e "    Logs:   ${CYAN}journalctl -u voidtower -f${RESET}"

  if [[ "$WITH_ODYSSEUS" == true ]]; then
    echo
    echo -e "  ${BOLD}Odysseus${RESET}"
    echo -e "    URL:    ${CYAN}http://localhost:${ODYSSEUS_PORT}${RESET}"
    echo -e "    Creds:  ${CYAN}/root/odysseus-bootstrap-token${RESET}"
    echo -e "    Logs:   ${CYAN}journalctl -u odysseus -f${RESET}"
  fi

  if [[ "$WITH_AI" == true ]]; then
    echo
    echo -e "  ${BOLD}AI${RESET}"
    if [[ "$AI_PROVIDER" == "ollama" ]]; then
      echo -e "    Provider: ${GREEN}Ollama${RESET} (http://localhost:${OLLAMA_PORT})"
      [[ -n "${AI_MODEL:-}" ]] && echo -e "    Model:    ${GREEN}${AI_MODEL}${RESET}" || echo -e "    Model:    ${YELLOW}not yet pulled${RESET}"
    else
      echo -e "    Provider: ${GREEN}${LLM_REMOTE_URL:-configured}${RESET}"
    fi
  fi

  if [[ "$WITH_VOIDWATCH" == true ]]; then
    echo
    echo -e "  ${BOLD}Voidwatch${RESET}"
    if [[ -n "$VOIDWATCH_TOKEN" ]]; then
      echo -e "    Status: ${GREEN}connected${RESET} (token created)"
    else
      echo -e "    Status: ${YELLOW}pending${RESET} — complete VoidTower bootstrap, then:"
      echo -e "            ${CYAN}bash scripts/install.sh --with-voidwatch --yes${RESET}"
    fi
    echo -e "    Policy: ${CYAN}${VOIDWATCH_POLICY_FILE}${RESET}"
    echo -e "    Info:   ${CYAN}/root/voidwatch-recovery-info${RESET}"
    echo
    echo -e "  ${BOLD}Emergency disable:${RESET}"
    echo -e "    ${CYAN}curl -X POST http://localhost:${ODYSSEUS_PORT}/api/voidwatch/emergency-disable${RESET}"
  fi

  echo
  echo -e "  ${BOLD}Next steps:${RESET}"
  echo -e "    1. Open VoidTower: ${CYAN}http://localhost:${VT_PORT}/bootstrap${RESET}"
  [[ "$WITH_ODYSSEUS" == true ]] && echo -e "    2. Open Odysseus:  ${CYAN}http://localhost:${ODYSSEUS_PORT}${RESET}"
  if [[ "$WITH_AI" == true && -z "${AI_MODEL:-}" ]]; then
    echo -e "    3. Pull a model:   ${CYAN}ollama pull qwen2.5-coder:7b-instruct${RESET}"
  fi
  if [[ "$WITH_VOIDWATCH" == true && -z "$VOIDWATCH_TOKEN" ]]; then
    echo -e "    4. After bootstrap, re-run: ${CYAN}bash scripts/install.sh --with-voidwatch --yes${RESET}"
  fi
  echo
}

# ─── Odysseus offer (interactive, non-integrated path) ────────────────────────
offer_odysseus() {
  [[ "$WITH_ODYSSEUS" == true ]] && return  # already handled
  command -v docker &>/dev/null || return 0
  echo; echo -e "  ${BOLD}Install Odysseus AI workspace?${RESET}"
  echo -e "  [1] Yes (Docker)  [2] Save custom workspace URL  [3] Skip"
  local choice; read -rp "  Choice [1-3]: " choice
  case "${choice:-3}" in
    1) local ODYSSEUS_DIR="${VT_INSTALL_DIR}/odysseus"
       git clone --depth 1 --branch "${ODYSSEUS_BRANCH}" "https://github.com/${ODYSSEUS_REPO}" "$ODYSSEUS_DIR" 2>/dev/null && \
         printf 'APP_PORT=7000\nAPP_BIND=127.0.0.1\nAUTH_ENABLED=true\nOLLAMA_BASE_URL=http://host.docker.internal:${OLLAMA_PORT}\n' > "${ODYSSEUS_DIR}/.env" && \
         docker compose -f "${ODYSSEUS_DIR}/docker-compose.yml" up -d 2>/dev/null && \
         success "Odysseus running at http://localhost:7000" || \
         warn "Odysseus Docker install failed. Install manually." ;;
    2) local ws_url; read -rp "  Workspace URL: " ws_url
       [[ -n "$ws_url" ]] && echo "VOIDTOWER_AI_WORKSPACE_URL=${ws_url}" >> "${VT_CONFIG_DIR}/llm.env" ;;
  esac
}

# ─── Uninstall ────────────────────────────────────────────────────────────────
cmd_uninstall() {
  step "Uninstall VoidTower"

  if [[ "$HAVE_SYSTEMD" == true ]]; then
    for unit in voidtower odysseus voidtower-llama; do
      systemctl stop    "${unit}.service" 2>/dev/null || true
      systemctl disable "${unit}.service" 2>/dev/null || true
    done
    for unit in voidwatch-configure; do
      systemctl stop    "${unit}.service" "${unit}.path" 2>/dev/null || true
      systemctl disable "${unit}.service" "${unit}.path" 2>/dev/null || true
    done
    systemctl daemon-reload
  fi

  # Service unit files
  rm -f "${SYSTEMD_DIR}/voidtower.service" \
        "${SYSTEMD_DIR}/odysseus.service" \
        "${SYSTEMD_DIR}/voidtower-llama.service" \
        "${SYSTEMD_DIR}/voidwatch-configure.service" \
        "${SYSTEMD_DIR}/voidwatch-configure.path"
  [[ "$HAVE_SYSTEMD" == true ]] && systemctl daemon-reload

  # Sudoers
  rm -f /etc/sudoers.d/voidtower-nginx /etc/sudoers.d/voidtower-odysseus

  # nginx proxy config
  rm -f /etc/nginx/conf.d/voidtower.conf \
        /etc/nginx/sites-enabled/voidtower \
        /etc/nginx/sites-available/voidtower 2>/dev/null || true
  command -v nginx &>/dev/null && nginx -t &>/dev/null && nginx -s reload &>/dev/null || true

  # Root credential / recovery files
  rm -f /root/voidtower-bootstrap-token \
        /root/odysseus-bootstrap-token \
        /root/voidwatch-recovery-info

  # App catalog
  rm -rf /usr/share/voidtower

  # VoidTower install dir (binary + frontend dist + llama.cpp)
  rm -rf "${VT_INSTALL_DIR}"
  success "Removed ${VT_INSTALL_DIR}"

  local ans
  if [[ "$UNATTENDED" == false ]]; then
    read -rp "  Remove data directory ${VT_DATA_DIR}? [Y/n]: " ans
  else
    ans="y"
  fi
  if [[ "${ans,,}" != "n" ]]; then
    rm -rf "$VT_DATA_DIR"; success "Removed $VT_DATA_DIR"
  else
    info "Keeping $VT_DATA_DIR"
  fi

  if [[ "$UNATTENDED" == false ]]; then
    read -rp "  Remove config directory ${VT_CONFIG_DIR}? [Y/n]: " ans
  else
    ans="y"
  fi
  if [[ "${ans,,}" != "n" ]]; then
    rm -rf "$VT_CONFIG_DIR"; success "Removed $VT_CONFIG_DIR"
  else
    info "Keeping $VT_CONFIG_DIR"
  fi

  if [[ -d "$ODYSSEUS_INSTALL_DIR" ]]; then
    if [[ "$UNATTENDED" == false ]]; then
      read -rp "  Remove Odysseus install directory ${ODYSSEUS_INSTALL_DIR}? [Y/n]: " ans
    else
      ans="y"
    fi
    if [[ "${ans,,}" != "n" ]]; then
      rm -rf "$ODYSSEUS_INSTALL_DIR"; success "Removed $ODYSSEUS_INSTALL_DIR"
    else
      info "Keeping $ODYSSEUS_INSTALL_DIR"
    fi
  fi

  if [[ "$UNATTENDED" == false ]]; then
    read -rp "  Remove system users '${VT_USER}' and '${ODYSSEUS_USER}'? [y/N]: " ans
  else
    ans="n"
  fi
  if [[ "${ans,,}" == "y" ]]; then
    userdel "$VT_USER"       2>/dev/null && success "Removed user $VT_USER"       || true
    userdel "$ODYSSEUS_USER" 2>/dev/null && success "Removed user $ODYSSEUS_USER" || true
  fi

  success "VoidTower uninstalled. Run the installer again for a clean install."
}

# ─── Reset ────────────────────────────────────────────────────────────────────
cmd_reset() {
  step "Reset VoidTower State"
  [[ "$HAVE_SYSTEMD" == true ]] && { systemctl stop voidtower.service 2>/dev/null || true; }

  local ans wipe_db=false wipe_envs=false wipe_secrets=false wipe_token=false wipe_apps=false

  if [[ "$UNATTENDED" == true ]]; then
    wipe_db=true; wipe_envs=true; wipe_secrets=true; wipe_token=true; wipe_apps=true
  else
    echo
    read -rp "  Wipe database (${VT_DATA_DIR}/voidtower.db)? [y/N]: " ans
    [[ "${ans,,}" == "y" ]] && wipe_db=true
    read -rp "  Wipe config env files (${VT_CONFIG_DIR}/*.env, *.json)? [y/N]: " ans
    [[ "${ans,,}" == "y" ]] && wipe_envs=true
    read -rp "  Wipe secrets encryption key (${VT_CONFIG_DIR}/secrets.key)? [y/N]: " ans
    [[ "${ans,,}" == "y" ]] && wipe_secrets=true
    read -rp "  Remove bootstrap token — generates a fresh one on restart? [Y/n]: " ans
    [[ "${ans,,}" != "n" ]] && wipe_token=true
    read -rp "  Remove deployed apps data (${VT_DATA_DIR}/apps)? [y/N]: " ans
    [[ "${ans,,}" == "y" ]] && wipe_apps=true
  fi

  $wipe_db      && rm -f  "${VT_DATA_DIR}/voidtower.db"              && success "Wiped database"        || true
  $wipe_envs    && rm -f  "${VT_CONFIG_DIR}"/*.env "${VT_CONFIG_DIR}"/*.json 2>/dev/null \
                && success "Wiped config env files"                                                      || true
  $wipe_secrets && rm -f  "${VT_CONFIG_DIR}/secrets.key"              && success "Wiped secrets key"     || true
  $wipe_token   && rm -f  "${VT_CONFIG_DIR}/bootstrap-token"          && success "Removed bootstrap token" || true
  $wipe_apps    && rm -rf "${VT_DATA_DIR}/apps"                       && success "Wiped deployed apps"   || true

  if [[ "$HAVE_SYSTEMD" == true ]]; then
    systemctl start voidtower.service 2>/dev/null \
      && success "VoidTower restarted" \
      || warn "Failed to restart — check: journalctl -u voidtower -e"
    # Wait up to 15 s for VoidTower to write the new bootstrap token
    local _t=0
    until [[ -f "${VT_CONFIG_DIR}/bootstrap-token" || $_t -ge 15 ]]; do
      sleep 1; ((_t++)) || true
    done
  fi
  show_token
}

# ─── Repair ───────────────────────────────────────────────────────────────────
cmd_repair() {
  step "Repair VoidTower"
  [[ "$HAVE_SYSTEMD" == true ]] && { systemctl stop voidtower.service 2>/dev/null || true; }

  if ! download_binary 2>/dev/null; then
    warn "Pre-built binary not found, building from source"
    build_from_source
  fi

  install_catalog
  install_service

  chown -R "${VT_USER}:${VT_GROUP}" "$VT_DATA_DIR" "$VT_CONFIG_DIR" "$VT_INSTALL_DIR" 2>/dev/null || true
  chmod 750 "$VT_INSTALL_DIR"
  chmod 700 "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  [[ -f "${VT_CONFIG_DIR}/secrets.key"     ]] && chmod 600 "${VT_CONFIG_DIR}/secrets.key"
  [[ -f "${VT_CONFIG_DIR}/bootstrap-token" ]] && chmod 600 "${VT_CONFIG_DIR}/bootstrap-token"

  [[ "$HAVE_SYSTEMD" == true ]] && {
    systemctl daemon-reload
    systemctl restart voidtower.service
    sleep 2
    systemctl is-active --quiet voidtower.service \
      && success "VoidTower running" \
      || warn "VoidTower did not start — check: journalctl -u voidtower -e"
  }
  success "Repair complete."
}

# ─── Update ───────────────────────────────────────────────────────────────────
cmd_update() {
  step "Update VoidTower"

  local current_ver=""
  [[ -x "${VT_INSTALL_DIR}/${BINARY_NAME}" ]] && \
    current_ver=$("${VT_INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null | awk '{print $NF}' || echo "unknown")
  info "Current: ${current_ver:-unknown}  →  Target: ${VT_VERSION}"

  [[ "$HAVE_SYSTEMD" == true ]] && { systemctl stop voidtower.service 2>/dev/null || true; }

  if ! download_binary 2>/dev/null; then
    warn "Pre-built binary not found, building from source"
    build_from_source
  fi

  install_catalog

  [[ "$HAVE_SYSTEMD" == true ]] && {
    systemctl daemon-reload
    systemctl restart voidtower.service
    sleep 2
    systemctl is-active --quiet voidtower.service \
      && success "VoidTower running" \
      || warn "VoidTower did not start — check: journalctl -u voidtower -e"
  }

  local new_ver=""
  [[ -x "${VT_INSTALL_DIR}/${BINARY_NAME}" ]] && \
    new_ver=$("${VT_INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null | awk '{print $NF}' || echo "unknown")
  success "Updated: ${current_ver:-?} → ${new_ver:-?}"
}

# ─── Selective wipe (used by prompt_reinstall) ────────────────────────────────
_selective_wipe() {
  echo -e "\n  ${BOLD}Select components to wipe before reinstall:${RESET}"
  local ans

  read -rp "  Wipe database (${VT_DATA_DIR}/voidtower.db)? [y/N]: " ans </dev/tty
  [[ "${ans,,}" == "y" ]] && rm -f "${VT_DATA_DIR}/voidtower.db" && success "Wiped database" || true

  read -rp "  Wipe config env files (${VT_CONFIG_DIR}/*.env)? [y/N]: " ans </dev/tty
  [[ "${ans,,}" == "y" ]] && rm -f "${VT_CONFIG_DIR}"/*.env 2>/dev/null && success "Wiped config env files" || true

  read -rp "  Wipe secrets key (${VT_CONFIG_DIR}/secrets.key)? [y/N]: " ans </dev/tty
  [[ "${ans,,}" == "y" ]] && rm -f "${VT_CONFIG_DIR}/secrets.key" && success "Wiped secrets key" || true

  read -rp "  Remove bootstrap token (${VT_CONFIG_DIR}/bootstrap-token)? [y/N]: " ans </dev/tty
  [[ "${ans,,}" == "y" ]] && rm -f "${VT_CONFIG_DIR}/bootstrap-token" && success "Removed bootstrap token" || true

  read -rp "  Remove deployed apps data (${VT_DATA_DIR}/apps)? [y/N]: " ans </dev/tty
  [[ "${ans,,}" == "y" ]] && rm -rf "${VT_DATA_DIR}/apps" && success "Wiped deployed apps" || true

  if [[ -d "$ODYSSEUS_INSTALL_DIR" ]]; then
    read -rp "  Remove Odysseus install (${ODYSSEUS_INSTALL_DIR})? [y/N]: " ans </dev/tty
    if [[ "${ans,,}" == "y" ]]; then
      [[ "$HAVE_SYSTEMD" == true ]] && {
        systemctl stop    odysseus.service 2>/dev/null || true
        systemctl disable odysseus.service 2>/dev/null || true
      }
      rm -rf "$ODYSSEUS_INSTALL_DIR"
      rm -f "${SYSTEMD_DIR}/odysseus.service"
      [[ "$HAVE_SYSTEMD" == true ]] && systemctl daemon-reload
      success "Removed Odysseus"
    fi
  fi
  echo
}

# ─── Interactive reinstall prompt ─────────────────────────────────────────────
prompt_reinstall() {
  [[ -x "${VT_INSTALL_DIR}/${BINARY_NAME}" ]] || return 0
  [[ "$UNATTENDED" == true ]] && return 0

  local existing_ver=""
  existing_ver=$("${VT_INSTALL_DIR}/${BINARY_NAME}" --version 2>/dev/null | awk '{print $NF}' || echo "unknown")
  echo
  warn "Existing VoidTower install detected (${existing_ver:-?}) at ${VT_INSTALL_DIR}"
  echo -e "  ${BOLD}How would you like to proceed?${RESET}"
  echo -e "  [1] Upgrade / overwrite binary only (keep all data)"
  echo -e "  [2] Full reinstall — choose what to wipe"
  echo -e "  [3] Repair (fix permissions + reinstall service unit)"
  echo -e "  [4] Abort"
  echo
  local choice
  read -rp "  Choice [1-4]: " choice </dev/tty
  case "${choice:-1}" in
    1) info "Upgrading binary only — data and config preserved." ;;
    2) _selective_wipe ;;
    3) cmd_repair; exit 0 ;;
    4) info "Aborted."; exit 0 ;;
    *) info "Upgrading binary only — data and config preserved." ;;
  esac
}

# ─── Main ─────────────────────────────────────────────────────────────────────
main() {
  # Ensure we have a valid working directory — the caller's cwd may not exist
  # (e.g. curl|bash started from a deleted directory), which corrupts subprocesses.
  cd / 2>/dev/null || true

  # When piped via curl | bash, stdin is the pipe. By the time main() runs
  # bash has the full script in memory, so we can safely reopen /dev/tty for
  # interactive prompts. Fall back to unattended mode if no terminal exists.
  if [[ ! -t 0 ]]; then
    exec 0</dev/tty 2>/dev/null || UNATTENDED=true
  fi

  echo
  echo -e "${BOLD}${CYAN}▓▓▒░ VoidTower Installer ░▒▓▓${RESET}"
  [[ "$WITH_ODYSSEUS" == true ]] && echo -e "     ${CYAN}+ Odysseus AI Workspace${RESET}"
  [[ "$WITH_VOIDWATCH" == true ]] && echo -e "     ${CYAN}+ Voidwatch Integration${RESET}"
  [[ "$WITH_AI" == true ]] && echo -e "     ${CYAN}+ Local AI (${AI_PROVIDER})${RESET}"
  [[ "$DRY_RUN" == true ]] && echo -e "     ${YELLOW}[DRY-RUN MODE]${RESET}"
  echo

  detect_os
  detect_arch

  # Dispatch maintenance modes — these exit when done
  case "$INSTALL_MODE" in
    uninstall) cmd_uninstall; exit 0 ;;
    reset)     cmd_reset;     exit 0 ;;
    repair)    cmd_repair;    exit 0 ;;
    update)    cmd_update;    exit 0 ;;
  esac

  if [[ "$DRY_RUN" == true ]]; then
    info "[DRY-RUN] Would install VoidTower to ${VT_INSTALL_DIR}, port ${VT_PORT}"
    [[ "$WITH_ODYSSEUS" == true ]] && info "[DRY-RUN] Would install Odysseus to ${ODYSSEUS_INSTALL_DIR}, port ${ODYSSEUS_PORT}"
    [[ "$WITH_AI" == true ]] && info "[DRY-RUN] Would install ${AI_PROVIDER} AI runtime"
    [[ "$WITH_VOIDWATCH" == true ]] && info "[DRY-RUN] Would configure Voidwatch integration"
    exit 0
  fi

  prompt_reinstall

  install_deps
  setup_system

  if ! download_binary 2>/dev/null; then
    warn "Pre-built binary not found, falling back to source build"
    build_from_source
  fi

  install_catalog
  install_service
  setup_domain

  # Integrated AI (Ollama) — runs before Odysseus so it's ready when Odysseus starts
  [[ "$WITH_AI" == true ]] && setup_ai_integrated

  # Odysseus
  [[ "$WITH_ODYSSEUS" == true ]] && install_odysseus

  # Legacy llama.cpp AI (only when not using integrated path)
  setup_ai_legacy

  # Offer Odysseus interactively (non-integrated path, AI done, Docker available)
  [[ "$AI_SETUP_DONE" == true && "$WITH_ODYSSEUS" != true ]] && offer_odysseus

  # Generate bootstrap token (token is created on first run and stored in config_dir)
  info "Generating bootstrap token…"
  sudo -u "$VT_USER" env \
    VOIDTOWER_DATA_DIR="$VT_DATA_DIR" \
    VOIDTOWER_CONFIG_DIR="$VT_CONFIG_DIR" \
    "${VT_INSTALL_DIR}/${BINARY_NAME}" --show-token 2>/dev/null || true

  # Start services
  if [[ "$HAVE_SYSTEMD" == true && "$SKIP_SYSTEMD" != true ]]; then
    info "Starting VoidTower…"
    systemctl restart voidtower.service
    sleep 2
    systemctl is-active --quiet voidtower.service && success "VoidTower running" || \
      warn "VoidTower did not start cleanly — check: journalctl -u voidtower -e"

    if [[ "$WITH_ODYSSEUS" == true ]]; then
      info "Starting Odysseus…"
      systemctl restart odysseus.service
      sleep 3
      systemctl is-active --quiet odysseus.service && success "Odysseus running" || \
        warn "Odysseus did not start cleanly — check: journalctl -u odysseus -e"
    fi
  fi

  show_token

  # Voidwatch is auto-wired by voidwatch-configure.service after bootstrap —
  # no manual configure_voidwatch call needed here.

  run_readiness_check
  print_summary
}

main "$@"
