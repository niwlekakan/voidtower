# Task P1-02: Secrets reveal-audit invariant tests

## Status: Ready
**ADR:** ADR-007
Depends-On: none
Requires-Path: backend/src/api/secrets.rs, backend/src/audit/mod.rs

## Source

- `docs/gap-analysis.md` §3, P1 table row 3: "Secrets | Redaction corpus (P0.5) +
  reveal-audit invariant tests | Highest-consequence data."
- `docs/adr/ADR-007-p1-secrets-audit-invariant.md` — **authoritative** for the exact grant
  and the specific gap it identifies (a decrypt failure inside `reveal` returns before the
  `audit::log(...)` call, so a failed reveal attempt against undecryptable ciphertext leaves
  no audit trail — verified in ADR-007's Context section against source at drafting time;
  re-verify against current `backend/src/api/secrets.rs`, since line numbers drift).
- `backend/src/audit/mod.rs` — `audit::log()`'s signature and the `outcome`/`details`
  parameters this task needs to use to distinguish a successful reveal from a failed attempt.

## Scope

Establish and test the invariant: **every attempt to create, update, delete, reveal, or
rotate a secret produces exactly one `audit_log` row, regardless of whether the attempt
succeeds** (auth failure, not-found, and decrypt failure all count as attempts). Today,
`create`/`update`/`delete`/`rotate` already audit-log unconditionally on their success path
(and, per ADR-007's Context, `reveal` does too — but only past the `decrypt()` call). Verify
the current behavior directly in source first — do not trust this spec's or ADR-007's
line-number citations, they will have drifted. Where a gap exists, close it with the smallest
diff that makes the invariant hold; where behavior already satisfies the invariant, write the
test that proves it and move on — this is not a rewrite of `secrets.rs`.

Do not change what any handler returns to the caller (status codes, error bodies) — only
whether an audit row is written on the way out. Do not touch `encrypt()`/`decrypt()` or key
handling — ADR-007 explicitly withholds those.

## Contract (verbatim, `docs/gap-analysis.md` §3)

> Secrets | Redaction corpus (P0.5) + reveal-audit invariant tests | Highest-consequence data

## Architecture decision already made (ADR-007 — do not re-litigate)

- Audit-log-on-every-attempt, not audit-log-on-success-only, is the invariant. A denied
  request (wrong scope, not found) still gets a row.
- Failed and successful attempts must be distinguishable in the row itself (`outcome` or
  `details`), not collapsed into an identical shape.
- The scoped-token restriction on `reveal` (`token_secret_ids`, P0-06 territory) is
  unauthorized-request handling, not audit logging — do not touch its logic, only ensure the
  denial path it produces is itself audited if it currently isn't.

## Files to touch

- `backend/src/api/secrets.rs` — **forbidden zone, ADR-007**. Audit-logging call sites only
  (see ADR-007's Constraints); `encrypt`/`decrypt`/key handling untouched.
- Test code may live inline in a `#[cfg(test)] mod tests` block in the same file, or a
  sibling `#[cfg(test)]`-gated file if that fits the task better — ADR-007 covers
  `backend/src/api/secrets.rs` only, so a separate test file must still be that same path
  (i.e., added to `secrets.rs` itself, not a new top-level file — a genuinely separate test
  file would need its own grant; if you find you need one, escalate rather than assume).

## Explicitly not to touch

- `encrypt()` / `decrypt()` (AES-GCM routine, nonce handling).
- `state.secrets_key` generation/loading, `backend/src/main.rs`, `backend/src/config/`.
- The duplicated decrypt routine in `backend/src/api/proxmox.rs` — separate file, not covered
  by this grant.
- `token_secret_ids` / scope-restriction logic itself — audit the outcome, don't change the
  authorization decision.
- `backend/src/api/redact.rs` or anything on the P0.5 redaction path — unrelated invariant.

## Acceptance tests (name before implementing)

- `successful_reveal_produces_an_audit_row` — baseline, should already pass; write it first
  to confirm current behavior before changing anything (TDD's "must fail for the right
  reason" doesn't apply here since this one may already be green — that's fine, document it).
- `failed_reveal_due_to_decrypt_error_still_produces_an_audit_row` — the core regression
  guard: seed a secret whose ciphertext won't decrypt under the current key (e.g. corrupt the
  stored value directly via SQL in the test), call `reveal`, assert the response is an error
  **and** an audit row exists for the attempt.
- `unauthorized_reveal_attempt_produces_an_audit_row_or_is_explicitly_exempted` — a
  wrong-scope token's denied `reveal` call: either it's audited (preferred, per ADR-007's
  invariant) or the task explicitly documents in the PR body why a pre-authentication/
  pre-authorization denial is out of the invariant's scope (e.g. because it never reaches a
  known secret id) — this must be a stated decision, not silently untested either way.
- `create_update_delete_rotate_each_produce_exactly_one_audit_row_per_call` — regression
  guard against double-logging or a future refactor silently dropping a call.
- `audit_rows_for_failed_and_successful_reveals_are_distinguishable` — asserts the `outcome`/
  `details` fields differ in a way a human reviewing `audit_log` after an incident could tell
  apart "someone tried and failed" from "someone succeeded."

## Forbidden zones for this task

`backend/src/api/secrets.rs` (ADR-007, audit-logging call sites only).

## Review tier

Full line review (EDD §15.5 — secrets-adjacent code, highest-consequence data per
gap-analysis's own framing).
