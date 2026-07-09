#!/usr/bin/env bash
# scripts/devteam/devteam.sh — VoidTower autonomous dev-team runner.
# Runs Claude Code worker + adversarial-review sessions over a file-based task queue.
# Safe ONLY inside the vt-forge sandbox VM. Refuses to run without .devteam/FORGE_HOST.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
DT="$ROOT/.devteam"
QUEUE="$DT/queue" ACTIVE="$DT/active" DONE="$DT/done" FAILED="$DT/failed"
LOGS="$DT/logs" ESC="$DT/escalations" BLOCKED="$DT/blocked"
mkdir -p "$QUEUE" "$ACTIVE" "$DONE" "$FAILED" "$LOGS" "$ESC" "$BLOCKED"

MAX_TASKS=0            # 0 = until queue empty
UNATTENDED=0           # 1 = never prompt; park blocked tasks (overnight/forge)
CLAUDE_BIN="${CLAUDE_BIN:-claude}"
PRETTY="$ROOT/scripts/devteam/pretty.js"
ADR="$ROOT/scripts/devteam/adr.sh"

usage() { echo "usage: devteam.sh {start [--tasks N] [--unattended]|pause|resume|stop|status|logs [id]}"; exit 1; }

ask() {  # ask() "prompt" -> 0 if yes. Reads the terminal, not the pipeline.
  [[ "$UNATTENDED" -eq 1 ]] && return 1
  [[ -e /dev/tty ]] || return 1
  local a; read -rp "$1 [y/N] " a </dev/tty; [[ "$a" =~ ^[Yy]$ ]]
}

# Render a session's stream-json as a live feed; raw JSONL still lands in the log.
stream() {  # stream <label> <logfile>
  if command -v node >/dev/null && [[ -f "$PRETTY" ]]; then
    tee -a "$2" | node "$PRETTY" "$1"
  else
    tee -a "$2" | grep -oE '"text":"[^"]{0,100}' | sed "s/^/  [$1] /" || cat >/dev/null
  fi
}

guard_exec() {
  # Decide execution tier. Full autonomy (--dangerously-skip-permissions) is
  # permitted ONLY in isolation: the forge VM (FORGE_HOST marker) or a container.
  if [[ -f "$DT/FORGE_HOST" ]]; then
    TIER="forge"
  elif [[ -f /run/.containerenv || -f /.dockerenv ]]; then
    TIER="sandbox"
  else
    TIER="attended"
  fi
  case "$TIER" in
    forge|sandbox)
      CLAUDE_FLAGS=(--dangerously-skip-permissions)
      echo "[devteam] tier=$TIER — full autonomy enabled (isolated environment)." ;;
    attended)
      CLAUDE_FLAGS=()
      [[ -f "$ROOT/.claude/settings.json" ]] || {
        echo "REFUSING: attended tier on a bare host requires .claude/settings.json"
        echo "with a permission allowlist (see scripts/devteam/settings.attended.json)."
        echo "Never create .devteam/FORGE_HOST on a real machine to bypass this."
        exit 1; }
      echo "[devteam] tier=attended — bare host detected. No permission bypass;"
      echo "          actions outside the settings.json allowlist will fail the task."
      echo "          For full-speed autonomy on this machine use scripts/devteam/sandbox.sh." ;;
  esac
}

check_flags() {
  [[ -f "$DT/STOP"  ]] && { echo "[devteam] STOP flag set — exiting."; rm -f "$DT/STOP"; exit 0; }
  while [[ -f "$DT/PAUSE" ]]; do echo "[devteam] paused ($(date +%H:%M)) — rm .devteam/PAUSE to resume"; sleep 30; done
}

next_task() { ls "$QUEUE"/*.md 2>/dev/null | sort | head -1; }

stash() {  # move a task file if it still exists (agents must not move files, but be tolerant)
  [[ -f "$1" ]] && mv "$1" "$2" || true
}

run_worker() {  # $1 = task file (in ACTIVE), $2 = log file
  local task="$1" log="$2" id; id="$(basename "$task" .md)"
  "$CLAUDE_BIN" -p "You are a WORKER on the VoidTower dev team. Read CLAUDE.md and obey it \
fully, including forbidden zones and the no-gate-weakening rule. Your task spec is \
.devteam/active/$(basename "$task"). Follow the task workflow in CLAUDE.md exactly: \
branch devteam/${id}, tests first, implement, run scripts/devteam/gates.sh until green, \
conventional commit, push, open a PR. If blocked, write .devteam/escalations/${id}.md \
and stop." \
    "${CLAUDE_FLAGS[@]}" --output-format stream-json --verbose 2>&1 | stream worker "$log"
}

run_review() {  # fresh adversarial session over the branch diff
  local task="$1" log="$2" id; id="$(basename "$task" .md)"
  "$CLAUDE_BIN" -p "You are the ADVERSARIAL REVIEWER (ECC code-reviewer + rust-reviewer + \
security-reviewer). You share no context with the author. Inputs: CLAUDE.md, the task spec \
.devteam/active/$(basename "$task"), and 'git diff main...devteam/${id}'. Find spec \
deviations, missing failure paths, tests that assert nothing, forbidden-zone touches, \
weakened gates, and secret leakage. Write your verdict to .devteam/logs/${id}.review.md \
ending with exactly APPROVE or REJECT: <reason>. Change no code." \
    "${CLAUDE_FLAGS[@]}" --output-format stream-json --verbose 2>&1 | stream reviewer "$log"
  grep -q '^APPROVE$' "$LOGS/${id}.review.md" 2>/dev/null
}

preflight_deps() {  # $1 = spec. Honors "Depends-On: P0-01, P0-02" and "Requires-Path: path/to/file".
  local spec="$1" missing=0 dep path
  for dep in $(grep -ioE '^Depends-On:.*' "$spec" | sed 's/^[^:]*://' | tr ',' ' '); do
    dep="$(echo "$dep" | tr -d '[:space:]')"; [[ -z "$dep" ]] && continue
    # satisfied if any commit reachable from origin/main references the task id
    if ! git -C "$ROOT" log origin/main --oneline --grep="$dep" | grep -q .; then
      echo "[devteam]   ✖ dependency $dep is not merged to origin/main"; missing=1
    fi
  done
  for path in $(grep -ioE '^Requires-Path:.*' "$spec" | sed 's/^[^:]*://'); do
    if ! git -C "$ROOT" cat-file -e "origin/main:$path" 2>/dev/null; then
      echo "[devteam]   ✖ required artifact missing on origin/main: $path"; missing=1
    fi
  done
  [[ $missing -eq 0 ]] && return 0
  echo "[devteam] $(basename "$spec"): dependencies unmet — building on a phantom is worse than waiting."
  ask "  Run it anyway?" && return 0 || return 1
}

preflight_grant() {  # $1 = task spec path. Returns 0 to proceed, 1 to park.
  local spec="$1" id status
  id="$(grep -ohE 'ADR-[0-9]{3}' "$spec" | head -1 || true)"
  [[ -z "$id" ]] && return 0                       # no zones cited → nothing to sign
  local f; f="$(ls "$ROOT/docs/adr/${id}"*.md 2>/dev/null | head -1 || true)"
  if [[ -z "$f" ]]; then
    echo "[devteam] spec cites $id but docs/adr/${id}*.md is missing."
    ask "  Proceed anyway (worker will draft it and escalate)?" && return 0 || return 1
  fi
  status="$(grep -m1 -oE '^\*\*Status:\*\*[[:space:]]*[A-Za-z]+' "$f" | awk '{print $NF}')"
  case "$status" in
    Accepted) return 0 ;;
    Proposed)
      echo
      echo "┌─ APPROVAL NEEDED ─ $id is Proposed; this task needs it signed."
      sed -n '/^```granted-paths/,/^```$/p' "$f" | sed '1d;$d' | sed 's/^/│  grant: /'
      sed -n '/^## Constraints/,/^## /p' "$f" | sed '1d;$d' | sed '/^$/d' | sed 's/^/│  /'
      echo "└─ full text: $f"
      if ask "  Sign $id as Accepted and run this task?"; then
        sed -i "s|^\*\*Status:\*\*.*|**Status:** Accepted (signed by operator $(git config user.name 2>/dev/null || echo operator) on $(date -I))|" "$f"
        echo "[devteam] ✔ $id accepted — remember to commit docs/adr/."
        return 0
      fi
      echo "[devteam] $id not signed — parking task."; return 1 ;;
    *) echo "[devteam] $id status is '$status' — parking task."; return 1 ;;
  esac
}

# A worker escalated. If it drafted an ADR, offer to sign it and retry immediately.
handle_escalation() {  # $1 = task spec, $2 = id  → 0 = retry now, 1 = park
  local esc="$ESC/${2}.md"
  echo; echo "[devteam] ⚠ $2 escalated. Question from the worker:"
  sed -n '/Minimal question/,/^## /p' "$esc" 2>/dev/null | sed '1d;$d' | sed 's/^/  │ /' | head -20
  local draft; draft="$(grep -rlE '^\*\*Status:\*\*[[:space:]]*Proposed' "$ROOT/docs/adr/" 2>/dev/null | head -1 || true)"
  if [[ -n "$draft" ]] && ask "  Worker drafted $(basename "$draft"). Review it now?"; then
    ${PAGER:-less} "$draft" </dev/tty >/dev/tty || cat "$draft"
    if ask "  Sign it and retry this task now?"; then
      sed -i "s|^\*\*Status:\*\*.*|**Status:** Accepted (signed by operator $(git config user.name 2>/dev/null || echo operator) on $(date -I))|" "$draft"
      rm -f "$esc"; return 0
    fi
  fi
  ask "  Retry this task anyway (spec edited elsewhere)?" && { rm -f "$esc"; return 0; }
  return 1
}

start_loop() {
  guard_exec
  local count=0
  git -C "$ROOT" fetch origin main -q || true
  while :; do
    check_flags
    local task; task="$(next_task)" || true
    [[ -z "${task:-}" ]] && { echo "[devteam] queue empty — done."; break; }
    local id; id="$(basename "$task" .md)"
    local log="$LOGS/${id}.$(date +%Y%m%d-%H%M%S).log"
    mv "$task" "$ACTIVE/"
    task="$ACTIVE/$(basename "$task")"
    echo "$id" > "$DT/CURRENT"
    echo "[devteam] ▶ $id (log: ${log#$ROOT/})"

    if ! preflight_deps "$task" || ! preflight_grant "$task"; then
      stash "$task" "$BLOCKED/"; rm -f "$DT/CURRENT"; continue
    fi

    if run_worker "$task" "$log"; then
      if [[ -f "$ESC/${id}.md" ]]; then
        if handle_escalation "$task" "$id"; then
          stash "$task" "$QUEUE/"; echo "[devteam] ↺ $id requeued after approval."
        else
          echo "[devteam] ⚠ $id parked — see .devteam/escalations/${id}.md"; stash "$task" "$BLOCKED/"
        fi
      elif ( cd "$ROOT" && git checkout -q "devteam/${id}" 2>/dev/null && scripts/devteam/gates.sh >>"$log" 2>&1 ) \
           && run_review "$task" "$log"; then
        echo "[devteam] ✔ $id — gates + review passed; PR awaiting human merge."
        stash "$task" "$DONE/"
      else
        if [[ -f "$DT/retried/${id}" ]]; then
          echo "[devteam] ✖ $id failed twice — moved to failed/."; stash "$task" "$FAILED/"
        else
          mkdir -p "$DT/retried"; touch "$DT/retried/${id}"
          printf '\n## RETRY CONTEXT\nPrevious attempt failed gates/review. See %s and %s\n' \
            "${log#$ROOT/}" ".devteam/logs/${id}.review.md" >> "$task"
          stash "$task" "$QUEUE/"; echo "[devteam] ↺ $id requeued with failure context."
        fi
      fi
    else
      echo "[devteam] ✖ $id worker session errored — see log."; stash "$task" "$FAILED/"
    fi
    git -C "$ROOT" checkout -q main || true
    rm -f "$DT/CURRENT"
    count=$((count+1))
    [[ "$MAX_TASKS" -gt 0 && "$count" -ge "$MAX_TASKS" ]] && { echo "[devteam] batch of $count done."; break; }
  done
}

status() {
  echo "current : $(cat "$DT/CURRENT" 2>/dev/null || echo idle)"
  echo "queue   : $(ls "$QUEUE"  2>/dev/null | wc -l)   blocked: $(ls "$BLOCKED" 2>/dev/null | wc -l)"
  echo "done    : $(ls "$DONE"   2>/dev/null | wc -l)   failed : $(ls "$FAILED"  2>/dev/null | wc -l)"
  [[ -f "$DT/PAUSE" ]] && echo "state   : PAUSED"
  echo "--- open devteam branches ---"; git -C "$ROOT" branch --list 'devteam/*' | sed 's/^/  /'
  echo "--- recent results ---"; ls -t "$LOGS"/*.review.md 2>/dev/null | head -5 | xargs -r grep -H '^APPROVE$\|^REJECT' 2>/dev/null || true
}

case "${1:-}" in
  start)
    shift
    while [[ $# -gt 0 ]]; do
      case "$1" in
        --tasks) MAX_TASKS="${2:?N required}"; shift 2 ;;
        --unattended) UNATTENDED=1; shift ;;
        *) usage ;;
      esac
    done
    start_loop ;;
  pause)  touch "$DT/PAUSE"; echo "pausing after current task." ;;
  resume) rm -f "$DT/PAUSE"; echo "resumed." ;;
  stop)   touch "$DT/STOP";  echo "stopping after current task." ;;
  status) status ;;
  logs)   tail -n 60 -f "$LOGS"/${2:-*}*.log ;;
  *) usage ;;
esac