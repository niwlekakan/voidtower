# VoidTower Development Agent and Workflow Design

Date: 2026-09-01
Status: Approved for implementation
Scope: Dedicated Hermes development profile, project skill, source-derived planning workflow, and repository development governance

## 1. Purpose

Create a dedicated `voidtower-dev` Hermes profile that can develop VoidTower from the current repository state to a working, release-supported application without mixing project memory, skills, sessions, or configuration into the operator's default Hermes profile.

The profile is the persistent principal developer and integration authority. Short-lived delegated workers may research, implement a bounded task, or review a fixed diff, but they do not own the roadmap or independently redefine project contracts.

## 2. Chosen approach

Use one specialized head profile with bounded delegated workers.

Do not create a permanent multi-profile Kanban team yet. VoidTower first needs one authoritative roadmap, canonical mutation convergence, deterministic release gates, and reliable handoffs. A durable Kanban team may be introduced after the first three convergence slices are green and task boundaries are stable.

Repository-only configuration was rejected because it would continue mixing project memory and skills with unrelated work in the default Hermes profile.

## 3. Product-development priority

The development order is:

1. Product truth and deterministic release gates.
2. Canonical AI, MCP, webhook, automation, CLI, and compatibility-route mutation intake.
3. AI-provider secret-manager convergence.
4. Linux inventory collector and authenticated CMDB reconciliation.
5. Windows collector contract and target gate.
6. Household identity, consent, and resource grants.
7. Stable versioned external client contracts.
8. Frontend, desktop, mobile, voice, and broader household functionality only after their backend contracts are release-supported.

The agent must not trade deterministic authority for integration breadth. VoidTower remains useful when every AI provider and external cloud service is unavailable.

## 4. Profile architecture

Create profile `voidtower-dev` with an isolated Hermes home containing its own:

- configuration;
- sessions and state database;
- memories;
- skills and skill usage state;
- `SOUL.md`;
- projects;
- checkpoints;
- cron or Kanban state if introduced later.

Clone model/provider credentials and the working baseline configuration from the default profile so the new profile is immediately usable, but do not clone session history or memory.

The profile description must identify it as the principal VoidTower developer responsible for architecture preservation, vertical-slice delivery, security review, release evidence, and reproducible handoffs.

The default Hermes profile must remain the sticky default unless the operator explicitly changes it.

## 5. Profile behavior

The profile's `SOUL.md` will establish the following behavior:

- current source and executable evidence establish present behavior and maturity;
- tracked `docs/development-plan.md` governs dependency order, acceptance criteria, and release gates;
- the newest dated local handoff and its approved design provide continuity but do not replace the tracked plan;
- exactly one bounded implementation slice is active at a time;
- test-first development occurs at a public seam;
- provider/destructive mutations must use the canonical operation path;
- secrets, raw diagnostics, SQL details, and identity material are never leaked;
- no release/runtime claim is made without corresponding evidence;
- unrelated worktree changes are preserved;
- commits remain local unless publication is explicitly requested;
- delegated output is treated as evidence and independently verified.

## 6. Tool and model posture

Enable the focused toolsets needed for development:

- file and coding tools;
- terminal and process management;
- delegation;
- todo planning;
- session search;
- skills;
- project workspace support;
- memory;
- web research for current dependencies and upstream documentation;
- GitHub only when remote state matters.

Keep approvals in smart mode, secret redaction enabled, checkpoints enabled, and stop verification enabled.

Use conservative parallelism. The head profile may delegate independent research, implementation, security review, or code review, but only one worker may modify a given high-collision subsystem at a time. Use isolated Git worktrees for concurrent coding.

Do not create a permanent agent swarm or enable autonomous publication.

## 7. Project skill

Maintain one deep profile-local skill named `voidtower-dev` rather than many narrow project skills.

The skill will contain the stable development procedure and link to:

- `references/product-contract.md` — canonical identities, security invariants, 1.0 scope, and non-goals;
- `references/slice-workflow.md` — evidence intake, slice selection, TDD loop, review, commit, and handoff;
- `references/verification-matrix.md` — required gates by backend, schema, frontend, mobile, agent, integration, deployment, and release impact;
- `templates/slice-plan.md` — bounded slice template;
- `templates/handoff.md` — evidence-labelled handoff template;
- `scripts/repo_truth.py` — deterministic source inventory and repository-state report.

The source-inventory script must derive rather than hard-code:

- branch, HEAD, ahead/behind state, and modified/staged files;
- route metadata count;
- structured action count;
- built-in MCP tool count;
- standalone MCP server count;
- App Vault manifest count;
- migration list;
- backend integration-test files;
- package versions;
- newest dated handoff.

It must not read or print credential values.

## 8. Skill loading policy

The standard implementation bundle is:

- `voidtower-dev`;
- `implement`;
- `tdd`;
- `requesting-code-review`.

Load additional skills only when triggered:

- `systematic-debugging` after a reproducible failure;
- `frontend-design` for user-facing web design;
- `mobile-app-ui-design` for mobile work;
- `github` for remote issues, PRs, CI, or release state;
- `improve-codebase-architecture` only for explicit architecture exploration;
- `simplify-code` only for an operator-requested cleanup pass.

Create a `voidtower-slice` bundle for the standard implementation set.

## 9. Repository governance changes

### 9.1 AGENTS.md

Update stale resume language so it always selects the newest dated handoff and does not name the completed 2026-08-31 API slice as current. Add evidence classifications and the canonical convergence priority.

`AGENTS.md` is currently ignored by repository policy, so it is portable local continuity guidance rather than durable tracked authority. Any sequencing, acceptance, or release-governance change expressed there must also be represented in tracked `docs/development-plan.md` or `ROADMAP.md`; do not force-add it or change ignore policy as part of this workflow.

### 9.2 ROADMAP.md

Replace the stale top-level 1.0 status with an evidence-based roadmap that:

- defines a web-first deterministic 1.0;
- separates source, contract-test, integration-test, runtime, and release evidence;
- orders work by dependency;
- excludes direct AI mutation outside canonical jobs and approvals;
- treats desktop/mobile, executable plugins, broad household services, and app-specific MCP servers as deferred or experimental until verified;
- references source-generated counts rather than copying mutable totals.

Legacy backlog sections may remain only when clearly marked historical.

### 9.3 Development plan

Create and maintain the dependency-ordered implementation plan at tracked path `docs/development-plan.md`. Dated handoffs under ignored local paths may aid continuity, but they cannot be the sole tracked development authority. Each slice must specify:

- goal and user-visible outcome;
- architectural invariants;
- exact files or modules likely to change;
- failing public-seam test;
- focused and full verification gates;
- explicit out-of-scope items;
- rollback or recovery behavior;
- completion evidence;
- next slice dependency.

### 9.4 Historical task state

Treat `.devteam/active/` as historical unless both the newest handoff explicitly reactivates an entry and tracked `docs/development-plan.md` includes it in the dependency-ready slice. The ignored `.devteam/` tree and its local status index preserve continuity evidence but can never be the sole tracked authority. Do not force-add, delete, or rewrite historical evidence; stale cards cannot override current authority.

## 10. Standard slice workflow

1. Read `AGENTS.md`, tracked `docs/development-plan.md`, `ROADMAP.md`, the newest local handoff, and its approved design.
2. Run the source-truth script and inspect the relevant source/tests.
3. Select exactly one dependency-ready slice.
4. Write or update a bounded slice plan with acceptance criteria and non-goals.
5. Write one failing test at the public seam.
6. Implement the smallest change that passes.
7. Repeat vertically; refactor only after the focused test is green.
8. Run the applicable verification matrix.
9. Obtain independent spec and security/code review.
10. Resolve all high- and medium-severity findings or explicitly block the slice.
11. Commit only the slice files with a focused conventional commit.
12. Write a dated handoff containing exact evidence and one next slice.
13. Re-read Git state and verify that unrelated changes remain untouched.

## 11. Verification matrix principles

- Focused tests run during implementation.
- Full applicable suites run after the final change.
- Schema work runs migration ownership, fresh database, legacy upgrade, and integrity checks.
- Security-boundary work runs real-router role and bearer-scope probes.
- Durable-operation work verifies plan immutability, idempotency, approval binding, recovery, audit, and events.
- Agent work verifies state-file safety, bounded transport, token/path binding, backoff, and platform fixtures.
- Frontend/mobile work requires tests, type-check, lint, production build, and at least one runtime connectivity path before release support.
- Integration tests that are ignored, filtered, mocked, or only assert workflow text are not reported as runtime verification.
- Release support requires a produced artifact plus installation/startup/upgrade evidence on the named platform.

## 12. Error handling and failure policy

- A failing full suite blocks a green handoff even if an isolated rerun passes; classify it as deterministic failure or unresolved flake.
- A worker failure does not authorize the head profile to assume success. Inspect the real worktree and logs.
- A partial provider outcome becomes `needs_attention`; callers do not blindly retry.
- Missing route/action metadata fails closed.
- Missing or ambiguous handoff scope blocks implementation until resolved.
- A test or tool unavailable on the host is reported as not run, with the missing prerequisite.
- Credentials are represented as `[REDACTED]` in every report and fixture.

## 13. Completion criteria

The optimized development workflow is complete when a fresh `voidtower-dev` profile session can:

1. open the VoidTower project without manual directory reconstruction;
2. load the current project skill and standard bundle;
3. identify the dependency-ready slice from the tracked plan and reconcile it with the newest local handoff;
4. produce a source-derived repository truth report;
5. preserve canonical resource, policy, approval, job, audit, event, and node-authentication invariants;
6. execute a test-first slice with focused and full gates;
7. obtain independent review;
8. create a reproducible local commit and handoff;
9. avoid runtime/release claims unsupported by actual evidence;
10. leave the default Hermes profile and unrelated worktree changes untouched.

## 14. Deferred evolution

After product-truth, canonical mutation intake, and secret-manager convergence are green, evaluate a dedicated VoidTower Kanban board with specialist profiles. Adoption requires stable task decomposition, deterministic worktree conventions, review routing, and a demonstrated throughput benefit over bounded delegation.
