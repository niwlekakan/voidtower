# VoidTower Slice Handoff

- **Date:** 2026-09-06
- **Status:** integration-verified
- **Tracked plan slice:** S2-02 — Legacy secret closure and rotation
- **Branch and commit:** `dev`, source commits `6fb4c9a29cbadaa6596deddb622722a644353dad` (`harden secret rotation and token scope handling`) and `9146729` (`fail closed on missing token secret scopes`); handoff documentation is committed separately.

## Outcome

Secret create, update, and rotation inputs are bounded before encryption or persistence. An invalid oversized rotation returns `BadRequest` without replacing the last valid ciphertext or version. Secret-list/reveal authorization now fails closed for malformed present `Authorization` headers, malformed bearer tokens, unknown tokens, database errors, `NULL` secret scopes, and malformed `secret_ids` JSON; only an absent header takes the unrestricted session path. No secret value is returned by these paths.

## Contract and invariants

- **Public seams changed:** `GET /api/secrets`, `POST /api/secrets`, `PATCH /api/secrets/:id`, `POST /api/secrets/:id/rotate`, and `GET /api/secrets/:id/reveal`; internal `token_secret_ids` authorization helper.
- **Canonical invariants preserved:**
  - Secret values remain encrypted at rest and are validated before encryption (`MAX_SECRET_VALUE_BYTES = 64 * 1024`). Focused tests and full backend tests pass.
  - Invalid rotation cannot replace the previous encrypted value/version: `oversized_rotation_is_rejected_without_replacing_last_good_value` passes and asserts ciphertext decryption plus version preservation.
  - Scoped bearer callers cannot broaden secret visibility through malformed or missing scope data: `malformed_token_secret_scope_fails_closed_before_reveal` and `null_token_secret_scope_fails_closed_before_list_or_reveal` pass and assert denial before secret access.
  - Reveal/list paths propagate authorization errors rather than treating malformed or missing scope as unrestricted access; no new migration or parallel identity was introduced.
  - Existing audit, provider-reference, and resolver boundaries remain unchanged outside this bounded hardening.
- **Explicit non-goals preserved:** household/resource-grant authorization, new secret consumers, provider additions, AI/MCP secret reveal, process-restart migration probes, external-provider runtime smoke, frontend UX, and release qualification.

## Files and commit scope

- **Committed files:** `backend/src/api/secrets.rs`
- **Preserved unrelated staged files:** `backend/src/agent/mod.rs`, `backend/src/agent/state.rs`
- **Preserved unrelated modified files:** `none`

## Verification evidence

| Command | Exit | Exact result | Evidence label |
|---|---:|---|---|
| `cargo test api::secrets::tests --all-features -- --nocapture` | 0 | `9 passed; 0 failed; 0 ignored; 437 filtered out`; focused golden-path target had `0` selected | unit-verified |
| `rustfmt --edition 2021 --check src/api/secrets.rs` | 0 | no output | unit-verified |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | completed without warnings | unit-verified |
| `cargo test --all-targets --all-features` | 0 | `446 passed; 0 failed`; `golden_path`: `2 passed; 0 failed` | integration-verified |
| `bash scripts/check-schema-migration-ownership.sh` | 0 | `Schema migration ownership check passed.` | integration-verified |
| `git diff --cached --check` | 0 | no whitespace errors; only preserved agent paths were staged after source commit | unit-verified |
| `python scripts/repo_truth.py --repo . --json --check` twice plus byte comparison | 0 | both checks passed; `repo_truth_deterministic=yes` | unit-verified |
| `git diff 6fb4c9a^ 9146729 -- backend/src/api/secrets.rs` plus added-line credential-literal scan | 0 | `added_lines=221 suspicious_credential_literals=0` | unit-verified |
| `hermes verify --json` | 1 | Compose build passed; readiness incorrectly probed `http://127.0.0.1:8000/` while the service listened on `8743` and Compose exposed nginx on `80` | blocked |
| `hermes verify --json --port 80` | 0 | `ok: true`; container booted, readiness returned HTTP `200`, and teardown completed | runtime-verified |

- **Independent review:** The delayed delegated review found a high-severity fail-open case: a presented bearer token with `NULL secret_ids` was treated as unrestricted. The finding was reproduced RED (`200` instead of `403`), fixed in source commit `9146729`, and covered by `null_token_secret_scope_fails_closed_before_list_or_reveal`; the focused test and all full gates pass. No unresolved security or logic finding remains.
- **Security/redaction review:** The committed-slice scan reported `221` added lines and `0` suspicious credential literals; secret values and credentials were not persisted in this handoff.
- **Not run:** authenticated provider-secret API runtime smoke, process-restart migration recovery, browser UX, release artifact qualification, and household/resource-grant authorization gates; these are outside this bounded slice or require later dependencies.

## Failures and limitations

- **Blocking failures:** `none` for this bounded code slice.
- **Known limitations:** This is not a release-qualified or runtime-verified S2-02 completion. The broader plan still requires proof that no supported consumer reads plaintext settings, restart-safe migration/recovery evidence, and broader scoped access/resource-grant integration.
- **Recovery/rollback:** To roll back the source change, revert commits `9146729` and `6fb4c9a29cbadaa6596deddb622722a644353dad` after confirming the working tree; the change is bounded to validation and fail-closed authorization, and the invalid-rotation path already preserves the last known-good ciphertext/version. Do not reset or unstage the preserved agent paths.

## Repository state after delivery

- **Branch/HEAD/ahead/behind:** `dev` at handoff commit `228c876` (source commit `9146729`); local branch is `ahead 44` of `origin/dev`.
- **Staged paths:** `backend/src/agent/mod.rs`, `backend/src/agent/state.rs`
- **Modified paths:** `none`
- **Remote publication:** `not pushed`

## Next bounded slice

- **Next slice:** S2-02 remaining legacy-secret closure and rotation recovery evidence.
- **Acceptance seam:** Add a real fresh/legacy/restart migration test that proves a failed migration preserves the old reference and disables the provider without leaving plaintext cleanup incomplete; then scan DB/log/API outputs for provider-key plaintext.
- **Non-goals:** no household grants, no new providers, no frontend changes, no broad secret authorization redesign, and no release claim.
- **Blockers:** `none` known; the next slice must first confirm the current migration fixture and restart harness available in the backend test environment.

## Reproduction rule

A fresh agent can read `docs/development-plan.md`, run the source-truth script, execute the commands above from the repository root/backend directory as appropriate, and reach the same evidence classification without relying on this conversation.
