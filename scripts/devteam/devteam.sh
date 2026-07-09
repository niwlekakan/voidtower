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
CLAUDE_BIN="${CLAUDE_BIN:-claude}"

usage() { echo "usage: devteam.sh {start [--tasks N]|pause|resume|stop|status|logs [id]}"; exit 1; }

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
    "${CLAUDE_FLAGS[@]}" >>"$log" 2>&1
}

run_review() {  # fresh adversarial session over the branch diff
  local task="$1" log="$2" id; id="$(basename "$task" .md)"
  "$CLAUDE_BIN" -p "You are the ADVERSARIAL REVIEWER (ECC code-reviewer + rust-reviewer + \
security-reviewer). You share no context with the author. Inputs: CLAUDE.md, the task spec \
.devteam/active/$(basename "$task"), and 'git diff main...devteam/${id}'. Find spec \
deviations, missing failure paths, tests that assert nothing, forbidden-zone touches, \
weakened gates, and secret leakage. Write your verdict to .devteam/logs/${id}.review.md \
ending with exactly APPROVE or REJECT: <reason>. Change no code." \
    "${CLAUDE_FLAGS[@]}" >>"$log" 2>&1
  grep -q '^APPROVE$' "$LOGS/${id}.review.md" 2>/dev/null
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

    if run_worker "$task" "$log"; then
      if [[ -f "$ESC/${id}.md" ]]; then
        echo "[devteam] ⚠ $id escalated — needs human."; stash "$task" "$BLOCKED/"
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
  start)  shift; [[ "${1:-}" == "--tasks" ]] && MAX_TASKS="${2:?N required}"; start_loop ;;
  pause)  touch "$DT/PAUSE"; echo "pausing after current task." ;;
  resume) rm -f "$DT/PAUSE"; echo "resumed." ;;
  stop)   touch "$DT/STOP";  echo "stopping after current task." ;;
  status) status ;;
  logs)   tail -n 60 -f "$LOGS"/${2:-*}*.log ;;
  *) usage ;;
esac
