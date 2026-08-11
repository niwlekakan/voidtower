# ADR-010 — Route and action registry convergence grant

**Status:** Accepted (signed by delegated agent Codex under operator authorization on 2026-08-12)
**Date:** 2026-08-12
**Task:** `.devteam/active/S0-03-route-action-registry-convergence.md`

## Context

S0-03 replaces independently maintained session-role, bearer-scope, HTTP-risk, approval, and
AI-action classifications with a typed shared registry. The migration necessarily changes
`backend/src/auth/scope_enforce.rs` and Voidwatch's risk/denylist consumers, which are forbidden
zones under `CLAUDE.md`. It also touches AI ingress classification in MCP and may need test-only
inventory access in integrations or Studio.

The approved design is behavior-preserving. Registry convergence must not become an opportunity
to broaden credentials, lower risk, weaken mandatory approval, or bypass the existing Voidwatch
choke point. In particular, the currently contradictory MCP bearer behavior remains denied until
a separate audited policy change addresses it.

## Decision

Authorize a narrow structural migration to one immutable Rust route/action registry:

- mechanically transcribe existing session, bearer, risk, approval, and AI/action outcomes;
- make every route's bearer decision explicit while retaining default denial for unknown routes;
- make every dispatched action's kind, ingress, risk, approval, and AI exposure explicit;
- make session, bearer, risk, denylist, and MCP consumers query the shared metadata;
- remove superseded editable ledgers and duplicate inventory parsers;
- add exhaustive and real-path regression tests proving behavior equivalence.

## Granted paths

```granted-paths
backend/src/action_registry.rs
backend/src/auth/scope_enforce.rs
backend/src/voidwatch/risk_class.rs
backend/src/voidwatch/denylist.rs
backend/src/voidwatch/mod.rs
backend/src/api/mcp.rs
backend/src/api/integrations.rs
backend/src/api/studio.rs
```

## Explicitly NOT granted

- `backend/src/policy.rs` or `backend/src/voidwatch/mode.rs`.
- Any other file under `backend/src/auth/`.
- `backend/src/api/auth.rs`, `backend/src/api/bearer_auth.rs`, or `backend/src/oidc.rs`.
- Changes to session role semantics, bearer scope names, token issuance, or handler authorization.
- Changes to risk classifications, denylist membership, mode-ladder semantics, or approval
  outcomes.
- Secrets/crypto code, database schema, migrations, CI/workflows, or the devteam harness.
- Enabling MCP bearer access or adding any new route, action, tool, feature, or external call.

## Constraints

1. Every existing observable access decision is preserved, including fail-closed unknowns.
2. Registry entries are compile-time immutable and require every security dimension explicitly.
3. Live route and dispatcher inventories are checked bidirectionally against registry metadata.
4. Hard-denylisted operations remain irreversible and require approval in every mode.
5. Every AI-callable mutation continues through `voidwatch::evaluate`.
6. Unknown actions retain the safest existing risk and mutating-action defaults.
7. MCP bearer denial remains unchanged and receives an explicit regression test.
8. No second editable security ledger remains when S0-03 completes.

## Consequences

Security review gains one authoritative inventory and future routes/actions cannot silently omit
a policy dimension. The migration is larger than a local table edit, but it avoids generating the
Axum router or introducing build-time code generation. Policy contradictions become visible
follow-ups instead of being silently resolved during structural work.

