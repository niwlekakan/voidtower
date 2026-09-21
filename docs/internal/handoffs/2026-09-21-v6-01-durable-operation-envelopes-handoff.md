# VoidTower V6-01 durable-operation envelopes handoff

Date: 2026-09-21T16:50:55Z
Branch: `dev`
Verified base commit: `1e636afe754fae566cb36fe42f09f0b4a9004d96`
Status: `integration-verified` for the durable-operation envelope sub-slice; broader V6-01 remains `blocked` on protected verifier activation.

## Active slice and boundary

- Tracked outcome: `V6-01 — Versioned API/event schemas` (`docs/development-plan.md:427`).
- Delivered sub-slice: versioned durable job and approval list/read/success envelopes across canonical routes, adopted compatibility mutation routes, frontend clients/parsers/types, checked-in contract artifacts, tests, and API documentation.
- Public seams: API-version negotiation, job list/read/submit/cancel, approval list/read/approve/reject, adopted compatibility durable submissions, frontend durable-operation client calls, and event-stream token documentation.
- Non-goals: full resource/action/inventory/event schema inventory, OpenAPI generation, compatibility deprecation windows, protected GitHub activation, browser/runtime qualification, provider/collector/adoption/deployment/release work, and V6-02 web qualification.

## Implemented

- Backend v1 job and approval envelope types and schema constants in `backend/src/api/version.rs`.
- Canonical job handlers and approval handlers return typed envelopes; approval decision comments are capped at 500 Unicode characters and decision failures preserve not-found, conflict, and redacted internal-error classes.
- Compatibility durable submissions in `backend/src/api/operation_adoption.rs` now return the same v1 job success envelope instead of a legacy `{ "job": ... }` wrapper.
- Frontend durable mutation calls use runtime envelope parsing, including compatibility routes and non-dry-run branches of update routes. Event-stream URL helpers and setup documentation no longer place bearer tokens in query strings.
- Checked-in envelope fixture and generated frontend contract remain synchronized; `docs/api.md` documents envelope shape, comment bounds, and Authorization-header-only event streams.
- Focused router tests cover exact envelope identity, idempotent compatibility replay, approval conflict/not-found/oversized-comment behavior, and role allowlists.

## Verification evidence

- `cd backend && cargo test --all-features operation_workflows -- --nocapture` — passed (10 focused workflow tests).
- `cd backend && cargo test --all-features api::version::tests -- --nocapture` — passed (10 version/contract tests).
- `cd backend && cargo test --all-targets --all-features` — passed (693 unit tests, 2 golden-path integration tests, examples).
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cargo fmt --manifest-path backend/Cargo.toml --check` — passed after formatting the changed Rust files with the repository manifest.
- `cd frontend && npm test` — passed (13 test files, 61 tests).
- `cd frontend && npm run type-check` — passed.
- `cd frontend && npm run lint` — passed.
- `cd frontend && npm run build` — passed; Vite emitted only existing chunk-size/dynamic-import warnings.
- `cd frontend && npm test -- --run src/api/envelopeClient.test.ts src/api/operationsClient.test.ts` — passed (34 tests).
- `node scripts/generate-api-contract.mjs --check` — passed; generated contract is current.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed; `runtime_support_claimed: false`.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check` — passed with `status: passed`, `unknown: []`.
- `bash scripts/check-repository-hygiene.sh` and `bash scripts/check-schema-migration-ownership.sh` — passed.
- `git diff --check` and staged diff checks — passed.

## Independent review and limitations

Two read-only specialist reports and an independent security/contracts review were obtained before final verification. The review found and the implementation fixed legacy compatibility job envelopes, blanket approval error conversion, missing server-side comment bounds, and frontend bearer-token query URL generation. Residual review guidance is not a blocker for this sub-slice: collection parsers validate the outer envelope but intentionally do not runtime-validate generic item payloads; full V6-01 source-owned schemas/OpenAPI/deprecation/SSE completion remains outside this bounded sub-slice.

Protected activation is still unresolved. The prior handoff records that `.github/workflows/compatibility-enforcement.yml` is absent from the remotely visible `dev` tip, the public workflow-runs lookup returned HTTP 404, branch-protection lookup returned HTTP 401, and no protected run ID/status context is observable in this sandbox. No runtime, browser, Docker, provider, installation, upgrade/recovery, or release qualification is claimed.

Untracked pre-existing `testing/`, `odysseus-mcp-servers/tests/__pycache__/`, and `scripts/__pycache__/` paths were preserved and are not part of this slice.

## Retrospective

- Learned: a durable-job envelope must be standardized at both canonical and compatibility submission boundaries; changing only `/api/jobs` leaves adopted routes and typed frontend callers inconsistent.
- Verified: backend and frontend envelope contracts, authorization behavior, approval failure classes, generated artifacts, full tests, strict Clippy, formatting, type-check, lint, build, repository truth, compatibility inventory, hygiene, and migration ownership all pass locally.
- Remains blocked: protected verifier publication/observation and the remaining V6-01 contract breadth (resource/action/inventory/event schemas, OpenAPI generation, deprecation policy, and protected drift enforcement). Runtime/provider/browser/release qualification remains unattempted.
- Reusable commands/fixtures: the focused backend workflow/version tests, full backend gate with disposable `TMPDIR`, frontend contract tests, `node scripts/generate-api-contract.mjs --check`, repository truth, compatibility inventory, hygiene, migration ownership, and `git diff --check`.
- End-user documentation changed: `docs/api.md` now documents v1 durable envelopes, adopted compatibility response consistency, the 500-character approval comment bound, and header-only token authentication for durable event streams.

## Next dependency-ready slice

After operator-side protected verifier activation evidence is recorded, continue V6-01 with source-owned resource/action/plan/inventory/event schemas, generated/OpenAPI client ownership, compatibility/deprecation rules, SSE reconnect/gap recovery integration evidence, and protected drift enforcement. Do not begin V6-02 or unrelated provider/collector/release initiatives first.

Commit: pending local focused commit after final diff review.
