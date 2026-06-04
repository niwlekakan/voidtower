#!/usr/bin/env bash
# VoidTower installer
# Supports: Debian/Ubuntu, Fedora/RHEL/CentOS, Arch Linux, openSUSE
set -euo pipefail

# ─── Colours ────────────────────────────────────────────────────────────────
RED='\033[0;31m'; YELLOW='\033[1;33m'; GREEN='\033[0;32m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}[INFO]${RESET}  $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}  $*"; }
die()     { echo -e "${RED}[ERROR]${RESET} $*" >&2; exit 1; }

# ─── Defaults ───────────────────────────────────────────────────────────────
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
REPO="elwla/voidtower"

UNATTENDED=false
SKIP_SYSTEMD=false
NO_TLS=false
SKIP_AI=false
INSTALL_NGINX=""  # "yes"|"no"|"" = prompt
AI_SETUP_DONE=false
LLM_REMOTE_URL=""
MODEL_PATH=""
VOIDTOWER_DOMAIN=""
MDNS_ENABLED=false

# ─── Argument parsing ────────────────────────────────────────────────────────
usage() {
  cat <<EOF
Usage: install.sh [OPTIONS]

Options:
  --unattended       Non-interactive install with defaults
  --port PORT        Port to listen on (default: 8743)
  --bind ADDR        Bind address (default: 127.0.0.1)
  --install-dir DIR  Installation directory (default: /opt/voidtower)
  --data-dir DIR     Data directory (default: /var/lib/voidtower)
  --no-tls           Disable TLS (use behind a reverse proxy)
  --skip-systemd     Do not install systemd service
  --skip-ai          Skip AI/llama.cpp setup entirely
  --version VER      Install specific version (default: latest)
  --help             Show this help
EOF
  exit 0
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --unattended)  UNATTENDED=true ;;
    --port)        VT_PORT="$2"; shift ;;
    --bind)        VT_BIND="$2"; shift ;;
    --install-dir) VT_INSTALL_DIR="$2"; shift ;;
    --data-dir)    VT_DATA_DIR="$2"; shift ;;
    --no-tls)      NO_TLS=true ;;
    --skip-systemd) SKIP_SYSTEMD=true ;;
    --skip-ai)     SKIP_AI=true ;;
    --with-nginx)  INSTALL_NGINX="yes" ;;
    --skip-nginx)  INSTALL_NGINX="no" ;;
    --version)     VT_VERSION="$2"; shift ;;
    --help|-h)     usage ;;
    *) die "Unknown option: $1" ;;
  esac
  shift
done

# ─── Root check ──────────────────────────────────────────────────────────────
[[ $EUID -eq 0 ]] || die "This installer must be run as root. Try: sudo bash install.sh"

# ─── OS detection ────────────────────────────────────────────────────────────
detect_os() {
  if [[ -f /etc/os-release ]]; then
    # shellcheck source=/dev/null
    source /etc/os-release
    OS_ID="${ID:-unknown}"
    OS_ID_LIKE="${ID_LIKE:-}"
  else
    die "Cannot detect OS: /etc/os-release not found"
  fi

  case "$OS_ID" in
    ubuntu|debian|linuxmint|pop)  PKG_MGR="apt" ;;
    fedora)                        PKG_MGR="dnf" ;;
    rhel|centos|rocky|almalinux)   PKG_MGR="dnf" ;;
    arch|manjaro|endeavouros)      PKG_MGR="pacman" ;;
    opensuse*|sles)                PKG_MGR="zypper" ;;
    *)
      if [[ "$OS_ID_LIKE" == *debian* ]]; then PKG_MGR="apt"
      elif [[ "$OS_ID_LIKE" == *rhel* || "$OS_ID_LIKE" == *fedora* ]]; then PKG_MGR="dnf"
      elif [[ "$OS_ID_LIKE" == *arch* ]]; then PKG_MGR="pacman"
      else die "Unsupported OS: $OS_ID. Supported: Debian/Ubuntu, Fedora/RHEL, Arch, openSUSE"
      fi
      ;;
  esac
  info "Detected OS: ${PRETTY_NAME:-$OS_ID} (package manager: $PKG_MGR)"
}

# ─── Arch detection ──────────────────────────────────────────────────────────
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
  local CURL_PKG="curl"
  local TAR_PKG="tar"

  info "Installing dependencies…"
  case "$PKG_MGR" in
    apt)
      DEBIAN_FRONTEND=noninteractive apt-get update -qq
      DEBIAN_FRONTEND=noninteractive apt-get install -y -qq curl tar ca-certificates unzip pciutils
      ;;
    dnf)
      dnf install -y -q curl tar ca-certificates unzip pciutils
      ;;
    pacman)
      pacman -Sy --noconfirm --needed curl tar ca-certificates unzip pciutils
      ;;
    zypper)
      zypper --non-interactive install -q curl tar ca-certificates unzip pciutils
      ;;
  esac
}

# ─── Binary download ─────────────────────────────────────────────────────────
download_binary() {
  local download_url

  if [[ "$VT_VERSION" == "latest" ]]; then
    info "Fetching latest release info…"
    VT_VERSION=$(curl -fsSL --max-time 15 "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | sed 's/.*"tag_name": *"v\([^"]*\)".*/\1/')
    [[ -n "$VT_VERSION" ]] || return 1
  fi

  local archive="voidtower-${VT_VERSION}-${ARCH}-unknown-linux-musl.tar.gz"
  download_url="https://github.com/${REPO}/releases/download/v${VT_VERSION}/${archive}"

  info "Downloading VoidTower v${VT_VERSION} for ${ARCH}…"
  local tmp_dir
  tmp_dir=$(mktemp -d)
  trap "rm -rf $tmp_dir" EXIT

  curl -fsSL --max-time 120 --progress-bar "$download_url" -o "$tmp_dir/$archive" || \
    return 1

  tar -xzf "$tmp_dir/$archive" -C "$tmp_dir"
  install -m 755 "$tmp_dir/${BINARY_NAME}" "${VT_INSTALL_DIR}/${BINARY_NAME}"
  echo "${VT_VERSION}" > "${VT_INSTALL_DIR}/.version"
  success "Binary installed to ${VT_INSTALL_DIR}/${BINARY_NAME}"
}

# ─── Build from source (fallback) ────────────────────────────────────────────
build_from_source() {
  info "No pre-built binary available. Attempting build from source…"
  command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust: https://rustup.rs"
  command -v npm   >/dev/null 2>&1 || die "npm not found. Install Node.js: https://nodejs.org"

  local SRC _cloned=false
  # When run via curl|bash, BASH_SOURCE[0] is empty or /dev/stdin and
  # doesn't point to the repo — clone instead.
  local _script_dir
  _script_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-/dev/stdin}")" 2>/dev/null && pwd || echo "")"
  if [[ -f "${_script_dir}/../backend/Cargo.toml" ]]; then
    SRC="$(cd "${_script_dir}/.." && pwd)"
  else
    info "Cloning VoidTower source (curl|bash install)…"
    SRC=$(mktemp -d)
    git clone --depth 1 "https://github.com/${REPO}.git" "$SRC" \
      || die "Failed to clone source from github.com/${REPO}"
    _cloned=true
  fi

  info "Building frontend…"
  (cd "$SRC/frontend" && npm ci --silent && npm run build --silent)

  info "Building backend…"
  (cd "$SRC/backend" && cargo build --release --quiet)

  install -m 755 "$SRC/backend/target/release/${BINARY_NAME}" "${VT_INSTALL_DIR}/${BINARY_NAME}"
  cp -r "$SRC/frontend/dist" "${VT_INSTALL_DIR}/frontend"
  git -C "$SRC" describe --tags --always 2>/dev/null > "${VT_INSTALL_DIR}/.version" || true
  [[ "$_cloned" == true ]] && rm -rf "$SRC"
  success "Built and installed from source"
}

# ─── Directory and user setup ─────────────────────────────────────────────────
setup_system() {
  info "Creating directories…"
  mkdir -p "$VT_INSTALL_DIR" "$VT_DATA_DIR" "$VT_CONFIG_DIR"

  if ! id "$VT_USER" &>/dev/null; then
    info "Creating system user ${VT_USER}…"
    useradd --system --no-create-home --shell /usr/sbin/nologin \
      --home-dir "$VT_DATA_DIR" "$VT_USER"
  fi

  chown -R "${VT_USER}:${VT_GROUP}" "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  chmod 750 "$VT_DATA_DIR" "$VT_CONFIG_DIR"
  success "System user and directories ready"
}

# ─── Systemd service ──────────────────────────────────────────────────────────
install_service() {
  [[ "$SKIP_SYSTEMD" == true ]] && return
  command -v systemctl >/dev/null 2>&1 || { warn "systemd not found, skipping service install"; return; }

  local EXTRA_FLAGS=""
  [[ "$NO_TLS" == true ]] && EXTRA_FLAGS=" --no-tls"

  info "Installing systemd service…"
  cat > "${SYSTEMD_DIR}/voidtower.service" <<EOF
[Unit]
Description=VoidTower Infrastructure Management
After=network.target
Wants=network.target

[Service]
Type=simple
User=${VT_USER}
Group=${VT_GROUP}
ExecStart=${VT_INSTALL_DIR}/${BINARY_NAME} --bind ${VT_BIND} --port ${VT_PORT} --data-dir ${VT_DATA_DIR} --config-dir ${VT_CONFIG_DIR}${EXTRA_FLAGS}
Restart=on-failure
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=voidtower
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${VT_DATA_DIR} ${VT_CONFIG_DIR}
PrivateTmp=true
CapabilityBoundingSet=
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable voidtower.service
  success "Systemd service installed and enabled"
}

# ─── GPU / hardware detection ────────────────────────────────────────────────
detect_gpu() {
  GPU_VENDOR="cpu"
  GPU_VRAM_MB=0
  GPU_NAME="CPU"

  if command -v nvidia-smi &>/dev/null && nvidia-smi &>/dev/null 2>&1; then
    GPU_VENDOR="nvidia"
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -1 | sed 's/^ *//')
    GPU_VRAM_MB=$(nvidia-smi --query-gpu=memory.total --format=csv,noheader,nounits 2>/dev/null \
                  | head -1 | tr -d ' ')
  elif command -v rocm-smi &>/dev/null 2>&1; then
    GPU_VENDOR="amd"
    GPU_NAME=$(rocm-smi --showproductname 2>/dev/null \
               | grep -i "card\|gpu" | head -1 | sed 's/.*: //' | sed 's/^ *//')
    GPU_VRAM_MB=$(rocm-smi --showmeminfo vram 2>/dev/null \
                  | awk '/Total Memory/ {printf "%d", $NF/1024/1024; exit}')
  elif lspci 2>/dev/null | grep -qi "nvidia"; then
    GPU_VENDOR="nvidia"
    GPU_NAME=$(lspci 2>/dev/null | grep -i nvidia | head -1 | sed 's/.*: //')
    GPU_VRAM_MB=0  # Can't query without driver; user can choose manually
  elif lspci 2>/dev/null | grep -qi "amd\|radeon"; then
    GPU_VENDOR="amd"
    GPU_NAME=$(lspci 2>/dev/null | grep -i "amd\|radeon" | grep -i "vga\|3d\|display" | head -1 | sed 's/.*: //')
    GPU_VRAM_MB=0
  fi

  # Ensure numeric
  GPU_VRAM_MB=$(( ${GPU_VRAM_MB:-0} + 0 ))
  SYSTEM_RAM_MB=$(awk '/MemTotal/ {print int($2/1024)}' /proc/meminfo 2>/dev/null || echo 0)
}

# ─── Model tier selection ─────────────────────────────────────────────────────
select_model_tier() {
  # Buckets: pick largest model that comfortably fits in VRAM (leaving ~15% headroom)
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
    1) MODEL_NAME="Qwen 2.5 3B Q4_K_M (CPU)";      MODEL_SIZE="~1.9 GB"
       MODEL_FILE="Qwen2.5-3B-Instruct-Q4_K_M.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-3B-Instruct-GGUF/resolve/main/Qwen2.5-3B-Instruct-Q4_K_M.gguf" ;;
    2) MODEL_NAME="Mistral 7B v0.2 Q4_K_M";        MODEL_SIZE="~4.1 GB"
       MODEL_FILE="mistral-7b-instruct-v0.2.Q4_K_M.gguf"
       MODEL_URL="https://huggingface.co/TheBloke/Mistral-7B-Instruct-v0.2-GGUF/resolve/main/mistral-7b-instruct-v0.2.Q4_K_M.gguf" ;;
    3) MODEL_NAME="Llama 3.1 8B Q8_0";             MODEL_SIZE="~8.5 GB"
       MODEL_FILE="Meta-Llama-3.1-8B-Instruct-Q8_0.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Meta-Llama-3.1-8B-Instruct-GGUF/resolve/main/Meta-Llama-3.1-8B-Instruct-Q8_0.gguf" ;;
    4) MODEL_NAME="Qwen 2.5 14B Q6_K";             MODEL_SIZE="~11 GB"
       MODEL_FILE="Qwen2.5-14B-Instruct-Q6_K.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Qwen2.5-14B-Instruct-GGUF/resolve/main/Qwen2.5-14B-Instruct-Q6_K.gguf" ;;
    5) MODEL_NAME="Llama 3.3 70B Q4_K_M";          MODEL_SIZE="~40 GB"
       MODEL_FILE="Llama-3.3-70B-Instruct-Q4_K_M.gguf"
       MODEL_URL="https://huggingface.co/bartowski/Llama-3.3-70B-Instruct-GGUF/resolve/main/Llama-3.3-70B-Instruct-Q4_K_M.gguf" ;;
  esac
}

# ─── llama.cpp binary download ───────────────────────────────────────────────
download_llama_cpp() {
  local LLAMA_DIR="${VT_INSTALL_DIR}/llama.cpp"
  local LLAMA_API="https://api.github.com/repos/ggml-org/llama.cpp/releases/latest"

  info "Fetching latest llama.cpp release info…"
  local LLAMA_TAG
  LLAMA_TAG=$(curl -fsSL "$LLAMA_API" | grep '"tag_name"' \
              | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
  [[ -n "$LLAMA_TAG" ]] || { warn "Could not fetch llama.cpp release info"; return 1; }

  local ASSET
  case "$GPU_VENDOR" in
    nvidia) ASSET="llama-${LLAMA_TAG}-bin-ubuntu-cuda-cu12.4-x64.zip" ;;
    amd)    ASSET="llama-${LLAMA_TAG}-bin-ubuntu-vulkan-x64.zip" ;;
    *)      ASSET="llama-${LLAMA_TAG}-bin-ubuntu-x64.zip" ;;
  esac
  local URL="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_TAG}/${ASSET}"

  mkdir -p "$LLAMA_DIR"
  local tmp; tmp=$(mktemp -d)

  info "Downloading llama.cpp ${LLAMA_TAG} (${GPU_VENDOR} build)…"
  if ! curl -fL --progress-bar "$URL" -o "$tmp/llama.zip"; then
    # Fallback: generic CPU build if GPU-specific asset not found
    warn "GPU build not found, falling back to CPU build…"
    ASSET="llama-${LLAMA_TAG}-bin-ubuntu-x64.zip"
    URL="https://github.com/ggml-org/llama.cpp/releases/download/${LLAMA_TAG}/${ASSET}"
    curl -fL --progress-bar "$URL" -o "$tmp/llama.zip" || { rm -rf "$tmp"; return 1; }
  fi

  unzip -q "$tmp/llama.zip" -d "$tmp/extracted"
  local server_bin
  server_bin=$(find "$tmp/extracted" -name "llama-server" -type f | head -1)
  if [[ -z "$server_bin" ]]; then
    # Older releases used "server" binary name
    server_bin=$(find "$tmp/extracted" -name "server" -type f | head -1)
  fi
  [[ -n "$server_bin" ]] || { warn "llama-server binary not found in archive"; rm -rf "$tmp"; return 1; }

  install -m 755 "$server_bin" "${LLAMA_DIR}/llama-server"
  # Copy shared libs if present
  find "$tmp/extracted" -name "*.so*" -exec cp -n {} "$LLAMA_DIR/" \; 2>/dev/null || true

  rm -rf "$tmp"
  chown -R "${VT_USER}:${VT_GROUP}" "$LLAMA_DIR"
  success "llama-server installed to ${LLAMA_DIR}/llama-server"
}

# ─── GGUF model download ──────────────────────────────────────────────────────
download_model() {
  local MODELS_DIR="${VT_DATA_DIR}/models"
  mkdir -p "$MODELS_DIR"

  info "Downloading ${MODEL_NAME} (${MODEL_SIZE})…"
  warn "Large download — time depends on your connection speed."

  if curl -fL --progress-bar "$MODEL_URL" -o "${MODELS_DIR}/${MODEL_FILE}"; then
    ln -sf "${MODELS_DIR}/${MODEL_FILE}" "${MODELS_DIR}/default.gguf"
    chown -R "${VT_USER}:${VT_GROUP}" "$MODELS_DIR"
    MODEL_PATH="${MODELS_DIR}/${MODEL_FILE}"
    success "Model saved to ${MODEL_PATH}"
  else
    warn "Model download failed. AI features will not be active until a model is loaded."
    rm -rf "$tmp" 2>/dev/null || true
    return 1
  fi
}

# ─── llama-server systemd service ────────────────────────────────────────────
install_llama_service() {
  [[ "$SKIP_SYSTEMD" == true ]] && return
  command -v systemctl >/dev/null 2>&1 || return
  [[ -f "${VT_INSTALL_DIR}/llama.cpp/llama-server" ]] || return
  [[ -n "$MODEL_PATH" ]] || return

  local N_GPU_LAYERS=0
  [[ "$GPU_VENDOR" == "nvidia" || "$GPU_VENDOR" == "amd" ]] && N_GPU_LAYERS=99

  local N_THREADS
  N_THREADS=$(nproc 2>/dev/null || echo 4)

  info "Installing llama-server systemd service…"
  cat > "${SYSTEMD_DIR}/voidtower-llama.service" <<EOF
[Unit]
Description=VoidTower AI (llama-server)
After=network.target
Wants=network.target

[Service]
Type=simple
User=${VT_USER}
Group=${VT_GROUP}
Environment=LD_LIBRARY_PATH=${VT_INSTALL_DIR}/llama.cpp
ExecStart=${VT_INSTALL_DIR}/llama.cpp/llama-server \\
  --model ${MODEL_PATH} \\
  --host 127.0.0.1 \\
  --port 8080 \\
  --ctx-size 4096 \\
  --n-gpu-layers ${N_GPU_LAYERS} \\
  --threads ${N_THREADS} \\
  --log-disable
Restart=on-failure
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=voidtower-llama
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
EOF

  systemctl daemon-reload
  systemctl enable voidtower-llama.service
  success "llama-server service installed and enabled (port 8080)"
}

# ─── Write LLM endpoint to VoidTower config ──────────────────────────────────
write_llm_config() {
  local endpoint="$1"
  local model_label="${2:-}"
  local cfg="${VT_CONFIG_DIR}/llm.env"
  cat > "$cfg" <<EOF
# Set by installer — edit via Settings → Integrations → AI
VOIDTOWER_LLM_ENDPOINT=${endpoint}
VOIDTOWER_LLM_MODEL=${model_label}
EOF
  chmod 640 "$cfg"
  chown "${VT_USER}:${VT_GROUP}" "$cfg"
}

# ─── Domain / mDNS setup ─────────────────────────────────────────────────────

_write_domain_cfg() {
  printf 'VOIDTOWER_DOMAIN=%s\n' "$1" > "${VT_CONFIG_DIR}/domain.env"
  chmod 644 "${VT_CONFIG_DIR}/domain.env"
}

_setup_nginx_voidtower() {
  local domain="$1"
  _install_nginx || return
  local sites_dir="/etc/nginx/sites-enabled"
  [[ -d "$sites_dir" ]] || sites_dir="/etc/nginx/conf.d"
  cat > "${sites_dir}/voidtower.conf" <<NGXEOF
# VoidTower — managed by installer
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
        # Allow VoidTower to be embedded in other dashboards/iframes
        proxy_hide_header X-Frame-Options;
        add_header X-Frame-Options "ALLOWALL" always;
        add_header Content-Security-Policy "frame-ancestors *" always;
    }
}
NGXEOF
  if nginx -t &>/dev/null; then
    nginx -s reload &>/dev/null || systemctl reload nginx &>/dev/null || true
    success "nginx proxy configured for ${domain}"
  else
    warn "nginx config test failed — review ${sites_dir}/voidtower.conf"
  fi
}

_install_avahi() {
  command -v avahi-daemon &>/dev/null && return  # already installed
  local pkg
  case "$PKG_MGR" in
    apt)    pkg="avahi-daemon libnss-mdns" ;;
    dnf)    pkg="avahi avahi-tools nss-mdns" ;;
    pacman) pkg="avahi nss-mdns" ;;
    zypper) pkg="avahi" ;;
    *)      warn "Cannot auto-install avahi — install it manually"; return 1 ;;
  esac
  info "Installing avahi ($pkg)…"
  case "$PKG_MGR" in
    apt)    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $pkg ;;
    dnf)    dnf install -y -q $pkg ;;
    pacman) pacman -S --noconfirm --needed $pkg ;;
    zypper) zypper install -y -q $pkg ;;
  esac
  # Arch: enable mdns in nsswitch.conf
  if [[ "$PKG_MGR" == "pacman" ]] && ! grep -q "mdns" /etc/nsswitch.conf 2>/dev/null; then
    sed -i 's/^\(hosts:.*\)resolve/\1mdns_minimal [NOTFOUND=return] resolve/' /etc/nsswitch.conf
  fi
}

_install_nginx() {
  command -v nginx &>/dev/null && { success "nginx already installed"; return 0; }

  if [[ "$INSTALL_NGINX" == "" && "$UNATTENDED" == false ]]; then
    echo
    read -rp "  nginx is not installed. Install it now? [Y/n]: " _yn
    case "${_yn:-Y}" in
      [Yy]*) INSTALL_NGINX="yes" ;;
      *)     INSTALL_NGINX="no"  ;;
    esac
  fi

  [[ "$INSTALL_NGINX" == "no" ]] && { warn "Skipping nginx — configure manually later."; return 1; }

  info "Installing nginx…"
  case "$PKG_MGR" in
    apt)    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq nginx ;;
    dnf)    dnf install -y -q nginx; systemctl enable nginx ;;
    pacman) pacman -S --noconfirm --needed nginx ;;
    zypper) zypper --non-interactive install -q nginx; systemctl enable nginx ;;
  esac
  systemctl enable --now nginx &>/dev/null || true
  success "nginx installed and started"
}

setup_domain() {
  [[ "$UNATTENDED" == true ]] && return

  local cur_host
  cur_host=$(hostname -f 2>/dev/null || hostname)

  echo
  echo -e "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  echo -e "${BOLD}  Network & Discovery${RESET}"
  echo -e "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  echo
  echo -e "  Hostname: ${CYAN}${cur_host}${RESET}"
  echo
  echo -e "  ${BOLD}[1]${RESET} localhost only  (no network discovery)"
  echo -e "  ${BOLD}[2]${RESET} mDNS — reach VoidTower as ${CYAN}${cur_host}.local${RESET} on your LAN"
  echo -e "  ${BOLD}[3]${RESET} Custom hostname + mDNS  (e.g. rename this machine to 'homelab')"
  echo -e "  ${BOLD}[4]${RESET} Public domain  (e.g. vt.example.com) + nginx reverse proxy"
  echo

  local choice
  read -rp "  Choice [1-4]: " choice

  case "${choice:-1}" in
    1)
      info "Localhost only — configure domain later in Settings if needed."
      ;;

    2)
      if _install_avahi; then
        systemctl enable --now avahi-daemon &>/dev/null || true
        VOIDTOWER_DOMAIN="${cur_host}.local"
        MDNS_ENABLED=true
        _write_domain_cfg "${VOIDTOWER_DOMAIN}"
        success "mDNS active — ${CYAN}http://${VOIDTOWER_DOMAIN}${RESET}"
      fi
      ;;

    3)
      echo
      read -rp "  New hostname (letters, digits, hyphens): " new_host
      if [[ "$new_host" =~ ^[a-zA-Z0-9][a-zA-Z0-9-]{0,62}$ ]]; then
        hostnamectl set-hostname "$new_host" 2>/dev/null || hostname "$new_host" 2>/dev/null
        # Update /etc/hosts 127.0.1.1 line
        if grep -q "127\.0\.1\.1" /etc/hosts 2>/dev/null; then
          sed -i "s/^127\.0\.1\.1.*/127.0.1.1\t${new_host}/" /etc/hosts
        else
          echo -e "127.0.1.1\t${new_host}" >> /etc/hosts
        fi
        success "Hostname set to ${new_host}"
        if _install_avahi; then
          systemctl enable --now avahi-daemon &>/dev/null || true
          VOIDTOWER_DOMAIN="${new_host}.local"
          MDNS_ENABLED=true
          _write_domain_cfg "${VOIDTOWER_DOMAIN}"
          success "mDNS active — ${CYAN}http://${VOIDTOWER_DOMAIN}${RESET}"
        fi
      else
        warn "Invalid hostname — skipping."
      fi
      ;;

    4)
      echo
      read -rp "  Domain (e.g. vt.example.com): " pub_domain
      pub_domain="${pub_domain// /}"
      if [[ "$pub_domain" =~ ^[a-zA-Z0-9*._-]+$ ]]; then
        VOIDTOWER_DOMAIN="$pub_domain"
        _write_domain_cfg "$pub_domain"
        _setup_nginx_voidtower "$pub_domain"
        echo
        warn "Action required:"
        warn "  DNS:  Point ${pub_domain} → this server's public IP"
        warn "  SSL:  certbot --nginx -d ${pub_domain}"
        success "Domain ${pub_domain} configured"
      else
        warn "Invalid domain — skipping."
      fi
      ;;
  esac
}

# ─── AI setup wizard ─────────────────────────────────────────────────────────
setup_ai() {
  [[ "$SKIP_AI" == true ]] && return

  detect_gpu
  select_model_tier

  echo
  echo -e "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  echo -e "${BOLD}  AI Setup${RESET}"
  echo -e "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"

  if [[ "$GPU_VENDOR" != "cpu" ]]; then
    echo -e "  GPU:         ${CYAN}${GPU_NAME}${RESET}"
    if [[ $GPU_VRAM_MB -gt 0 ]]; then
      echo -e "  VRAM:        ${CYAN}$(( GPU_VRAM_MB / 1024 )) GB${RESET}"
    else
      echo -e "  VRAM:        ${YELLOW}(driver not loaded — detection limited)${RESET}"
    fi
  else
    echo -e "  Hardware:    ${CYAN}CPU-only inference${RESET}"
    echo -e "  System RAM:  ${CYAN}$(( SYSTEM_RAM_MB / 1024 )) GB${RESET}"
  fi
  echo -e "  Recommended: ${GREEN}${MODEL_NAME}${RESET} (${MODEL_SIZE})"
  echo

  if [[ "$UNATTENDED" == true ]]; then
    info "Unattended mode: skipping AI setup. Configure later in Settings → Integrations → AI."
    return
  fi

  echo -e "  ${BOLD}[1]${RESET} Download recommended model and set up AI locally"
  echo -e "  ${BOLD}[2]${RESET} Choose a different model"
  echo -e "  ${BOLD}[3]${RESET} Point to an existing llama.cpp server (local or remote)"
  echo -e "  ${BOLD}[4]${RESET} Skip — configure AI later in Settings"
  echo

  local choice
  read -rp "  Choice [1-4]: " choice

  case "${choice:-4}" in
    1)
      if download_llama_cpp && download_model; then
        install_llama_service
        write_llm_config "http://127.0.0.1:8080/v1" "$MODEL_NAME"
        AI_SETUP_DONE=true
      else
        warn "AI setup incomplete. Configure manually later."
      fi
      ;;
    2)
      echo
      echo -e "  ${BOLD}Select a model:${RESET}"
      echo -e "  ${BOLD}[1]${RESET} Qwen 2.5 3B   Q4_K_M  ~1.9 GB  — very fast, CPU-friendly"
      echo -e "  ${BOLD}[2]${RESET} Mistral 7B    Q4_K_M  ~4.1 GB  — 4+ GB VRAM or 16+ GB RAM"
      echo -e "  ${BOLD}[3]${RESET} Llama 3.1 8B  Q8_0    ~8.5 GB  — 8+ GB VRAM recommended"
      echo -e "  ${BOLD}[4]${RESET} Qwen 2.5 14B  Q6_K    ~11 GB   — 12+ GB VRAM"
      echo -e "  ${BOLD}[5]${RESET} Llama 3.3 70B Q4_K_M  ~40 GB   — 24+ GB VRAM"
      echo
      local mchoice
      read -rp "  Model [1-5]: " mchoice
      _set_model "${mchoice:-2}"
      if download_llama_cpp && download_model; then
        install_llama_service
        write_llm_config "http://127.0.0.1:8080/v1" "$MODEL_NAME"
        AI_SETUP_DONE=true
      else
        warn "AI setup incomplete. Configure manually later."
      fi
      ;;
    3)
      echo
      read -rp "  llama.cpp server URL (e.g. http://192.168.1.5:8080): " LLM_REMOTE_URL
      if [[ -n "$LLM_REMOTE_URL" ]]; then
        local test_url="${LLM_REMOTE_URL%/}/v1/models"
        if curl -fsSL --max-time 8 "$test_url" &>/dev/null; then
          success "Connected to ${LLM_REMOTE_URL}"
        else
          warn "Could not reach ${LLM_REMOTE_URL} — saving URL anyway. Verify it in Settings."
        fi
        write_llm_config "${LLM_REMOTE_URL}/v1" "remote"
        AI_SETUP_DONE=true
      else
        warn "No URL entered. Skipping AI setup."
      fi
      ;;
    4|*)
      info "AI setup skipped. Configure later: Settings → Integrations → AI"
      ;;
  esac

  # After a successful local install, offer Odysseus AI workspace
  if [[ "$AI_SETUP_DONE" == true && -z "$LLM_REMOTE_URL" ]]; then
    offer_odysseus
  fi
}

# ─── Odysseus AI workspace ────────────────────────────────────────────────────
offer_odysseus() {
  command -v docker &>/dev/null || return

  echo
  echo -e "  ${BOLD}Install Odysseus AI workspace?${RESET}"
  echo -e "  A privacy-first AI chat that will be pre-wired to ${MODEL_NAME:-your model}."
  echo -e "  ${BOLD}[1]${RESET} Yes — install Odysseus (Docker)"
  echo -e "  ${BOLD}[2]${RESET} No, but save a custom AI workspace URL"
  echo -e "  ${BOLD}[3]${RESET} Skip"
  echo
  local choice
  read -rp "  Choice [1-3]: " choice

  case "${choice:-3}" in
    1)
      local ODYSSEUS_DIR="${VT_INSTALL_DIR}/odysseus"
      info "Cloning Odysseus (VoidLink fork)…"
      if git clone --depth 1 -b odysseus-voidlink https://github.com/niwlekakan/odysseus "$ODYSSEUS_DIR" 2>/dev/null; then
        local llm_base="http://host.docker.internal:8080"
        cat > "${ODYSSEUS_DIR}/.env" <<ODENV
OLLAMA_BASE_URL=${llm_base}/v1
LM_STUDIO_URL=${llm_base}
EMBEDDING_URL=${llm_base}/v1/embeddings
APP_PORT=7000
APP_BIND=127.0.0.1
AUTH_ENABLED=true
ODENV
        local cfile="docker-compose.yml"
        [[ "$GPU_VENDOR" == "nvidia" ]] && cfile="docker-compose.yml:docker/gpu.nvidia.yml"
        [[ "$GPU_VENDOR" == "amd"    ]] && cfile="docker-compose.yml:docker/gpu.amd.yml"
        if COMPOSE_FILE="$cfile" docker compose -f "${ODYSSEUS_DIR}/docker-compose.yml" up -d 2>/dev/null; then
          echo "VOIDTOWER_AI_WORKSPACE_URL=http://localhost:7000" >> "${VT_CONFIG_DIR}/llm.env"
          success "Odysseus running at http://localhost:7000"
        else
          warn "Odysseus failed to start. Check: docker compose -f ${ODYSSEUS_DIR}/docker-compose.yml logs"
        fi
      else
        warn "Clone failed. Install manually: https://github.com/niwlekakan/odysseus (branch: odysseus-voidlink)"
      fi
      ;;
    2)
      local ws_url
      read -rp "  Workspace URL (e.g. http://localhost:8080): " ws_url
      [[ -n "$ws_url" ]] && echo "VOIDTOWER_AI_WORKSPACE_URL=${ws_url}" >> "${VT_CONFIG_DIR}/llm.env"
      ;;
    *) info "Skipped. Configure later: Settings → Integrations → AI" ;;
  esac
}

# ─── Bootstrap token ─────────────────────────────────────────────────────────
show_token() {
  local token_file="${VT_DATA_DIR}/bootstrap.token"
  if [[ -f "$token_file" ]]; then
    local token
    token=$(cat "$token_file")
    echo
    echo -e "${BOLD}${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo -e "${BOLD}  Bootstrap Token (shown once)${RESET}"
    echo -e "${BOLD}${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo -e "  ${CYAN}${token}${RESET}"
    echo -e "${BOLD}${YELLOW}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo -e "  Use this token at: http://localhost:${VT_PORT}/bootstrap"
    echo
  fi
}

# ─── Main ────────────────────────────────────────────────────────────────────
main() {
  echo
  echo -e "${BOLD}${CYAN}▓▓▒░ VoidTower Installer ░▒▓▓${RESET}"
  echo

  detect_os
  detect_arch
  install_deps
  setup_system

  if ! download_binary 2>/dev/null; then
    warn "Pre-built binary not found, falling back to source build"
    build_from_source
  fi

  install_service
  setup_domain
  setup_ai

  info "Generating bootstrap token…"
  sudo -u "$VT_USER" "${VT_INSTALL_DIR}/${BINARY_NAME}" \
    --data-dir "$VT_DATA_DIR" --config-dir "$VT_CONFIG_DIR" \
    --show-token 2>/dev/null || true

  show_token

  if [[ "$SKIP_SYSTEMD" != true ]] && command -v systemctl >/dev/null 2>&1; then
    info "Starting VoidTower…"
    systemctl start voidtower.service
    sleep 1
    if systemctl is-active --quiet voidtower.service; then
      success "VoidTower is running"
    else
      warn "Service did not start cleanly. Check: journalctl -u voidtower -e"
    fi
  fi

  echo
  echo -e "${BOLD}${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  echo -e "${BOLD}${GREEN}  Installation complete!${RESET}"
  echo -e "${BOLD}${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
  echo -e "  Web UI:  ${CYAN}http://localhost:${VT_PORT}${RESET}"
  if [[ -n "$VOIDTOWER_DOMAIN" ]]; then
    echo -e "  Domain:  ${CYAN}http://${VOIDTOWER_DOMAIN}${RESET}"
  fi
  if [[ "$MDNS_ENABLED" == true ]]; then
    echo -e "  LAN:     ${CYAN}discoverable on local network via mDNS${RESET}"
  fi
  echo -e "  Logs:    ${CYAN}journalctl -u voidtower -f${RESET}"
  echo -e "  Config:  ${CYAN}${VT_CONFIG_DIR}${RESET}"
  if [[ "$AI_SETUP_DONE" == true ]]; then
    if [[ -n "$LLM_REMOTE_URL" ]]; then
      echo -e "  AI:      ${GREEN}Connected to ${LLM_REMOTE_URL}${RESET}"
    else
      echo -e "  AI:      ${GREEN}${MODEL_NAME} — ready on port 8080${RESET}"
      echo -e "           ${CYAN}journalctl -u voidtower-llama -f${RESET}"
    fi
  else
    echo -e "  AI:      ${YELLOW}Not configured — Settings → Integrations → AI${RESET}"
  fi
  echo
}

main "$@"
