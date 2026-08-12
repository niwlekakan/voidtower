---
title: VoidTower Next Development Session Handoff
date: 2026-08-12
status: ready
tags:
  - voidtower
  - handoff
  - development
  - release/1-0
aliases:
  - VoidTower development handoff 2026-08-12
---

# VoidTower Next Development Session Handoff

> [!success] Current outcome
> Security-foundation work S0-01 through S0-03 and verification work V0-01 are merged to
> `main`. The repository is now at the numbered-migration boundary. Do not restart roadmap
> discovery or re-implement completed security work.

## Session objective

Start **D0-01/D0-02**: freeze and verify the live SQLite schema, authorize the forbidden-zone
changes, and convert startup schema ownership to ordered, transactional, fail-fast numbered
migrations without losing existing user data.

The canonical product context is:

- [[ROADMAP|Roadmap and current 1.0 delivery status]]
- [[docs/superpowers/specs/2026-08-11-voidtower-1-0-product-architecture-design|Approved 1.0 product and architecture design]]
- [[docs/superpowers/plans/2026-08-11-voidtower-1-0-implementation-plan|Dependency-ordered 1.0 implementation plan]]
- [[docs/edd|Engineering design document]]

[[docs/codebase-map|The codebase map]] remains useful for module orientation but contains historical
status prose; verify its claims against `main` rather than treating every status paragraph as
current.

## Repository state at handoff

- Repository: `niwlekakan/voidtower`
- Working directory: `/home/elwla/Documents/voidtower_project_files_full/hive/voidtower`
- Base branch: `main`
- Expected base commit before this roadmap/handoff documentation PR: `49fe93b`
- Latest completed pull requests:
  - [PR #20 — S0-03 route/action registry convergence](https://github.com/niwlekakan/voidtower/pull/20)
  - [PR #21 — V0-01 full CI merge gates](https://github.com/niwlekakan/voidtower/pull/21)
- Open PR [#6](https://github.com/niwlekakan/voidtower/pull/6) is a stale historical rescue branch
  from P0-01. Do not merge it or use it as a current base without a fresh explicit audit.
- Application version remains `0.9.0`; do not claim 1.0 readiness.

Before this documentation update, the tracked tree was clean. These pre-existing untracked files
belong to the operator or earlier sessions and must not be staged implicitly:

- `Codex Handoff — VoidTower CMDB - Asset Registry.md`
- `backend/vt_manual_test.sh`
- `docs/project-reality-audit-2026-08-11.md`

Review and adopt them only through an explicit, focused task. The CMDB handoff is product input,
not permission to commit the source artifact unchanged.

## Live GitHub protection

`main` is protected and requires a pull request, resolved review conversations, a current branch,
and these six successful contexts:

1. `Frontend (lint + build)`
2. `Backend (clippy + test)`
3. `Supply chain (cargo-deny + cargo-audit)`
4. `Mutation testing (policy + voidwatch)`
5. `Golden path (Docker + App Vault + restic)`
6. `Build & push image`

Administrator enforcement is enabled. Force-push and branch deletion are disabled. Required human
approvals are zero because the repository currently has one eligible maintainer. See
[[docs/operations/github-merge-gates|GitHub merge gates]] and the desired state in
`docs/operations/github-main-protection.json`.

The first protected proof PR remained blocked until all six checks succeeded. Recent observed
durations were approximately 40 minutes for mutation testing and 78 minutes for the
multi-architecture image build. Long-running checks are expected; do not bypass them merely for
duration.

## Completed foundation

### S0-01 — positive role guards

- Low-trust roles no longer pass privileged handlers through denylist-style comparisons.
- Owner/admin/operator/session behavior is projected from positive typed metadata.
- Exhaustive real-router tests cover unauthenticated and insufficient-role requests.

### S0-02 — host-route authentication

- Previously unintended public capability, diagnostics, version, and model-job routes were
  classified and corrected.
- ADR-009 is accepted and records the bearer/session policy.

### S0-03 — route/action registry

- `backend/src/action_registry.rs` is authoritative for 327 mounted routes and structured actions.
- It owns method/path, session policy, concrete credentials, bearer policy, risk, approval, and AI
  exposure.
- Session authorization, bearer enforcement, route/action risk, denylist validation, and MCP
  read/write classification derive from it.
- Bidirectional inventory tests prevent missing, duplicate, or stale route/action metadata.
- Unknown route metadata returns structured `insufficient_scope`; unknown actions fail safe.
- The two embed-router paths deliberately retain `BearerPolicy::Unscoped` and have real-router
  regression coverage.
- MCP handlers still expect bearer credentials while global policy denies MCP bearer access. This
  contradiction is explicit and intentionally unresolved; do not broaden access accidentally.

### V0-01 — merge-gating infrastructure

- The real Docker/App Vault/restic golden path is required on pull requests.
- The complete six-check policy is stored and documented in `docs/operations/`.
- Standards and spec reviews found no issues in S0-03 or V0-01.

## D0 starting facts

The current schema implementation is concentrated in `backend/src/db/mod.rs`:

- `init_pool()` configures SQLite, calls `run_migrations()`, then performs roughly 50 additional
  best-effort schema, seed, and backfill operations.
- Most post-baseline schema statements discard their result with `let _ = ...`; startup cannot
  distinguish “already exists” from corruption, a typo, an incompatible database, or a failed
  migration.
- `run_migrations()` is not a numbered runner. It contains the original inline baseline schema.
- `backend/tests/schema_golden.sql` represents the current fresh schema.
- `backend/tests/schema_v0_9_0_seed.sql` is the representative pre-1.0 upgrade fixture.
- Existing tests verify fresh-schema equality and preservation of old tables/columns, but they do
  not yet prove ordered versions, interrupted-migration recovery, or strict repeated startup.
- `sqlx` already enables its `migrate` feature in `backend/Cargo.toml`; no new migration framework
  is required merely to begin D0.
- No `backend/migrations/` directory exists yet.

## Recommended D0 design direction

Use a one-time legacy adopter plus SQLx numbered migrations:

1. Introduce a reviewed `0000_baseline.sql` for a genuinely fresh database.
2. Add ordered compatibility migrations for historical additions rather than replaying a monolith
   of best-effort alterations.
3. Detect an existing unversioned VoidTower database by explicit schema fingerprints; adopt it only
   when it matches a supported legacy shape.
4. Refuse ambiguous or partially compatible legacy schemas with a structured backup-and-recovery
   message. Never silently mark an unknown schema current.
5. Run migrations transactionally where SQLite permits it and propagate every real error.
6. Keep data seeding/backfill behavior separate from schema DDL and make it idempotent.
7. Prove fresh install, v0.9.0 seeded upgrade, current-unversioned adoption, repeated startup,
   interrupted/failed migration, and backup/restore behavior.

Do not begin the code conversion until the D0 design and ADR define the exact adoption algorithm,
migration boundaries, supported legacy fingerprints, failure/recovery semantics, and granted paths.

## ADR authority

The operator explicitly authorized Codex to sign future ADRs. Sign transparently as:

> Accepted — signed by Codex under operator-delegated authorization on `<date>`.

Never impersonate the operator. The next ADR must grant only the D0 paths actually required,
expected to include `backend/src/db/mod.rs`, `backend/migrations/**`, schema fixtures/tests, and
possibly `backend/Cargo.toml`/`Cargo.lock` only if the final design genuinely needs them.

ADR-008 authorized the earlier read-only schema golden/upgrade work; it explicitly did not grant
numbered migrations. Do not reuse it as authorization for D0.

## First actions for the next session

1. Read `CLAUDE.md` completely and inspect `git status`, `git log -5`, open PRs, and live branch
   protection.
2. Read this handoff and the D0-01/D0-02 sections of the 1.0 implementation plan.
3. Re-read all of `backend/src/db/mod.rs`, `schema_golden.sql`, and `schema_v0_9_0_seed.sql`.
4. Mechanically inventory every schema statement, its order, whether errors are swallowed, and
   whether it performs DDL, seed data, or backfill logic.
5. Compare the fresh live schema, current golden schema, v0.9.0 seed, and at least one current
   unversioned database shape.
6. Present two or three migration/adoption approaches and select the safest explicit design.
7. Write the D0 design, self-review it, author a narrowly scoped ADR, and sign it under delegated
   authority.
8. Land the ADR/design through the protected PR path before forbidden-zone implementation.
9. Implement test-first, run `scripts/devteam/gates.sh`, perform independent standards/spec review,
   and wait for all six required GitHub checks before merge.

## Product constraints to preserve

- All three 1.0 pillars remain required: full infrastructure, household experience, and AI.
- The combined/local node and every remote machine must converge on the same `vt-agent` contract.
- Any capable machine may join the managed cluster, subject to explicit trust and capabilities.
- Docker/App Vault and VM/LXC placement must be explained, constraint-aware, and recoverable.
- Full Proxmox integration and managed community scripts remain 1.0 scope.
- AI must be able to view, explain, report, recommend, and safely propose/manage actions across
  applicable domains without privileged bypasses.
- Smartphone and tablet support are first-class release surfaces, including stale-state and offline
  safety.
- Household branding must be distinctive; do not use “homeOS” as the product name.
- The CMDB/Asset Registry is a core platform boundary and must use shared resource, job, policy,
  audit, and AI contracts rather than becoming a parallel subsystem.

## Definition of a successful next session

The next session is successful when D0's architecture and authorization are merged and the first
implementation slice has trustworthy tests—or when the whole D0-01/D0-02 conversion is merged if
it remains reviewable as one change. Merely creating migration files without safe legacy adoption,
failure behavior, and upgrade evidence is not completion.
