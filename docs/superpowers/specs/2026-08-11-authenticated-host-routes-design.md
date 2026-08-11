# Authenticated Host Routes Design

## Context

The exhaustive session matrix identifies six live routes whose handlers never validate a session: host capabilities, full diagnostics, build/version metadata, and three asynchronous model-job status feeds. They are currently represented as `Role::Public` only because that matches shipped behavior, not because public access was intended.

The risk registry already classifies all six as reads. Bearer enforcement explicitly exempts capabilities from scopes, grants diagnostics to `diagnostics:read`, and default-denies the other four by omission. S0-02 makes the session, bearer, and demo decisions explicit without changing risk semantics.

## Authorization decisions

| Route group | Session tier | Demo behavior | Bearer behavior | Rationale |
|---|---|---|---|---|
| Host capabilities | Any session | Allowed | `diagnostics:read` | Needed across the authenticated UI; reveals installed host tooling |
| System version | Any session | Allowed | `diagnostics:read` | Ordinary authenticated metadata; can reveal branch/dirty state |
| Diagnostics | Admin | Denied | `diagnostics:read` | Exposes detailed host checks, paths, ports, and service state |
| Model job status | Admin | Denied | Session-only | Belongs to admin-only model download/pull/create workflows; no model token scope exists |

Public health, status, settings-public, OpenAI-compatible proxy, authentication-entry, and integration-discovery routes remain unchanged.

## Handler design

Each affected handler validates the existing `vt_session` cookie before reading host or in-memory job state. Capabilities and version require only a valid session. Diagnostics and model-job status reuse the existing positive owner/admin pattern in their modules. No data shape, subprocess behavior, or job lifecycle changes.

The session matrix reclassifies the two ordinary reads as `RoleTier::Session` and the four administrative reads as `RoleTier::Admin`. Real-router tests prove credential-free rejection and representative allowed/denied roles.

## Bearer design

The bearer registry changes capabilities from `NoScopeRequired` to `Scope("diagnostics:read")`, keeps diagnostics at that scope, and adds version to it. A new explicit `SessionOnly` requirement classifies the three model-job status routes while producing the same fail-closed 403 that an unlisted bearer route receives today.

This explicit marker is intentionally narrow and temporary groundwork for S0-03’s unified action registry. It does not create a new scope or change human-session behavior.

## Error behavior and tests

- Missing/invalid session: 401 before host inspection or job lookup.
- Valid session below Admin: 403 on diagnostics/model status.
- Missing/wrong bearer scope: existing structured `insufficient_scope` 403.
- Model status via bearer: the same 403 even for an admin-owned token.
- Valid requests preserve current response behavior, including 404 for unknown model-job IDs.

Tests drive the real Axum router, retain exhaustive route/matrix/risk checks, and add bearer-registry regression coverage after ADR authorization.

## Scope

No public-by-design route, risk class, Voidwatch policy, token inventory, response schema, model operation, or frontend behavior changes.
