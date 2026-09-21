
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

## C3-03 Linux agent release-package readiness — 2026-09-18

- Release archives now contain the backend binary, frontend assets, and both checked-in systemd units under `packaging/systemd/`; the same layout is produced by `scripts/build-release.sh` and `.github/workflows/release.yml`.
- Release archive names normalize a leading `v` from Git tags, and the published architecture set is explicitly `x86_64` plus `aarch64`; unsupported host architectures fail closed. Offline installation skips package-manager/network dependency work and remote catalog/model/runtime downloads, uses Cargo/npm offline modes, requires a local source tree and local build dependencies, and requires an exact local `v<version>` tag for explicit offline versions. Release checksum entries are unique and case-normalized before comparison. Reset stops controller plus agent before state mutation and restarts controller then agent.
- `scripts/install.sh::download_binary` now installs packaged frontend assets and the agent unit. Source builds copy the agent unit as well. `install_service` renders the controller and agent units using the configured install/data/user paths, creates `${VT_DATA_DIR}/agent` with mode `0700`, enables both units, and guards agent boot with `ConditionPathExists` until an enrolled owner-only state file exists. Uninstall stops/disables/removes the agent unit; update/repair stop the old agent, refresh the generated unit, and restart it only when enrolled. Release downloads verify `SHA256SUMS` before extraction and reject traversal, symlink/hardlink/device archive members; source and catalog tarballs use the same validation helper, and a missing release manifest fails closed.
- `packaging/systemd/voidtower.service` no longer passes unsupported `--data-dir`/`--config-dir` flags and uses explicit `VOIDTOWER_*` environment settings. The agent unit continues to omit `PrivateDevices=true` so host `/dev` remains visible for bounded `lsblk` collection.
- `scripts/test_agent_package.py` exercises shell syntax, the release archive layout with a disposable fake build, installer contracts, adversarial traversal/symlink archive members, unit hardening, and the corrected server unit. The release-gate manifest runs this suite as the required `agent-package-contracts` gate.
- Evidence: package contracts (8) and release-gate tests (11) passed; `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` passed (627 unit, 2 workflow-contract integration tests, example); `cargo test agent:: --all-features` passed (40); repository truth and diff checks passed. The focused package/release-gate rerun after archive, checksum, and systemd detection hardening passed (19 tests). `scripts/check-repository-hygiene.sh` remains blocked by its pre-existing tracked internal-history policy list, schema ownership emits the known missing-`rg` diagnostic, and fmt/Clippy are unavailable.
- This is `unit-verified`/package-contract verified, not runtime- or release-qualified. A named supported Linux host with real systemd and host `/dev` is still required for installation/start/status, protected state, collection/upload, outage/restart, upgrade, rollback, and artifact checksum evidence.

## C3-03 collector/CMDB contract parity — 2026-09-18T14:57:30Z

- `backend/src/collector.rs::collect_linux_snapshot` now validates the complete `InventorySnapshotV1` before returning. Invalid snapshot UUIDs and non-positive collection times fail closed as `CollectorError::InvalidSnapshot`; the fixture helper uses a fixed valid UUID and positive timestamp rather than emitting a controller-invalid fixture.
- Physical-disk output now maps `lsblk` `TRAN` to `attributes.protocol`, `ROTA` to `attributes.rotation`, and retains bounded serial/WWN values in attributes as well as identity evidence. `runtime` remains reserved for collection-runtime facts, matching `cmdb::observations` classification and registration rules.
- Public-seam evidence: `cargo test collector::tests --all-features` passed 6 tests, including invalid metadata and field mapping. Real-router evidence: `cargo test api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features` passed and proved one trusted physical disk registers without review while persisted attributes retain the classification fields.
- Maturity is `integration-verified` for the collector-to-router/database boundary only. C3-03 supported-host systemd, host `/dev`, outage/restart, upgrade/rollback, and artifact/release qualification remain blocked by the Docker sandbox.

## C3-03 collector/CMDB capacity vocabulary parity — 2026-09-18

- The Linux collector now maps `lsblk` `SIZE` to the CMDB canonical `attributes.capacity_bytes`; it rejects missing, null, zero, negative, and string sizes before producing a snapshot, and it no longer emits the unrelated `size_bytes` alias used by other product domains.
- The real Axum/router/SQLite test `linux_collector_snapshot_reaches_reconciliation_classification` proves the positive capacity value persists alongside protocol, rotation, serial, and WWN evidence. `inventory_upload_rejects_semantically_invalid_snapshots` proves zero capacity returns bounded `400 bad_request` and creates no inventory snapshot.
- Focused evidence is `integration-verified` for this collector-to-router/database contract. Supported-host systemd/device runtime, outage/restart, upgrade/rollback, release artifacts, strict Clippy/rustfmt, and complete schema-ownership verification remain blocked as previously recorded.

## C3-03 supported-host runtime qualification session 18 — 2026-09-18

- The runtime prerequisite was re-probed at HEAD `1b418a42f947fecb2bfceef0915b6e5f6e9e5bfa`: `systemctl` and `systemd-analyze` are absent, PID 1 is `/sbin/docker-init -- sleep infinity`, `/run/systemd/private` and `/dev/block` are absent, `/usr/bin/lsblk` exists, and the process runs as UID/GID 1000. No service-managed or host-device qualification claim is permitted.
- Deterministic evidence passed without product-code changes: agent (40), CMDB API (15), contract (4), package/release-gate (19), and full backend targets (630 unit, 2 workflow-contract integration tests, example) after removing only generated `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures. Repository truth, shell/JSON syntax, and diff checks passed.
- `cargo fmt --check` and strict Clippy are blocked by missing toolchain components; schema ownership exits 0 but emits the known missing-`rg` diagnostic. This checkpoint adds no integration coverage or maturity promotion.
- The next dependency-ready action remains a named supported Linux host/VM run of `docs/agent/linux-agent-service.md:43-54`, not another source-only C3-03 implementation slice.

## C3-03 supported-host runtime qualification session 19 — 2026-09-18

- The sandbox boundary was re-probed at the current checkout: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` and `/dev/block` are absent; `/usr/bin/lsblk` exists; and the process runs as UID/GID 1000. Service-managed installation, protected host-state, collection/upload, outage/restart, upgrade, rollback, and artifact qualification therefore remain blocked.
- Deterministic evidence was re-run without product-code changes: `cargo test agent:: --all-features` passed 40 tests; `cargo test api::cmdb::tests --all-features` passed 15; `cargo test cmdb::contracts::tests:: --all-features` passed 4; package/release-gate unittest coverage passed 19; and `GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features` passed 630 unit tests, 2 workflow-contract integration tests, and the example target.
- `python3 scripts/repo_truth.py --repo . --json --check` passed with `runtime_support_claimed: false`; shell/JSON syntax and `git diff --check` passed. `cargo fmt --check` and strict Clippy are unavailable because the active toolchain lacks `cargo-fmt` and `cargo-clippy`; schema ownership exits 0 but emits the known `rg: command not found` diagnostic.
- The unrelated worktree paths remain untouched: `backend/src/api/apps.rs`, `scripts/release_gate.py`, `scripts/test_release_gate.py`, and untracked `testing/`. No source-only C3-03 implementation was started because the newest handoff explicitly identifies supported-host runtime as the next dependency.

## C3-03 supported-host runtime qualification session 20 — 2026-09-19

- The current Docker sandbox still cannot provide the required supported-host boundary: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` and `/dev/block` are absent; `/usr/bin/lsblk` exists; and the process runs as UID/GID 1000. Service installation/status, protected host-state, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact qualification remain blocked.
- Deterministic evidence was re-run without product-code changes: `cargo test agent:: --all-features` passed 40 tests; `cargo test api::cmdb::tests --all-features` passed 15; `cargo test cmdb::contracts::tests:: --all-features` passed 4; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` passed 19; and `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` passed 630 unit tests, 2 workflow-contract integration tests, and the example target.
- `python3 scripts/repo_truth.py --repo . --json --check`, shell/JSON syntax, and `git diff --check` passed; source truth reports `runtime_support_claimed: false`. `cargo fmt --check` and strict Clippy remain unavailable because the active toolchain lacks `cargo-fmt` and `cargo-clippy`; schema ownership exits 0 but emits `rg: command not found`.
- No product source or end-user documentation changed. The next dependency-ready action remains the named supported Linux host/VM procedure at `docs/agent/linux-agent-service.md:56-64`; do not start an unrelated source-only C3-03 change or promote runtime/release evidence from this sandbox.

## C3-03 supported-host runtime qualification session 21 — 2026-09-19

- The Docker sandbox remains outside the supported-host boundary: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` and `/dev/block` are absent; `/usr/bin/lsblk` exists; and the process runs as UID/GID 1000. Service installation/status, protected host-state, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact qualification remain blocked.
- Deterministic evidence passed without product-code changes: agent (40), CMDB API (15), contract (4), package/release-gate (19), and full backend targets (630 unit, 2 workflow-contract integration tests, example). `python3 scripts/repo_truth.py --repo . --json --check`, `bash -n scripts/install.sh scripts/build-release.sh`, `python3 -m json.tool scripts/release-gates.json`, and `git diff --check` passed.
- Schema ownership is blocked: `scripts/check-schema-migration-ownership.sh` exits 0 while emitting `rg: command not found`, so its success cannot be treated as verification. `cargo fmt --check` and strict Clippy remain unavailable because the active toolchain lacks `cargo-fmt` and `cargo-clippy`.
- This checkpoint adds only continuity evidence. The next dependency-ready action remains a named supported Linux host/VM run of `docs/agent/linux-agent-service.md:56-64`, not another source-only C3-03 implementation slice.

## C3-03 executable recovery and collector failure contracts — 2026-09-19

- `collector::collect_linux_program` now delegates through an explicit timeout parameter while production collection retains the fixed 10-second bound. Supervision uses the same bounded program seam and production path remains `/usr/bin/lsblk`.
- Collector tests exercise missing/empty programs, non-zero exit, non-UTF-8 output, stderr overflow, timeout, and timeout child termination. A timed-out process is explicitly killed and awaited before retry; diagnostics are retained only within the configured bound plus one sentinel and never appear in the returned error.
- The supervision integration test drives a sanitized executable fixture and a loopback HTTP controller: an ambiguous upload leaves the owner-only node-bound sidecar, a fresh supervision run reuses the exact snapshot ID, and a typed matching acknowledgement clears the sidecar. It does not establish a real controller/database or systemd runtime claim.
- Focused evidence after the final edit: `cargo test collector::tests --all-features` passed 11 tests; `cargo test agent:: --all-features` passed 41 tests; `cargo test api::cmdb::tests --all-features` passed 15 tests; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` passed 19 tests; and `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` passed 635 unit tests, 2 workflow-contract integration tests, and the example target.
- The first concurrent final-gate attempt exhausted the documented 512 MiB `/tmp` disposable-fixture space; removing only `/tmp/vt-p1-*` and `/tmp/voidtower-*` and rerunning serially produced the green counts above. `cargo fmt --check` and strict Clippy remain unavailable because `cargo-fmt` and `cargo-clippy` are absent; schema ownership remains unverified because the wrapper emits `rg: command not found`.
- End-user documentation changed in `docs/agent/linux-agent-service.md` to enumerate bounded command failure behavior. Supported-host systemd installation/status, protected state, service-managed collection/upload, outage/restart, upgrade, rollback, artifact checksum, and release qualification remain blocked by the sandbox boundary.

## C3-03 supported-host runtime qualification session 23 — 2026-09-19

- The supported-host boundary was re-probed at base commit `0422bf34f60e3abfc4b1c154f9c5cd07bbad4dc2`: PID 1 is `/sbin/docker-init -- sleep infinity`; `systemctl` and `systemd-analyze` are absent; `/run/systemd/private` and `/dev/block` are absent; `/usr/bin/lsblk` exists; and the process runs as UID/GID 1000. Service installation/status, protected state, service-managed collection/upload, outage/restart, upgrade, rollback, and artifact qualification remain blocked.
- Deterministic evidence passed without product-code changes: agent (41), CMDB API (15), contracts (4), package/release-gate (19), and the serial full backend target (635 unit tests, 2 workflow-contract integration tests, and the example). Repository truth, shell/JSON syntax, and diff checks passed; source truth reports `runtime_support_claimed: false`.
- `cargo fmt --check` and strict Clippy are unavailable because the active toolchain lacks `cargo-fmt` and `cargo-clippy`; schema ownership remains unverified because the wrapper emits `rg: command not found`; repository hygiene remains blocked by its pre-existing tracked internal-history policy list.
- A concurrent full-test attempt exhausted the 512 MiB `/tmp` disposable fixture space. Removing only `/tmp/vt-p1-*` and `/tmp/voidtower-*` and rerunning serially produced the green full-test result. No product source or unrelated worktree path was changed.
- The dated handoff is `docs/internal/handoffs/2026-09-19-c3-03-runtime-qualification-session-23-blocked.md`. The next dependency-ready action remains a named supported Linux host/VM run of `docs/agent/linux-agent-service.md:56-64`, not another source-only C3-03 implementation slice.

## C3-03 state recovery integrity hardening — 2026-09-19

- `AgentState::load` and `PendingSnapshotStore::load` now walk the existing parent chain through stable directory descriptors (`openat` with `O_NOFOLLOW`) and open the protected file relative to that descriptor before parsing. On Unix, every existing parent must be a real directory, contain no symlinks, avoid non-sticky group/other write permissions, and have an immediate parent owner matching the protected file owner; protected targets use nonblocking reads and reject special files without hanging.
- The save path already enforced the directory policy; focused regression tests now prove both load seams fail closed when their parent becomes group-writable, a parent/target is symlinked, or a FIFO replaces the protected file. Error text names only the bounded security condition and no state contents.
- Focused evidence after implementation: `cargo test --manifest-path backend/Cargo.toml agent::state::tests:: --all-features` passed 23 tests. This is unit-verified only; the supported systemd/device runtime remains blocked by the sandbox.

## C3-03 enrollment input and diagnostic hardening — 2026-09-19

- `voidtower agent enroll` retains the legacy `--pairing-code VALUE` compatibility option and adds `--pairing-code-stdin`; the latter reads the first newline-terminated bounded UTF-8 line from stdin, accepts LF or CRLF, rejects empty/invalid-UTF-8/oversized input without echoing the value, and avoids putting the credential in argv. The legacy option remains argv-visible during execution and is documented as compatibility-only; trailing stdin is not interpreted as pairing-code data.
- Optional CA enrollment reads use Unix `O_NOFOLLOW | O_NONBLOCK` before regular-file, owner-only permission, size, and PEM validation. A FIFO or other special path therefore fails promptly instead of blocking on a writer; the existing symlink-parent and permission checks remain in force.
- `AppError::Database` and `AppError::Internal` now render generic messages and emit only stable error codes to tracing; nested SQL/provider diagnostics are not serialized or logged by this boundary.
- Focused evidence: agent tests passed 4, lifecycle tests passed 15, and error tests passed 1 with `cargo test --manifest-path backend/Cargo.toml ... --all-features`; the serial full backend gate passed 647 unit tests, 2 integration tests, and the example target. Runtime/release qualification remains blocked because the sandbox lacks systemd, `/run/systemd/private`, host `/dev`, and a named packaged-host procedure.

## C3-03 enrollment input and diagnostic hardening handoff — 2026-09-19 (commit a7f5a98)

- `voidtower agent enroll --pairing-code-stdin` is the recommended enrollment credential path: it consumes one bounded UTF-8 line, accepts LF/CRLF, rejects empty/oversized/invalid input without echoing the value, ignores trailing stdin, and keeps the pairing code out of argv. The legacy `--pairing-code VALUE` option remains compatibility-only and argv-visible.
- Optional CA loading opens Unix paths with `O_NOFOLLOW | O_NONBLOCK` before regular-file, owner-only permission, size, and PEM checks; FIFOs and other special paths fail promptly rather than blocking agent startup.
- `AppError::Database` and `AppError::Internal` redact nested diagnostics from both user-facing rendering and debug formatting while retaining stable tracing error codes.
- Evidence at commit `a7f5a98657e49a1b78e7540f1eba725ef5720c32`: agent focused tests 46 passed; lifecycle 15 passed; error 1 passed; full backend 647 unit tests, 2 integration tests, and example passed; package/release tests 19 passed; source truth, shell/JSON syntax, and diff checks passed. Source truth reports `runtime_support_claimed: false`.
- The next dependency-ready action remains the supported-host C3-03 runtime/release runbook in `docs/agent/linux-agent-service.md:56-64`; systemd, `/run/systemd/private`, host `/dev`, installation privileges, and a packaged candidate are unavailable in this sandbox. Do not promote runtime/release maturity or start an unrelated source-only C3-03 slice.

## API-token cache invalidation hardening — 2026-09-19

- The Bearer compatibility-session cache now retains the token's own `expires_at` separately from the one-hour temporary session lifetime; natural token expiry therefore cannot be extended by a cache hit.
- `DELETE /api/integrations/tokens/:id` deletes the database row and invalidates cached sessions by stable token identity before returning. Real-router coverage proves a cached token succeeds before revocation and returns `401 Unauthorized` on the next request after revocation; a one-second expiry fixture proves the same after natural expiry.
- Evidence: `cargo test --manifest-path backend/Cargo.toml scope_bypass_tests --all-features` passes 13 tests, including the revoke and expiry cases; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features` passes; `python3 scripts/repo_truth.py --repo . --json --check` and `git diff --check` pass; independent review passed with empty security and logic findings. `cargo fmt --check` and strict Clippy are unavailable because the active toolchain lacks those components.
- The Docker sandbox still cannot provide C3-03 supported-host systemd/device/release qualification; this authentication slice does not change that boundary.

## C3-02 inventory convergence safety — 2026-09-19

- `POST /api/nodes/:node_id/inventory` accepts a valid host-only snapshot with zero entities as a recorded, non-converging upload. `converge_missing` returns zero before examining prior observations, so an empty or host-only collection cannot mark existing assets or observations missing; the response reports `missing: 0`. Non-empty snapshots retain omission convergence.
- Real-router coverage in `backend/src/api/cmdb/tests.rs` proves the empty-snapshot response and prior disk state remain online, administrator-owned resource/asset fields survive a later matching discovery upload, and inventory audit/event records share the request correlation ID with node actor attribution and no token payload.
- Evidence at this checkpoint: `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml api::cmdb::tests --all-features` passed 17 tests. Runtime service/release qualification remains blocked by absent systemd, `/run/systemd/private`, host `/dev`, installation privileges, and packaged-host evidence.

## M1-04 App Vault response and proxy contract hardening — 2026-09-19

- App Vault deployed/external-stack DTOs no longer serialize host compose or storage-root paths; frontend types and App Vault adoption UI match the safe response shape.
- Deployed-app members are owner-scoped for read/proxy seams and mismatches use not-found envelopes; external discovery is owner/admin/operator-only. Compose reads are bounded to 256 KiB and redact sensitive key values; logs are bounded to 64 KiB on UTF-8-safe byte boundaries; Docker logs/status drain both streams concurrently, fail on oversized output, terminate after 30 seconds, and return bounded `503 feature_unavailable` responses instead of success fallbacks. External discovery uses a separate bounded metadata stream and does not probe host paths.
- Embed proxy responses use the standard error envelope, preserve the incoming query string, disable redirects, remove cookies/hop-by-hop/frame-policy headers, reject upstream non-success statuses, and cap successful bodies at 4 MiB. A real-router test uses a loopback upstream and proves query forwarding and header filtering.
- Focused evidence: `cargo test --manifest-path backend/Cargo.toml api::apps --all-features` passed, including the deployed-app API and security regression tests; `npm run type-check` passed. Runtime Docker/App Vault/browser qualification remains unverified in this sandbox.

## M1-04 App Vault review remediation — 2026-09-20

- Compose command/entrypoint redaction now covers inline `KEY=VALUE`, short `-e` environment arguments, and `-H`/`--header` credential-bearing values; the sensitive-key corpus includes authorization and bearer forms. Regression coverage exercises scalar and sequence forms without retaining credential values.
- Docker compose and discovery commands now stop draining at the first over-bound read, kill the child, await termination, and enforce the 30-second timeout. GPU/container probes use the same immediate kill behavior on bounded-output failure. A Unix test runs an unbounded `yes` writer and proves overflow returns within two seconds.
- `/api/members/me/access` returns `MemberSelfAccessSummary` with `MemberSelfDriveSummary` entries that omit administrator-only `host_path`; the frontend client and member deploy picker use the separate safe type. App Vault preserves a visible error state when Docker status returns bounded `503` instead of rendering an empty successful result, and refresh forces a new status request.
- The embedded proxy bounds path/query sizes and filtered response headers in addition to the 4 MiB body bound; it preserves query forwarding while rejecting oversized request metadata.
- Final deterministic evidence after remediation: backend full target gate passed 670 unit tests, 2 integration tests, and the example; frontend test gate passed 59 tests in 13 files, type-check/build/lint passed; repository truth, release-gate tests, shell/JSON checks, and diff checks passed. `cargo fmt --check` and strict Clippy remain unavailable because the active toolchain lacks those components. Runtime/browser/Docker qualification remains blocked by the sandbox boundary.

## M1-04 App Vault final trust-boundary hardening — 2026-09-20

- JSON-bearing App Vault and member-management handlers now authenticate and enforce their role before parsing request bodies; malformed unauthenticated bodies therefore cannot reach JSON validation or host-path handling.
- Docker command cancellation and bounded-output cleanup signal the complete process group on Unix, wait for the direct child, and poll both leader and group after escalation; compose status/probe and container metadata outputs remain bounded. Long-form published ports reject values above `65535` before narrowing.
- Latest focused evidence: `cargo test --manifest-path backend/Cargo.toml containers::log_output_tests --all-features` passed 3 tests and `cargo test --manifest-path backend/Cargo.toml api::apps::security_tests:: --all-features` passed 17 tests. The subsequent full backend target gate passed 670 unit tests, 2 integration tests, and the example after storage cleanup. `python3 scripts/repo_truth.py --repo . --json --check` passed; `python3 scripts/test_release_gate.py -v` passed 11 tests. The direct release-gate runner remains blocked by sandbox disk/resource limits, unavailable cargo-clippy/cargo-deny, and pre-existing unrelated worktree changes. Runtime/browser/Docker qualification remains blocked by the sandbox boundary.

## R0-02 release-candidate gate runner — 2026-09-20

- `scripts/release_gate.py` is the repository-contained, non-publishing evidence collector. `scripts/release-gates.json` is the source-owned manifest for explicit argv, repository-relative working directories, subsystem applicability, required status, bounded timeouts, and declared artifact hashes.
- Gate subprocesses run through the parent-death-aware `scripts/process_supervisor.py`, which keeps the supervisor and command in one process group, uses Linux child-subreaper tree cleanup for `setsid()` descendants, and concurrently drains stdout/stderr into bounded in-memory diagnostics. Repository-truth Git commands apply the output-size limit only inside the Git target, avoiding inherited limits that can break nested repository-truth subprocesses; timeout and cleanup remain fail closed. Manifest paths, executables, inline interpreter code, credential-like argv values, and output diagnostics are validated or redacted before report emission.
- Focused evidence after the boundary fix: `python3 scripts/test_release_gate.py -v` passed 21 tests and `python3 scripts/test_repo_truth.py -v` passed 18 tests. The two repository-truth fixture cases that require executable Git wrappers use checkout-local temporary directories because this sandbox's `/tmp` mount is `noexec`.
- Current release-run limitations remain explicit: the changed-scope report records mandatory unchanged gates as skipped and returns failure by design; repository hygiene reports pre-existing forbidden tracked `docs/internal` paths; `cargo deny` is unavailable; process supervision is Linux-specific; and runtime/browser/Docker qualification is outside this source-only milestone. See `docs/release-gates.md` for the public operator contract.

## R0-02 release-candidate gate runner follow-up — 2026-09-20

- The runner now executes each validated gate through `scripts/process_supervisor.py`. The supervisor and command share a process group; Linux `PR_SET_PDEATHSIG` plus a supervisor signal handler kills forked descendants when the runner is forcibly terminated. A regression test starts a forked descendant, kills the runner, and proves the descendant cannot write its delayed marker.
- Credential redaction now covers separate and `=`-embedded AWS access/secret/session flags in both hyphen and underscore forms, access-token and auth-token flag variants, and quoted whitespace-containing diagnostic values. Regression assertions prove those values do not enter the machine-readable report.
- Repository-truth Git commands use the same parent-death-aware supervisor while retaining bounded temporary-file output and timeout/process-group cleanup. Focused release-gate and repository-truth suites remain green after this change.
- The changed-scope release report is not release-qualified in this sandbox: repository hygiene reports pre-existing tracked `docs/internal` history, the supply-chain gate reports the unavailable/blocked `cargo-deny` path, and runtime/browser/Docker qualification remains outside this source-only milestone.

## R0-02 repository prerequisite closure — 2026-09-20

- `scripts/check-repository-hygiene.sh` treats tracked `docs/internal/*` continuity evidence as an explicit `continue` while retaining fall-through violations for private planning, generated output, credentials, keys, local machine state, and all tracked symlinks. Git paths are consumed with NUL-safe records, the script anchors nested calls to the repository root, and the real checkout passes without weakening sensitive-path rejection.
- `scripts/check-schema-migration-ownership.sh` delegates to `scripts/check_schema_migration_ownership.py` and has no `rg` dependency. The Python gate rejects Rust `sqlx` DDL outside `backend/src/db/legacy.rs`, including comment-separated paths, turbofish/macro forms, and the complete conservative `query_file_*` family, rejects source/migration symlinks and canonical paths escaping the repository, requires contiguous tracked `backend/migrations/NNNN_name.sql` files starting at `0001`, and emits a nonzero result for malformed or missing policy inputs.
- `scripts/test_repository_prerequisites.py` provides nine fixture-level contract tests for the allowlist, NUL-safe credential rejection, tracked symlinks, forbidden DDL variants, query-file rejection, canonical-path containment, missing `rg`, and migration gaps. Focused evidence is unit-verified; the real checkout's hygiene and schema gates pass.
- `rustfmt` and `clippy` are now installed through the active rustup toolchain. CI pins `cargo-deny@0.20.2` because older cargo-deny cannot parse current CVSS 4.0 RustSec metadata; `cargo deny check` passes after updating the vulnerable `rustls` lock entry from `0.23.40` to `0.23.45`. Full backend tests pass (671 unit, 2 integration, 0 example tests failed); strict Clippy and formatting still expose pre-existing unrelated backend findings and are not promoted by this slice.

## R0-03 backend quality prerequisite closure — 2026-09-20

- The tracked prerequisite continuation formatted the backend with `cargo fmt --all` and resolved the strict Clippy baseline without changing product route behavior: the only non-format source adjustments are a named `ProviderRow` tuple alias for `clippy::type_complexity` and explicit `dead_code` allowances for intentionally retained supervision/LXC compatibility seams.
- `scripts/release-gates.json` now declares `backend-format` as a required gate beside strict `backend-clippy`; `docs/release-gates.md` documents that both must pass in the same evidence run. The manifest remains explicit-argv, repository-relative, non-publishing evidence collection.
- Final source/test evidence: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `GITHUB_WORKSPACE=\"/workspace/Documents/voidtower_project_files_full/hive/voidtower\" cargo test --all-targets --all-features` (671 unit, 2 integration, and 0 example failures), `cargo deny check`, repository truth, hygiene/schema gates, and the all-subsystem release manifest all passed. The manifest's standalone MCP gate required declared dependencies installed outside the checkout at `/workspace/.voidtower-mcp-deps`; it passed 31 tests.
- This is `integration-verified` for the repository/build/test boundary, not runtime-verified or release-qualified. Host systemd, Docker, browser, clean-install, upgrade/recovery, named-platform, and packaged-artifact evidence remain outside the sandbox. Untracked `testing/` and `scripts/__pycache__/` paths were preserved and not staged.

## M1-02 built-in MCP and Studio ingress boundary — 2026-09-20

- Built-in MCP routes `/api/mcp` and `/api/mcp/message` are handler-authenticated API-token seams. Their route metadata is explicitly `BearerPolicy::Unscoped` so valid tokens reach `mcp::check_mcp_auth`; tool-specific scope enforcement remains in `operations::invocation::authorize_action` and the shared `mcp::invoke_tool` choke point. Unknown route metadata remains default-deny.
- The approved built-in mutation is `container.start`, with `containers:restart` scope, canonical `resource_id`, typed `request_id`, immutable plan/submit, durable job, policy/approval, idempotency, audit/event behavior inherited from the operation kernel. Wrong scope is rejected before resource lookup/provider mutation. MCP and Studio share the same invocation and redaction path.
- JSON-RPC requests reject unknown outer fields and unsupported `jsonrpc` versions; `tools/call` requires object params and object arguments. MCP and Studio invocation bodies have a 64 KiB route limit. `POST /api/ai/ask` remains provider streaming only and no longer advertises write-capable tools in its system prompt.
- Real-router tests in `backend/src/api/scope_bypass_tests.rs` prove valid MCP bearer reachability, wrong-tool-scope denial, JSON-RPC invalid-version/unknown-field behavior, Studio tool discovery, and Studio unknown-field rejection. Focused MCP, Studio, scope-enforcement, and AI prompt tests pass.
- Evidence at this checkpoint is `integration-verified` for the source/real-Axum/authentication/serialization boundary. Standalone MCP, webhook/automation/scheduler/CLI convergence, provider/runtime qualification, browser qualification, and release qualification remain separate gaps. The two untracked paths `testing/` and `scripts/__pycache__/` remain unrelated and preserved.

## M1-02 ingress validation correction — 2026-09-20

- The built-in MCP message handler now authenticates and checks the feature flag before consuming the request body. Unauthenticated malformed requests therefore return `401` without JSON parsing; authenticated malformed JSON returns a bounded `400` JSON-RPC `-32600` response, and serde data/unknown-field failures remain `422` where the extractor classifies them that way.
- `tools/call` params and every currently exposed direct read-tool argument schema reject unknown fields; initialize/tools/list reject non-object params, explicit null tool arguments are rejected, and JSON-RPC ids are restricted to null/string/number. The published `tools/list` schemas now set `additionalProperties: false` consistently. Studio uses the same strict typed tool validation through `invoke_tool`.
- Tool errors pass through the shared AI redaction pipeline and a 4096-character bound before MCP/Studio serialization. This preserves error-path secret scrubbing and bounded diagnostics; provider/runtime qualification remains unverified.
- Focused regression coverage now includes authentication-before-body parsing, malformed-body JSON-RPC behavior, unknown `tools/call` params, direct read-tool unknown arguments, and the standard Studio `unprocessable_entity` envelope. `cargo clippy --all-targets --all-features -- -D warnings`, rustfmt check, focused tests, and the prior full backend gate are the required final checks.

## M1-02 final review correction — 2026-09-20

- Studio MCP maps the extractor's status instead of collapsing all body failures to `422`: malformed JSON is `400`, unsupported media is `415`, oversized bodies are `413`, and serde validation/unknown fields are `422`, each with a bounded public error envelope.
- Final evidence for this slice is `unit-verified` and `integration-verified`: 679 backend unit tests plus 2 golden-path integration tests pass, clippy and rustfmt pass, schema migration ownership passes, and `git diff --check` passes. Every `invoke_tool` error path now shares the redaction/bound helper, and MCP's final `Error: ` wrapper is bounded as well. No runtime or browser qualification was performed because the host supervisor did not provide a disposable runtime.

## M1-03 automation/webhook ingress contract hardening — 2026-09-20

- `GET /api/automation` and `GET /api/automation/:id/runs` now require the operator role allowlist because their responses expose shell commands and captured output. Run-history limits are bounded to 1–200; invalid limits return `400 bad_request`.
- Automation create/update bodies reject unknown fields and validate required/bounded name, command, description, timeout, and schedule values. Supported schedules are the named intervals plus bounded `*/N` minute forms; invalid schedules no longer silently run hourly. Missing update targets return `404 not_found`.
- Inbound `/api/integrations/webhooks` requests reject unknown fields and require exactly one automation or structured-action intent. Secret verification now precedes content-type and JSON parsing; the route accepts only bounded `application/json` bodies. Automation webhook intent uses canonical `automation.run` planning/policy/durable-job submission with `automation` actor and `webhook` ingress; dry-run does not create a job, valid idempotency replays, and changed intent conflicts. The route metadata explicitly advertises every reachable canonical action, including `automation.run`.
- Real-Axum-router coverage in `backend/src/api/operation_workflows_tests.rs` proves non-operator read denial, bounded query/write validation, webhook auth-before-parse, media-type and malformed-body errors, webhook dry-run, durable job shape, replay/conflict, and ambiguous/unknown intent rejection. Focused operation-workflow, integration, and action-registry tests pass after the final change.
- Evidence is `integration-verified` for the source/real-router/database boundary. Outbound webhook SSRF/signature hardening, service webhook adapters, CLI convergence, provider execution, runtime/browser, and release qualification remain separate gaps.

## M1-03 signed inbound webhook verification and replay protection — 2026-09-20

- `POST /api/integrations/webhooks` now requires `X-VoidTower-Timestamp`, `X-VoidTower-Nonce`, and `X-VoidTower-Signature: sha256=<hex>`. The HMAC-SHA256 message is the exact raw body bound as `timestamp.nonce.raw_body`; timestamps use a ±300-second window and nonces are bounded ASCII delivery identities.
- The handler reads at most 64 KiB, verifies the signature before JSON parsing, and atomically claims `(source_id, nonce)` in `webhook_replay_receipts`. Receipts are pruned after 15 minutes; duplicate claims return bounded `409 webhook_replay` and cannot create another job. Bearer-only, malformed, stale, future, and tampered credentials return bounded `401 webhook_authentication_failed`.
- Migration `backend/migrations/0006_webhook_replay_receipts.sql` and `backend/tests/schema_golden.sql` extend the numbered schema; existing canonical automation/container planning, dry-run, idempotency-key replay, changed-intent conflict, policy, and deferred service behavior remain unchanged.
- Focused real-router evidence: `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture` passed 10 tests; database migration/golden evidence: `cd backend && cargo test db::tests --all-features -- --nocapture` passed 27 tests. Strict Clippy, rustfmt, frontend tests/type-check/lint/build, and diff checks also passed.
- The earlier M1-03 note's Bearer-era phrase "secret verification precedes content-type" is superseded: signed-header parsing precedes body reads, while HMAC verification necessarily follows the bounded raw-body read and still precedes JSON parsing.
- Evidence is `integration-verified` at the Axum-router/SQLite boundary. Host runtime, provider, browser, Docker, installation, upgrade/recovery, and release qualification remain blocked or untested.

## M1-03 encrypted inbound webhook credential continuation — 2026-09-20

- The Odysseus inbound webhook credential now uses the encrypted `secrets` store with purpose `odysseus_webhook`, referenced by `settings.key = 'odysseus.webhook_secret_id'`. Webhook verification resolves the reference through the secret manager and fails closed for missing, disabled, corrupt, oversized, or database-unavailable credentials.
- Startup migration `migrate_legacy_webhook_secret` transactionally encrypts legacy `odysseus.webhook_secret`, writes the reference, and removes the plaintext only after all writes succeed. Empty/oversized legacy values fail migration without deleting the legacy setting.
- `POST /api/integrations/odysseus/config` accepts explicit regenerate/revoke lifecycle flags, rejects plaintext credential fields, rejects simultaneous regenerate and revoke requests, returns a newly regenerated credential once, and never returns it from GET. Unrecognized JSON fields remain ignored for compatibility with the existing extractor contract. Revoke disables metadata in place; regenerate re-enables the referenced secret or repairs a dangling reference.
- Focused real-router and migration tests cover one-time response/GET redaction, encrypted persistence, legacy removal, oversized migration recovery, revoke disablement, signed webhook behavior, and plaintext field rejection. Runtime/provider/browser/release qualification remains unverified.

## M1-03 Odysseus outbound URL and egress hardening — 2026-09-20

- `POST /api/integrations/odysseus/config` validates each non-empty supplied `allowed_url` before any setting mutation; an empty value explicitly clears the endpoint. The shared `backend/src/ai/egress.rs::validate_local_endpoint` contract bounds the value to 2048 bytes, accepts only HTTP(S) URLs without credentials/query/fragment, resolves DNS, and permits only the narrow loopback/RFC1918 local exception; IPv4/IPv6 metadata, link-local, documentation, special-purpose, and prohibited addresses remain blocked.
- `GET /api/integrations/odysseus/theme` and the legacy Odysseus AI fallback use the pinned, redirect-free, ambient-proxy-free local egress client. Theme responses are bounded to 64 KiB and all transport, status, malformed-body, and oversized-body failures use stable redacted messages.
- Real-router coverage in `backend/src/api/integrations.rs` proves unsafe destinations are rejected without persistence and a controlled loopback upstream is reached through the configured theme route. The native Integrations panel now uses the registered POST method rather than an unsupported PATCH.
- Evidence is `integration-verified` for the focused Axum/SQLite plus controlled-upstream boundary and `unit-verified` for the egress policy. Host runtime, external provider, browser, Docker, installation, upgrade/recovery, and release qualification remain unavailable or untested.

## M1-04 production compatibility mutation inventory — 2026-09-20

- `scripts/compatibility_mutation_inventory.py` scans production Rust functions under `backend/src/api` (test modules and non-code strings/comments excluded) and emits only bounded metadata. It fails closed on unknown provider HTTP mutation, filesystem mutation, process execution, or known direct-provider call markers; operation adapters are outside the scanned ingress boundary.
- Current classifications are source-derived and explicit: canonical adapter delegation and exact adapter helpers, registered fail-closed deferred exceptions, read-only probes, inference-only model forwarding, the explicit Proxmox VNC ephemeral exception, bounded local synchronous exceptions, dead compatibility helpers, and generic notification-webhook outbound delivery. Notification delivery remains a separate egress milestone and is not silently counted as canonical mutation convergence.
- Focused fixture tests prove unknown callsites fail, canonical/deferred/out-of-scope classes pass, mutation-before-deferred and direct-provider-before-submit paths fail, function classifications are module-qualified, output ordering is deterministic, raw strings and source symlinks fail closed, marker variants are covered, and source contents/string values are not emitted. CI runs both the fixture tests and `--check` enforcement before repository hygiene.
- Final source check passed on this checkout with zero unknown callsites. This is `unit-verified` for the scanner/fixtures and `implemented` for CI enforcement; it is not provider runtime or release evidence.
- Reusable checks: `python3 -m unittest scripts.test_compatibility_mutation_inventory -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.

## M1-04 parser-backed inventory hardening — 2026-09-20

- The local parser validates every scanned Rust file with `rustfmt --edition 2021 --emit stdout`, tokenizes comments/raw strings/aliases, tracks inline module and impl scope, and records exact call markers. Unsupported macros are emitted as unknown rather than expanded or trusted; leading paths, raw identifiers, filesystem option builders, directory builders, permissions, and explicit route registrations have dedicated behavior.
- Every identity-based classification is pinned to an in-checkout function file and normalized-token SHA-256 digest in `scripts/compatibility_mutation_exception_evidence.json`; registry-declared deferred records are parsed as `DeferredMutationException` records and missing evidence remains an unknown finding. Extra evidence identities and symlinked/out-of-checkout evidence fail closed.
- Final focused evidence after hardening: 27 compatibility inventory tests and 18 repository-truth tests passed; the real source check passed with 146 classified findings and zero unknown findings. This remains `unit-verified`/source-enforcement evidence only; provider, host, browser, Docker, installation, upgrade/recovery, and release qualification remain unavailable.
- CI now provisions both `clippy` and `rustfmt` through `dtolnay/rust-toolchain` before backend gates. Do not treat the classified total as a stable product inventory; rerun the command after source changes.

## M1-04 parser-backed compatibility inventory continuation — 2026-09-20

- `scripts/rust_source_parser.py` validates each scanned Rust file through stdin-fed `rustfmt`, then parses bounded tokens and structural scopes. It resolves direct and imported aliases, `OpenOptions` write/create/truncate calls, directory APIs, process constructors, exact canonical adapter calls, inline modules, `impl` context, and immediate `#[cfg(test)]` item ranges. Malformed syntax/attributes and unbalanced delimiters fail closed.
- `scripts/compatibility_mutation_exception_evidence.json` records SHA-256 digests of the 11 deferred production function bodies that currently contain provider/filesystem/process markers. The inventory rejects missing evidence for registered deferred identities, wrong file identity, or changed body digest; fixture evidence can be supplied explicitly for synthetic registry sources.
- Focused coverage now includes aliased imports, directory and `OpenOptions` mutations, canonical text spoofing, inline module/`impl` identity, malformed `cfg(test)`, and changed exception bodies. The real source inventory remains credential-safe and reports zero unknown callsites after the parser-backed replacement.
- Evidence target is `unit-verified` for parser fixtures and source enforcement plus `implemented` for CI/docs integration. Provider runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain separate and unperformed.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `bash scripts/check-schema-migration-ownership.sh`; `bash scripts/check-repository-hygiene.sh`; and `git diff --check`.

## M1-04 parser-resolution closure — 2026-09-20

- The bounded source parser now normalizes Rust raw identifiers and recursively resolves multi-hop import aliases before classifying filesystem/provider/process paths. It records indirect function-value use and request-builder receiver aliases as unknown rather than trusting a later canonical-looking call.
- Executable `const`/`static` mutation initializers fail closed before inventory classification. Unsupported mutation call shapes, unresolved receiver provenance, function values, and macros remain unknown with bounded reasons; registered exception evidence cannot override these markers.
- Focused evidence after the final edit: `python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v` passed 51 tests; the production inventory passed with 146 classified and 0 unknown findings; the reviewed batch manifest passed source truth, both fixture suites, inventory enforcement, and diff checks. This remains unit/source-enforcement evidence, not runtime/provider/release qualification.
- Reusable batch evidence: `docs/internal/evidence/2026-09-20-m1-04-parser-resolution-closure/batch.json` and `docs/internal/evidence/2026-09-20-m1-04-parser-resolution-closure/final-report/evidence.json`.

## M1-04 parser-resolution closure review blocker — 2026-09-20

- Final focused execution in this worktree passed 55 Python tests (37 compatibility-inventory tests plus 18 repository-truth tests); the parser-backed source command reported `status=passed classified=146 unknown=0`; schema ownership, repository hygiene, Python compilation, diff checks, rustfmt, strict Clippy, and one backend full target run (693 unit tests, 2 workflow-contract integration tests, examples) passed in the named sandbox runs. A separate review run reproduced the existing SQLite `database is locked` failure, so deterministic full-gate status remains blocked.
- Independent adversarial review still blocks the slice. Re-export/module provenance, UFCS/angle-bracket calls, closure/control-flow around `FeatureUnavailable`, and suffix-based canonical-call recognition are not proven fail closed. The checkout-local exception digest is edit detection, not an independently immutable approval ledger.
- The parser module and exception evidence are untracked while the older inventory implementation is staged; no commit was created. Ignored batch evidence exists under `docs/internal/evidence/2026-09-20-m1-04-parser-resolution-closure/` but is not commit-backed. Runtime/provider/browser/Docker/install/upgrade/recovery qualification was not attempted.
- Next dependency: replace the bounded token resolver with compiler-grade AST/module/import/call-shape resolution, or explicitly reject every unresolved form before classification; bind exceptions to a separately reviewed immutable/generated ledger, align staged and worktree state, then rerun the complete gates and independent review.

## M1-04 parser-resolution fail-closed continuation — 2026-09-21

- `scripts/rust_source_parser.py` now rejects production `pub use` re-exports unless they are the one known-safe `std::str::FromStr` trait import. Local/wildcard mutation provenance therefore fails the source check instead of being treated as a resolved alias.
- UFCS/angle-bracket calls whose method is mutation- or request-shaped emit `unsupported_call_shape`. A local helper named `prepare_or_submit` or `prepare_or_submit_with_credential` is not canonical by suffix: the parser marks a helper trusted only when its same-module call graph reaches an exact `operation_adoption`/`invocation` adapter call.
- Deferred classification rejects `FeatureUnavailable` returned from a closure marker (`||`) before a reachable mutation; conditional/control-flow forms remain fail-closed. The inventory continues to require exact checkout-local exception body evidence and rejects missing, changed, extra, or unsafe evidence paths.
- Focused evidence after this continuation: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v` passed 59 tests; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check` passed with source-derived `status=passed classified=146 unknown=0`; Python compilation passed. This remains unit/source-enforcement evidence only; the checkout-local exception file is not an independently immutable approval ledger, and provider/runtime/release qualification remains unperformed.
- Reusable checks: the 59-test command above; the inventory command above; `python3 -m py_compile scripts/rust_source_parser.py scripts/compatibility_mutation_inventory.py scripts/test_compatibility_mutation_inventory.py`; then repository schema/hygiene, backend focused/full, and diff gates.

## M1-04 parser-resolution adversarial closure — 2026-09-21

- The resolver's canonical helper proof is keyed by `(fully qualified module, function)` and only propagates along same-module local calls. Same-named helpers in sibling inline modules cannot share canonical proof.
- Private imported aliases whose resolved terminal is mutation-shaped, and bare imported aliases with any unallowlisted resolved provenance, emit `unsupported_call_shape`; unknown wildcard imports are rejected at parse time; qualified associated `std::io`/`tokio::io` mutation methods and uncovered filesystem `File` mutators do the same. Route-builder aliases under `axum::routing` and established local service/provider targets remain recognized framework/domain boundaries.
- Deferred `FeatureUnavailable` checks reject closure, `async` block, and nested-function scopes before classifying reachable provider/filesystem/process markers. Adversarial fixtures now cover private imports, qualified associated calls, async blocks, nested exceptions, local-module canonical spoofing, trusted-name alias spoofing, parameter/mutable-local shadowing, and cross-module helper shadowing.
- Final focused evidence for this continuation: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v` passed 70 tests; the production inventory passed with `status=passed classified=146 unknown=0`. The prior full backend run passed 693 unit tests, 2 integration tests, and examples after disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` cleanup; rustfmt, strict Clippy, schema ownership, repository hygiene, and diff checks also passed.
- Remaining trust limitation: exception SHA-256 evidence is still checkout-local edit detection, not an independently immutable approval ledger. Do not promote this source-only continuation to runtime/release evidence or treat the classified total as a stable claim.

## M1-04 parser-resolution canonical scope closure — 2026-09-21

- Canonical delegation is now allowlisted by exact resolved targets (`operation_adoption::{submit,submit_with_key,prepare}`, their bounded `super::`/`crate::api::` forms, and `crate::operations::invocation::submit`). Broad `crate::operations::`, `crate::networking::`, `crate::cmdb::`, and `super::support` prefixes no longer authorize mutation aliases; reviewed CMDB/support calls and local proxy helpers use explicit exact identities.
- `_shadowed_aliases` disables canonical/mutation alias proof when imported names are rebound by function parameters, locals, destructuring/`if let`/`for`/braced `match` patterns, or closure parameters anywhere in the item; the same pattern analysis gates same-module helper propagation. Nested functions, typed-return closures, and async-block bodies are removed from enclosing canonical proof, and nested items cannot become same-module trusted helpers for their parent. Typed/imported/type-aliased/reference-qualified/temporary-chained/assignment-forwarded/reference-forwarded/parenthesized-reference filesystem receivers and imported request-builder receivers now emit mutation markers instead of disappearing from the inventory; external `cfg(test)` modules declared from any Rust file are excluded only when their target is not also a production module target, valid raw/quoted production `#[path]` targets inside `backend/src` are scanned, escaping paths fail closed, and local/ancestor module/use/extern canonical declarations disable canonical proof.
- Focused evidence for this continuation: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory -q` passed 82 compatibility tests and the combined command with repository truth passed 100 tests; the production source check reported `status=passed classified=144 unknown=0`. This is `unit-verified` source-enforcement evidence only; runtime/provider/browser/Docker/install/upgrade/recovery/release qualification remains unperformed.
- Blocker: independent adversarial review still rejects the slice because the exception-evidence SHA-256 file is checkout-local and self-updatable; it is edit detection, not an immutable approval ledger. No commit or release qualification is allowed until exception approval is independently anchored and the final reviewer passes.
- Reusable checks: the focused command above; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `python3 -m py_compile scripts/rust_source_parser.py scripts/compatibility_mutation_inventory.py scripts/test_compatibility_mutation_inventory.py`; repository truth; schema ownership; repository hygiene; backend format/Clippy/full tests; and `git diff --check`.
- Remaining limitation: the checkout-local exception SHA-256 manifest detects edits but is not an independently immutable approval ledger. Do not treat the classified count as a stable product inventory or promote this slice to runtime/release evidence.

## M1-04 final independent-review blocker — 2026-09-21

- Independent review rejected commit readiness despite the 70-test, 146-classified/0-unknown source checks. Reproduced gaps: broad `crate::operations::`/`crate::networking::` provenance prefixes permit arbitrary alias targets; canonical imported names can be shadowed by parameters/locals; nested canonical calls contaminate the containing function's call set.
- Required next step is exact canonical adapter identity plus lexical/item-scope proof, or explicit fail-closed rejection of unresolved canonical forms. The current candidate is `blocked`, not committed, and has no runtime/release qualification.

## M1-04 Git-anchored exception approval closure — 2026-09-21

- `scripts/compatibility_mutation_inventory.py --base <commit>` now resolves a full Git commit without a shell, reads evidence-bound source from that commit, and rejects changed, new, moved, or missing exception bodies even when the checkout-local SHA-256 manifest is regenerated. Git-backed local runs default to `HEAD`; CI passes the protected pull-request base SHA through `.github/workflows/ci.yml`.
- The inventory report exposes only `exception_approval.mode` and the resolved commit ID alongside its existing bounded findings. A checkout without a resolvable Git base fails closed; production checkouts use `git_base` approval mode, and fixtures initialize an isolated temporary Git baseline.
- Receiver provenance now propagates only for locally named functions whose parsed return type is a filesystem or request-builder type, while covering qualified, method/turbofish, parenthesized, and simple tuple-pattern calls. This avoids treating ordinary values such as `Vec::truncate` results as filesystem mutations while still making unresolved receiver flows visible through the inventory fixtures.
- Focused evidence after this closure: 104 compatibility/repository-truth tests passed after adding changed-body and new-body approval regressions; the production source check passed with `status=passed classified=144 unknown=0` against `HEAD` `b9a24729c2a7750900f285d61daa4439e0cd95f9`; Python compilation passed. Runtime/provider/browser/Docker/install/upgrade/recovery/release qualification remains unperformed.

## M1-04 parser-resolution approval and provenance hardening — 2026-09-21

- The inventory now requires the active `DeferredMutationException` source set itself to match the requested Git base, so a newly registered existing function cannot be approved merely by adding a regenerated body digest. Requested approval bases must be full object IDs; symbolic refs fail closed.
- Filesystem mutator methods on unproven receivers emit `unresolved_receiver_provenance` instead of disappearing when values flow through conditional, block, match, parenthesized, closure, or other unsupported expressions. The focused fixture family covers these forms.
- CI runs compatibility approval on pull requests or protected-branch pushes and explicitly rejects unprotected push approval contexts. This remains source/unit evidence only; provider, runtime, browser, Docker, install/upgrade/recovery, and release qualification are unperformed.
- The final independent review must pass after these fixes; no commit is authorized before that review.
- The public developer contract in `docs/api.md` now defines the protected-base approval boundary and two-phase exception change workflow. Remaining documentation work covers canonical adapters behind deferred routes and generic notification-webhook egress controls; this slice does not make those routes available.

## M1-04 final review blocker — 2026-09-21

- Latest focused evidence after parser rollback of over-broad qualified-call heuristics: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -q` passed 109 tests; Python compilation and `git diff --check` passed; the Git-base inventory reported `status=passed classified=148 unknown=0 mode=git_base base=b9a24729c2a7750900f285d61daa4439e0cd95f9`.
- Independent review still rejects release of this candidate: valid qualified/UFCS, generic, borrowed, function-value, and helper-returned mutation forms need a compiler-backed or explicitly comprehensive fail-closed contract; CI still executes the PR-controlled verifier in the mutable checkout rather than an independently trusted verifier artifact.
- This candidate remains `blocked`, uncommitted, and not runtime/integration/release qualified. The next bounded slice is to define and test a trusted clean-checkout verifier boundary plus canonical compiler-backed mutation extraction before widening parser coverage.

## M1-04 trusted verifier and bounded syntax contract — 2026-09-21

- `.github/workflows/compatibility-enforcement.yml` defines the trust boundary for future pull requests: `pull_request_target` checks out the approved base as `trusted-verifier`, checks out the PR head as `candidate-source` data, and executes only the approved-base inventory implementation. A missing verifier artifact fails closed and requires protected-branch bootstrap; the ordinary `ci.yml` job retains fixture tests but no longer makes enforcement decisions with PR-controlled verifier code.
- `scripts.test_compatibility_mutation_inventory.test_rust_mutation_syntax_contract_is_explicit_and_never_silent` is the executable bounded syntax contract for qualified calls, UFCS, generic calls, borrowed typed receivers, mutation function values, helper-returned receivers, and exact canonical delegation. Every listed form must emit a recognized or explicit unsupported/provenance marker.
- Focused RED/GREEN evidence: the new syntax test initially failed because the canonical call was checked in the wrong marker collection; after correcting the public test seam it passed, and the trusted-workflow contract test passed. Full suite and final inventory still require rerun after all edits.

## M1-04 trusted verifier and bounded syntax contract final — 2026-09-21

- `.github/workflows/compatibility-enforcement.yml` pins both `actions/checkout` uses to reviewed commit `fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09` (`v5.1.0`), disables persisted credentials, explicitly permits fork-head materialization as inert data, checks out the approved base as `trusted-verifier`, and executes only that base-owned verifier against `candidate-source`. Missing verifier artifacts fail closed. The ordinary `ci.yml` job retains fixture tests but does not run candidate-controlled enforcement logic.
- Final focused evidence: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -q` passed 111 tests; the Git-base inventory passed with `status=passed`, `classified=148`, `unknown=0`, `mode=git_base`, and base `b9a24729c2a7750900f285d61daa4439e0cd95f9`; repository truth, schema ownership, repository hygiene, and `git diff --check` passed.
- Final Rust evidence: `cd backend && cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets --all-features` passed; the full test gate reported 693 unit tests, 2 integration tests, and examples with zero failures. No runtime provider, GitHub workflow, browser, Docker, installation, upgrade/recovery, or release qualification was performed in this sandbox.
- Independent final review passed with no security or logic findings. Non-blocking follow-up is to observe a protected-branch `pull_request_target` run and branch-protection status after merge, then advance to the tracked V6-01 versioned API/event schema slice; do not treat local YAML contract tests as GitHub runtime activation evidence.
- Current maturity is `unit-verified` for the parser/inventory/workflow contracts and `implemented` for the trusted workflow source boundary. GitHub workflow activation, provider, browser, Docker, install/upgrade/recovery, and release evidence remain unverified because this sandbox cannot exercise the protected-host/GitHub boundary; the next tracked dependency is V6-01.

## V6-01 activation-gate checkpoint — 2026-09-21

- No new V6-01 versioned-schema/generated-client work was started in this checkpoint because the newest approved handoff requires observed protected-branch activation of `.github/workflows/compatibility-enforcement.yml` before the remaining schema work begins; earlier V6-01 version-negotiation, envelope, and web recovery work remains historical source/test evidence.
- At local `HEAD` `676c3212e762b5d454405188a88de91385a51fe8`, the verifier workflow exists only in local commits; `git ls-remote origin refs/heads/dev` remains `b9a24729c2a7750900f285d61daa4439e0cd95f9`.
- Paginated public GitHub Actions API read-only queries covered all 583 listed runs and returned no compatibility-named run; the workflow-specific endpoint returned HTTP 404. `gh` is unavailable and the public branch-protection endpoint returned HTTP 401, so no run ID or required-status configuration can be recorded from this sandbox.
- `scripts/repo_truth.py --repo . --json --check` and `scripts/compatibility_mutation_inventory.py --repo . --check` passed at local `HEAD`; this is source/inventory evidence only and does not activate GitHub enforcement.
- Maturity remains `blocked` for V6-01 activation. The next action is operator-side protected-branch publication and observation, followed by V6-01 source-owned schemas, generated clients/OpenAPI where applicable, negotiation, SSE recovery, drift tests, and documentation.

## V6-01 activation recheck — 2026-09-21

- No V6-01 product-contract implementation was started because the newest approved handoff still gates the remaining schema/generated-client work on observed protected-branch activation of `.github/workflows/compatibility-enforcement.yml`.
- At local `HEAD` `77e9529a05f9074862f57a580e24f5728223964d`, `origin/dev` remains `b9a24729c2a7750900f285d61daa4439e0cd95f9`; the workflow file is present locally, but protected GitHub run and required-status evidence remain unavailable in this sandbox.
- Recheck evidence: repository truth passed, compatibility mutation inventory passed with `status: passed` and `unknown: []`, `git diff --check` passed, and the local workflow file exists. These are source/inventory checks only and do not activate protected enforcement.
- Maturity remains `blocked` for V6-01 activation. The next dependency is operator-side protected-branch publication and observation, then source-owned schemas, generated clients/OpenAPI where applicable, negotiation, SSE recovery, drift tests, and documentation.

## V6-01 activation recheck session — 2026-09-21

- No new V6-01 product-contract implementation was started in this session because the approved handoff still requires observed protected activation of `.github/workflows/compatibility-enforcement.yml` before the remaining schema/generated-client work; existing version-negotiation, envelope, and SSE-recovery seams remain historical source/test evidence.
- Current `HEAD` is `cf78751e7af644ff68db3df5a70dd6f3aa419de0`; `origin/dev` remains `b9a24729c2a7750900f285d61daa4439e0cd95f9`. The workflow is locally present, but this sandbox has no authenticated protected workflow-run or branch-protection evidence.
- Repository truth and compatibility mutation inventory passed; `git diff --check` passed; pre-existing untracked cache/testing paths remain untouched. These are source/inventory checks only and do not activate protected enforcement.
- Maturity remains `blocked`. After operator-side activation evidence, resume V6-01 across source-owned schemas, generated clients/OpenAPI where applicable, negotiation/errors, SSE recovery, drift tests, and documentation.
