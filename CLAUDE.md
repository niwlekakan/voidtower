# CLAUDE.md — VoidTower Autonomous Dev Team

You are one worker in a multi-agent development team building **VoidTower** (Rust/Axum/SQLite control plane + React frontend). This file is your standing orders. It is read at the start of every session. Deviating from it is a task failure, not a judgment call.

## Source of truth (read in this order, every session)

1. `docs/gap-analysis.md` — the active plan. Work proceeds **P0 → P1 → P2 → P3**. Nothing outside the current phase.
2. `docs/edd.md` — target specs: §3.2/§10.3 (Voidwatch mode ladder + denylist), §4 (agent protocol), §15 (this team's operating model).
3. `docs/codebase-map.md` — the committed codebase survey: modules, route→file map, DB tables→creation sites, AI ingress points, conventions. **Navigate by the map; verify behavior in source.** The map is descriptive and can be stale — never cite it as proof of how code behaves, and never re-survey the whole codebase yourself (read the map + the files your task names, nothing more). If you find the map materially wrong, note it in your PR description.
4. `.devteam/active/<task>.md` — your current task spec. If it conflicts with the docs above, **stop and file a spec issue** (see Escalation). Never improvise architecture.
5. `ROADMAP.md`, `CHANGELOG.md` — repo history and current-state claims. Same rule as the map: verify against source before relying on them.

## Team roles (ECC agents)

| Role | ECC agent | When |
|---|---|---|
| Planner | `planner` + `/epic-decompose` | Decomposes gap-analysis phases into task specs. Runs only when the operator seeds/reseeds the queue. |
| Worker (you, default) | base session + `tdd-guide`, `rust-build-resolver` | One task, one branch, one PR. |
| Adversarial reviewer | `code-reviewer` + `rust-reviewer` + `security-reviewer` | Fresh session, no shared context with the author. Reviews the diff against the task spec only. |
| Failure hunter | `silent-failure-hunter` | Runs over merged batches; files issues, changes nothing. |
| Loop operator | `loop-operator` | Monitors the runner, detects stalls/retry storms, pauses the loop. Writes no product code. |

Active ECC skills for this repo: `continuous-agent-loop`, `verification-loop`, `tdd-workflow`, `rust-patterns`, `rust-testing`, `security-review`, `database-migrations`, `plan-orchestrate`, `architecture-decision-records`.

## Task workflow (every task, no exceptions)

1. **Claim**: your task file is in `.devteam/active/`. Read it fully. Confirm its acceptance tests are listed; if not, stop and escalate.
2. **Branch**: `devteam/<task-id>-<slug>` from latest `main`. Never commit to `main`.
3. **TDD**: write the acceptance tests named in the spec *first*. They must fail for the right reason before you implement.
4. **Implement** to spec. Match existing code conventions (this is a single-crate codebase — see `backend/src/`; do not introduce workspace restructuring, new heavy dependencies, or new patterns without an ADR).
5. **Verify** (`verification-loop`): `scripts/devteam/gates.sh` must pass locally — fmt, clippy `-D warnings`, tests, plus any task-specific gates. Fix until green; if blocked 3 attempts, escalate.
6. **Commit** with Conventional Commits. Update docs touched by the change. Push branch, open PR with the task-id in the title, fill the PR template's spec-compliance checklist.
7. **Stop.** Do not start another task. Do not review your own PR.

## Hard rules

- **Forbidden zones** — never modify without a human-approved ADR referenced in your task spec: `backend/src/policy.rs` and Voidwatch semantics (mode ladder, risk classes, denylist), `backend/src/auth/`, `backend/src/api/auth.rs`, `backend/src/oidc.rs`, secrets/crypto code, `backend/src/db/mod.rs` schema, `.github/workflows/`, `scripts/devteam/` (the harness itself), `CLAUDE.md`, `docs/edd.md`, `docs/gap-analysis.md`.
- **Never weaken a gate.** If a test, lint, or CI check blocks you, fix the code or escalate. Deleting/skipping/`#[ignore]`-ing a test, loosening clippy, or editing CI to pass is the single worst action you can take.
- **Default-deny mindset**: any AI-reachable endpoint you touch must route through the policy choke point (P0.1). Any new action name must have a `risk_class`. Any serialized output that could reach an AI context bundle goes through redaction.
- **No new features.** During P0/P1 the backlog is hardening only. "While I'm here" improvements are scope creep — file an issue instead.
- **No production contact.** You run inside the `vt-forge` sandbox. You have no credentials for the real homelab, and you must never add network calls to LAN addresses, real PVE endpoints, or non-allowlisted domains.
- **Secrets never appear** in code, tests, fixtures, logs, or commit messages.

## Escalation

Write `.devteam/escalations/<task-id>.md` with: what blocked you, what you tried, the minimal question for the human. Move your task file to `.devteam/blocked/`. End the session. Do not guess through ambiguity in policy, auth, schema, or protocol work — those are exactly the places guessing costs the most.

---

# Operator guide (human — Ewwi)

Everything below is for the human operator. Agents: this section is informational; you never run these commands.

## One-time setup (inside the vt-forge VM only)

```bash
# 1. Clone ECC and install its agents/skills/hooks into the harness
git clone https://github.com/affaan-m/ecc ~/ecc && cd ~/ecc && ./install.sh

# 2. In the voidtower repo: commit the plan docs and harness
#    docs/edd.md, docs/gap-analysis.md, CLAUDE.md, scripts/devteam/
#    Protect forbidden zones: .github/CODEOWNERS mapping the paths above to you.

# 3. Safety interlock — the runner refuses to start without this marker,
#    which you create ONLY inside the forge, never on a real machine:
mkdir -p .devteam && touch .devteam/FORGE_HOST

# 4. Snapshot the forge VM (PVE) — do this before every unattended batch.
```

## First run — generate the codebase map (once, before any seeding)

```bash
claude -p "Act as code-explorer using the codebase-onboarding skill. Survey this repository and \
write docs/codebase-map.md containing: (1) backend module inventory with one-line purposes, \
(2) API route → handler file map for every module in backend/src/api/, (3) DB table → creation \
site map from backend/src/db/mod.rs, (4) an exhaustive list of AI-reachable ingress points \
(mcp.rs, integrations.rs, studio.rs mcp_invoke, ai_ask.rs, webhooks) with their current policy \
gating status verified in source, (5) frontend panel → API dependency map, (6) observed code \
conventions. Cite file paths for every claim. Change no code."
```

Review the map before committing — item (4) doubles as the P0.1 ingress inventory, so errors here propagate into the security work. Refresh the map at each phase exit (`doc-updater` agent) and whenever workers repeatedly flag map inaccuracies in PRs.

## Seeding the task queue

```bash
# Have the planner decompose the current phase into task specs (review them before running!)
# The planner must use docs/codebase-map.md to name the exact files each task touches.
claude -p "Act as planner. Read docs/gap-analysis.md phase P0 and docs/edd.md §3.2/§10.3. \
Decompose P0 into task specs in .devteam/queue/, one file per task, numbered P0-01…, \
each with: scope, contract copied verbatim from the docs, named acceptance tests, \
forbidden zones, and review tier. Do not write any product code."
```

Inspect `.devteam/queue/` and edit/delete specs before starting the loop. **The queue is your steering wheel** — the team builds exactly what's in it, nothing else.

## Where to run it — three tiers

| Tier | Command | Autonomy | Use for |
|---|---|---|---|
| **Forge VM** | `devteam.sh start` (needs `.devteam/FORGE_HOST`) | Full | Overnight/unattended batches — the canonical environment |
| **Podman sandbox on your workstation** | `scripts/devteam/sandbox.sh setup` once, then `sandbox.sh run start --tasks 3` | Full | Fast daytime batches at native CPU speed |
| **Attended, bare host** | copy `scripts/devteam/settings.attended.json` → `.claude/settings.json`, then `devteam.sh start --tasks 1` | Allowlisted only | Quick single tasks while you're at the keyboard |

Rules that keep the tiers meaningful: **never** create `.devteam/FORGE_HOST` outside the forge VM (the runner treats it as proof of isolation); the sandbox works on its own clone under `~/.devteam-sandbox/` and syncs through `origin` — never point it at your working checkout; the sandbox's Claude auth lives in its own config dir, not your main one; and the attended tier cannot prompt (print mode), so out-of-allowlist actions fail the task — that's intentional on a machine holding real credentials. Overnight runs you won't watch belong on the forge: the podman sandbox protects your files and keys but its network egress is open, which is an acceptable daytime tradeoff and a poor unattended one.

## Running the team

```bash
scripts/devteam/devteam.sh start            # run until queue empty or stopped
scripts/devteam/devteam.sh start --tasks 5  # bounded overnight batch (recommended)
scripts/devteam/devteam.sh status           # active task, queue depth, last results
scripts/devteam/devteam.sh pause            # finishes current task, then waits
scripts/devteam/devteam.sh resume
scripts/devteam/devteam.sh stop             # finishes current task, then exits
scripts/devteam/devteam.sh logs [task-id]   # tail logs
```

Mechanics: the runner takes one task from `.devteam/queue/`, runs a fresh worker session on it, runs `gates.sh`, then runs a **separate** adversarial-review session on the diff. Pass → PR is left open for you; fail → one retry with the failure appended to the spec, then the task moves to `.devteam/failed/`. Pause/stop are file-based (`.devteam/PAUSE`, `.devteam/STOP`) and honored between tasks — a hard kill is `Ctrl-C` plus VM snapshot rollback if the tree is suspect.

## Your daily checkpoint (non-negotiable, ~30–60 min)

1. `devteam.sh status`; read `.devteam/escalations/` and `.devteam/failed/`.
2. Review the PR queue: full line review for forbidden-zone-adjacent code (policy/auth/secrets/schema); boundary review elsewhere per EDD §15.5. Merge what passes.
3. Amend/requeue failed specs; answer escalations; reseed the queue when a phase exits.
4. Snapshot the forge before the next unattended batch.

**Phase exits are yours alone** (G4): the live-hardware checklist in the gap analysis runs on the real homelab, by you, from outside the forge. The loop can empty its queue; it cannot declare a phase done.

## What "fully autonomous until complete" means here

The loop will happily run 24/7, and inside the forge that's safe. But merges to `main`, phase-exit verification, and anything touching a forbidden zone converge on you by design. If the queue runs dry and PRs pile up unreviewed, the system is *paused on you* — that's the intended behavior, not a bug. Budget note: each task ≈ one full agent session + one review session; watch usage after the first batch and size `--tasks` accordingly.
