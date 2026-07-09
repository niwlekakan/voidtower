#!/usr/bin/env bash
# scripts/devteam/auto.sh — autonomous multi-phase driver.
#
# Runs the full gap-analysis roadmap (P0→P3) end to end with two human touchpoints per phase:
#   1. Sign the phase's forbidden-zone grants  (adr.sh accept — four keystrokes, once)
#   2. Phase-exit verification on real hardware (you create .devteam/phase-exit/<PHASE>.ok)
#
# Everything else — planning, spec authoring, ADR drafting, implementation, adversarial review,
# gates, merging, phase transition, map refresh — happens without you.
#
# Guardrails that make unattended operation safe rather than merely unattended:
#   - Runs only in an isolated tier (forge VM or container); refuses on a bare host.
#   - Agents cannot sign ADRs, edit gates/CI, or merge full-review paths. Enforced by
#     gates.sh + CODEOWNERS + a PAT without `workflows` scope + branch protection on main.
#   - Budget ceiling, stall detection, and consecutive-failure circuit breaker.
#   - Every phase boundary is a hard stop for human verification.
#
# Usage:
#   auto.sh run                 # drive phases from PHASES until one needs you
#   auto.sh status              # where are we
#   auto.sh notify-test         # verify ntfy wiring
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
DT="$ROOT/.devteam"; DEV="$ROOT/scripts/devteam"
PHASES=(${DEVTEAM_PHASES:-P0 P1 P2 P3})
BUDGET_USD="${DEVTEAM_BUDGET_USD:-40}"          # per auto.sh run
MAX_CONSEC_FAIL="${DEVTEAM_MAX_CONSEC_FAIL:-3}"
NTFY="${DEVTEAM_NTFY_URL:-}"                     # e.g. https://ntfy.sh/voidtower-devteam
mkdir -p "$DT/phase-exit" "$DT/logs"

say()   { echo "[auto] $*"; }
notify(){ [[ -n "$NTFY" ]] && curl -fsS -H "Title: VoidTower devteam" -d "$1" "$NTFY" >/dev/null 2>&1 || true; }

isolated() { [[ -f "$DT/FORGE_HOST" || -f /run/.containerenv || -f /.dockerenv ]]; }

spend() {  # sum session costs from today's stream-json logs
  grep -ho '"total_cost_usd":[0-9.]*' "$DT/logs/"*.log 2>/dev/null \
    | cut -d: -f2 | awk '{s+=$1} END{printf "%.2f", s+0}'
}

plan_phase() {  # $1 = phase id. Agent writes specs + drafts ADRs. Never signs.
  say "planning $1 …"
  claude -p "Act as planner. Read docs/gap-analysis.md phase $1, docs/edd.md, docs/codebase-map.md, \
CLAUDE.md's forbidden zones, and 'scripts/devteam/adr.sh list'. \
1. Write task specs to .devteam/queue/ named ${1}-NN-<slug>.md. Each: scope, contract copied \
verbatim, exact files (from the codebase map), named acceptance tests, review tier, and \
machine-readable headers on their own lines: '## Status: Ready', '**ADR:** <ids or none>', \
'Depends-On: <task ids or none>', 'Requires-Path: <files that must exist on main>'. \
2. Order tasks so nothing depends on infrastructure a later task builds. Derive Depends-On from \
the actual contract, NOT from task numbering. \
3. For any forbidden-zone paths not covered by an ACCEPTED ADR, draft docs/adr/ADR-NNN-<slug>.md \
with Status: Proposed, a fenced granted-paths block, an 'Explicitly NOT granted' section, and \
Constraints. Group tasks under one ADR. Never write Status: Accepted. \
4. Commit specs and drafts and push to a branch 'devteam/plan-$1'. \
Write no product code." --dangerously-skip-permissions 2>&1 | tee -a "$DT/logs/plan-$1.log" | node "$DEV/pretty.js" planner
}

grants_ready() {  # all ADRs cited by this phase's specs are Accepted?
  local missing=0 id f
  for id in $(grep -ohE 'ADR-[0-9]{3}' "$DT/queue/"*.md 2>/dev/null | sort -u); do
    f="$(ls "$ROOT/docs/adr/${id}"*.md 2>/dev/null | head -1 || true)"
    [[ -n "$f" ]] && grep -qE '^\*\*Status:\*\*[[:space:]]*Accepted' "$f" || { echo "  needs signature: $id"; missing=1; }
  done
  return $missing
}

run_phase() {
  local phase="$1" fails=0
  say "═══ $phase ═══"

  # already planned? (queue has specs for this phase)
  ls "$DT/queue/${phase}-"*.md >/dev/null 2>&1 || plan_phase "$phase"
  "$DEV/devteam.sh" doctor --fix || true

  if ! grants_ready; then
    notify "$phase is planned but needs your signature: run 'scripts/devteam/adr.sh accept <ID>' then rerun auto.sh"
    say "STOP: forbidden-zone grants need your signature (the one human step)."
    say "      scripts/devteam/adr.sh list   →   adr.sh accept ADR-NNN   →   auto.sh run"
    return 10
  fi

  "$DEV/devteam.sh" lint || { notify "$phase specs malformed; auto halted"; return 11; }

  while ls "$DT/queue/"*.md >/dev/null 2>&1; do
    [[ "$(awk -v a="$(spend)" -v b="$BUDGET_USD" 'BEGIN{print (a>b)?1:0}')" == "1" ]] && {
      notify "budget ceiling \$$BUDGET_USD reached in $phase; halting"; say "budget reached."; return 12; }
    [[ $fails -ge $MAX_CONSEC_FAIL ]] && {
      notify "$phase: $fails consecutive failures; circuit breaker tripped"; say "circuit breaker."; return 13; }

    local before after
    before="$(ls "$DT/done" 2>/dev/null | wc -l)"
    "$DEV/devteam.sh" start --tasks 1 --unattended || true
    after="$(ls "$DT/done" 2>/dev/null | wc -l)"
    if [[ "$after" -gt "$before" ]]; then fails=0; else fails=$((fails+1)); fi

    # land what needs no eyes; the rest queues for review
    "$DEV/devteam.sh" automerge 2>/dev/null || say "automerge deferred (run it from the host)"
  done

  # refresh the map so the next phase plans against reality
  claude -p "Act as doc-updater. Regenerate docs/codebase-map.md to reflect the current tree, \
preserving its six-section structure. Verify every claim against source. Commit and push." \
    --dangerously-skip-permissions >>"$DT/logs/map-$phase.log" 2>&1 || true

  local open_prs; open_prs="$(git -C "$ROOT" branch -r --list 'origin/devteam/*' | wc -l)"
  notify "$phase queue drained. $open_prs branch(es) await your review. Phase-exit test needed."
  say "$phase drained. Awaiting phase-exit verification."
  say "  Review remaining PRs, run the hardware checks in docs/gap-analysis.md, then:"
  say "    touch .devteam/phase-exit/${phase}.ok && scripts/devteam/auto.sh run"
  [[ -f "$DT/phase-exit/${phase}.ok" ]] || return 20

  "$DEV/adr.sh" list | awk '$2=="Accepted"{print $1}' | while read -r id; do "$DEV/adr.sh" revoke "$id" >/dev/null; done
  say "$phase complete; grants revoked."
  return 0
}

case "${1:-run}" in
  run)
    isolated || { echo "REFUSING: auto.sh runs agents unattended; use the forge VM or sandbox."; exit 1; }
    for p in "${PHASES[@]}"; do
      [[ -f "$DT/phase-exit/${p}.ok" ]] && { say "$p already verified — skipping."; continue; }
      run_phase "$p" || { rc=$?; say "halted in $p (rc=$rc). Spend so far: \$$(spend)"; exit $rc; }
    done
    notify "All phases complete. VoidTower 1.0 candidate."; say "all phases complete." ;;
  status)
    echo "phases : ${PHASES[*]}"
    for p in "${PHASES[@]}"; do printf '  %-4s %s\n' "$p" "$([[ -f "$DT/phase-exit/${p}.ok" ]] && echo verified || echo pending)"; done
    echo "queue  : $(ls "$DT/queue" 2>/dev/null | wc -l)   done: $(ls "$DT/done" 2>/dev/null | wc -l)   failed: $(ls "$DT/failed" 2>/dev/null | wc -l)"
    echo "spend  : \$$(spend) / \$$BUDGET_USD"
    "$DEV/adr.sh" list ;;
  notify-test) notify "devteam notification test"; echo "sent (if DEVTEAM_NTFY_URL set)" ;;
  *) echo "usage: auto.sh {run|status|notify-test}"; exit 1 ;;
esac
