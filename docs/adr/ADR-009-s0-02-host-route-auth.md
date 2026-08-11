# ADR-009 — Bearer classification for S0-02 host routes

**Status:** Accepted (signed by operator niwlekakan on 2026-08-12)
**Date:** 2026-08-11
**Task:** `.devteam/active/S0-02-authenticated-host-routes.md`

## Context

S0-02 closes six unintended unauthenticated routes. Session checks alone are insufficient for `GET /api/capabilities`: the bearer scope registry currently marks it `NoScopeRequired`, so any valid token would retain access after the handler begins requiring a session. The three model-job status routes are default-denied to bearer clients only by being absent from the registry, which does not satisfy S0-02’s explicit-classification requirement.

`backend/src/auth/scope_enforce.rs` is a forbidden auth zone. ADR-003 originally established its default-deny architecture, but that grant is revoked and cannot authorize this change.

## Decision

Authorize a narrow registry-only update:

- Change capabilities from `NoScopeRequired` to `Scope("diagnostics:read")`.
- Keep diagnostics at `Scope("diagnostics:read")`.
- Add system version at `Scope("diagnostics:read")`.
- Add an explicit `SessionOnly` requirement for the three model download/pull/create status routes.
- Preserve the existing structured insufficient-scope denial response and the default-deny behavior for all unlisted routes.

No new token scope is introduced. `SessionOnly` documents a deliberate denial; it does not create another authorization path.

## Granted paths

```granted-paths
backend/src/auth/scope_enforce.rs
```

## Explicitly NOT granted

- Any other file under `backend/src/auth/`.
- `backend/src/api/auth.rs`, `backend/src/api/bearer_auth.rs`, or `backend/src/oidc.rs`.
- Token creation, capability tiers, `ALL_SCOPES`, or database/schema changes.
- Voidwatch policy, mode, denylist, or risk-class changes.
- Public-by-design route behavior.

## Constraints

1. The only new requirement variant is an explicit fail-closed session-only marker.
2. Human session-cookie requests remain unaffected by bearer middleware.
3. Real-router tests cover allowed, insufficient-scope, and session-only bearer requests.
4. The exhaustive route-scope default-deny invariant remains green.
5. No route receives broader bearer access than it has before S0-02.

## Consequences

Capabilities and version become available to correctly scoped diagnostic clients; detailed model-job status remains human-admin-only. The bearer registry explicitly represents all six S0-02 routes until S0-03 replaces the parallel registries with shared action metadata.
