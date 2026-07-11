# ADR-007 — Secrets reveal-audit invariant grant for Phase P1

**Status:** Proposed
**Date:** 2026-07-11
**Expires:** at P1 phase exit (`scripts/devteam/adr.sh revoke ADR-007`)

## Context

Gap-analysis §3's P1 table, row 3: "Secrets | Redaction corpus (P0.5) + reveal-audit
invariant tests | Highest-consequence data." P0.5's redaction corpus already covers the
AI-context path (`api/mcp.rs`, `api/studio.rs`, `api/ai_context.rs`, all under P0's own
ADR-001, now expired). This ADR covers the remaining half: `backend/src/api/secrets.rs`
itself, which is not in CLAUDE.md's literal forbidden-zone path list but falls under its
"secrets/crypto code" category — the encryption routines (`encrypt`/`decrypt` via
`aes-gcm`), the `secrets.key` handling, and every mutating handler that touches ciphertext or
the audit trail around it. Verified directly in source at ADR-drafting time
(`backend/src/api/secrets.rs`): `create`/`update`/`delete`/`rotate` all call `audit::log(...)`
unconditionally on their success path; `reveal` also does, **after** `decrypt()` succeeds —
meaning a `reveal` call that fails during decryption (`AppError::Internal`) returns early at
line 136, before the audit-log call at line 139, so a failed reveal attempt against a secret
whose ciphertext doesn't decrypt (e.g. after a key rotation gone wrong) leaves no audit
trail. Whether that's the invariant gap this task should close, or working as intended
because "attempt, not access" isn't the invariant EDD/gap-analysis actually wants, is exactly
the kind of judgment call the citing task's spec should not have to guess at alone — hence a
narrow grant rather than a blanket one.

## Decision

Task specs numbered `P1-*` that cite **ADR-007** are granted modification rights to exactly:

```granted-paths
backend/src/api/secrets.rs
```

Scoped to:

- Adding `audit::log(...)` calls on paths that currently skip auditing an attempted
  reveal/rotate/create/update/delete of a secret (success or failure), so the invariant
  "every attempt to access or change a secret's value produces an audit_log row, regardless
  of outcome" holds.
- Test code (`#[cfg(test)] mod tests` or a sibling `#[cfg(test)]`-gated file this task adds)
  exercising that invariant end-to-end (real router dispatch via `tower::ServiceExt::oneshot`,
  following `api/scope_bypass_tests.rs`'s precedent, not just unit-testing the handler
  function in isolation).

## Explicitly NOT granted

- `encrypt()` / `decrypt()` themselves (lines 9–30 as of this ADR's drafting) — the AES-GCM
  routine, nonce handling, and key derivation are cryptography, not audit logging. A task
  under this grant does not change how secrets are encrypted, only whether/when access to
  them is logged.
- `state.secrets_key` generation/loading (`<config_dir>/secrets.key`, in `main.rs`/`config`)
  — out of scope; this ADR does not cover `backend/src/main.rs` or `backend/src/config/`.
- The duplicated decrypt routine in `backend/src/api/proxmox.rs` (noted as a known
  consolidation candidate in `docs/codebase-map.md` §6) — a separate file, not covered here;
  a citing task that wants to consolidate it escalates for a fresh ADR rather than assuming
  this one extends to it.
- `token_secret_ids` / the scoped-token restriction on `reveal` (P0-06 territory) — do not
  change authorization logic, only audit logging around it.
- Everything else in CLAUDE.md's forbidden list.

## Constraints

1. **Audit logging only — no behavior change to what succeeds or fails.** A request that
   was previously denied/404/500 must still be denied/404/500 after this task; the only
   permitted change is that more paths now also write an `audit_log` row on the way out.
2. If closing the gap requires restructuring `reveal`'s early-return control flow (e.g. a
   `defer`-style guard so the audit call always fires), keep the diff minimal and follow the
   file's existing style (no new abstraction layer for one function).
3. Audit rows for a *failed* attempt must be distinguishable from a *successful* one in the
   `outcome`/`details` column `audit::log` already supports — don't collapse both into an
   identical row shape that a future incident review couldn't tell apart.
4. Redaction (P0.5) is unaffected — this ADR is about whether an event is logged, not about
   what the AI-context/MCP path redacts from its output. Don't touch `api/redact.rs`.

## Consequences

Closes gap-analysis P1's "reveal-audit invariant tests" row without reopening the
cryptography itself. At P1 exit this ADR is revoked and `backend/src/api/secrets.rs` closes
again.
