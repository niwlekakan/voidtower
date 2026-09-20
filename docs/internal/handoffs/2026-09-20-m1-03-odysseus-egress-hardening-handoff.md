# M1-03 Odysseus outbound URL and egress hardening handoff — 2026-09-20

Status: implemented, unit-verified, and integration-verified at the backend/frontend source and test boundaries; runtime-verified: blocked; release-qualified: blocked.

Implementation commit: `def4ffdde2bad593605b3147709d8d3afaa1ddd7`
Branch: `dev` (ahead of `origin/dev` by 3; not pushed)

Active slice and authority resolution

- This slice continues M1-03 from `docs/development-plan.md:417` and the current M1-03 encrypted-secret handoff: validate the configured Odysseus destination at save and runtime, preserve SSRF protections, prove the assembled router, and document the public contract.
- The physically newest ignored handoff `2026-09-20-r0-03-backend-quality-prerequisite-closure-handoff.md` is an earlier 09:37 checkpoint whose next slice is M1-02. Current source and commit history at 15:39 contain the later M1-02/M1-03 completion commits, so current source and the tracked plan take precedence for this continuation.
- Non-goals were generic notification-webhook SSRF, provider/runtime qualification, browser qualification, Docker/release work, collectors, household grants, and the next M1-04 compatibility inventory.

Implemented

- Added one shared egress parser/validator in `backend/src/ai/egress.rs` with a 2048-byte bound, HTTP(S)-only scheme, no credentials/query/fragment, DNS resolution, pinned addresses, disabled redirects, disabled ambient proxy use, and a narrow loopback/RFC1918 exception for self-hosted/local Odysseus.
- Expanded prohibited-address classification and tests for IPv4 metadata/special ranges plus IPv6 loopback, ULA, link-local, site-local, documentation, protocol/special ranges (`2001:2::/48`, `2001:10::/28`, `3fff::/20`, and related reserved blocks), and IPv4-mapped addresses.
- `POST /api/integrations/odysseus/config` validates non-empty `allowed_url` before any setting mutation; unsafe values are rejected with the bounded `400 bad_request` envelope and are not persisted. An empty value explicitly clears the endpoint to preserve the existing UI behavior.
- `GET /api/integrations/odysseus/theme` now uses the guarded client, refuses redirects, bounds upstream response bodies to 64 KiB, and returns stable redacted errors for unavailable, non-success, oversized, malformed, or failed upstream responses.
- The legacy Odysseus AI fallback now uses the same strict local egress boundary rather than the unrestricted public-provider policy, preserving self-hosted local Odysseus support.
- Added real Axum-router tests for unsafe destination rejection/no persistence, explicit clearing, and a controlled loopback upstream theme response. Added focused egress unit tests for URL grammar, length, IPv4/IPv6 ranges, local allowance, and direct-client enforcement.
- Corrected the native Integrations panel from unsupported `PATCH /api/integrations/odysseus/config` to the registered POST method and updated the main UI label.
- Updated `docs/integrations/odysseus.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md` with the verified URL policy, response/error contract, documentation gaps, reusable checks, and evidence boundary.

Independent review

- Final independent review passed with `passed=true`, `security_concerns=[]`, and `logic_errors=[]` after two prior review rounds identified and were used to fix direct-client validation bypass, IPv6 special-range gaps, and URL-clear regression.
- Added-line security scan found no hardcoded credential assignment, shell injection, eval/exec, unsafe deserialization, or interpolated SQL matches.

Verification evidence after final source changes

- `cd backend && cargo test ai::egress::tests --all-features -- --nocapture` — passed, 6 tests.
- `cd backend && cargo test api::integrations::tests --all-features -- --nocapture` — passed, 10 tests.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cd backend && cargo fmt --all -- --check` — passed.
- `cd frontend && npm test -- --passWithNoTests` — passed, 59 tests in 13 files.
- `cd frontend && npm run type-check` — passed.
- `cd frontend && npm run lint` — passed.
- `cd frontend && npm run build` — passed; Vite emitted only the existing dynamic-import and chunk-size warnings.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only and `runtime_support_claimed` remained false.
- `git diff --check` — passed before commit.
- `cd backend && cargo test --all-targets --all-features -- --test-threads=1` — one known unrelated SQLite-lock failure in `cmdb::assets::tests::concurrent_manual_creates_receive_distinct_identifiers`; 692 of 693 unit tests passed and the command stopped before the golden-path target. The same test also failed in an isolated focused rerun after disposable `/tmp/vt-*` and `/tmp/voidtower-*` cleanup. A prior pre-final full run passed 692 unit tests and 2 golden-path tests before the last IPv6-only refinement, but the final full gate is recorded as blocked. This is not attributed to this slice.

Limitations and blockers

- No host runtime, Docker Compose runtime, browser qualification, external Odysseus/provider execution, installation, upgrade/recovery, packaged artifact, or release qualification was performed. The sandbox has no host/browser supervisor or Docker socket.
- The full backend gate remains blocked by the known unrelated SQLite `database is locked` failure in the existing concurrent CMDB asset test; focused slice tests, Clippy, rustfmt, frontend gates, schema ownership, and source truth passed.
- Generic notification webhooks remain outside this bounded slice and retain a separate outbound-egress hardening gap documented for future work.
- Unrelated untracked `scripts/__pycache__/` and `testing/` paths were preserved and not staged, modified, or deleted.

Retrospective

- Learned that the configured Odysseus URL is shared by legacy AI and theme paths, so save-time validation alone was insufficient; every runtime caller must enter the same strict parser/client boundary.
- Verified that private self-hosted Odysseus requires a distinct local exception while metadata, link-local, mapped, documentation, and IPv6 special-purpose ranges remain blocked; this is now covered by executable tests.
- Reusable fixtures and commands are the egress and integrations focused tests, the controlled loopback Axum upstream in `odysseus_theme_route_uses_validated_local_endpoint`, serial full-suite invocation, frontend gates, schema ownership, and repository-truth check. Clean only disposable `/tmp/vt-*` and `/tmp/voidtower-*` artifacts when the 512 MiB tmpfs is exhausted.
- End-user documentation changed in `docs/integrations/odysseus.md`; future operator documentation still needs a named runtime runbook with DNS-rebinding, redirect, oversized-response, outage/restart, and browser evidence.

Next dependency-ready slice

- M1-04 compatibility bypass closure: run the source-derived mutation inventory, classify every remaining provider/destructive call site, and close or explicitly ledger typed synchronous exceptions. Preserve the canonical action/plan/policy/approval/job/audit/event boundaries. Do not bundle generic webhook egress, household grants, collectors, runtime qualification, or release packaging.
