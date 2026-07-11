# Task P1-01: Authz matrix — every route asserts its required role/scope mechanically

## Status: Ready
**ADR:** none
Depends-On: none
Requires-Path: backend/src/api/mod.rs, backend/src/auth/scope_enforce.rs, backend/src/api/scope_bypass_tests.rs, backend/src/voidwatch/risk_class.rs

## Source

- `docs/gap-analysis.md` §3, P1 table row 2: "Auth (sessions, TOTP, OIDC, token scopes) |
  Authz matrix test: enumerate the router (~60 API modules), assert every route declares
  required role/scope; unauthenticated + wrong-role probes generated per route | Catches the
  'new endpoint forgot auth' class mechanically, forever."
- `docs/codebase-map.md` §2 (route → handler map, ~60 files under `backend/src/api/`), §6
  ("Auth pattern (repeated per-file, not shared middleware)" — nearly every `api/*.rs` file
  defines its own local `require_user`/`require_admin`/`require_owner`; there is no shared
  extractor), and §4's "scope-bypass gap — CLOSED" section (`auth::scope_enforce::ROUTE_SCOPES`
  is the existing deny-by-default table for **token**-originated requests only).
- `backend/src/voidwatch/risk_class.rs`'s `every_registered_route_has_a_risk_class` test —
  the precedent pattern this task's exhaustiveness test should follow: parse the route table
  out of `api::router()` and assert a bijection against a hand-maintained classification.
- `backend/src/api/scope_bypass_tests.rs` — the precedent pattern for *driving requests
  through the real router* end-to-end via `tower::ServiceExt::oneshot` rather than unit-testing
  handler functions in isolation.

## Scope

This repo's auth model has two independent layers that were verified and closed separately:
`auth::scope_enforce::ROUTE_SCOPES` (P0-06, forbidden zone, not touched by this task) governs
**Bearer-token** requests; each handler file's own local `require_user`/`require_admin`/
`require_owner` governs **session-cookie** requests, with no central declaration anywhere of
which role a given route requires — that's the gap this task closes. Build a **new**,
non-forbidden-zone, hand-maintained table declaring the minimum session role each route in
`api::router()` requires (`none` for public routes like `/api/health`, `/status`,
`/api/settings/public`, `/v1/*`), an exhaustiveness test tying it to the live route table
(same parser-based technique `risk_class.rs` already uses — do not assume you can import
`risk_class::ROUTE_RISK_CLASSES`'s parser; write your own or factor a shared helper only if
it doesn't require touching `voidwatch/risk_class.rs`, which is out of this task's grant-free
scope), and **generated probes**: for a sample of routes at every declared role tier, an
actual unauthenticated request and an actual wrong-role-session request, both driven through
the real router, both asserted to fail with 401/403 as appropriate.

Do not attempt to touch `auth::scope_enforce::ROUTE_SCOPES` or any file under
`backend/src/auth/` — this task has no ADR and must not need one. If, while building the
route→role table, you find a route whose *actual* handler behavior contradicts what its role
looks like it should be (e.g. a route with no role check at all), do not fix the handler —
that's a security finding, not this task's job. Note it in the PR description and, if it
looks exploitable, escalate instead of committing quietly.

## Contract (verbatim, `docs/gap-analysis.md` §3)

> Auth (sessions, TOTP, OIDC, token scopes) | Authz matrix test: enumerate the router (~60
> API modules), assert every route declares required role/scope; unauthenticated + wrong-role
> probes generated per route | Catches the "new endpoint forgot auth" class mechanically,
> forever

## Files to touch

- New: `backend/src/api/authz_matrix.rs` — the hand-maintained `(method, path, Role)` table
  (`Role` enum: something like `Public`, `Session(min_role)`, mirroring the existing role
  ladder `owner > admin > operator > viewer` plus `guest`/`demo`/`member`, per
  `docs/codebase-map.md` §3's "Role ladder as of this map") and its exhaustiveness test.
- New: `backend/src/api/authz_matrix_tests.rs` — the generated probe tests (unauthenticated +
  wrong-role, driven via `tower::ServiceExt::oneshot` against the real router, following
  `scope_bypass_tests.rs`'s setup pattern).
- `backend/src/api/mod.rs` — one line each: `#[cfg(test)] mod authz_matrix_tests;` and (if
  `authz_matrix.rs` needs to be reachable outside `#[cfg(test)]`, e.g. because the
  exhaustiveness test lives in it directly) `mod authz_matrix;`. This file is **not** in
  CLAUDE.md's forbidden list, but it has a documented `rustfmt`-cascade hazard
  (`docs/codebase-map.md` §6, `router()`'s `#[rustfmt::skip]`) — hand-format your added lines
  to match the surrounding style; do not run `cargo fmt`/`cargo fmt --all` (CLAUDE.md hard
  rule; `gates.sh`'s own G0 step formats only the files your diff touches).

## Explicitly not to touch

- `backend/src/auth/**`, `backend/src/api/auth.rs`, `backend/src/oidc.rs` — forbidden zone,
  no ADR granted to this task.
- `backend/src/voidwatch/**`, `backend/src/policy.rs` — unrelated layer, forbidden zone.
- Any individual `api/*.rs` handler file's `require_user`/`require_admin`/`require_owner` —
  read them to build the table, do not edit them.

## Acceptance tests (name before implementing)

- `every_registered_route_declares_a_required_role` — exhaustiveness: parses `api::router()`
  (or reuses `risk_class.rs`'s route list if you can do so without editing that file) and
  asserts a bijection against the new table; fails on both an unclassified new route and a
  stale entry for a removed one.
- `unauthenticated_request_is_rejected_for_every_non_public_route` — generated, not sampled:
  every route the table marks non-`Public` gets a real `oneshot` request with no cookie/token
  and must return 401 (not 200, not a panic, not a 500 that happens to look like a rejection).
- `wrong_role_session_is_rejected_for_every_role_gated_route` — for each role tier above
  `viewer`, mint a real session at a lower role (via the same login flow
  `scope_bypass_tests.rs` uses) and assert every route requiring a higher role returns 403.
- `public_routes_remain_reachable_without_auth` — the inverse check: routes the table marks
  `Public` (health, status, settings/public, `/v1/*`) must NOT be accidentally caught by the
  wrong-role/unauthenticated assertions above — a regression guard against over-classifying.
- `member_and_guest_and_demo_roles_are_represented_in_the_matrix` — the newer role ladder
  entries (`guest`, `demo`, `member`, per `docs/codebase-map.md` §3) aren't accidentally
  treated as equivalent to `viewer`; at least one route each role is specifically gated on
  (or explicitly excluded from) is asserted.

## Forbidden zones for this task

None — this task must complete without touching any CLAUDE.md forbidden-zone path. If you
find yourself needing to, stop and escalate; that means the task's premise was wrong.

## Review tier

Boundary review (EDD §15.5) — new test infrastructure over existing auth behavior, not a
change to `vt-auth`-equivalent code itself.
