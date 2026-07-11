# Task P1-03: Voidwatch/policy exhaustiveness audit + cargo-mutants CI gate

## Status: Ready
**ADR:** ADR-005, ADR-006
Depends-On: none
Requires-Path: backend/src/voidwatch/mod.rs, backend/src/voidwatch/mode.rs, backend/src/voidwatch/risk_class.rs, backend/src/voidwatch/denylist.rs, backend/src/policy.rs, .github/workflows/ci.yml

## Source

- `docs/gap-analysis.md` §3, P1 table row 1: "Policy/Voidwatch (P0 output) | Exhaustive
  matrix tests + `cargo-mutants` | The security boundary gets the EDD's full G1 treatment."
- `docs/edd.md` §13 (Testing Requirements): "Unit | Policy engine (highest bar: every mode ×
  risk_class × denylist combination table-tested) ... | 90% line coverage on `vt-voidwatch`;
  no global target elsewhere" and §15.3's G1 gate: "mutation testing (`cargo-mutants`) on
  `vt-voidwatch`, `vt-proto`, `vt-auth`" (retargeted per gap-analysis to this repo's
  single-crate shape: `backend/src/policy.rs` + `backend/src/voidwatch/**`).
- `docs/adr/ADR-005-p1-ci-hardening.md` and `docs/adr/ADR-006-p1-voidwatch-policy-test-hardening.md`
  — **authoritative** for what's granted. ADR-006 covers test-only edits inside
  `policy.rs`/`voidwatch/**`; ADR-005 covers the new CI job.
- `docs/codebase-map.md` §1 and §6 — as of this map's regeneration, `voidwatch/mod.rs` has 17
  tests including `exhaustive_mode_by_risk_class_matrix`; `risk_class.rs` and `denylist.rs`
  each have their own exhaustiveness/parser-based tests; `policy.rs` has a data-driven
  "default verdict by actor class" test table. **Re-verify this directly** — the map can be
  stale and P0-04 (denylist) landed after this map's last full regeneration per its own
  preamble note.

## Scope

Two deliverables, and the first gates whether the second is meaningful:

1. **Audit, don't assume.** Enumerate what test coverage already exists against the mode
   (Observer/Assisted/Trusted/YOLO) × risk_class (Read/Mutate/Destructive/Irreversible) ×
   denylist-item (the twelve ADR-004 items in `denylist.rs`) matrix. Given this module's
   documented maturity (see Source above), expect to find most of the matrix already covered
   — your job is to find and close the **genuine** remaining gaps, not to pad the test count.
   A plausible gap to check specifically: does any existing test assert that all twelve
   `IRREVERSIBILITY_DENYLIST` items individually resolve to a non-`Allow` verdict in **every**
   mode including YOLO (not just YOLO, which is the one mode where the denylist is the *only*
   thing standing between "auto-approve everything" and an irreversible action)? If that
   specific combination is already covered by `yolo_mode_auto_approves_except_denylist` or
   similar, say so in the PR description and add only what's missing.
2. **Wire `cargo-mutants` into CI**, scoped to `backend/src/policy.rs` and
   `backend/src/voidwatch/**` (ADR-005 constraint 3 — not the whole crate). The job must run
   on every PR touching those paths (or on every PR, if path-filtering `cargo-mutants` proves
   more complex than it's worth — document the tradeoff you picked) and fail the build on any
   surviving mutant. If a first run surfaces surviving mutants in code this task has no grant
   to fix (non-test code in `policy.rs`/`voidwatch/**`), do not fix them under this ADR —
   document each one in the PR description as a follow-up finding; fixing them is a separate
   task with its own spec (ADR-006 explicitly withholds non-test code).

## Contract (verbatim, `docs/gap-analysis.md` §3)

> Policy/Voidwatch (P0 output) | Exhaustive matrix tests + `cargo-mutants` | The security
> boundary gets the EDD's full G1 treatment

## Architecture decisions already made (ADR-005 / ADR-006 — do not re-litigate)

- Mutation testing targets `policy.rs` + `voidwatch/**` only, not the whole crate.
- Test additions in `policy.rs`/`voidwatch/**` may only touch `#[cfg(test)]`-gated code —
  any non-test fix belongs to a follow-up task.
- The new CI job is additive; the existing `backend` job (clippy, `cargo test`) is untouched.

## Files to touch

- `backend/src/policy.rs` — **forbidden zone, ADR-006, test code only**.
- `backend/src/voidwatch/mod.rs`, `mode.rs`, `risk_class.rs`, `denylist.rs`,
  `allowlist_seed.rs` — **forbidden zone, ADR-006, test code only** (only the files where a
  genuine gap is found need edits — don't touch files you don't need to).
- `.github/workflows/ci.yml` — **forbidden zone, ADR-005**, new job only.
- Possibly `backend/Cargo.toml`/`Cargo.lock` if `cargo-mutants`-related config needs a
  `[dev-dependencies]` addition — check whether `cargo-mutants` needs to be a crate dependency
  at all first; it's normally installed as a standalone CLI tool in the CI runner
  (`cargo install cargo-mutants` or a pinned-version download step), not a `Cargo.toml` entry.
  If you do need a `Cargo.toml` change, it falls under ADR-005's Cargo.toml grant.

## Explicitly not to touch

- Any non-`#[cfg(test)]` code in `policy.rs`/`voidwatch/**` — even a "trivial" fix. Document
  and defer instead.
- `backend/src/api/mcp.rs`, `backend/src/api/studio.rs`, `backend/src/api/integrations.rs`,
  `backend/src/api/ai_ask.rs` — the AI-ingress call sites, out of both ADRs' scope.
- Any existing job in `ci.yml` other than adding the new one.
- `backend/src/db/mod.rs`, `backend/src/auth/**`, `backend/src/api/auth.rs`,
  `backend/src/oidc.rs`, `backend/src/api/secrets.rs` — unrelated forbidden zones.

## Acceptance tests (name before implementing)

- Exact names depend on what the audit (deliverable 1) finds missing — do not pre-commit to
  fabricated test names for gaps that may not exist. At minimum, name and justify:
- `irreversibility_denylist_items_deny_or_require_approval_in_every_mode` (or confirm an
  equivalent already exists and cite it) — the specific combination called out in Scope
  above: all twelve denylist items × all four modes, not just YOLO.
- `mutation_testing_ci_job_targets_policy_and_voidwatch_only` — not a Rust test; a CI
  smoke-check (can be as simple as a documented manual verification step in the PR body, or a
  `shellcheck`/dry-run of the new job's path filter) proving the job doesn't silently expand
  to the whole crate.

## Forbidden zones for this task

`backend/src/policy.rs`, `backend/src/voidwatch/**` (ADR-006, test-only).
`.github/workflows/ci.yml` (ADR-005, new job only).

## Review tier

Full line review (EDD §15.5 — `vt-voidwatch`/policy-engine equivalent, even for test-only
diffs, per CLAUDE.md's forbidden-zone treatment of this module).
