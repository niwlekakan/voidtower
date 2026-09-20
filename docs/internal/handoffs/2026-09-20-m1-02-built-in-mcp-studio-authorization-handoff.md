# VoidTower M1-02 MCP/Studio ingress handoff — 2026-09-20

Status: integration-verified; implementation committed locally, not pushed

## Implemented

- Commit `c9f42423795910385e38a767d9122f6bb6bd5413` (`[verified] harden built-in MCP and Studio ingress contracts`).
- Added explicit central route metadata for built-in `/api/mcp`, `/api/mcp/message`, and Studio MCP routes. Unknown bearer routes remain default-deny; handler authentication and scope checks remain defense in depth.
- Hardened MCP JSON-RPC parsing: authentication and feature-flag checks precede body consumption; unknown envelope fields, unsupported protocol versions, invalid identifier types, non-object structured params, explicit null tool arguments, and unknown tool fields fail closed.
- Hardened all currently exposed direct read-tool schemas with strict unknown-field validation and matching `tools/list` `additionalProperties: false` contracts.
- Studio MCP preserves extractor status classes (`400` malformed JSON, `415` unsupported media, `413` oversized body, `422` typed validation) through a bounded public error envelope.
- Every `invoke_tool` error path uses shared AI redaction and a 4096-character bound; the MCP `Error: ` wrapper is bounded too.
- Preserved AI prompt safety: `/api/ai/ask` does not advertise or execute mutation tools.
- Updated `docs/api.md`, `docs/internal/agent-knowledge/system-map.md`, and `docs/internal/agent-knowledge/documentation-backlog.md` with verified contracts, evidence, limitations, and reusable checks.

## Verification

- Independent reviewer verdict: PASS; no security concerns, logic errors, or scope issues. The reviewer inspected the final current diff read-only.
- `cd backend && cargo fmt --all -- --check` — passed.
- `cd backend && cargo clippy --all-targets --all-features -- -D warnings` — passed.
- `cd backend && cargo test api::mcp::tests --all-features -- --nocapture` — 13 passed.
- `cd backend && cargo test built_in_mcp_rejects_invalid_json_rpc_version_and_unknown_request_fields --all-features -- --nocapture` — passed, including auth-before-body, invalid ID, params/argument validation, and body-limit checks.
- `cd backend && cargo test studio_mcp_router_exposes_tools_and_rejects_unknown_request_fields --all-features -- --nocapture` — passed.
- `cd backend && cargo test --all-targets --all-features` — 679 unit tests passed, 2 golden-path integration tests passed, examples passed.
- `bash scripts/check-schema-migration-ownership.sh` — passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check` — passed; source inventory only.
- `git diff --check` — passed before commit.

A parallel full-test attempt once exhausted the 512 MiB `/tmp` tmpfs through disposable SQLite/WAL fixtures. Only generated `/tmp/vt-*` and `/tmp/voidtower-*` artifacts were removed; the full suite was then rerun serially after space recovery and passed as recorded above.

## Limitations and preserved state

- No runtime, browser, external provider, packaged installation, upgrade/recovery, or release qualification was performed. The host supervisor did not provide a disposable runtime/browser boundary.
- Standalone MCP server parity and webhook/automation/scheduler/CLI convergence remain future M1-02 work.
- Unrelated untracked `testing/` and `scripts/__pycache__/` paths were preserved and were not staged or committed.
- No credentials, tokens, or provider secrets were persisted in this handoff.

## Retrospective

- Learned that manual request extraction must preserve status distinctions instead of collapsing all Studio body failures into one status, and that strict serde schemas need explicit field renames when Rust fields use underscore-prefixed dead-code suppression.
- Verified authentication-before-body parsing, JSON-RPC identifier and structured-parameter contracts, strict direct-tool schemas, bounded/redacted tool errors, central route metadata, and shared MCP/Studio invocation.
- Reusable fixtures are `scope_bypass_tests`'s MCP settings row with `updated_at = 0`, token/session helpers, 64 KiB boundary requests, and the focused commands listed above.
- Next dependency-ready slice: adopt one bounded non-MCP machine ingress family (webhook or automation) onto the same action-registry, scope, policy/approval, durable-job, audit/event, and redaction boundary; do not widen to providers or runtime qualification in that slice.
