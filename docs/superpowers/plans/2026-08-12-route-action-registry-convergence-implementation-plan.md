# Route and Action Registry Convergence Implementation Plan

> Design source: `docs/superpowers/specs/2026-08-12-route-action-registry-convergence-design.md`

## Objective

Replace the independent session-role, bearer-scope, HTTP risk, and AI-action ledgers with one
typed, behavior-preserving metadata source. A new route or dispatched action must fail tests
unless every security dimension is explicit.

## Guardrails

- Preserve all currently shipped authorization, risk, approval, and AI-exposure outcomes.
- Do not enable MCP bearer access as part of convergence.
- Keep unknown bearer routes denied and unknown actions at the existing safest risk default.
- Do not change handlers, role semantics, scope names, Voidwatch's mode ladder, database schema,
  CI, or API response shapes.
- Retain the named irreversibility denylist and its rationale.
- Touch forbidden-zone consumers only under the narrow S0-03 ADR.
- Work test-first and run `scripts/devteam/gates.sh` before committing implementation.

## Files in scope

Expected production and test files:

- New `backend/src/action_registry.rs`
- `backend/src/main.rs` (module declaration only)
- `backend/src/api/authz_matrix.rs`
- `backend/src/api/authz_matrix_tests.rs`
- `backend/src/api/scope_bypass_tests.rs`
- `backend/src/api/mcp.rs`
- `backend/src/api/integrations.rs` only if a dispatcher inventory must be exposed to tests
- `backend/src/auth/scope_enforce.rs`
- `backend/src/voidwatch/risk_class.rs`
- `backend/src/voidwatch/denylist.rs`
- `backend/src/voidwatch/mod.rs` only for type re-exports or registry lookup wiring
- `docs/api.md`
- `docs/codebase-map.md`
- S0-03 task/ADR/design/plan documents

`api/mod.rs` should remain unchanged unless the test-only route extractor cannot be centralized
without a module declaration adjustment. Any additional production file requires a scope check
before editing.

## Task 1 — Establish the contract with failing inventory tests

Add the S0-03 task specification and narrow forbidden-zone ADR before product changes. Then add
tests that express the new contract but fail against the current architecture:

1. The live Axum route set and registry route set are equal in both directions.
2. Route keys are unique.
3. Canonical action names are unique and every dispatched MCP action is registered.
4. Hard-denylisted routes resolve to irreversible, always-approved metadata.
5. The two MCP endpoints explicitly retain bearer denial while declaring handler bearer
   credentials.

The first red run must fail because the shared registry/module does not exist or because its
initial empty fixture lacks live routes—not because of an unrelated compile error.

Targeted red commands:

```bash
cd backend
cargo test action_registry
cargo test every_registered_route_has_complete_metadata
cargo test every_dispatched_mcp_tool_has_action_metadata
```

## Task 2 — Introduce typed metadata and mechanically merge route ledgers

Create `action_registry.rs` with closed enums for HTTP method, session policy, concrete
credential kind, bearer policy, approval policy, AI exposure, and ingress kind. Move or re-export
the existing risk and role-tier types so consumers do not define competing versions.

Populate one sorted `ROUTES` slice by mechanically joining:

- every entry in `SESSION_ROLE_MATRIX`;
- every entry in `ROUTE_RISK_CLASSES`;
- explicit bearer behavior for every route, where previous absence becomes an explicit denied
  value and existing `NoScopeRequired`, scoped, and `SessionOnly` entries retain their outcome;
- handler credential kinds for webhook HMAC, MCP bearer token, node token, and pairing code;
- default non-AI exposure unless a verified ingress requires another value;
- approval derived from existing irreversible classifications and denylist membership.

Add lookup functions that return `Option` for routes and preserve fail-closed decisions in their
callers. Do not add wildcard/fallback metadata records.

Green commands:

```bash
cd backend
cargo test action_registry
cargo test every_registered_route_has_complete_metadata
```

## Task 3 — Migrate session and bearer consumers

Replace `SESSION_ROLE_MATRIX` with a registry projection used by the existing exhaustive and
real-router session tests. Preserve `WS_UPGRADE_ROUTES` only if it remains a transport-test
fixture rather than security policy.

Replace `ROUTE_SCOPES` and its private requirement type with registry bearer lookup. Preserve:

- no-op behavior for ordinary human sessions;
- correctly scoped bearer access;
- structured `insufficient_scope` denial for wrong, denied, and unknown bearer routes;
- public-with-bearer behavior only for routes already carrying `NoScopeRequired`;
- explicit session-only behavior for model job status;
- current denial of MCP routes before handler token validation.

Run targeted regressions after each consumer migration:

```bash
cd backend
cargo test authz_matrix
cargo test scope_bypass
cargo test bearer
```

## Task 4 — Migrate route risk, approval, and denylist consumers

Replace `ROUTE_RISK_CLASSES` with the route registry's risk field. Consolidate the duplicated
source parser into one test helper and keep the bidirectional route check.

Keep `IRREVERSIBILITY_DENYLIST` as the documentation-grade list of item IDs, descriptions, and
route references. Change its tests/lookups to resolve route metadata and require both:

- `RiskClass::Irreversible`;
- `ApprovalPolicy::Always`.

Do not change mode evaluation or the set of denylisted routes.

Targeted commands:

```bash
cd backend
cargo test risk_class
cargo test denylist
cargo test voidwatch
```

## Task 5 — Converge AI and automation action metadata

Populate `ACTIONS` from the canonical names currently passed to `voidwatch::evaluate` and the
MCP tool dispatcher. Each record declares ingress, action kind/read-vs-mutating exposure, risk,
approval, and AI exposure.

Replace `risk_class::for_action`'s hand-written match and MCP's independent `READ_ONLY_TOOLS`
classification with registry lookups. Preserve their fail-safe fallbacks:

- unknown action risk remains irreversible;
- unknown MCP tool action kind remains mutating;
- every actually dispatched tool/action must have a registry record in tests.

Where one canonical action is used by multiple ingresses, represent the ingress set explicitly
rather than duplicate the action record.

Targeted commands:

```bash
cd backend
cargo test every_dispatched_mcp_tool_has_action_metadata
cargo test for_action
cargo test mcp
cargo test integrations
cargo test studio
```

## Task 6 — Remove compatibility ledgers and prove behavior equivalence

Delete the superseded editable tables and duplicated route extractors. Search for stale symbols
and narrative claims:

```bash
rg -n "SESSION_ROLE_MATRIX|ROUTE_SCOPES|ROUTE_RISK_CLASSES|READ_ONLY_TOOLS" backend/src
```

Only compatibility re-exports justified by an unchanged external test interface may remain; no
second editable list may survive. Add projection snapshots/equality tests if a temporary
compatibility API remains.

Run representative real-router probes for:

- public requests with and without bearer headers;
- valid session roles and insufficient roles;
- valid, wrong-scope, session-only, handler-credential, and unknown bearer cases;
- read, mutating, destructive, irreversible, and unknown AI actions;
- MCP and Studio use of the shared invocation choke point.

## Task 7 — Documentation, full gates, and review

Update `docs/api.md` and `docs/codebase-map.md` to name `action_registry.rs` as the authoritative
inventory and explain handler checks as defense in depth. Record the MCP mismatch as a separate
follow-up rather than a fixed behavior.

Run the repository gate exactly as required:

```bash
scripts/devteam/gates.sh
```

Then inspect `git diff --stat main...HEAD` and every changed file. Confirm the untracked CMDB
handoff, manual test script, and project reality audit are not included. Complete independent
Standards and Spec reviews before publishing the branch.

## Acceptance checklist

- [ ] Every live route has exactly one complete metadata record.
- [ ] Every dispatched AI/automation action has exactly one complete metadata record.
- [ ] Session checks and tests consume the registry.
- [ ] Bearer middleware consumes explicit registry policy and remains fail-closed.
- [ ] Route and action risk lookup consume the registry.
- [ ] Denylist routes are irreversible and always-approved in the registry.
- [ ] AI exposure and action kind are explicit for every canonical action.
- [ ] No independent editable security ledger remains.
- [ ] MCP bearer behavior is unchanged and its contradiction is covered by a regression.
- [ ] Targeted tests and `scripts/devteam/gates.sh` pass.
- [ ] Documentation identifies the new source of truth.
- [ ] Two-axis review reports no unresolved findings.
