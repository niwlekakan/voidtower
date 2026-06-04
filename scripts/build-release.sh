#!/usr/bin/env bash
# Build a release tarball: frontend → backend → package
set -euo pipefail

CYAN='\033[0;36m'; GREEN='\033[0;32m'; RESET='\033[0m'
info()    { echo -e "${CYAN}[BUILD]${RESET} $*"; }
success() { echo -e "${GREEN}[OK]${RESET}    $*"; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="${VERSION:-$(git -C "$ROOT" describe --tags --abbrev=0 2>/dev/null || echo "0.0.0")}"
TARGETS="${TARGETS:-x86_64-unknown-linux-musl aarch64-unknown-linux-musl}"
DIST="$ROOT/dist"

mkdir -p "$DIST"

info "Building frontend (version: $VERSION)…"
(cd "$ROOT/frontend" && npm ci --silent && npm run build --silent)
success "Frontend built → frontend/dist"

for TARGET in $TARGETS; do
  info "Building backend for $TARGET…"
  (cd "$ROOT/backend" && cargo build --release --target "$TARGET" --quiet)

  ARCHIVE="voidtower-${VERSION}-${TARGET%%-*}-unknown-linux-musl.tar.gz"
  TMP=$(mktemp -d)
  cp "$ROOT/backend/target/$TARGET/release/voidtower" "$TMP/"
  cp -r "$ROOT/frontend/dist" "$TMP/frontend"
  tar -czf "$DIST/$ARCHIVE" -C "$TMP" .
  rm -rf "$TMP"
  success "Packaged → dist/$ARCHIVE"
done

info "Generating checksums…"
(cd "$DIST" && sha256sum voidtower-*.tar.gz > SHA256SUMS)
success "dist/SHA256SUMS written"
