# VoidTower M1-03 automation/webhook ingress contract handoff — 2026-09-20

Status: implemented, unit-verified, and integration-verified at the real Axum-router/SQLite boundary; runtime-verified: blocked; release-qualified: blocked.

Implementation commit: `8829b6d2c9b97e21be7f219f570ca77d1ec7f02d` (`[verified] harden automation and webhook ingress contracts`).
Branch: `dev`, ahead of `origin/dev` by 1; not pushed.

Active slice

- Harden the directly coupled automation CRUD/read/scheduler and Odysseus webhook ingress family without widening to providers, frontend, collectors, deployment, or runtime qualification.
- Public seams covered: operator-only automation reads, strict bounded automation writes, canonical automation run/scheduler submission, strict webhook intent parsing, webhook durable replay/dry-run, and action-registry metadata.

Implemented

- `GET /api/automation` and `GET /api/automation/:id/runs` require operator sessions because responses expose commands and captured output; run-history limits are bounded to 1–200.
- Automation create/update bodies reject unknown fields, require JSON content type, enforce bounded name/description/command/timeout/schedule values, and return stable bounded request-body errors. Request bodies are read only after session authorization and are capped at 64 KiB; oversized responses use the standard `payload_too_large` envelope.
- Invalid schedules are not treated as due. Missing update targets return `404 not_found`.
- Webhook bodies are read only after integration enablement/emergency checks and constant-time secret verification. The route requires JSON, caps bodies at 64 KiB, maps malformed/oversized input to the public error envelope, and rejects ambiguous/unknown intents.
- `automation_id` webhook requests use the canonical `automation.run` resource/action/policy/plan/durable-job path with `automation` actor and `webhook` ingress. Dry-run creates no job; identical idempotency replays the same job; changed intent conflicts. Webhook action metadata now includes every reachable canonical action, including `automation.run`.
- Deferred `service.*` webhook actions never call the legacy service helper. Policy-denied requests return a generic bounded `403 policy_denied`; allowlisted requests return bounded `503 feature_unavailable` until a canonical service adapter exists.
- Updated `docs/api.md`, `docs/integrations/odysseus.md`, and the living system-map/documentation backlog with verified contracts, bounds, evidence, limitations, and future documentation gaps.

Verification after final source changes

- `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture` — passed, 8 tests.
- `cd backend && cargo test api::integrations::tests --all-features -- --nocapture` — passed, 6 tests.
- `cd backend && cargo test api::mcp::action_registry::tests --all-features -- --nocapture` — passed, 14 tests during final review cycle.
- `cd backend && cargo fmt --all -- --check` — passed.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cd backend && cargo test --all-targets --all-features` — passed after disposable `/tmp/vt-*` and `/tmp/voidtower-*` SQLite/WAL cleanup: 683 unit tests, 2 golden-path integration tests, and examples passed.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `bash scripts/check-repository-hygiene.sh` — passed, 652 tracked files checked.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed at pre-commit HEAD; source inventory only.
- `git diff --check` and `git diff --cached --check` — passed.
- Independent final read-only review — passed: no security concerns or logic errors. Non-blocking suggestions were to add a policy-denial response assertion and consider conflict-safe concurrent first observation; neither is required to close this bounded slice.

Review and scope

- The final diff was independently reviewed after the last code changes. Review found no security or logic blockers.
- No credentials, tokens, environment files, provider secrets, remote branches, Docker state, browser state, system services, or host runtime were changed.
- Unrelated untracked `testing/` and `scripts/__pycache__/` paths were preserved and were not staged or committed.

Limitations and blockers

- No host runtime, browser, external provider, Docker daemon, packaged installation, upgrade/recovery, or release qualification was performed. This sandbox cannot provide those boundaries.
- The first post-change full-test attempt exhausted the 512 MiB `/tmp` tmpfs and produced environment-driven SQLite/storage failures. Only disposable `/tmp/vt-*` and `/tmp/voidtower-*` artifacts were removed; the exact full command was rerun with space available and passed.
- Signed inbound webhook requests/replay protection beyond the configured shared bearer secret, outbound webhook delivery/egress security, canonical service actions, CLI convergence, and provider/runtime qualification remain future work.

Retrospective

- Learned that Axum body extractors run before handler code; using a raw `Request` and authenticated bounded `to_bytes` read is required when secret/session validation must precede parsing and oversized-body failures must use the public JSON envelope.
- Verified operator authorization, strict DTOs and media types, bounded 64 KiB request bodies, auth-before-parse behavior, malformed/oversized error envelopes, canonical automation durable jobs, scheduler idempotency, webhook dry-run/replay/conflict, strict webhook intent validation, and action metadata completeness.
- Reusable fixtures and commands are `operation_workflows_tests`'s role/session helpers, webhook settings/allowlist rows, 64 KiB boundary bodies, the focused Cargo commands above, the full backend gate, repository truth, schema ownership, hygiene, and diff checks. If the 512 MiB `/tmp` limit is reached, remove only disposable `vt-*`/`voidtower-*` test artifacts.
- End-user documentation changed in `docs/api.md` and `docs/integrations/odysseus.md`; future documentation remains required for signed webhook configuration/replay operations, canonical service actions, and named runtime/provider qualification.

Next dependency-ready slice

- M1-03 continuation: define and prove signed inbound webhook verification/replay protection for the existing webhook boundary, including canonical timestamp/signature error behavior and operator documentation. Keep service adapters, CLI convergence, providers, and runtime qualification out of that slice.
