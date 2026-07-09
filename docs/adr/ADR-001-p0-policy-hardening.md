# ADR-001 — Forbidden-zone grant for Phase P0 (AI blast-radius hardening)

**Status:** Accepted (human-authored)
**Date:** 2026-07-09
**Expires:** at P0 phase exit (gap-analysis §2 exit criteria met and verified by the operator)

## Context

`docs/gap-analysis.md` Phase P0 requires modifying files that CLAUDE.md lists as forbidden
zones — by design: P0 *is* the hardening of the policy engine and the AI ingress surface.
Without an explicit grant, workers correctly refuse and escalate (observed on P0-01/P0-02).

## Decision

Task specs numbered `P0-*` that cite **ADR-001** are granted modification rights to exactly the
paths in the machine-readable block below (parsed by `scripts/devteam/adr.sh check`, enforced
by `gates.sh` — a diff touching anything outside this block fails the gate):

```granted-paths
backend/src/policy.rs
backend/src/voidwatch.rs
backend/src/voidwatch/**
backend/src/api/mcp.rs
backend/src/api/integrations.rs
backend/src/api/studio.rs
backend/src/api/ai_ask.rs
```

In prose, that is:

- `backend/src/policy.rs` — and new files under a `backend/src/voidwatch/` module if the
  implementation extracts into one
- `backend/src/api/mcp.rs`, `backend/src/api/integrations.rs`, `backend/src/api/studio.rs`,
  `backend/src/api/ai_ask.rs`, and webhook ingress handlers — **only** to route their
  mutating actions through the policy choke point and attach risk classes; no other
  behavioral changes
- Redaction middleware insertion points on AI-bound response serialization (P0.5)
- Token-scope handling for integration tokens (P0.6) — the minting/scoping code paths only

## Explicitly NOT granted (remain closed under this ADR)

- `backend/src/auth/`, `backend/src/api/auth.rs`, `backend/src/oidc.rs` (session/login auth)
- Schema in `backend/src/db/mod.rs` — new tables needed by P0 (e.g. approvals) must be
  proposed via escalation for a spec amendment; additive `CREATE TABLE` only, no alters
- `.github/workflows/`, `scripts/devteam/`, `CLAUDE.md`, the plan docs, this file

## Constraints on all work under this grant

1. AI-actor verdict semantics are **default-deny** with explicit allowlist (gap-analysis P0.2);
   user-session RBAC behavior unchanged.
2. The irreversibility denylist (EDD §10.3 / gap-analysis P0.4) is hardcoded and not
   editable via any API.
3. Every action name touched gets a `risk_class`; the classification table must be
   exhaustive (compile-time or test-enforced).
4. The mode×risk×actor policy matrix is table-tested exhaustively in the same PR that
   changes verdict logic.
5. A generated allowlist preserving currently-observed automation behavior ships with the
   default-deny flip, so upgrade does not break running automations.

## Consequences

`gates.sh` accepts forbidden-zone diffs while the active spec cites "ADR-"; the scope above
is enforced by the adversarial reviewer and the operator's full line review — every P0 PR is
full-review tier regardless of gate status. On P0 exit, this ADR expires and the zones close.
