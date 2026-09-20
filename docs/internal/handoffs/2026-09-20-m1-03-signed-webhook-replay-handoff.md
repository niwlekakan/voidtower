# VoidTower M1-03 signed inbound webhook replay handoff — 2026-09-20

Status: implemented, unit-verified, and integration-verified at the real Axum-router/SQLite boundary; runtime-verified: blocked; release-qualified: blocked.

Implementation commit: `65074a0452f968ca810019f7f25aae53e5a7fafe` (`[verified] harden signed webhook replay protection`).
Branch: `dev`; base before this slice: `8d4d389ecd97ec1cb79713b15a2c0924b0e8fa95`; no push performed.

Active slice

- Complete the directly coupled M1-03 continuation for inbound Odysseus webhook authentication and replay protection.
- Public seams: signed request parsing and HMAC verification, bounded/error-mapped webhook handling, durable replay receipt claim, canonical automation/container webhook behavior, integration/frontend documentation.
- Non-goals: outbound webhook egress/SSRF hardening, canonical service adapters, CLI convergence, provider execution, collectors, deployment, host/browser runtime qualification, and release packaging.

Implemented

- Replaced Bearer-only inbound webhook authentication with `X-VoidTower-Timestamp`, `X-VoidTower-Nonce`, and `X-VoidTower-Signature: sha256=<hex>`.
- HMAC-SHA256 covers the exact raw message `timestamp.nonce.raw_body`; timestamps are limited to ±300 seconds and nonces are bounded to 1–128 ASCII characters from the documented grammar.
- Bounded raw-body reads remain capped at 64 KiB; signed-header parsing precedes the read and HMAC verification precedes JSON parsing. Unsupported media, oversized bodies, malformed JSON, and authentication failures use the existing bounded public error envelope/status mapping.
- Added migration `0006_webhook_replay_receipts.sql` with an atomic `(source_id, nonce)` receipt claim, 15-minute pruning, and replay rejection. Existing canonical durable-job, dry-run, policy, idempotency, and deferred-service behavior remains on the same operation boundary.
- Updated schema golden coverage, migration-count tests, integration workflow tests, frontend integration instructions, `docs/api.md`, and `docs/integrations/odysseus.md`.
- Updated `docs/internal/agent-knowledge/system-map.md` and `documentation-backlog.md` with verified contracts, evidence boundaries, reusable commands, and future documentation gaps.

Verification after final source changes

- `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture` — passed, 10 tests.
- `cd backend && cargo test api::integrations::tests --all-features -- --nocapture` — passed, 6 tests.
- `cd backend && cargo test db::tests --all-features -- --nocapture` — passed, 27 tests.
- `cd backend && cargo test --all-targets --all-features` — passed, 685 unit tests, 2 golden-path integration tests, and examples with no failures.
- `cd backend && cargo fmt --all -- --check` — passed.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cd backend && cargo deny check` — passed advisories, bans, licenses, and sources; existing warnings remain for a yanked `spin` dependency and duplicate transitive crate versions.
- `cd frontend && npm test -- --passWithNoTests` — passed, 13 files / 59 tests.
- `cd frontend && npm run type-check` — passed.
- `cd frontend && npm run lint` — passed.
- `cd frontend && npm run build` — passed; Vite emitted only existing chunk-size/dynamic-import warnings.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `bash scripts/check-repository-hygiene.sh` — passed, 654 tracked files checked.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed; current source inventory reports six migrations. This is source evidence only.
- `git diff --cached --check` and final `git diff --check` — passed.
- Added-line static security scan for hardcoded secrets, shell injection, eval/exec, pickle deserialization, and formatted SQL — zero matches.
- Independent read-only review — passed: no security concerns or logic errors. Non-blocking suggestions were a concurrency-focused duplicate-receipt test, explicit precedence tests for media/body-limit versus signature failures, moving the webhook credential into the encrypted secret manager, and future failure-atomic rotation; those are outside this bounded continuation except where already documented as follow-up gaps.

Limitations and blockers

- No host process, browser, Docker/provider, packaged installation, upgrade/recovery, named-platform, or release qualification was performed. The sandbox cannot expose those boundaries; no runtime or release claim is made.
- The webhook secret remains resolved from the existing `odysseus.webhook_secret` setting. Encrypted secret-manager storage and failure-atomic rotation are explicitly future work and must precede a production/release claim for this credential path.
- Outbound webhook delivery/egress security, canonical service adapters, CLI convergence, and provider execution remain separate dependencies.
- Unrelated pre-existing untracked `testing/` and `scripts/__pycache__/` paths were preserved and not staged.

Retrospective

- Learned that a signed raw-body contract requires a raw `Request` handler and bounded `to_bytes`; extractor ordering cannot provide the required authentication-before-parse behavior.
- Verified that SQLite `INSERT ... ON CONFLICT DO NOTHING` inside a transaction provides durable first-observation replay claims, while canonical automation/container work still flows through typed plan/policy/job boundaries.
- Reusable fixtures are `webhook_request_with_timestamp_nonce`, the real-router operation workflow tests, migration golden setup, and the focused/full Cargo plus frontend commands above. If full tests exhaust the sandbox tmpfs, remove only disposable `vt-*`/`voidtower-*` test artifacts and rerun; do not remove repository files.
- End-user documentation changed in `docs/api.md`, `docs/integrations/odysseus.md`, and `frontend/src/pages/Integrations.tsx`. Future documentation must cover encrypted credential rotation, outbound delivery, service adapters, and runtime/release operations.

Next dependency-ready slice

- M1-03 continuation: move the inbound webhook credential into the encrypted secret manager with failure-atomic create/rotate/revoke semantics and prove the existing signed/replay router contract still works. Keep outbound delivery, service adapters, CLI convergence, providers, collectors, and runtime qualification out of that slice.
