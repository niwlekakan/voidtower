# ADR-006 — Voidwatch/policy test-hardening grant for Phase P1

**Status:** Accepted (signed by operator niwlekakan on 2026-07-11)
**Date:** 2026-07-11
**Expires:** at P1 phase exit (`scripts/devteam/adr.sh revoke ADR-006`)

## Context

Gap-analysis §3's P1 table, row 1: "Policy/Voidwatch (P0 output) | Exhaustive matrix tests +
`cargo-mutants` | The security boundary gets the EDD's full G1 treatment." P0 already left
this module in unusually good shape — `backend/src/voidwatch/mod.rs` has 17 tests including
an `exhaustive_mode_by_risk_class_matrix` table test, `risk_class.rs` and `denylist.rs` each
have their own parser-based/exhaustiveness tests, and `policy.rs` has a data-driven
"default verdict by actor class" test table (all verified directly in source at ADR-drafting
time, `docs/codebase-map.md` §6). P0's own forbidden-zone grant, ADR-001, is scoped to P0 and
expired at P0 phase exit (`.devteam/phase-exit/P0.ok`) — a fresh grant is needed for any P1
task that must add to or reorganize these tests, even though no verdict-semantics change is
intended.

Because `backend/src/policy.rs` and `backend/src/voidwatch/**` are already this mature, the
P1 task under this grant is expected to spend most of its effort on **closing genuine gaps
found by inspection** (if any) and on wiring `cargo-mutants` into CI (covered by ADR-005, not
this one) — not on writing a large volume of new matrix tests from scratch.

## Decision

Task specs numbered `P1-*` that cite **ADR-006** are granted modification rights to exactly
the paths below, **test code only**:

```granted-paths
backend/src/policy.rs
backend/src/voidwatch/mod.rs
backend/src/voidwatch/mode.rs
backend/src/voidwatch/risk_class.rs
backend/src/voidwatch/denylist.rs
backend/src/voidwatch/allowlist_seed.rs
```

"Test code only" means: additions/edits inside `#[cfg(test)] mod tests { ... }` blocks (or
equivalent `#[cfg(test)]`-gated items) in these files. Non-test code in these files —
`evaluate()`, `mode_pre_pass()`, `trusted_mode_verdict()`, `PolicyRule`/`check()`,
`ROUTE_RISK_CLASSES`, `IRREVERSIBILITY_DENYLIST`, `for_action()`, `get_mode()`/`set_mode()`,
`seed_default_allowlist_if_empty()` — is **not** granted; a citing task that finds a genuine
bug in this non-test code (as opposed to a test gap) stops and escalates rather than fixing
it under this ADR, per CLAUDE.md's "never improvise architecture" rule for forbidden-zone
work outside a grant's scope.

## Explicitly NOT granted

- Any change to verdict semantics, the mode ladder's behavior, risk classification of any
  action/route, or the denylist's contents (the twelve ADR-004 items) — this is a
  test-coverage task, not a policy-design task.
- `backend/src/api/mcp.rs`, `backend/src/api/studio.rs`, `backend/src/api/integrations.rs`,
  `backend/src/api/ai_ask.rs` (the AI-ingress choke-point call sites) — out of scope; if a
  gap is found in how they call `voidwatch::evaluate`, escalate, don't patch.
- `.github/workflows/ci.yml` — covered by ADR-005, not this ADR; cite both if a task needs
  both.
- Everything else in CLAUDE.md's forbidden list.

## Constraints

1. **Read before you write.** The citing task's first step is enumerating what
   `voidwatch::mod::tests`, `risk_class::tests`, `denylist`'s own tests, and `policy::tests`
   already cover, against the mode × risk_class × denylist-item matrix gap-analysis asks for.
   Only genuinely missing combinations get new tests. Do not duplicate an existing test under
   a new name to pad coverage numbers.
2. If mutation testing (wired by the sibling ADR-005 CI job) surfaces a *surviving mutant*
   in non-test code this ADR doesn't grant, the fix belongs to a follow-up task with its own
   spec — this task documents the finding (e.g. in its PR description or a filed issue) and
   does not chase it into forbidden code.
3. Any new test file (as opposed to an addition to an existing `mod tests` block) must follow
   the codebase's established pattern (`voidwatch::tests::create_policy_tables` being
   `pub(crate)` specifically so sibling test modules can reuse it) rather than duplicating
   fixture setup.

## Consequences

P1's policy/voidwatch verification work can proceed without a semantics-change grant, which
keeps the review bar appropriately low (test-only diffs) even though the files touched are
maximally sensitive. At P1 exit this ADR is revoked and these files close again.
