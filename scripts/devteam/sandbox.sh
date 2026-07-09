#!/usr/bin/env bash
# scripts/devteam/sandbox.sh — run the dev-team loop in a rootless podman
# sandbox on a workstation, at native speed, without exposing the host.
#
# Isolation properties:
#   - Works on ITS OWN CLONE (~/.devteam-sandbox/voidtower), never your working
#     checkout: a shared mount would let an agent plant .git/hooks that execute
#     on the host later. Branches flow back through origin only.
#   - No ~/.ssh, no home dir, no host Claude config. A dedicated config dir
#     (~/.devteam-sandbox/claude-config) holds ONLY the auth for this purpose.
#   - Container detection in devteam.sh enables full autonomy inside; the same
#     binary refuses bypass on the bare host.
#
# Honest limits (vs the forge VM):
#   - Network egress is OPEN (cargo/npm need it). Container isolation protects
#     your files and keys, not your LAN — the clone has no LAN credentials,
#     but a hostile process could still probe the network. Overnight batches
#     you won't watch belong on the forge.
#   - Container escape is harder than "no barrier" but easier than a VM.
#
# Usage:
#   scripts/devteam/sandbox.sh setup                    # once
#   scripts/devteam/sandbox.sh run [devteam args...]    # e.g. run start --tasks 3
#   scripts/devteam/sandbox.sh shell                    # debug shell inside
set -euo pipefail

BASE="${DEVTEAM_SANDBOX_HOME:-$HOME/.devteam-sandbox}"
CLONE="$BASE/voidtower"
CFG="$BASE/claude-config"
CARGO_CACHE="$BASE/cargo-cache"
CREDS="$BASE/git-credentials"
IMG="devteam-sandbox:latest"
ORIGIN="${DEVTEAM_ORIGIN:-$(git config --get remote.origin.url 2>/dev/null || true)}"

setup() {
  command -v podman >/dev/null || { echo "podman required (pacman -S podman)"; exit 1; }
  mkdir -p "$CLONE" "$CFG" "$CARGO_CACHE"
  [[ -n "$ORIGIN" ]] || { echo "set DEVTEAM_ORIGIN=<git url> or run from a clone"; exit 1; }
  [[ -d "$CLONE/.git" ]] || git clone "$ORIGIN" "$CLONE"

  # Repo-scoped git identity for agent commits. Agents are forbidden from touching
  # git config (CLAUDE.md); the operator provides identity once, here.
  git -C "$CLONE" config user.name  "${DEVTEAM_GIT_NAME:-VoidTower DevTeam}"
  git -C "$CLONE" config user.email "${DEVTEAM_GIT_EMAIL:-devteam@voidtower.local}"

  # Push credentials: a fine-grained PAT scoped to THIS REPO ONLY.
  #   Contents: RW · Pull requests: RW · Metadata: R
  #   Workflows: NOT granted → GitHub itself rejects any push touching .github/workflows/,
  #   which enforces forbidden zone #5 server-side rather than by convention.
  # Protect main on GitHub (require PR) so this token can never push main directly.
  if [[ ! -f "$CREDS" ]]; then
    echo
    echo "Paste a fine-grained GitHub PAT for niwlekakan/voidtower (input hidden):"
    read -rs TOKEN
    [[ -n "$TOKEN" ]] || { echo "no token given — agents will not be able to push."; }
    if [[ -n "$TOKEN" ]]; then
      umask 077
      printf 'https://x-access-token:%s@github.com\n' "$TOKEN" > "$CREDS"
      chmod 600 "$CREDS"
      unset TOKEN
      echo "stored in $CREDS (0600, mounted read-only into the sandbox)"
    fi
  fi
  git -C "$CLONE" config credential.helper "store --file=/gitcreds"
  git -C "$CLONE" remote set-url origin "https://github.com/$(basename "$(dirname "$ORIGIN")")/$(basename "$ORIGIN" .git).git" 2>/dev/null || true

  podman build -t "$IMG" -f - "$BASE" <<'EOF'
FROM docker.io/library/rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      git curl ca-certificates nodejs npm ripgrep jq \
    && rm -rf /var/lib/apt/lists/* \
    && npm install -g @anthropic-ai/claude-code
RUN rustup component add rustfmt clippy
RUN useradd -m dev
USER dev
WORKDIR /work
ENV CARGO_HOME=/cargo CLAUDE_CONFIG_DIR=/claude-config
EOF

  echo
  echo "Now authenticate Claude Code INSIDE the sandbox (one time):"
  echo "  $0 shell    # then run: claude   (login flow), then exit"
  echo "This stores auth in $CFG only — your normal Claude config is never mounted."
}

_podman() {
  podman run --rm -it \
    --userns=keep-id \
    --security-opt no-new-privileges \
    --cap-drop=ALL \
    --memory=12g --pids-limit=2048 \
    -v "$CLONE:/work:Z" \
    -v "$CFG:/claude-config:Z" \
    -v "$CARGO_CACHE:/cargo:Z" \
    $( [[ -f "$CREDS" ]] && echo "-v $CREDS:/gitcreds:ro,Z" ) \
    -w /work \
    "$IMG" "$@"
}

case "${1:-}" in
  setup) setup ;;
  run)
    shift
    # -it (already set) gives the container a TTY so preflight/escalation prompts work.
    # For overnight batches with no human present, pass --unattended through to devteam.sh.
    # The sandbox clone is disposable; origin is the source of truth. Never merge here —
    # hard-track origin/main. Any local main commits (a worker that ignored the branch rule)
    # are preserved under refs sandbox-backup/* rather than silently discarded.
    (
      cd "$CLONE"
      git fetch origin -q
      if [[ -n "$(git log --oneline origin/main..main 2>/dev/null)" ]]; then
        bk="sandbox-backup/main-$(date +%Y%m%d-%H%M%S)"
        git branch -f "$bk" main
        echo "[sandbox] WARNING: local commits on main were preserved as '$bk':"
        git log --oneline origin/main..main | sed 's/^/[sandbox]   /'
        echo "[sandbox] Push it if you want it: git -C $CLONE push origin $bk"
      fi
      if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "[sandbox] NOTE: uncommitted changes in the clone will be discarded."
        git stash push -u -m "devteam-autostash-$(date +%s)" >/dev/null 2>&1 || true
        echo "[sandbox] (recover with: git -C $CLONE stash list)"
      fi
      git checkout -q -B main origin/main
    )
    _podman scripts/devteam/devteam.sh "$@"
    echo
    echo "[sandbox] branches pushed to origin by workers; review PRs from your"
    echo "[sandbox] normal checkout. Sandbox clone lives at: $CLONE"
    ;;
  shell) _podman bash ;;
  *) echo "usage: sandbox.sh {setup|run [devteam args]|shell}"; exit 1 ;;
esac