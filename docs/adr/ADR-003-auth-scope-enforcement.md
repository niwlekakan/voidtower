# ADR-003 — Auth-model grant for Phase P0.6 (bearer-token scope enforcement)

**Status:** Accepted (signed by operator niwlekakan on 2026-07-09)
**Date:** 2026-07-09
**Expires:** at P0 phase exit (gap-analysis §2 exit criteria met and verified by the operator)

## Context

`.devteam/blocked/P0-06-scope-token-and-bearer-auth-bypass.md` is blocked pending this ADR.
Per `docs/codebase-map.md` §4 ("The scope-bypass gap") and confirmed directly against source
for this ADR:

1. `bearer_auth::middleware` (`backend/src/api/bearer_auth.rs:10-27`) is mounted globally on
   `main_router` (`backend/src/api/mod.rs:412`). When a request carries `Authorization: Bearer
   <token>` and no session cookie, it calls `auth::validate_api_token_any()`
   (`backend/src/auth/mod.rs:292-318`) — which checks only that the token hash exists and isn't
   expired, **never scopes** — and on success mints a real 1-hour session via
   `auth::create_temp_session()` (`auth/mod.rs:321-332`) for the token's owning user.
2. Every downstream handler's `require_user`/`require_admin`/`require_owner` helper — a pattern
   copy-per-file across ~20 modules (`containers.rs:46`, `integrations.rs:94`, `disaster.rs:23`,
   `firewall.rs:88`, etc., confirmed via grep) — then sees an ordinary session and applies **the
   token owner's actual role**, never the token's declared scope.
3. `ALL_SCOPES` (`integrations.rs:29-55`, 23 named scopes) is consulted in exactly two places in
   the whole codebase: `auth::validate_api_token()` (the scoped variant), called only from
   `events.rs:49/58` and `integrations.rs:551/560`. Every other scope-implying endpoint —
   `POST /api/system/update`, `GET /api/secrets/:id/reveal`, `POST /api/storage/format`,
   `POST /api/disaster/*`, etc. — performs no scope check at all.
4. Net effect: a token minted with only `metrics:read`, if owned by an admin/owner user (the
   only roles allowed to mint tokens, `integrations.rs:94-107`), can call any admin/owner-gated
   endpoint. The scoped-token system in the Integrations UI is decorative for the vast majority
   of the API surface.

Gap-analysis P0.6 ("scope down `VOIDTOWER_TOKEN`... mode ladder + scopes compose") cannot land
as written without fixing this upstream bypass first — the task spec says so explicitly and is
correctly `BLOCKED` without an ADR, since the fix requires touching `backend/src/auth/mod.rs`
and `backend/src/api/bearer_auth.rs`, both hard forbidden zones not covered by ADR-001 (ADR-001
explicitly excludes `backend/src/auth/`, `backend/src/oidc.rs`, `backend/src/api/auth.rs`).

## Decision

**Enforcement architecture: a single mandatory middleware layer, not per-handler
session-carries-scopes.** The task spec's own "Design questions" section poses this as the
central open call. Choosing the middleware layer for three reasons:

1. **Consistency with P0-01.** This whole phase's operating principle (EDD §3.2, gap-analysis
   P0.1) is "route every reachable action through one choke point," already implemented for the
   AI-ingress side as `voidwatch::evaluate()`. A second, parallel choke point for token-scope
   enforcement — checked once, centrally, before any handler runs — is the same idea applied to
   the auth layer, not a competing pattern.
2. **Diff size and audit surface.** Session-carries-scopes requires editing every one of the
   ~20 `require_user`/`require_admin`/`require_owner` copies to additionally consult scopes,
   each a forbidden-zone-adjacent change needing individual review. A middleware layer is one
   new file plus one line wiring it into the router; the per-route scope requirement lives in
   one exhaustive, reviewable table instead of being scattered across 20 files.
3. **Preserves the human-session invariant cleanly.** The middleware is a no-op whenever the
   request did not originate from a bearer token (i.e., `bearer_auth::middleware` did not mark
   the request — it already inserts a `ApiTokenActor` extension for exactly this purpose,
   `bearer_auth.rs:23`). Human session-cookie logins pass through completely unchanged, which
   is a direct, structural satisfaction of the spec's own
   `human_session_cookie_login_is_unaffected_by_scope_changes` acceptance test rather than
   something that has to be maintained by convention across 20 edited files.

**Route → required-scope table is a compile-time, exhaustive constant** (same
clippy-exhaustiveness spirit as P0-03's planned `risk_class` table — this ADR does not define
`risk_class` itself, that remains P0-03's deliverable, but the two tables may end up
co-located or cross-referenced by the implementer if that's cleaner; not mandated here). The
middleware rejects with 403 before the handler runs if: the request is token-originated
(`ApiTokenActor` present) AND the route has a required-scope entry AND the token's scopes don't
contain it. A route with no table entry is **not** implicitly permissive — the implementer must
decide (and the acceptance tests must cover) whether an unlisted route defaults to
deny-for-tokens or requires an explicit "no scope required" marker; either is acceptable but
must be a deliberate, tested choice, not a silent gap matching today's bug.

**P0-02 dependency direction: they land independently.** P0-02's own task spec already names a
stopgap for exactly this ambiguity — sourcing its "generated allowlist matching current
observed usage" from `audit_log` filtered by `source = "odysseus"`
(`audit::log_sourced`, `integrations.rs:722-734`) rather than from raw token-actor request logs,
specifically because it anticipated not being able to reliably distinguish AI-originated from
human-originated token use yet. P0-06 does not need to block P0-02, and P0-02 does not need to
block P0-06. Once P0-06 lands, P0-02's default-deny allowlist generation can optionally be
tightened to use real scope data instead of the audit-log stopgap, but that is a future
improvement, not a P0 blocker either direction.

**Capability-tier tokens (part 2 of the task) build on top of part 1, not instead of it.** The
four tiers (read, deploy, exec, admin-never) are a coarser, user-facing minting convenience —
each tier maps to a fixed subset of the existing `ALL_SCOPES` names — layered on top of the now
properly-enforced fine-grained scope check. `admin-never` is a hard invariant: no token
carrying that tier may reach any admin/owner-gated route, enforced by the same middleware table,
not by trusting the mint-time label alone.

**Migration:** the existing `VOIDTOWER_TOKEN` currently has *implicit* full admin/owner access
via the bypass this ADR closes. On this fix landing, mint new capability-scoped tokens
(read/deploy/exec as needed by the AI stack's actual usage, determined the same way P0-02
sources its allowlist) and require the operator to explicitly re-issue `VOIDTOWER_TOKEN` under
the new model — do not silently grandfather the old token into an equivalent-privilege scoped
token, since that would just re-create the god-token under a new name. This is a one-time,
operator-driven cutover, not a backward-compatibility window; the implementer should escalate
narrowly if this proves to break something not anticipated here.

## Granted paths

```granted-paths
backend/src/auth/mod.rs
backend/src/api/bearer_auth.rs
backend/src/auth/scope_enforce.rs
backend/src/auth/scope_enforce/**
backend/src/api/integrations.rs
```

`backend/src/api/integrations.rs` is listed here **in addition to** ADR-001's existing grant for
that file: ADR-001 authorizes routing its mutating actions through the policy choke point only;
this ADR separately authorizes changes to `ALL_SCOPES`, token-minting (`create_token`), and
capability-tier handling in the same file. A diff touching `integrations.rs` for scope/token
reasons should cite this ADR; for choke-point reasons, ADR-001.

## Explicitly NOT granted

- `backend/src/api/auth.rs`, `backend/src/oidc.rs` — login/session-establishment flows are
  unrelated to bearer-token scope enforcement and stay closed.
- `backend/src/db/mod.rs` schema — if `api_tokens`'s existing `scopes` column (already present,
  `auth/mod.rs:348`) is insufficient for capability tiers, propose a schema change via
  escalation for a separate ADR; do not assume one is needed without checking first (a tier
  could plausibly be derived from the existing `scopes` array without a new column).
- Any of the ~20 `require_user`/`require_admin`/`require_owner` handler files, **unless** the
  implementer's route→scope table genuinely cannot be built without a per-file marker (e.g., if
  route metadata isn't otherwise inspectable at the router layer). If that turns out to be
  necessary, escalate narrowly before touching them — the whole point of the Decision above is
  to avoid this.
- Any change to `backend/src/policy.rs` or `voidwatch.rs` — this is an authentication-layer fix,
  not a policy-engine change; if the implementer finds a genuine need to have `voidwatch::evaluate`
  consult scope data, that composition question belongs to P0-01/P0-02's authors, escalate rather
  than reach into those files under this grant.

## Constraints

1. Token-scope enforcement must run as a single middleware, mounted so it observes the
   `ApiTokenActor` extension `bearer_auth::middleware` already sets
   (`bearer_auth.rs:23`) — verify the exact axum layer ordering needed for extension visibility
   in source before implementing; do not assume from this ADR alone.
2. Human session-cookie logins (no bearer origin) must be provably unaffected — the acceptance
   test `human_session_cookie_login_is_unaffected_by_scope_changes` is the regression guard and
   must actually exercise a session-cookie request path, not just assert the middleware's logic
   in isolation.
3. The route→required-scope table must be exhaustive and reviewed for every admin/owner-gated
   route named in `docs/codebase-map.md` §2's route table, not a partial sample — the whole
   point of this ADR is closing "decorative except two call sites."
4. `admin-never` is enforced structurally (via the same table), not by convention or by trusting
   a token's self-reported tier at mint time.
5. No silent grandfathering of `VOIDTOWER_TOKEN`'s current de facto admin access into an
   equivalent scoped token at cutover (see Migration above).

## Consequences

Once accepted, `.devteam/blocked/P0-06-scope-token-and-bearer-auth-bypass.md` should move back
to `.devteam/queue/`, its `**ADR:**` field updated to `ADR-003`, and its `## Status:` flipped off
`BLOCKED`. `gates.sh` will enforce that every forbidden-zone path in the resulting diff falls
within this ADR's `granted-paths` block above (separately from, and in addition to, ADR-001's).
Full-line review applies regardless of gate status (EDD §15.5 — auth is `vt-auth`-equivalent).
This ADR expires with the rest of the P0 grant family at phase exit.
