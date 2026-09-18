
## C3-02 inventory evidence — 2026-09-13

- `POST /api/nodes/:node_id/inventory` verifies the node-bound bearer token, approved/agent-capable status, and path node ID, then derives the canonical CMDB host resource from `resources.node_id`; payloads cannot select a `resources.id`.
- `backend/src/cmdb/observations.rs::ingest` validates `InventorySnapshotV1`, fingerprints canonical content, persists snapshots/observations transactionally, replays identical snapshot IDs, conflicts on changed intent, and performs deterministic correlation/missing convergence.
- Real-router tests in `backend/src/api/cmdb/tests.rs` verify successful upload, replay without duplicate snapshot rows, wrong-path denial, changed-content conflict, and approval revocation denial using sanitized fixtures.
- Current evidence is `integration-verified` for the tested router/database boundary. Linux process collection, supervised scheduling, outage/restart recovery, service packaging, and runtime qualification remain blocked for C3-03.
- A node enrollment does not create the CMDB host projection; operators/tests must provision or adopt the canonical host resource and bind `resources.node_id` before upload.


## C3-03 supervision evidence — 2026-09-14

- `backend/src/collector.rs::collect_linux_command` runs the fixed `lsblk` field list with a 10-second timeout, bounded stdout/stderr reads, UTF-8 validation, and the existing fixture parser; command failure, empty output, malformed JSON, and bounds failures do not create a snapshot.
- `AgentTransport::upload_inventory` posts `InventorySnapshotV1` to `/api/nodes/{node_id}/inventory` with the enrolled node token and the existing bounded JSON response reader.
- `agent::supervision::run` now starts independent heartbeat and inventory loops. Each loop is cancellation-aware and has its own bounded exponential backoff; inventory creates a new UUID only for a collection attempt and never uploads after collection failure.
- `packaging/systemd/voidtower-agent.service` is the foreground service unit. It preserves `/var/lib/voidtower/agent`, runs without an inbound listener, and uses restrictive systemd hardening.
- C3-03 is unit-verified by focused tests and full backend tests. Runtime service install/restart/outage/upgrade/rollback evidence is blocked because this sandbox has no host systemd runtime.

## C3-03 bounded command I/O — 2026-09-15

- `backend/src/collector.rs::collect_linux_program` drains stdout and stderr concurrently through `read_bounded`, retaining at most the configured limit plus one byte while continuing to drain the child pipes. This prevents an over-limit producer from blocking on a full pipe and classifies stdout/stderr overflow as `CollectorError::Oversized` before parsing or snapshot creation.
- Focused evidence: `cd backend && cargo test collector::tests --all-features` passed 5 tests after the overflow sentinel test was added. This is unit-verified only; real service-managed collection remains blocked by the absent systemd host boundary.


## C3-03 runtime qualification checkpoint — 2026-09-14

- The dependency-ready continuation is Linux service/runtime qualification, not another source-only implementation slice.
- The Docker sandbox proves the blocker directly: `systemctl` is absent and `/run/systemd/private` is absent. Therefore service installation, restart, outage/restart recovery, upgrade, rollback, and real service-managed `/usr/bin/lsblk` collection cannot be runtime-verified here.
- Final evidence at HEAD `94f8da244c2d56ef69526ec1596fda96723c1f16`: focused collector (4), transport (7), supervision (3), and full backend `cargo test --all-targets --all-features` (586 unit + 2 integration) passed; repository truth passed; schema ownership returned success with a pre-existing missing-`rg` diagnostic; Clippy and rustfmt are blocked because toolchain components are not installed.
- Do not promote C3-03 beyond `unit-verified` until a supported Linux host or equivalent supervisor runtime executes the named service scenarios.


## C3-03 runtime qualification session 2 — 2026-09-14

- Current HEAD remains `94f8da244c2d56ef69526ec1596fda96723c1f16`; the only staged paths are the unrelated agent hardening files `backend/src/agent/mod.rs` and `backend/src/agent/state.rs`.
- Re-ran repository truth, focused collector/transport/supervision tests, full backend tests, schema ownership, and diff checks successfully. Source truth remains source-only and reports `runtime_support_claimed: false`.
- Runtime qualification is blocked by the sandbox boundary: `systemctl` is absent and `/run/systemd/private` is absent. No service install/restart, host `/usr/bin/lsblk`, outage recovery, upgrade, or rollback claim is permitted here.


## C3-03 runtime qualification session 3 — 2026-09-14

- The exact fixed `/usr/bin/lsblk --json --bytes --output NAME,KNAME,TYPE,SIZE,MODEL,SERIAL,WWN,ROTA,TRAN,RM,RO,PATH,MOUNTPOINTS` command executes and parses in the sandbox (valid `blockdevices` JSON; 4 top-level devices; 4258 bytes), but this is not service evidence.
- Runtime qualification remains blocked: `systemctl` is unavailable and `/run/systemd/private` is absent. No systemd install/restart, outage/restart recovery, upgrade, rollback, or service-managed collection claim is allowed.
- Final available checks passed: repository truth, collector/transport/supervision focused tests, full backend tests (586 unit + 2 integration), schema ownership, and diff check. Clippy and rustfmt remain unavailable toolchain components.

## C3-03 runtime qualification session 4 — 2026-09-14

- Re-ran C3-03 focused collector/transport/supervision tests and the full backend gate. The first full run exhausted the 512 MiB `/tmp` tmpfs through disposable SQLite/WAL fixtures; after removing only `/tmp/vt-p1-*` and `/tmp/voidtower-*` artifacts, the exact full command passed with 586 unit tests, 2 integration tests, and examples.
- Repository truth, schema ownership, and diff checks passed. Clippy and rustfmt remain blocked because the toolchain lacks `cargo-clippy` and `cargo-fmt`.
- Runtime qualification remains blocked: `systemctl` is unavailable and `/run/systemd/private` is absent. The service install/restart, real service-managed collection/upload, outage/restart recovery, upgrade, and rollback scenarios were not exercised.


## M1-04 implicit model switching closure — 2026-09-14

- Public `POST /v1/chat/completions` is an unauthenticated OpenAI-compatible inference proxy to local llama.cpp. At commit `6ad0891c5d9ae00fda0c6e91f603ddf5c6ea3a75`, it forwards inference only and does not select/reload models, edit compose state, invoke Docker, write the database/filesystem, spawn work, or audit.
- Authenticated legacy model mutation POSTs (`/api/models/load`, `/api/models/llama-config`, `/api/models/ollama-config`, `/api/models/ollama/create`) fail closed with `503 feature_unavailable` until canonical operation adapters exist.
- The exact source inventory test `operations::registry::tests::deferred_direct_execution_inventory_is_exact` now requires zero `crate::containers::deploy_compose(` occurrences in `models.rs`; focused model tests and the full backend gate passed.
- Evidence label: `integration-verified` for the tested source/real-router boundary; runtime provider and release qualification remain unverified.


## M1-04 local-host mutation closure — 2026-09-14

- Commit `2e5a431` closes direct filesystem write/mkdir/delete/rename, plugin install/update/uninstall, and repository-mod fetch/apply/rollback handlers. They authenticate owner/admin sessions, then return bounded `503 feature_unavailable` until canonical operation adapters exist.
- Read-only file/plugin/mod status paths remain available. `POST /api/mods/config` stores source settings only; it does not fetch or apply a repository.
- Source enforcement and a real Axum-router test live in `backend/src/operations/registry.rs`; focused registry tests and the full backend gate passed.
- Runtime provider/host mutation evidence is intentionally absent; remaining M1-04 work is source-derived classification of other synchronous/provider-adjacent paths before V6-01.


## M1-04 deferred mutation ledger — 2026-09-14

- `backend/src/operations/registry.rs::DEFERRED_MUTATION_EXCEPTIONS` is the executable ledger for 23 compatibility mutation routes that are intentionally unavailable until canonical resource/action adapters exist. It covers filesystem, plugins, repository mods, services, local LXC/VM, WireGuard, and storage mutations.
- The ledger test checks route metadata has no canonical action, the named handler exists, and the handler contains `FeatureUnavailable`; the real-router test checks authentication remains first (`401`) and authenticated requests return bounded `503 feature_unavailable` for service, LXC, storage, and WireGuard representatives.
- `backend/src/api/storage.rs::set_storage_paths` was a discovered bypass: it previously wrote settings and emitted success audit directly. It now authenticates and fails closed; its private writer was removed.
- Evidence target for this continuation is `integration-verified` after focused and full backend checks. Runtime provider and release qualification remain blocked.


## M1-04 system restart closure — 2026-09-14

- `POST /api/system/restart` is registered as an explicit deferred mutation exception in `backend/src/operations/registry.rs`; the ledger now contains 24 entries and checks route metadata, handler registration, and fail-closed source markers.
- `backend/src/api/system.rs::restart` authenticates an owner/admin session before returning bounded `503 feature_unavailable`; it no longer writes `/tmp/voidtower-restart.sh`, spawns `setsid`, or signals the process.
- The existing real-router deferred-route test covers this route’s unauthenticated `401` and authenticated `503 feature_unavailable` behavior. System update/check routes remain on their separately adopted durable operation path.
- Final backend evidence: focused registry tests passed (15); full `cargo test --all-targets --all-features` passed (591 unit, 2 integration). Runtime/release support remains unclaimed.


## M1-04 AI process unload closure — 2026-09-14

- `POST /api/ai/llama/unload` is an authenticated owner/admin compatibility route that now returns bounded `503 feature_unavailable` until a canonical AI process lifecycle adapter exists; it no longer signals or terminates host processes.
- The executable `DEFERRED_MUTATION_EXCEPTIONS` ledger contains 25 entries and source-checks the AI handler alongside the existing deferred routes.
- The shared real-router deferred-route test proves unauthenticated `401` ordering and authenticated `503 feature_unavailable` for the AI unload route.
- Evidence: focused `operations::registry::tests` (15 passed) and full backend `cargo test --all-targets --all-features` (591 unit, 2 integration) passed; runtime/provider and release evidence remain unclaimed.


## M1-04 interactive shell closure — 2026-09-14

- `GET /api/terminal/ws`, `GET /api/terminal/ssh/ws`, and `GET /api/containers/:id/exec` are now executable deferred exceptions in `backend/src/operations/registry.rs`; their route metadata remains non-canonical and the shared real-router test proves authentication-first `401` followed by bounded `503 feature_unavailable`.
- The three handlers no longer accept WebSocket upgrades, spawn local shells, invoke SSH, or invoke `docker exec`; terminal/PTY implementation code remains present but unreachable from these HTTP handlers until canonical adapters exist.
- Session metadata CRUD and encrypted SSH password-reference storage remain separate from interactive execution. Container logs remain read-only.
- Evidence target is `integration-verified` for the real-router/source boundary after the focused registry and final backend gates; runtime/release qualification remains unclaimed.


## M1-04 model/provider lifecycle closure — 2026-09-14

- The executable deferred ledger in `backend/src/operations/registry.rs` now contains 35 entries, adding the model file download/delete, model load, llama/Ollama configuration, Ollama pull, and Ollama create mutation seams.
- The shared ledger test verifies route metadata, router registration, handler presence, `FeatureUnavailable`, and absence of filesystem, process, provider, database, detached-task, and audit markers. The shared real-router test verifies unauthenticated `401` before the feature boundary and authenticated `503 feature_unavailable` for all seven model lifecycle routes.
- Read-only model listing/status, Ollama tags/status reads, and `/v1/chat/completions` remain separate. The OpenAI-compatible proxy forwards inference only and does not select or reload models.
- Evidence target: `integration-verified` for the source/real-router boundary after final focused and full backend checks. Runtime provider and release qualification remain blocked; no live model provider or destructive model mutation was exercised.


## M1-04 provider and App Vault boundary closure — 2026-09-14

- AI provider create/update now validate bounded names/models, non-negative priority, credential-free HTTP(S) base URLs, and reject localhost/private/link-local/metadata targets before persistence.
- AI health retains detailed failure diagnostics only in debug logs and returns the bounded generic provider health check failed response; provider credentials remain secret-manager references.
- App embed proxy applies the same member owner-or-admin rule as status/logs/compose before any upstream request, disables redirect following, and emits frame-ancestors self instead of wildcard framing.
- Evidence target: unit-verified focused tests plus full backend/source/schema/diff gates; live provider, Docker App Vault, browser, and release qualification remain blocked.

## M1-04 provider egress closure — 2026-09-14

- `backend/src/ai/egress.rs::client_for` resolves provider hostnames with Tokio DNS before constructing each reqwest client, rejects loopback/private/link-local/metadata/documentation/special ranges, pins all accepted addresses with `ClientBuilder::resolve`, and disables redirects. All OpenAI, Anthropic, Odysseus, and local provider completion/stream/health paths use this helper.
- Provider update validation now loads the persisted name, URL, model, priority, and secret reference before validating an omitted-field update; an unsafe existing URL cannot be bypassed by updating only another field.
- Evidence: `cargo test ai::egress --all-features` (3 passed), `cargo test api::ai_providers::tests --all-features` (3 passed), `cargo test ai::orchestrator::tests --all-features` (3 passed), and `git diff --check` passed. This is unit-verified; real external provider/Docker/browser runtime remains unavailable.


## M1-04 provider egress closure final — 2026-09-14

- Commit `7c7c5d1977b0be09df7016170e591996742c4e85` routes provider completion, streaming, and health calls plus the legacy Odysseus fallback through DNS-pinned, redirect-free, ambient-proxy-free clients. External targets reject prohibited and mapped IPv6 addresses; explicit `local` providers have a narrow loopback/RFC1918 exception while metadata/link-local/special targets remain blocked.
- Non-success provider bodies are bounded to generic errors, and provider update validation revalidates persisted omitted fields. Final evidence is unit-verified/full-test verified; runtime and release remain blocked by unavailable external/host environments.

## C3-03 runtime qualification recovery — 2026-09-14

- Recovery was attempted in the Docker sandbox at `2026-09-14T16:10:48+00:00`; `systemctl` is not installed and `/run/systemd/private` is absent. This is an environment boundary, not a product test failure.
- Current C3-03 evidence remains `unit-verified`: `cd backend && cargo test agent:: --all-features` passed 27 tests; collector (4), transport (7), and supervision (3) focused suites also passed. No service install, restart, controller outage/recovery, upgrade, rollback, or service-managed upload claim is permitted.
- The active Rust toolchain is `1.98.1-x86_64-unknown-linux-gnu` with only cargo, rust-std, and rustc components; `cargo-clippy` and `cargo-fmt` are unavailable. The `/tmp` tmpfs is 512 MiB and was 84% used during this attempt; clean only disposable test artifacts before a full-suite rerun.
- Tracked operator documentation now contains the supported-host qualification checklist in `docs/agent/linux-agent-service.md`, including the pre-existing canonical host/adoption prerequisite and the process-lifetime pending-upload retry boundary. The next dependency-ready action is a named supported Linux host run, not another source-only implementation slice.

## M1-04 AI Studio generation closure — 2026-09-14

- `POST /api/studio/image/generate`, `/api/studio/tts/generate`, and `/api/studio/stt/transcribe` are executable deferred mutation exceptions in `backend/src/operations/registry.rs`.
- Each route authenticates before returning bounded `503 feature_unavailable`; no route contacts SD WebUI, ComfyUI, Kokoro, or Whisper, and no generation file is written while canonical AI/media adapters are absent.
- The ledger source test verifies route metadata, router registration, handler presence, fail-closed markers, and no direct provider/filesystem/DB/audit markers. The real-router test verifies unauthenticated `401` then authenticated `503`, including STT body-parser ordering.
- Evidence at the session commit is integration-verified for the source/real-router boundary. Runtime provider, browser, and release qualification remain unclaimed.

## M1-04 deferred compatibility extractor ordering — 2026-09-14

- Deferred compatibility handlers for filesystem, plugins, services, local LXC/VM, storage, and WireGuard now authenticate without declaring body/query extractors; their unavailable boundary cannot parse or execute legacy mutation intent.
- `operations::registry::tests::deferred_mutation_routes_preserve_authentication_then_fail_closed` covers malformed body/query requests across these families and verifies `401` before parsing, while authenticated sessions receive the bounded `503 feature_unavailable` envelope.
- Source/real-router evidence is integration-verified. Provider, host, browser, and release runtime evidence remain unclaimed; C3-03 systemd qualification is blocked by absent `systemctl` and `/run/systemd/private`.

## C3-03 runtime qualification recovery session 6 — 2026-09-14

- Recovery reproduced the host boundary at `2026-09-14T18:33:20+00:00`: `systemctl` is unavailable and `/run/systemd/private` is absent. This Docker sandbox cannot provide valid systemd-managed service evidence.
- C3-03 remains `unit-verified`: agent (27), collector (4), transport (7), supervision (3), and full backend (598 unit + 2 integration) tests passed; repository truth and diff checks passed; schema ownership returned success with the pre-existing missing-`rg` diagnostic.
- Runtime qualification remains `blocked`: service installation/startup, protected host state, real service-managed `/usr/bin/lsblk` upload, outbound-only runtime observation, controller outage/restart recovery, upgrade, and rollback were not exercised.
- Preserved unrelated worktree paths remain `backend/src/agent/mod.rs`, `backend/src/agent/state.rs`, `backend/src/api/apps.rs`, and `testing/`. The next action is a named supported Linux host or VM with a real systemd supervisor, not another source-only C3-03 change.

## C3-03 response-contract recovery — 2026-09-14 (commit 6dfa330)

- `AgentTransport::heartbeat` now decodes a typed acknowledgement and rejects a successful HTTP response unless `ok` is true; `upload_inventory` decodes `InventorySnapshotResultV1` instead of accepting arbitrary JSON.
- `agent::supervision::run` validates the loaded `AgentState` before spawning heartbeat or inventory loops and returns without network activity on invalid state. The direct invalid-schedule test proves this fail-closed behavior.
- Focused transport (8 tests) and supervision (4 tests) suites passed after the change. This remains `unit-verified`; host systemd, restart durability, outage recovery, upgrade, rollback, and runtime upload evidence remain blocked by the Docker environment.

## C3-03 runtime qualification recovery session 7 — 2026-09-15

- Recovery reproduced the host boundary at `2026-09-15T16:37:24+00:00`: `systemctl` is unavailable and `/run/systemd/private` is absent. This Docker sandbox cannot provide valid systemd-managed service evidence.
- Rust toolchain recovery succeeded: `rustup component add rustfmt clippy` exited 0 and installed both components. Repository-wide `cargo fmt --check` still reports pre-existing drift, including `backend/src/agent/supervision.rs` and `backend/src/agent/transport.rs` plus unrelated files; repository-wide Clippy still reports unrelated warnings/errors. No formatting or lint fixes were applied.
- C3-03 focused transport (8), supervision (4), and inventory router (2) tests passed. Repository truth and diff checks passed. Runtime qualification remains blocked for service install/startup, protected state, service-managed collection/upload, outage/restart recovery, upgrade, rollback, and enrollment-to-host-adoption.
- No source or service-package files were changed. Preserved staged `backend/src/agent/mod.rs` and `backend/src/agent/state.rs`, modified `backend/src/api/apps.rs`, and untracked `testing/` paths. The dated blocked handoff is `docs/internal/handoffs/2026-09-15-c3-03-runtime-qualification-recovery-session-7-blocked.md`.
## C3-03 runtime qualification recovery session 7 — 2026-09-15

- Recovery reproduced the host boundary at `2026-09-15T16:37:24+00:00`: `systemctl` is unavailable and `/run/systemd/private` is absent. This Docker sandbox cannot provide valid systemd-managed service evidence.
- Rust toolchain recovery succeeded: `rustup component add rustfmt clippy` exited 0 and installed both components. Repository-wide `cargo fmt --check` still reports pre-existing drift, including `backend/src/agent/supervision.rs` and `backend/src/agent/transport.rs` plus unrelated files; repository-wide Clippy still reports unrelated warnings/errors. No formatting or lint fixes were applied.
- C3-03 focused transport (8), supervision (4), and inventory router (2) tests passed. Repository truth and diff checks passed. Runtime qualification remains blocked for service install/startup, protected state, service-managed collection/upload, outage/restart recovery, upgrade, rollback, and enrollment-to-host-adoption.
- No source or service-package files were changed. Preserved staged `backend/src/agent/mod.rs` and `backend/src/agent/state.rs`, modified `backend/src/api/apps.rs`, and untracked `testing/` paths. The dated blocked handoff is `docs/internal/handoffs/2026-09-15-c3-03-runtime-qualification-recovery-session-7-blocked.md`.

## C3-03 runtime qualification recovery session 8 — 2026-09-15

- Recovery re-confirmed the host boundary at `2026-09-15T18:47:41+00:00`: PID 1 is Docker's `docker-init -- sleep infinity`, `systemctl` is absent, and neither `/run/systemd/private` nor the equivalent `/proc/1/root` socket is visible. The sandbox runs as UID 1000, so package installation cannot provision a supported host supervisor; no container simulation was promoted.
- Current checkout is `612c525f91466ad441eaa46a846b838fb0ee1205` on `dev` (ahead 1, behind 0). Existing modified and untracked paths were preserved; this checkpoint added only the dated handoff and knowledge appendices.
- Current evidence: C3-03 focused transport (8), supervision (4), inventory router (3), full backend (604 unit + 2 integration + examples), release-gate tests (11), repository truth, schema ownership, and diff checks passed. `cargo fmt --check` and strict Clippy remain blocked by existing repository-wide drift/errors; runtime/release qualification remains blocked.
- The dated blocked handoff is `docs/internal/handoffs/2026-09-15-c3-03-runtime-qualification-recovery-session-8-blocked.md`. The next dependency-ready action remains a named supported Linux host or VM with real systemd and host `/dev` visibility.

## C3-02 agent response-contract completion — 2026-09-15

- `backend/src/agent/transport.rs::EnrollmentRequest::validate` now caps pairing codes at 512 bytes, matching the controller's `MAX_PAIRING_CODE_BYTES` validation and the published enrollment contract.
- Transport tests prove malformed successful heartbeat JSON and incomplete successful inventory results fail closed; the inventory result requires all six `InventorySnapshotResultV1` fields.
- Focused evidence after the final test edits: `cd backend && cargo test agent::transport --all-features` passed 11 tests. Existing compiler dead-code warnings remain.
- This slice is unit-verified at the agent transport seam and complements the existing real-router/database C3-02 evidence. It does not establish supported-host runtime, restart durability, or release qualification.

## C3-03 durable pending inventory — 2026-09-15

- Production `agent::run` passes the protected state path into supervision. After a successful collection, supervision atomically persists the bounded snapshot to the owner-only `.<state>.pending.json` sidecar before upload; it reuses that sidecar after process restart and clears it only after a typed successful upload response.
- The sidecar is capped at 256 KiB, uses atomic replacement and `0600` permissions on Unix, rejects symlinked parent chains, and fails the inventory loop closed when an existing sidecar cannot be loaded. Collection failures do not create a pending snapshot.
- Pending sidecar persistence is intentionally unavailable on Windows until a supported ACL implementation exists; no Windows support claim is made by this Linux slice.
- Focused evidence: `cd backend && cargo test agent::state --all-features` passed 17 tests and `cd backend && cargo test agent::supervision --all-features` passed 4 tests after implementation. Runtime outage/restart and systemd evidence remain blocked by the sandbox.

## C3-03 inventory acknowledgement binding — 2026-09-15

- `AgentTransport::upload_inventory` now accepts a successful typed response only when its `snapshot_id` exactly equals the uploaded `InventorySnapshotV1.snapshot_id`; a different acknowledgement is a retryable error.
- This preserves the supervision invariant that pending inventory is cleared only after acknowledgement of the exact persisted snapshot. The transport test `inventory_upload_rejects_success_for_a_different_snapshot` covers the fail-closed response contract.
- Evidence after this checkpoint is unit-verified at the transport seam. Supported-host systemd, outage/restart, upgrade, rollback, and runtime upload evidence remain blocked by the Docker environment.

## C3-03 runtime qualification recovery session 9 — 2026-09-16

- The dependency-ready continuation remains supported-host runtime qualification, not another source-only implementation slice. The tracked plan requires service install/start/status, protected state, real collection/upload, controller outage/restart recovery, upgrade, rollback, outbound-only evidence, and artifact evidence (`docs/development-plan.md:420`; `docs/agent/linux-agent-service.md:43-54`).
- This sandbox boundary was re-probed: `systemctl` is unavailable, `/run/systemd/private` is absent, and PID 1 is Docker's `docker-init`; therefore service-managed runtime and release qualification remain blocked. `/usr/bin/lsblk` exists, but direct execution is not service evidence.
- Available source checks passed on 2026-09-16: transport (12), supervision (4), inventory router (3), and release-gate tests (11); `git diff --check` passed. Schema ownership was not fully verified because the required `rg` executable is unavailable; the script exited 0 after its known `rg: command not found` diagnostic. No unrelated paths were included in this staged continuity diff; pre-existing worktree changes were preserved.
- One full `cd backend && cargo test --all-targets --all-features` attempt reached 610 passing tests but failed in the existing `cmdb::assets::tests::concurrent_manual_creates_receive_distinct_identifiers` with SQLite `database is locked` (code 517). A rerun on 2026-09-16 passed with 611 unit tests, 2 integration tests, and examples; the earlier result is retained as a transient historical observation, not a current gate failure.

## C3-03 runtime qualification recovery session 10 — 2026-09-16

- The current uncommitted API/release hardening work passes focused CMDB (13), enrollment (7), App Vault (43), collector (4), transport (12), supervision (4), and release-gate (11) tests. Repository truth and diff checks also pass.
- After removing only disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures created by the failed attempt, `cd backend && cargo test --all-targets --all-features` passed with 611 unit tests, 2 integration tests, and the example target. The initial same-session full run failed from `/tmp` exhaustion and is not treated as a code failure.
- `cargo clippy --all-targets --all-features -- -D warnings` remains blocked by existing repository-wide diagnostics. The schema wrapper exits 0 but emits `rg: command not found`; schema ownership is not fully verified in this environment.
- Runtime qualification remains blocked: `systemctl` and `/run/systemd/private` are absent and PID 1 is Docker `docker-init`. Direct `/usr/bin/lsblk` execution is only a utility smoke check, not service-managed evidence.
- No product source or end-user documentation changed. The next dependency-ready action remains a named supported Linux host/VM run of `docs/agent/linux-agent-service.md:43-54`.

## C3-03 enrollment and upload boundary hardening — 2026-09-16

- Agent-side enrollment validation now rejects display names containing control characters, matching controller validation; node-bound bearer verification rejects empty or over-512-byte token values before hashing/database matching.
- The real-router CMDB upload path authenticates before its 4 MiB application check and returns the bounded `payload_too_large` error for oversized authenticated bodies; the route framework allowance is 4 MiB plus one byte so the handler owns that response.
- App Vault response and proxy hardening plus release-gate argv/output redaction are present in the same uncommitted worktree but remain separately attributable changes; they are not treated as C3 runtime qualification evidence.

## C3-03 enrollment/fingerprint contract closure — 2026-09-16

- `POST /api/nodes/enroll` now applies a 64 KiB route body limit and maps Axum JSON length-limit rejection to the stable `413 payload_too_large` envelope; other JSON extraction failures remain bounded `400 bad_request`.
- Inventory ingestion fingerprints a cloned snapshot after trimming `snapshot_id`, matching the trimmed persisted/replay identity. Equivalent surrounding-whitespace representations therefore replay instead of conflicting.
- Focused real-router tests cover oversized enrollment response shape and whitespace-normalized inventory replay. Supported-host systemd/runtime qualification, strict repository Clippy, and complete schema ownership remain blocked as previously recorded.
- Focused evidence after this checkpoint is recorded in the dated handoff; host systemd/runtime and strict repository lint gates remain blocked as previously documented.

## C3-03 managed-node authorization contract — 2026-09-17

- The node authentication seam now compares the HTTP `Bearer` scheme case-insensitively, while trimming and enforcing the existing 512-byte credential bound before the node-token hash lookup.
- Real Axum-router test `api::node_enroll::tests::lowercase_bearer_scheme_authenticates_approved_agent` proves a lowercase scheme succeeds for an approved, agent-capable node; existing missing/empty/oversized, wrong-node, and revoked credential tests remain the negative boundary evidence.
- This is `integration-verified` at the router/database boundary and does not promote C3-03 host runtime or release qualification. The Docker sandbox still lacks systemd and the supported host/device boundary.
- The direct end-user contract is documented in `docs/agent/node-enrollment.md`.

## C3-03 token contract parity — 2026-09-17

- `agent::state::MAX_NODE_TOKEN_BYTES` is the shared 512-byte bound used by persisted `HeartbeatToken` validation and controller node-token authentication.
- The state seam accepts exactly 512 bytes and rejects 513 bytes; the real Axum heartbeat route test authenticates an approved agent-capable node with an exactly 512-byte token, while the existing route test rejects over-limit credentials before database matching.
- Focused state, node-enrollment, and supervision test suites pass after this change. This remains unit/integration evidence only; the Docker sandbox lacks `systemd-analyze`, systemd, and supported host-device runtime boundaries.

## C3-03 runtime qualification session 11 — 2026-09-17

- Current source and focused contract evidence were rechecked without product-code changes: state (18), enrollment (10), supervision (4), transport (13), and inventory-router (3) Cargo tests passed; release-gate tests (11), repository truth, and `git diff --check` passed.
- The runtime blocker is directly reproduced in this Docker sandbox: `systemctl` is absent, `/run/systemd/private` is absent, and PID 1 is Docker `docker-init`. Consequently systemd lifecycle, service-managed `/usr/bin/lsblk`, host-state permissions, outage/restart recovery, upgrade, rollback, outbound-only observation, and artifact checksum evidence remain blocked.
- This checkpoint adds no runtime or release claim. The next dependency-ready action remains a named supported Linux host or VM run of `docs/agent/linux-agent-service.md:43-54`.

## C3-03 runtime qualification session 12 — 2026-09-17

- The supported-host prerequisite remains unavailable in the coding sandbox: `systemctl` is absent, `/run/systemd/private` is absent, PID 1 is Docker's `docker-init -- sleep infinity`, and only direct `/usr/bin/lsblk` execution is available. No service-managed or host-device qualification claim is permitted.
- Rechecked managed-node and upload seams without product-code changes: state (18), enrollment (10), supervision (4), transport (13), and inventory-router (3) focused tests passed. The full backend unit target passed 616 tests, while the existing `backend/tests/golden_path.rs` integration target failed because its expected `ci.yml` fixture is absent.
- `python3 scripts/repo_truth.py --repo . --json --check`, `python3 scripts/test_release_gate.py`, and `git diff --check` passed. Strict Clippy is blocked because `cargo-clippy` is not installed; schema ownership exits 0 but emits the known `rg: command not found` diagnostic.
- This checkpoint changed no product source or end-user documentation. The next dependency-ready action remains a named supported Linux host/VM run of `docs/agent/linux-agent-service.md:43-54`, not another source-only C3-03 implementation slice.

## C3-03 runtime qualification session 13 — 2026-09-17

- The prior golden-path workflow-contract failure was reproduced as an invocation-environment issue: `backend/tests/golden_path.rs` uses `GITHUB_WORKSPACE` when set, and the sandbox did not set it. With `GITHUB_WORKSPACE` set to the repository root, the workflow contract test passed and the tracked `.github/workflows/ci.yml` was read successfully.
- Final backend target evidence with the explicit workspace variable: 616 unit tests, 2 workflow-contract integration-test cases, and the example target passed. Focused state (18), transport (13), supervision (4), lowercase Bearer enrollment (1), inventory upload (3), release-gate (11), repository-truth, and diff checks passed. The YAML workflow-contract test is separate from the `integration-verified` router/database evidence.
- `systemctl`, `systemd-analyze`, and `/run/systemd/private` remain unavailable; PID 1 is Docker `docker-init`. C3-03 service lifecycle, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact qualification remain blocked and must not be promoted from this sandbox.
- Strict Clippy and rustfmt remain unavailable in the active toolchain; schema ownership still emits the known missing-`rg` diagnostic. The dated handoff is `docs/internal/handoffs/2026-09-17-c3-03-runtime-qualification-session-13-blocked.md`.

## C3-03 agent prequalification hardening — 2026-09-17

- `AgentTransport::upload_inventory` serializes the snapshot once and rejects payloads over 256 KiB before constructing or sending the HTTP request. The bound matches the collector/pending-snapshot contract and prevents a manually constructed public snapshot from bypassing the client-side limit.
- Focused transport test `agent::transport::tests::inventory_upload_rejects_oversized_snapshot_before_network_request` passed; the full backend unit target previously reached 618 tests before the known transient SQLite lock failure. The golden-path integration test requires `GITHUB_WORKSPACE` to resolve the tracked workflow and fails without that explicit environment in this sandbox.
- `docs/agent/linux-inventory-collector.md` now describes the shipped fixed-command collection, pending persistence, enrolled-node upload, 256 KiB request bound, and the separate supported-host qualification boundary.
- C3-03 remains blocked for systemd/service/device runtime, outage/restart, upgrade, rollback, and artifact qualification because `systemctl`, `systemd-analyze`, and `/run/systemd/private` are unavailable in this Docker sandbox.

## C3-03 enrollment lifecycle and error contract hardening — 2026-09-18

- `POST /api/nodes/enroll` claims `node_pairing_codes.used_at` with an atomic `used_at IS NULL` update before node creation. The real-router test `concurrent_enrollment_claims_a_pairing_code_once` proves two concurrent requests produce exactly one node, one `200`, and one `401`.
- Successful enrollment audit details are now JSON with `display_name` and `device_type` fields rather than delimiter-built text. The enrollment test parses the persisted audit detail and verifies both fields.
- The public `AppError` response seam has a focused redaction/bounds test proving database and internal errors omit raw SQL/provider text and remain under 256 bytes. Existing response mapping already emits generic `database_error`/`internal_error` messages while detailed causes stay in server logs.
- This checkpoint is unit/integration-verified at the enrollment/error seams. It does not establish supported-host systemd, service-managed collection, outage/restart runtime, upgrade, rollback, artifact, or release evidence; those remain blocked by the Docker sandbox boundary.

## C3-03 enrollment persistence and audit-boundary hardening — 2026-09-18

- Enrollment now wraps the pairing-code claim, owner existence check, and node insertion in one SQLite transaction; a node persistence failure rolls back `used_at` and leaves the code retryable. The real-router test `enrollment_rolls_back_pairing_claim_when_node_persistence_fails` uses a database trigger to exercise the failure boundary and verifies no node or claim remains.
- Node deletion audit details now use structured JSON for `display_name`; `node_delete_audit_details_are_structured` verifies exact preservation of commas, equals signs, and quotes.
- `docs/agent/node-enrollment.md` documents the transaction rollback/retry contract and structured audit representation.
- Focused `api::node_enroll::tests` passed 14 tests after the final change. Runtime/systemd/device and release qualification remain blocked by the sandbox boundary; unrelated worktree paths remain excluded.

## C3-03 supported-host runtime qualification session 14 — 2026-09-18

- The supported-host prerequisite was directly re-probed on commit `c8ee67eaca172259488e96fb7cc4c2691435a999`: `systemctl` is absent, `/run/systemd/private` is absent, PID 1 is Docker `docker-init -- sleep infinity`, `/dev/block` is absent, and the process runs as UID 1000. Direct `/usr/bin/lsblk` parsed four top-level devices but is not service evidence.
- Available source/router evidence remains green without product-code changes: transport (14), supervision (4), CMDB inventory/API (13), enrollment (14), full backend (622 unit + 2 workflow-contract integration tests + example), release-gate (11), repository truth, and diff checks passed. The preceding real-router/database tests remain the source of the existing `integration-verified` baseline; this checkpoint adds no integration coverage or maturity promotion.
- Runtime/service/release qualification remains `blocked` for systemd lifecycle, protected state, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact checksum evidence. `cargo fmt --check` and strict Clippy are blocked by missing toolchain components; schema ownership emits the known missing-`rg` diagnostic; repository hygiene reports pre-existing tracked internal-history paths.
- No product source or end-user documentation changed. The dated handoff is `docs/internal/handoffs/2026-09-18-c3-03-runtime-qualification-session-14-blocked.md`; the next dependency-ready action remains a named supported Linux host/VM run of `docs/agent/linux-agent-service.md:43-54`.

## C3-03 agent identity-bound recovery and input contract hardening — 2026-09-18

- `InventorySnapshotV1::validate` is the shared bounded contract for schema version, UUID snapshot ID, required bounded text, positive collection time, identity fields, entity count, control characters, and duplicate entity keys. The inventory route applies it after node authentication and before canonical host lookup; `api::cmdb::tests::inventory_upload_rejects_semantically_invalid_snapshots` proves the stable `400 bad_request` boundary.
- `PendingSnapshotStore` now persists a versioned envelope containing the enrolled node UUID and validates the envelope and snapshot before load/save. An absent sidecar is the normal first-run condition; a present sidecar that is legacy, malformed, semantically invalid, or bound to a different node fails closed instead of risking upload under a replaced state file; `agent::state::tests::pending_snapshot_store_rejects_a_sidecar_bound_to_another_node` proves the cross-node boundary.
- Custom CA enrollment files now require owner-only Unix `0600` permissions in addition to regular-file, symlink-chain, size, and PEM checks; `agent::tests::ca_read_rejects_group_or_world_readable_files` covers the permission failure.
- Focused evidence after the change: `cargo test agent:: --all-features` passed 40 tests; contract tests passed 4; semantic inventory route test passed 1. Runtime/systemd/device and release qualification remain blocked by the sandbox boundary.

## C3-03 supported-host runtime qualification session 15 — 2026-09-18

- The runtime prerequisite remains unavailable in this Docker sandbox: `systemctl` and `systemd-analyze` are absent, PID 1 is `/sbin/docker-init -- sleep infinity`, `/run/systemd/private` and `/dev/block` are absent, and the process runs as UID/GID 1000. These probes prevent service install/start/status, protected host-state, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact qualification claims.
- No product source or end-user documentation changed. Existing source/router evidence was rechecked after cleaning only disposable generated `/tmp` entries with the `vt-p1-*` and `voidtower-*` prefixes; no repository paths were removed or modified.
- Final available checks passed: `cargo test agent:: --all-features` (40), `cargo test api::cmdb::tests --all-features` (14), `cargo test cmdb::contracts::tests:: --all-features` (4), `python3 scripts/test_release_gate.py` (11), and `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` from `backend` (627 unit, 2 workflow-contract integration tests, and the example). `python3 scripts/repo_truth.py --repo . --json --check` and `git diff --check` passed; repository truth remains source inventory only with `runtime_support_claimed: false`.
- The schema ownership wrapper exits 0 but emits `rg: command not found`; `cargo fmt --check` and strict Clippy cannot run because the active 1.98.1 toolchain lacks `cargo-fmt` and `cargo-clippy`. These are evidence blockers, not reasons to alter unrelated worktree paths.
- This checkpoint makes no runtime or release maturity promotion. The next dependency-ready action remains the named supported Linux host/VM procedure at `docs/agent/linux-agent-service.md:43-54`.
